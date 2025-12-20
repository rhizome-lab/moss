"""Persistent terminal session for shell command execution.

Provides a stateful shell session that maintains working directory,
environment variables, and command history across multiple invocations.

Usage:
    async with PersistentShell() as shell:
        result = await shell.run("cd /tmp")
        result = await shell.run("pwd")  # Returns /tmp
        result = await shell.run("export FOO=bar")
        result = await shell.run("echo $FOO")  # Returns bar
"""

from __future__ import annotations

import asyncio
import logging
import os
import re
import shlex
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

# Marker for detecting command completion
END_MARKER = "__MOSS_END_"


@dataclass
class ShellResult:
    """Result of a shell command execution."""

    command: str
    stdout: str
    stderr: str
    returncode: int
    cwd: str
    duration_ms: int = 0

    @property
    def success(self) -> bool:
        """Check if command succeeded."""
        return self.returncode == 0

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "command": self.command,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "returncode": self.returncode,
            "cwd": self.cwd,
            "duration_ms": self.duration_ms,
            "success": self.success,
        }

    def to_compact(self) -> str:
        """Compact string representation."""
        status = "✓" if self.success else f"✗ ({self.returncode})"
        output = self.stdout.strip()[:200]
        if len(self.stdout.strip()) > 200:
            output += "..."
        return f"{status} {self.cwd}$ {self.command}\n{output}"


def _find_shell() -> str:
    """Find an available shell."""
    import shutil

    for shell in ["bash", "sh", "zsh"]:
        path = shutil.which(shell)
        if path:
            return path

    # Fallback to common paths
    for path in ["/bin/bash", "/bin/sh", "/usr/bin/bash", "/usr/bin/sh"]:
        if Path(path).exists():
            return path

    return "sh"  # Last resort


@dataclass
class ShellConfig:
    """Configuration for the persistent shell."""

    shell: str = ""  # Empty means auto-detect
    cwd: Path | None = None
    env: dict[str, str] = field(default_factory=dict)
    timeout_seconds: float = 60.0
    inherit_env: bool = True
    env_blocklist: list[str] = field(
        default_factory=lambda: [
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
        ]
    )


