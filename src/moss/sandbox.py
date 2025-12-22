"""Safe shell execution with sandboxing.

Provides restricted versions of dangerous commands with safety guardrails.
Integrates with the policy engine for command evaluation.
"""

from __future__ import annotations

import shlex
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class CommandResult:
    """Result of a sandboxed command execution."""

    returncode: int
    stdout: str
    stderr: str
    command: str
    blocked: bool = False
    block_reason: str | None = None

    @property
    def success(self) -> bool:
        return self.returncode == 0 and not self.blocked

    def to_dict(self) -> dict[str, Any]:
        return {
            "returncode": self.returncode,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "command": self.command,
            "blocked": self.blocked,
            "block_reason": self.block_reason,
        }


@dataclass
class SandboxConfig:
    """Configuration for the sandbox."""

    # Working directory restriction
    workspace: Path | None = None
    allow_outside_workspace: bool = False

    # URL allowlisting for curl/wget
    allowed_url_prefixes: list[str] = field(
        default_factory=lambda: [
            "https://api.github.com/",
            "https://raw.githubusercontent.com/",
            "https://pypi.org/",
            "https://registry.npmjs.org/",
            "https://crates.io/",
        ]
    )

    # File deletion restrictions
    allow_delete: bool = False
    delete_require_confirmation: bool = True
    delete_max_files: int = 10
    delete_blocked_patterns: list[str] = field(
        default_factory=lambda: [
            ".git",
            ".env",
            "node_modules",
            "__pycache__",
            ".venv",
            "venv",
        ]
    )

    # Command timeout
    timeout_seconds: int = 60

    # Environment filtering
    inherit_env: bool = True
    env_blocklist: list[str] = field(
        default_factory=lambda: [
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "DATABASE_URL",
            "DB_PASSWORD",
        ]
    )


