"""Watch mode for tests - auto-run tests on file changes.

This module provides a file watcher that automatically re-runs tests
when Python files are modified.

Usage:
    from moss_orchestration.watch_tests import WatchRunner

    watcher = WatchRunner(Path("."))
    await watcher.run()  # Runs until Ctrl+C

Or via CLI:
    moss watch [--pattern "tests/*.py"] [--debounce 500]
"""

from __future__ import annotations

import asyncio
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

from moss_cli.output import Output, get_output
from .watcher import EventType, FileWatcher, WatchConfig, WatchEvent


@dataclass
class RunResult:
    """Result of a test run."""

    success: bool
    exit_code: int
    stdout: str
    stderr: str
    duration_ms: float
    files_changed: list[Path] = field(default_factory=list)


@dataclass
class WatchTestConfig:
    """Configuration for test watcher."""

    # Test runner settings
    test_command: list[str] = field(default_factory=lambda: [sys.executable, "-m", "pytest", "-v"])
    test_patterns: list[str] = field(default_factory=lambda: ["test_*.py", "*_test.py"])
    source_patterns: list[str] = field(default_factory=lambda: ["*.py"])

    # Watch settings
    debounce_ms: int = 500  # Wait for changes to settle
    clear_screen: bool = True  # Clear screen before each run
    run_on_start: bool = True  # Run tests immediately on start
    stop_on_fail: bool = False  # Stop watching on first failure
    incremental: bool = False  # Only run tests related to changed files

    # Output settings
    show_summary: bool = True
    show_duration: bool = True


class WatchRunner:
    """Watch for file changes and re-run tests automatically."""

    def __init__(
        self,
        path: Path,
        config: WatchTestConfig | None = None,
        output: Output | None = None,
    ) -> None:
        """Initialize test watcher.

        Args:
            path: Directory to watch
            config: Watch configuration
            output: Output handler for messages
        """
        self.path = Path(path).resolve()
        self.config = config or WatchTestConfig()
        self.output = output or get_output()
        self._running = False
        self._pending_files: set[Path] = set()
        self._debounce_task: asyncio.Task[None] | None = None
        self._run_count = 0
        self._pass_count = 0
        self._fail_count = 0
        self._last_result: RunResult | None = None

    async def run(self) -> None:
        """Run the test watcher until interrupted."""
        self._running = True

        # Setup file watcher
        watch_config = WatchConfig(
            patterns=self.config.source_patterns,
            debounce_ms=50,  # Use our own debouncing
            recursive=True,
        )
        watcher = FileWatcher(self.path, watch_config)
        watcher.on_change(self._on_file_change)

        self.output.header("Moss Test Watcher")
        self.output.info(f"Watching: {self.path}")
        self.output.info(f"Test command: {' '.join(self.config.test_command)}")
        self.output.info("Press Ctrl+C to stop")
        self.output.blank()

        try:
            await watcher.start()

            # Run tests immediately if configured
            if self.config.run_on_start:
                await self._run_tests([])

            # Wait until interrupted
            while self._running:
                await asyncio.sleep(0.1)

        except asyncio.CancelledError:
            pass
        finally:
            await watcher.stop()
            self._show_final_summary()

    def stop(self) -> None:
        """Stop the test watcher."""
        self._running = False

    async def _on_file_change(self, event: WatchEvent) -> None:
        """Handle file change events."""
        if event.event_type == EventType.DELETED:
            return

        # Check if it's a Python file
        if not event.path.suffix == ".py":
            return

        self._pending_files.add(event.path)

        # Cancel existing debounce task
        if self._debounce_task:
            self._debounce_task.cancel()

        # Schedule test run after debounce period
        self._debounce_task = asyncio.create_task(self._debounced_run())

    async def _debounced_run(self) -> None:
        """Run tests after debounce period."""
        try:
            await asyncio.sleep(self.config.debounce_ms / 1000)
        except asyncio.CancelledError:
            return

        files = list(self._pending_files)
        self._pending_files.clear()

        await self._run_tests(files)

        # Stop if configured and tests failed
        if self.config.stop_on_fail and self._last_result and not self._last_result.success:
            self._running = False

    async def _run_tests(self, changed_files: list[Path]) -> None:
        """Run the test suite."""
        import time

        self._run_count += 1

        if self.config.clear_screen:
            print("\033[2J\033[H", end="")  # Clear screen

        self.output.step(f"Run #{self._run_count}")

        if changed_files:
            for f in changed_files[:5]:  # Show first 5
                try:
                    rel = f.relative_to(self.path)
                    self.output.info(f"  Changed: {rel}")
                except ValueError:
                    self.output.info(f"  Changed: {f}")
            if len(changed_files) > 5:
                self.output.info(f"  ... and {len(changed_files) - 5} more")
        else:
            self.output.info("  Initial run")

        # Build test command
        test_command = list(self.config.test_command)

        # Incremental mode: find and run only related tests
        if self.config.incremental and changed_files:
            related_tests = find_related_tests(changed_files, self.path)
            if related_tests:
                self.output.info(f"  Running {len(related_tests)} related test file(s)")
                # Append test files to command
                test_command.extend(str(t) for t in related_tests)
            else:
                self.output.warning("  No related tests found, running all tests")
        elif self.config.incremental:
            self.output.info("  Incremental mode (initial run = all tests)")

        self.output.blank()

        # Run tests
        start = time.perf_counter()
        try:
            result = subprocess.run(
                test_command,
                capture_output=True,
                text=True,
                cwd=self.path,
            )
            duration = (time.perf_counter() - start) * 1000

            self._last_result = RunResult(
                success=result.returncode == 0,
                exit_code=result.returncode,
                stdout=result.stdout,
                stderr=result.stderr,
                duration_ms=duration,
                files_changed=changed_files,
            )

            # Show output
            if result.stdout:
                print(result.stdout)
            if result.stderr:
                print(result.stderr, file=sys.stderr)

            # Show result banner
            self.output.blank()
            if self._last_result.success:
                self._pass_count += 1
                if self.config.show_duration:
                    msg = f"Tests passed ({duration:.0f}ms)"
                else:
                    msg = "Tests passed"
                self.output.success(msg)
            else:
                self._fail_count += 1
                self.output.error(
                    f"Tests failed (exit code {result.returncode}, {duration:.0f}ms)"
                    if self.config.show_duration
                    else f"Tests failed (exit code {result.returncode})"
                )

        except FileNotFoundError as e:
            self.output.error(f"Test command failed: {e}")
            self._fail_count += 1

        self.output.blank()
        self.output.info("Watching for changes...")

    def _show_final_summary(self) -> None:
        """Show final summary when stopping."""
        if not self.config.show_summary:
            return

        self.output.blank()
        self.output.header("Test Watch Summary")
        self.output.info(f"Total runs: {self._run_count}")
        self.output.info(f"Passed: {self._pass_count}")
        self.output.info(f"Failed: {self._fail_count}")