class PersistentShell:
    """A persistent shell session that maintains state.

    Unlike subprocess.run(), this maintains a single shell process
    and preserves working directory, environment variables, aliases,
    and shell state across commands.
    """

    def __init__(self, config: ShellConfig | None = None):
        self.config = config or ShellConfig()
        self._process: asyncio.subprocess.Process | None = None
        self._stdin: asyncio.StreamWriter | None = None
        self._stdout: asyncio.StreamReader | None = None
        self._stderr: asyncio.StreamReader | None = None
        self._cwd: str = str(self.config.cwd or Path.cwd())
        self._history: list[ShellResult] = []
        self._session_id = str(uuid.uuid4())[:8]

    @property
    def is_running(self) -> bool:
        """Check if shell process is running."""
        return self._process is not None and self._process.returncode is None

    @property
    def cwd(self) -> str:
        """Current working directory."""
        return self._cwd

    @property
    def history(self) -> list[ShellResult]:
        """Command history for this session."""
        return self._history.copy()

    async def start(self) -> None:
        """Start the shell process."""
        if self.is_running:
            return

        env = self._build_env()
        cwd = self.config.cwd or Path.cwd()
        shell_path = self.config.shell or _find_shell()

        self._process = await asyncio.create_subprocess_exec(
            shell_path,
            "-i",  # Interactive mode
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env,
            cwd=str(cwd),
        )

        self._stdin = self._process.stdin
        self._stdout = self._process.stdout
        self._stderr = self._process.stderr
        self._cwd = str(cwd)

        # Set up shell for better output parsing
        await self._setup_shell()

        logger.debug(f"Shell started: pid={self._process.pid}, cwd={self._cwd}")

    async def _setup_shell(self) -> None:
        """Configure shell for programmatic use."""
        # Disable prompts and set up clean output
        setup_commands = [
            "set +o history",  # Disable history
            "PS1=''",  # No prompt
            "PS2=''",  # No continuation prompt
            "export TERM=dumb",  # Simple terminal
        ]
        for cmd in setup_commands:
            await self._raw_exec(cmd)

    def _build_env(self) -> dict[str, str]:
        """Build environment for the shell."""
        if self.config.inherit_env:
            env = dict(os.environ)
        else:
            env = {}

        # Remove blocked variables
        for key in self.config.env_blocklist:
            env.pop(key, None)

        # Add custom env
        env.update(self.config.env)

        return env

    async def _raw_exec(self, command: str) -> None:
        """Execute command without waiting for response."""
        if not self._stdin:
            raise RuntimeError("Shell not started")
        self._stdin.write(f"{command}\n".encode())
        await self._stdin.drain()

    async def run(
        self,
        command: str,
        timeout: float | None = None,
    ) -> ShellResult:
        """Execute a command and return the result.

        Args:
            command: Shell command to execute
            timeout: Command timeout in seconds

        Returns:
            ShellResult with output and status
        """
        if not self.is_running:
            await self.start()

        timeout = timeout or self.config.timeout_seconds
        start_time = asyncio.get_event_loop().time()

        # Create unique markers for this command
        marker = f"{END_MARKER}{uuid.uuid4().hex[:8]}"

        # Build command that captures output and exit code
        # Note: No subshell wrapper to preserve cd, export, etc.
        wrapped = (
            f'{command}; __moss_rc=$?; pwd 1>&2; echo "{marker}:$__moss_rc"; echo "{marker}" >&2'
        )

        # Send command
        assert self._stdin is not None
        self._stdin.write(f"{wrapped}\n".encode())
        await self._stdin.drain()

        # Read output until we see the marker
        stdout_parts: list[str] = []
        stderr_parts: list[str] = []
        returncode = 0
        new_cwd = self._cwd

        try:
            # Read stdout
            stdout_done = False
            stderr_done = False

            async def read_stdout():
                nonlocal stdout_done, returncode
                assert self._stdout is not None
                while not stdout_done:
                    try:
                        line = await asyncio.wait_for(
                            self._stdout.readline(),
                            timeout=0.1,
                        )
                        if not line:
                            break
                        decoded = line.decode("utf-8", errors="replace")
                        if marker in decoded:
                            # Extract return code
                            match = re.search(rf"{marker}:(\d+)", decoded)
                            if match:
                                returncode = int(match.group(1))
                            stdout_done = True
                        else:
                            stdout_parts.append(decoded)
                    except TimeoutError:
                        continue

            async def read_stderr():
                nonlocal stderr_done, new_cwd
                assert self._stderr is not None
                while not stderr_done:
                    try:
                        line = await asyncio.wait_for(
                            self._stderr.readline(),
                            timeout=0.1,
                        )
                        if not line:
                            break
                        decoded = line.decode("utf-8", errors="replace")
                        if marker in decoded:
                            stderr_done = True
                        else:
                            # Look for pwd output (absolute path to directory)
                            stripped = decoded.strip()
                            # Extract path from line (may have ANSI codes or prompts)
                            # Look for absolute paths starting with /
                            for word in stripped.split():
                                clean = re.sub(r"\x1b\[[0-9;]*m", "", word)
                                if clean.startswith("/") and Path(clean).is_dir():
                                    new_cwd = clean
                                    break
                            else:
                                # No path found, add to stderr
                                if stripped and not stripped.startswith("bash:"):
                                    stderr_parts.append(decoded)
                    except TimeoutError:
                        continue

            await asyncio.wait_for(
                asyncio.gather(read_stdout(), read_stderr()),
                timeout=timeout,
            )

        except TimeoutError:
            # Kill the command but keep the shell
            if self._process:
                try:
                    self._process.send_signal(2)  # SIGINT
                except ProcessLookupError:
                    pass
            return ShellResult(
                command=command,
                stdout="".join(stdout_parts),
                stderr=f"Command timed out after {timeout}s",
                returncode=-1,
                cwd=self._cwd,
                duration_ms=int(timeout * 1000),
            )

        duration_ms = int((asyncio.get_event_loop().time() - start_time) * 1000)
        self._cwd = new_cwd

        result = ShellResult(
            command=command,
            stdout="".join(stdout_parts).rstrip("\n"),
            stderr="".join(stderr_parts).rstrip("\n"),
            returncode=returncode,
            cwd=self._cwd,
            duration_ms=duration_ms,
        )

        self._history.append(result)
        return result

    async def close(self) -> None:
        """Close the shell session."""
        if self._process:
            try:
                if self._stdin and not self._stdin.is_closing():
                    self._stdin.write(b"exit\n")
                    try:
                        await self._stdin.drain()
                    except (ConnectionResetError, BrokenPipeError):
                        pass
                    self._stdin.close()
                await asyncio.wait_for(self._process.wait(), timeout=2.0)
            except (TimeoutError, ProcessLookupError, ConnectionResetError):
                try:
                    self._process.kill()
                except ProcessLookupError:
                    pass
            finally:
                self._process = None
                self._stdin = None
                self._stdout = None
                self._stderr = None

        logger.debug(f"Shell closed: session={self._session_id}")

    async def __aenter__(self) -> PersistentShell:
        await self.start()
        return self

    async def __aexit__(self, *args) -> None:
        await self.close()