class SafeShell:
    """Sandboxed shell execution with safety guardrails."""

    def __init__(self, config: SandboxConfig | None = None):
        self.config = config or SandboxConfig()

    def _filter_env(self) -> dict[str, str]:
        """Get filtered environment variables."""
        import os

        if not self.config.inherit_env:
            return {}

        env = dict(os.environ)
        for key in self.config.env_blocklist:
            env.pop(key, None)
        return env

    def _check_workspace(self, path: Path) -> str | None:
        """Check if path is within workspace. Returns error message if not."""
        if self.config.workspace is None or self.config.allow_outside_workspace:
            return None

        try:
            resolved = path.resolve()
            workspace = self.config.workspace.resolve()
            if not str(resolved).startswith(str(workspace)):
                return f"Path {path} is outside workspace {workspace}"
        except (OSError, ValueError) as e:
            return f"Cannot resolve path: {e}"

        return None

    def run(
        self,
        command: str,
        *,
        cwd: Path | None = None,
        timeout: int | None = None,
        check: bool = False,
    ) -> CommandResult:
        """Run a command through the sandbox.

        Args:
            command: Shell command to execute
            cwd: Working directory (defaults to workspace)
            timeout: Command timeout in seconds
            check: Raise exception on non-zero exit

        Returns:
            CommandResult with output and status
        """
        timeout = timeout or self.config.timeout_seconds
        cwd = cwd or self.config.workspace or Path.cwd()

        # Check workspace restriction
        workspace_error = self._check_workspace(cwd)
        if workspace_error:
            return CommandResult(
                returncode=1,
                stdout="",
                stderr=workspace_error,
                command=command,
                blocked=True,
                block_reason=workspace_error,
            )

        try:
            result = subprocess.run(
                command,
                shell=True,
                cwd=cwd,
                capture_output=True,
                text=True,
                timeout=timeout,
                env=self._filter_env(),
            )
            return CommandResult(
                returncode=result.returncode,
                stdout=result.stdout,
                stderr=result.stderr,
                command=command,
            )
        except subprocess.TimeoutExpired:
            return CommandResult(
                returncode=124,
                stdout="",
                stderr=f"Command timed out after {timeout}s",
                command=command,
                blocked=True,
                block_reason="timeout",
            )
        except subprocess.SubprocessError as e:
            return CommandResult(
                returncode=1,
                stdout="",
                stderr=str(e),
                command=command,
                blocked=True,
                block_reason=str(e),
            )

    def shrink_to_fit(self, accessed_paths: list[Path]) -> None:
        """Proactively restrict workspace to the common parent of accessed paths.

        Adaptive Workspace Scoping: Automatically shrink the sandbox based
        on detected access patterns to minimize blast radius.
        """
        if not accessed_paths:
            return

        if len(accessed_paths) == 1:
            self.config.workspace = accessed_paths[0].parent
            return

        # Find common parent
        import os

        common = os.path.commonpath([str(p.resolve()) for p in accessed_paths])
        self.config.workspace = Path(common)

    def expand_to_include(self, additional_paths: list[Path]) -> None:
        """Proactively expand workspace to include additional paths.

        Adaptive Workspace Expansion: Proactively grow the sandbox when
        cross-file dependencies or related components are detected.
        """
        if not additional_paths or self.config.workspace is None:
            return

        current_paths = [self.config.workspace]
        all_paths = current_paths + additional_paths

        import os

        common = os.path.commonpath([str(p.resolve()) for p in all_paths])
        self.config.workspace = Path(common)

    def safe_curl(
        self,
        url: str,
        *,
        output: Path | None = None,
        method: str = "GET",
        headers: dict[str, str] | None = None,
        data: str | None = None,
        timeout: int = 30,
    ) -> CommandResult:
        """Safe curl wrapper with URL allowlisting.

        Args:
            url: URL to fetch (must match allowed prefixes)
            output: Optional output file path
            method: HTTP method
            headers: Optional headers
            data: Optional request body
            timeout: Request timeout

        Returns:
            CommandResult with response
        """
        # Check URL against allowlist
        url_allowed = any(url.startswith(prefix) for prefix in self.config.allowed_url_prefixes)
        if not url_allowed:
            return CommandResult(
                returncode=1,
                stdout="",
                stderr=f"URL not in allowlist: {url}",
                command=f"curl {url}",
                blocked=True,
                block_reason=f"URL not allowed. Allowed prefixes: "
                f"{self.config.allowed_url_prefixes}",
            )

        # Build curl command
        cmd_parts = ["curl", "-sSL", "--max-time", str(timeout)]

        if method != "GET":
            cmd_parts.extend(["-X", method])

        if headers:
            for key, value in headers.items():
                cmd_parts.extend(["-H", f"{key}: {value}"])

        if data:
            cmd_parts.extend(["-d", data])

        if output:
            workspace_error = self._check_workspace(output)
            if workspace_error:
                return CommandResult(
                    returncode=1,
                    stdout="",
                    stderr=workspace_error,
                    command=" ".join(cmd_parts),
                    blocked=True,
                    block_reason=workspace_error,
                )
            cmd_parts.extend(["-o", str(output)])

        cmd_parts.append(url)
        command = shlex.join(cmd_parts)

        return self.run(command, timeout=timeout + 5)

    def safe_delete(
        self,
        paths: list[Path],
        *,
        force: bool = False,
        recursive: bool = False,
        dry_run: bool = False,
    ) -> CommandResult:
        """Safe file deletion with restrictions.

        Args:
            paths: Files/directories to delete
            force: Skip confirmation for protected patterns
            recursive: Allow recursive deletion
            dry_run: Show what would be deleted without deleting

        Returns:
            CommandResult with operation status
        """
        if not self.config.allow_delete:
            return CommandResult(
                returncode=1,
                stdout="",
                stderr="File deletion is disabled in sandbox config",
                command=f"rm {' '.join(str(p) for p in paths)}",
                blocked=True,
                block_reason="delete_disabled",
            )

        # Check path count
        if len(paths) > self.config.delete_max_files:
            return CommandResult(
                returncode=1,
                stdout="",
                stderr=f"Too many files ({len(paths)}). Max: {self.config.delete_max_files}",
                command=f"rm {' '.join(str(p) for p in paths)}",
                blocked=True,
                block_reason="too_many_files",
            )

        # Check each path
        errors = []
        valid_paths = []
        for path in paths:
            # Workspace check
            workspace_error = self._check_workspace(path)
            if workspace_error:
                errors.append(workspace_error)
                continue

            # Pattern check
            path_str = str(path)
            for pattern in self.config.delete_blocked_patterns:
                if pattern in path_str:
                    if not force:
                        errors.append(f"Protected pattern '{pattern}' in path: {path}")
                        continue

            # Recursive check
            if path.is_dir() and not recursive:
                errors.append(f"Directory deletion requires recursive=True: {path}")
                continue

            valid_paths.append(path)

        if errors:
            return CommandResult(
                returncode=1,
                stdout="",
                stderr="\n".join(errors),
                command=f"rm {' '.join(str(p) for p in paths)}",
                blocked=True,
                block_reason="validation_failed",
            )

        if dry_run:
            would_delete = "\n".join(f"  {p}" for p in valid_paths)
            return CommandResult(
                returncode=0,
                stdout=f"Would delete:\n{would_delete}",
                stderr="",
                command=f"rm --dry-run {' '.join(str(p) for p in paths)}",
            )

        # Actually delete
        deleted = []
        failed = []
        for path in valid_paths:
            try:
                if path.is_dir():
                    import shutil

                    shutil.rmtree(path)
                else:
                    path.unlink()
                deleted.append(str(path))
            except OSError as e:
                failed.append(f"{path}: {e}")

        if failed:
            return CommandResult(
                returncode=1,
                stdout=f"Deleted: {', '.join(deleted)}" if deleted else "",
                stderr="\n".join(failed),
                command=f"rm {' '.join(str(p) for p in paths)}",
            )

        return CommandResult(
            returncode=0,
            stdout=f"Deleted {len(deleted)} files",
            stderr="",
            command=f"rm {' '.join(str(p) for p in paths)}",
        )

    def safe_git(
        self,
        args: list[str],
        *,
        cwd: Path | None = None,
    ) -> CommandResult:
        """Safe git wrapper.

        Blocks dangerous operations like force push to main.

        Args:
            args: Git command arguments
            cwd: Working directory

        Returns:
            CommandResult with git output
        """
        if not args:
            return CommandResult(
                returncode=1,
                stdout="",
                stderr="No git command specified",
                command="git",
                blocked=True,
                block_reason="no_command",
            )

        # Block dangerous operations
        dangerous_patterns = [
            (["push", "--force"], "main", "force push to main"),
            (["push", "--force"], "master", "force push to master"),
            (["push", "-f"], "main", "force push to main"),
            (["push", "-f"], "master", "force push to master"),
            (["reset", "--hard"], "HEAD~", "hard reset with history loss"),
            (["clean", "-fd"], None, "force clean"),
            (["checkout", "--"], None, "discard changes"),
        ]

        args_str = " ".join(args)
        for pattern, target, reason in dangerous_patterns:
            if all(p in args for p in pattern):
                if target is None or target in args_str:
                    return CommandResult(
                        returncode=1,
                        stdout="",
                        stderr=f"Blocked: {reason}",
                        command=f"git {args_str}",
                        blocked=True,
                        block_reason=reason,
                    )

        command = shlex.join(["git", *args])
        return self.run(command, cwd=cwd)

    def safe_pip(
        self,
        args: list[str],
        *,
        cwd: Path | None = None,
    ) -> CommandResult:
        """Safe pip wrapper.

        Blocks installation from untrusted sources.

        Args:
            args: Pip command arguments
            cwd: Working directory

        Returns:
            CommandResult with pip output
        """
        if not args:
            return CommandResult(
                returncode=1,
                stdout="",
                stderr="No pip command specified",
                command="pip",
                blocked=True,
                block_reason="no_command",
            )

        # Block dangerous patterns
        args_str = " ".join(args)

        # Block installation from URLs (except pypi)
        if "install" in args:
            for arg in args:
                if arg.startswith("http://") or arg.startswith("https://"):
                    if "pypi.org" not in arg and "pythonhosted.org" not in arg:
                        return CommandResult(
                            returncode=1,
                            stdout="",
                            stderr=f"Blocked: installation from non-PyPI URL: {arg}",
                            command=f"pip {args_str}",
                            blocked=True,
                            block_reason="untrusted_source",
                        )

            # Block --index-url to untrusted sources
            if "--index-url" in args or "-i" in args:
                for i, arg in enumerate(args):
                    if arg in ("--index-url", "-i") and i + 1 < len(args):
                        url = args[i + 1]
                        if "pypi.org" not in url:
                            return CommandResult(
                                returncode=1,
                                stdout="",
                                stderr=f"Blocked: custom index URL: {url}",
                                command=f"pip {args_str}",
                                blocked=True,
                                block_reason="untrusted_index",
                            )

        command = shlex.join(["pip", *args])
        return self.run(command, cwd=cwd)


