"""Progress indicators for CLI operations.

This module provides progress tracking and display for long-running operations
like scanning large codebases, indexing files, or batch analysis.

Usage:
    from moss.progress import ProgressTracker, with_progress

    # Simple usage
    with ProgressTracker("Scanning files", total=100) as progress:
        for i in range(100):
            do_work()
            progress.advance()

    # Or with a context manager decorator
    @with_progress("Processing")
    def process_files(files: list[Path], progress: ProgressTracker) -> None:
        for f in files:
            process(f)
            progress.advance()
"""

from __future__ import annotations

import sys
import threading
import time
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from dataclasses import dataclass, field
from typing import ClassVar, TextIO

# =============================================================================
# Configuration
# =============================================================================


@dataclass
class ProgressConfig:
    """Configuration for progress display."""

    # Display settings
    show_percentage: bool = True
    show_count: bool = True
    show_rate: bool = True
    show_eta: bool = True
    show_spinner: bool = True

    # Formatting
    bar_width: int = 30
    bar_filled: str = "="
    bar_empty: str = " "
    bar_current: str = ">"

    # Timing
    refresh_rate: float = 0.1  # seconds
    min_update_interval: float = 0.05  # Don't update faster than this

    # Output
    output: TextIO = field(default_factory=lambda: sys.stderr)
    use_colors: bool = True


# =============================================================================
# Progress Tracker
# =============================================================================