class TerminalSubagent:
    """Subagent that uses a persistent shell for command execution.

    Provides a high-level interface for running shell commands with
    error handling, retries, and structured output.
    """

    def __init__(
        self,
        shell: PersistentShell | None = None,
        config: ShellConfig | None = None,
    ):
        self._shell = shell
        self._config = config or ShellConfig()
        self._owns_shell = shell is None

    @property
    def shell(self) -> PersistentShell:
        """Get or create the shell instance."""
        if self._shell is None:
            self._shell = PersistentShell(self._config)
        return self._shell

    async def start(self) -> None:
        """Start the terminal session."""
        await self.shell.start()

    async def close(self) -> None:
        """Close the terminal session."""
        if self._owns_shell and self._shell:
            await self._shell.close()

    async def execute(
        self,
        command: str,
        *,
        check: bool = False,
        timeout: float | None = None,
    ) -> ShellResult:
        """Execute a command.

        Args:
            command: Shell command to run
            check: Raise exception on failure
            timeout: Command timeout

        Returns:
            ShellResult with output
        """
        result = await self.shell.run(command, timeout=timeout)

        if check and not result.success:
            raise RuntimeError(
                f"Command failed: {command}\n"
                f"Exit code: {result.returncode}\n"
                f"Stderr: {result.stderr}"
            )

        return result

    async def cd(self, path: str) -> ShellResult:
        """Change directory."""
        return await self.execute(f"cd {shlex.quote(path)}")

    async def pwd(self) -> str:
        """Get current working directory."""
        return self.shell.cwd

    async def ls(self, path: str = ".") -> list[str]:
        """List directory contents."""
        result = await self.execute(f"ls -1 {shlex.quote(path)}")
        if result.success:
            return result.stdout.strip().split("\n") if result.stdout.strip() else []
        return []

    async def cat(self, path: str) -> str:
        """Read file contents."""
        result = await self.execute(f"cat {shlex.quote(path)}")
        return result.stdout

    async def write(self, path: str, content: str) -> ShellResult:
        """Write content to file."""
        escaped = content.replace("'", "'\"'\"'")
        return await self.execute(f"echo '{escaped}' > {shlex.quote(path)}")

    async def run_script(
        self,
        script: str,
        *,
        check: bool = False,
    ) -> list[ShellResult]:
        """Run a multi-line script, executing each line.

        Args:
            script: Multi-line script
            check: Stop on first failure

        Returns:
            List of results for each line
        """
        results: list[ShellResult] = []

        for line in script.strip().split("\n"):
            line = line.strip()
            if not line or line.startswith("#"):
                continue

            result = await self.execute(line)
            results.append(result)

            if check and not result.success:
                break

        return results

    async def __aenter__(self) -> TerminalSubagent:
        await self.start()
        return self

    async def __aexit__(self, *args) -> None:
        await self.close()