async def run_watch_mode(
    path: Path,
    test_command: list[str] | None = None,
    debounce_ms: int = 500,
    clear_screen: bool = True,
    run_on_start: bool = True,
    output: Output | None = None,
) -> None:
    """Run test watcher (convenience function).

    Args:
        path: Directory to watch
        test_command: Custom test command (default: pytest -v)
        debounce_ms: Debounce delay in milliseconds
        clear_screen: Clear screen before each run
        run_on_start: Run tests immediately
        output: Output handler
    """
    config = WatchTestConfig(
        debounce_ms=debounce_ms,
        clear_screen=clear_screen,
        run_on_start=run_on_start,
    )
    if test_command:
        config.test_command = test_command

    watcher = WatchRunner(path, config, output)

    try:
        await watcher.run()
    except KeyboardInterrupt:
        watcher.stop()


def _parse_test_command(cmd_str: str | None) -> list[str] | None:
    """Parse test command string into list."""
    if not cmd_str:
        return None
    import shlex

    return shlex.split(cmd_str)


def find_related_tests(changed_files: list[Path], project_root: Path) -> list[Path]:
    """Find test files related to changed source files.

    Maps source files to test files using common conventions:
    - src/module.py -> tests/test_module.py
    - src/pkg/module.py -> tests/pkg/test_module.py, tests/test_pkg_module.py
    - test_*.py files are returned as-is
    """
    test_files: set[Path] = set()
    tests_dir = project_root / "tests"

    for changed in changed_files:
        try:
            rel = changed.relative_to(project_root)
        except ValueError:
            continue

        # If it's already a test file, include it directly
        if rel.name.startswith("test_") or rel.name.endswith("_test.py"):
            if changed.exists():
                test_files.add(changed)
            continue

        # Skip non-Python files
        if rel.suffix != ".py":
            continue

        # Try to find matching test file
        stem = rel.stem
        parts = list(rel.parts)

        # Remove common source prefixes
        if parts and parts[0] in ("src", "lib"):
            parts = parts[1:]

        if not parts:
            continue

        # Generate candidate test file paths
        candidates = []

        # Direct mapping: src/pkg/module.py -> tests/pkg/test_module.py
        test_name = f"test_{parts[-1]}"
        if len(parts) > 1:
            candidates.append(tests_dir / Path(*parts[:-1]) / test_name)
        candidates.append(tests_dir / test_name)

        # Flat mapping: src/pkg/module.py -> tests/test_pkg_module.py
        if len(parts) > 1:
            flat_name = f"test_{'_'.join(parts[:-1])}_{stem}.py"
            candidates.append(tests_dir / flat_name)

        # Also check same directory for test file
        if changed.parent != project_root:
            candidates.append(changed.parent / f"test_{stem}.py")

        for candidate in candidates:
            if candidate.exists():
                test_files.add(candidate)

    return sorted(test_files)


def get_git_changed_files(project_root: Path, base: str = "HEAD") -> list[Path]:
    """Get list of files changed since base commit."""
    try:
        result = subprocess.run(
            ["git", "diff", "--name-only", base],
            capture_output=True,
            text=True,
            cwd=project_root,
        )
        if result.returncode != 0:
            return []

        files = []
        for line in result.stdout.strip().split("\n"):
            if line:
                path = project_root / line
                if path.exists():
                    files.append(path)
        return files
    except FileNotFoundError:
        return []