def create_sandbox(
    workspace: Path | None = None,
    **kwargs: Any,
) -> SafeShell:
    """Create a sandbox with sensible defaults.

    Args:
        workspace: Working directory to restrict to
        **kwargs: Additional SandboxConfig options

    Returns:
        Configured SafeShell instance
    """
    config = SandboxConfig(workspace=workspace, **kwargs)
    return SafeShell(config)


# ============================================================================
# Tool Executor Integration
# ============================================================================


@dataclass
class SandboxedExecutorConfig:
    """Configuration for sandboxed tool execution."""

    workspace: Path | None = None
    sandbox_config: SandboxConfig | None = None
    use_policy_engine: bool = True
    allow_unknown_commands: bool = False


class SandboxedToolExecutor:
    """Wraps tool execution with sandbox and policy checks.

    Integrates with the agent loop by implementing the ToolExecutor protocol.
    All bash/shell commands are evaluated by CommandPolicy before execution.

    Example:
        from moss.sandbox import SandboxedToolExecutor, SandboxedExecutorConfig
        from moss.agent_loop import AgentLoopRunner

        config = SandboxedExecutorConfig(workspace=Path.cwd())
        executor = SandboxedToolExecutor(config)

        # Use with DWIMLoop or custom AgentLoop
        runner = AgentLoopRunner(executor)
        result = await runner.run(loop, input_data)
    """

    def __init__(
        self,
        config: SandboxedExecutorConfig | None = None,
        inner_executor: Any | None = None,
    ):
        self.config = config or SandboxedExecutorConfig()
        self._inner = inner_executor
        self._sandbox: SafeShell | None = None
        self._policy_engine: Any | None = None

    def _get_sandbox(self) -> SafeShell:
        """Get or create the sandbox instance."""
        if self._sandbox is None:
            sandbox_config = self.config.sandbox_config or SandboxConfig(
                workspace=self.config.workspace
            )
            self._sandbox = SafeShell(sandbox_config)
        return self._sandbox

    def _get_policy_engine(self) -> Any:
        """Get or create the policy engine."""
        if self._policy_engine is None and self.config.use_policy_engine:
            from moss.policy import create_default_policy_engine

            self._policy_engine = create_default_policy_engine(
                root=self.config.workspace,
                include_command=True,
            )
        return self._policy_engine

    async def _check_command(self, command: str) -> tuple[bool, str | None]:
        """Check if a command is allowed by the policy engine.

        Returns:
            Tuple of (allowed, reason if denied)
        """
        engine = self._get_policy_engine()
        if engine is None:
            return True, None

        from moss.policy import ToolCallContext

        context = ToolCallContext(
            tool_name="bash",
            action="execute",
            parameters={"command": command},
        )

        result = await engine.evaluate(context)
        if result.allowed:
            return True, None
        return False, result.blocking_result.reason if result.blocking_result else "Policy denied"

    async def execute_command(
        self,
        command: str,
        *,
        cwd: Path | None = None,
        timeout: int | None = None,
    ) -> CommandResult:
        """Execute a command through the sandbox with policy checks.

        Args:
            command: Shell command to execute
            cwd: Working directory
            timeout: Command timeout in seconds

        Returns:
            CommandResult with output and status
        """
        # Check policy first
        allowed, reason = await self._check_command(command)
        if not allowed:
            return CommandResult(
                returncode=1,
                stdout="",
                stderr=f"Command blocked by policy: {reason}",
                command=command,
                blocked=True,
                block_reason=reason,
            )

        # Execute through sandbox
        sandbox = self._get_sandbox()
        return sandbox.run(command, cwd=cwd, timeout=timeout)

    async def execute(self, tool_name: str, context: Any, step: Any) -> tuple[Any, int, int]:
        """Execute a tool with sandbox checks.

        Implements the ToolExecutor protocol for agent loop integration.
        Routes bash/shell tools through the sandbox, delegates others.
        """
        tool_lower = tool_name.lower()

        # Check if this is a bash/shell tool
        if any(t in tool_lower for t in ("bash", "shell", "exec", "command", "run")):
            # Extract command from context
            cmd = None
            if hasattr(step, "parameters") and step.parameters:
                for key in ("command", "cmd", "script"):
                    if key in step.parameters:
                        cmd = step.parameters[key]
                        break

            if cmd is None and hasattr(context, "input"):
                if isinstance(context.input, str):
                    cmd = context.input
                elif isinstance(context.input, dict):
                    cmd = context.input.get("command") or context.input.get("cmd")

            if cmd is None and hasattr(context, "last"):
                if isinstance(context.last, str):
                    cmd = context.last

            if cmd:
                result = await self.execute_command(cmd)
                if result.blocked:
                    raise RuntimeError(f"Command blocked: {result.block_reason}")
                output = {
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "returncode": result.returncode,
                }
                return output, 0, 0

        # Delegate to inner executor for non-bash tools
        if self._inner:
            return await self._inner.execute(tool_name, context, step)

        raise ValueError(f"No handler for tool: {tool_name}")


def create_sandboxed_executor(
    workspace: Path | None = None,
    inner_executor: Any | None = None,
    **kwargs: Any,
) -> SandboxedToolExecutor:
    """Create a sandboxed executor with sensible defaults.

    Args:
        workspace: Working directory to restrict to
        inner_executor: Executor to delegate non-bash tools to
        **kwargs: Additional SandboxedExecutorConfig options

    Returns:
        Configured SandboxedToolExecutor
    """
    config = SandboxedExecutorConfig(workspace=workspace, **kwargs)
    return SandboxedToolExecutor(config, inner_executor)