class ProgressTracker:
    """Track and display progress for long-running operations."""

    SPINNER_CHARS: ClassVar[str] = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
    COLORS: ClassVar[dict[str, str]] = {
        "green": "\033[32m",
        "yellow": "\033[33m",
        "blue": "\033[34m",
        "reset": "\033[0m",
        "bold": "\033[1m",
    }

    def __init__(
        self,
        description: str = "",
        total: int | None = None,
        config: ProgressConfig | None = None,
    ) -> None:
        self.description = description
        self.total = total
        self.config = config or ProgressConfig()

        self._current = 0
        self._start_time: float | None = None
        self._last_update: float = 0
        self._spinner_idx = 0
        self._finished = False
        self._lock = threading.Lock()

    @property
    def current(self) -> int:
        """Current progress count."""
        return self._current

    @property
    def percentage(self) -> float | None:
        """Current progress as percentage (0-100)."""
        if self.total is None or self.total == 0:
            return None
        return min(100.0, (self._current / self.total) * 100)

    @property
    def elapsed(self) -> float:
        """Elapsed time in seconds."""
        if self._start_time is None:
            return 0.0
        return time.time() - self._start_time

    @property
    def rate(self) -> float | None:
        """Items per second."""
        elapsed = self.elapsed
        if elapsed <= 0 or self._current == 0:
            return None
        return self._current / elapsed

    @property
    def eta(self) -> float | None:
        """Estimated time remaining in seconds."""
        rate = self.rate
        if rate is None or rate <= 0 or self.total is None:
            return None
        remaining = self.total - self._current
        return remaining / rate

    def start(self) -> ProgressTracker:
        """Start tracking progress."""
        self._start_time = time.time()
        self._render()
        return self

    def advance(self, amount: int = 1) -> None:
        """Advance progress by amount."""
        with self._lock:
            self._current += amount
        self._maybe_render()

    def update(self, current: int) -> None:
        """Set current progress value."""
        with self._lock:
            self._current = current
        self._maybe_render()

    def set_total(self, total: int) -> None:
        """Update the total count."""
        self.total = total
        self._maybe_render()

    def set_description(self, description: str) -> None:
        """Update the description."""
        self.description = description
        self._maybe_render()

    def finish(self, message: str | None = None) -> None:
        """Mark progress as complete."""
        self._finished = True
        self._render(final=True, message=message)

    def _maybe_render(self) -> None:
        """Render if enough time has passed."""
        now = time.time()
        if now - self._last_update >= self.config.min_update_interval:
            self._render()

    def _render(self, final: bool = False, message: str | None = None) -> None:
        """Render the progress bar."""
        self._last_update = time.time()

        output = self.config.output
        use_color = self.config.use_colors and output.isatty()

        # Build progress line
        parts = []

        # Spinner (only for non-final)
        if self.config.show_spinner and not final:
            self._spinner_idx = (self._spinner_idx + 1) % len(self.SPINNER_CHARS)
            parts.append(self.SPINNER_CHARS[self._spinner_idx])

        # Description
        if self.description:
            parts.append(self.description)

        # Progress bar
        if self.total is not None:
            pct = self.percentage or 0
            filled = int(self.config.bar_width * pct / 100)
            bar = (
                self.config.bar_filled * filled
                + self.config.bar_current
                + self.config.bar_empty * (self.config.bar_width - filled - 1)
            )
            if use_color:
                bar_color = self.COLORS["green"] if pct >= 100 else self.COLORS["blue"]
                parts.append(f"{bar_color}[{bar}]{self.COLORS['reset']}")
            else:
                parts.append(f"[{bar}]")

        # Percentage
        if self.config.show_percentage and self.percentage is not None:
            pct_str = f"{self.percentage:5.1f}%"
            parts.append(pct_str)

        # Count
        if self.config.show_count:
            if self.total is not None:
                parts.append(f"{self._current}/{self.total}")
            else:
                parts.append(f"{self._current}")

        # Rate
        if self.config.show_rate and self.rate is not None:
            parts.append(f"{self.rate:.1f}/s")

        # ETA
        if self.config.show_eta and self.eta is not None:
            eta_str = self._format_time(self.eta)
            parts.append(f"ETA: {eta_str}")

        # Final message or elapsed time
        if final:
            if message:
                parts.append(message)
            else:
                elapsed_str = self._format_time(self.elapsed)
                if use_color:
                    green = self.COLORS["green"]
                    reset = self.COLORS["reset"]
                    parts.append(f"{green}done{reset} in {elapsed_str}")
                else:
                    parts.append(f"done in {elapsed_str}")

        line = " ".join(parts)

        # Write with carriage return for in-place updates
        if output.isatty():
            output.write(f"\r{line}\033[K")  # Clear to end of line
            if final:
                output.write("\n")
        else:
            # For non-TTY, just print on completion
            if final:
                output.write(f"{line}\n")

        output.flush()

    def _format_time(self, seconds: float) -> str:
        """Format seconds as human-readable time."""
        if seconds < 60:
            return f"{seconds:.1f}s"
        elif seconds < 3600:
            mins = int(seconds // 60)
            secs = int(seconds % 60)
            return f"{mins}m{secs}s"
        else:
            hours = int(seconds // 3600)
            mins = int((seconds % 3600) // 60)
            return f"{hours}h{mins}m"

    def __enter__(self) -> ProgressTracker:
        """Context manager entry."""
        return self.start()

    def __exit__(self, *args) -> None:
        """Context manager exit."""
        if not self._finished:
            self.finish()


# =============================================================================
# Convenience Helpers
# =============================================================================


@contextmanager
def progress_context(
    description: str = "",
    total: int | None = None,
    config: ProgressConfig | None = None,
) -> Iterator[ProgressTracker]:
    """Context manager for progress tracking.

    Args:
        description: Description of the operation
        total: Total number of items (if known)
        config: Progress display configuration

    Yields:
        ProgressTracker instance
    """
    tracker = ProgressTracker(description, total, config)
    tracker.start()
    try:
        yield tracker
    finally:
        if not tracker._finished:
            tracker.finish()


def with_progress(
    description: str,
    total_arg: str | None = None,
) -> Callable:
    """Decorator to add progress tracking to a function.

    Args:
        description: Description of the operation
        total_arg: Name of the argument containing total count

    Returns:
        Decorated function
    """

    def decorator(func: Callable) -> Callable:
        def wrapper(*args, **kwargs):
            # Determine total from argument if specified
            total = None
            if total_arg and total_arg in kwargs:
                total = len(kwargs[total_arg])

            with progress_context(description, total) as progress:
                kwargs["progress"] = progress
                return func(*args, **kwargs)

        return wrapper

    return decorator


def track_iterable(
    iterable,
    description: str = "",
    total: int | None = None,
    config: ProgressConfig | None = None,
) -> Iterator:
    """Wrap an iterable with progress tracking.

    Args:
        iterable: Iterable to track
        description: Description of the operation
        total: Total count (auto-detected from len if possible)
        config: Progress display configuration

    Yields:
        Items from the iterable
    """
    if total is None:
        try:
            total = len(iterable)  # type: ignore
        except TypeError:
            pass

    with progress_context(description, total, config) as progress:
        for item in iterable:
            yield item
            progress.advance()


# =============================================================================
# Specialized Progress Indicators
# =============================================================================


class FileProgress(ProgressTracker):
    """Progress tracker specialized for file operations."""

    def __init__(self, description: str = "Processing files", total: int | None = None):
        super().__init__(description, total)
        self._current_file: str | None = None

    def set_current_file(self, path: str) -> None:
        """Update the current file being processed."""
        self._current_file = path
        self._maybe_render()

    def _render(self, final: bool = False, message: str | None = None) -> None:
        """Render with current file info."""
        if self._current_file and not final:
            old_desc = self.description
            self.description = f"{old_desc} ({self._current_file})"
            super()._render(final, message)
            self.description = old_desc
        else:
            super()._render(final, message)


class MultiStageProgress:
    """Progress tracker for multi-stage operations."""

    def __init__(self, stages: list[str]):
        self.stages = stages
        self.current_stage = 0
        self._trackers: list[ProgressTracker] = []

    def start_stage(self, total: int | None = None) -> ProgressTracker:
        """Start the next stage.

        Args:
            total: Total items in this stage

        Returns:
            ProgressTracker for this stage
        """
        if self.current_stage >= len(self.stages):
            raise RuntimeError("All stages complete")

        stage_name = self.stages[self.current_stage]
        stage_desc = f"[{self.current_stage + 1}/{len(self.stages)}] {stage_name}"

        tracker = ProgressTracker(stage_desc, total)
        self._trackers.append(tracker)
        self.current_stage += 1

        return tracker.start()

    def finish_all(self) -> None:
        """Finish all stages."""
        for tracker in self._trackers:
            if not tracker._finished:
                tracker.finish()


# =============================================================================
# CLI Integration
# =============================================================================


def create_progress(
    description: str,
    total: int | None = None,
    quiet: bool = False,
) -> ProgressTracker:
    """Create a progress tracker with CLI-appropriate settings.

    Args:
        description: Description of the operation
        total: Total number of items
        quiet: If True, disable all progress output

    Returns:
        Configured ProgressTracker
    """
    if quiet:
        # Create a dummy tracker that does nothing
        config = ProgressConfig(
            show_percentage=False,
            show_count=False,
            show_rate=False,
            show_eta=False,
            show_spinner=False,
        )
        return ProgressTracker(description, total, config)

    return ProgressTracker(description, total)
