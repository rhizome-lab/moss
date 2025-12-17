"""Profiling utilities for Moss.

This module provides tools for performance profiling and optimization
of Moss applications.
"""

import cProfile
import io
import pstats
import time
from collections.abc import Callable
from contextlib import contextmanager
from dataclasses import dataclass, field
from functools import wraps
from typing import Any


@dataclass
class TimingResult:
    """Result of a timing operation."""

    name: str
    duration_ms: float
    calls: int = 1
    min_ms: float = 0.0
    max_ms: float = 0.0
    avg_ms: float = 0.0


@dataclass
class ProfileResult:
    """Result of a profiling operation."""

    name: str
    total_time: float
    stats: str
    top_functions: list[tuple[str, float]] = field(default_factory=list)


class Timer:
    """Simple timer for measuring execution time."""

    def __init__(self, name: str = ""):
        """Initialize the timer.

        Args:
            name: Optional name for the timer
        """
        self.name = name
        self.start_time: float | None = None
        self.end_time: float | None = None
        self._durations: list[float] = []

    def start(self) -> "Timer":
        """Start the timer."""
        self.start_time = time.perf_counter()
        return self

    def stop(self) -> float:
        """Stop the timer and return duration in milliseconds."""
        if self.start_time is None:
            raise RuntimeError("Timer was not started")

        self.end_time = time.perf_counter()
        duration = (self.end_time - self.start_time) * 1000
        self._durations.append(duration)
        return duration

    def reset(self) -> None:
        """Reset the timer."""
        self.start_time = None
        self.end_time = None
        self._durations.clear()

    @property
    def duration_ms(self) -> float | None:
        """Get the last duration in milliseconds."""
        if not self._durations:
            return None
        return self._durations[-1]

    @property
    def total_ms(self) -> float:
        """Get total duration across all measurements."""
        return sum(self._durations)

    @property
    def average_ms(self) -> float:
        """Get average duration in milliseconds."""
        if not self._durations:
            return 0.0
        return sum(self._durations) / len(self._durations)

    @property
    def min_ms(self) -> float:
        """Get minimum duration in milliseconds."""
        if not self._durations:
            return 0.0
        return min(self._durations)

    @property
    def max_ms(self) -> float:
        """Get maximum duration in milliseconds."""
        if not self._durations:
            return 0.0
        return max(self._durations)

    def result(self) -> TimingResult:
        """Get timing result."""
        return TimingResult(
            name=self.name,
            duration_ms=self.total_ms,
            calls=len(self._durations),
            min_ms=self.min_ms,
            max_ms=self.max_ms,
            avg_ms=self.average_ms,
        )


@contextmanager
def measure_time(name: str = ""):
    """Context manager for measuring execution time.

    Args:
        name: Optional name for the measurement

    Yields:
        Timer instance
    """
    timer = Timer(name)
    timer.start()
    try:
        yield timer
    finally:
        timer.stop()


def timed_function(name: str | None = None):
    """Decorator to time a function's execution.

    Args:
        name: Optional name (defaults to function name)
    """

    def decorator(func: Callable) -> Callable:
        func_name = name or func.__name__

        @wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            timer = Timer(func_name)
            timer.start()
            try:
                return func(*args, **kwargs)
            finally:
                duration = timer.stop()
                # Store timing on the function for inspection
                if not hasattr(wrapper, "_timings"):
                    wrapper._timings = []
                wrapper._timings.append(duration)

        wrapper.get_timings = lambda: getattr(wrapper, "_timings", [])
        wrapper.clear_timings = lambda: setattr(wrapper, "_timings", [])
        return wrapper

    return decorator


class Profiler:
    """cProfile-based profiler for detailed analysis."""

    def __init__(self):
        """Initialize the profiler."""
        self._profiler = cProfile.Profile()
        self._enabled = False

    def enable(self) -> None:
        """Enable profiling."""
        self._profiler.enable()
        self._enabled = True

    def disable(self) -> None:
        """Disable profiling."""
        self._profiler.disable()
        self._enabled = False

    def is_enabled(self) -> bool:
        """Check if profiling is enabled."""
        return self._enabled

    def get_stats(self, sort_by: str = "cumulative", limit: int = 20) -> str:
        """Get profiling statistics as a string.

        Args:
            sort_by: Sort key (cumulative, time, calls)
            limit: Number of top functions to include

        Returns:
            Formatted statistics string
        """
        stream = io.StringIO()
        stats = pstats.Stats(self._profiler, stream=stream)
        stats.sort_stats(sort_by)
        stats.print_stats(limit)
        return stream.getvalue()

    def get_top_functions(
        self, sort_by: str = "cumulative", limit: int = 10
    ) -> list[tuple[str, float]]:
        """Get top functions by time.

        Args:
            sort_by: Sort key
            limit: Number of functions

        Returns:
            List of (function_name, cumulative_time) tuples
        """
        stats = pstats.Stats(self._profiler)
        stats.sort_stats(sort_by)

        # Extract stats
        results = []
        for func, (_cc, _nc, _tt, ct, _callers) in stats.stats.items():
            filename, line, name = func
            func_name = f"{filename}:{line}({name})"
            results.append((func_name, ct))

        results.sort(key=lambda x: x[1], reverse=True)
        return results[:limit]

    def reset(self) -> None:
        """Reset the profiler."""
        self._profiler = cProfile.Profile()
        self._enabled = False


@contextmanager
def profile(name: str = ""):
    """Context manager for profiling a code block.

    Args:
        name: Optional name for the profile

    Yields:
        Profiler instance
    """
    profiler = Profiler()
    profiler.enable()
    try:
        yield profiler
    finally:
        profiler.disable()


def profile_function(func: Callable) -> Callable:
    """Decorator to profile a function.

    Args:
        func: Function to profile
    """

    @wraps(func)
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        profiler = Profiler()
        profiler.enable()
        try:
            return func(*args, **kwargs)
        finally:
            profiler.disable()
            # Store profile on function
            wrapper._last_profile = profiler

    wrapper.get_last_profile = lambda: getattr(wrapper, "_last_profile", None)
    return wrapper


class BenchmarkSuite:
    """Suite for running benchmarks."""

    def __init__(self, name: str = ""):
        """Initialize the benchmark suite.

        Args:
            name: Suite name
        """
        self.name = name
        self._benchmarks: dict[str, Callable] = {}
        self._results: dict[str, TimingResult] = {}

    def add(self, name: str, func: Callable) -> None:
        """Add a benchmark.

        Args:
            name: Benchmark name
            func: Function to benchmark
        """
        self._benchmarks[name] = func

    def benchmark(self, name: str):
        """Decorator to add a benchmark.

        Args:
            name: Benchmark name
        """

        def decorator(func: Callable) -> Callable:
            self.add(name, func)
            return func

        return decorator

    def run(self, iterations: int = 100, warmup: int = 10) -> dict[str, TimingResult]:
        """Run all benchmarks.

        Args:
            iterations: Number of iterations per benchmark
            warmup: Number of warmup iterations

        Returns:
            Dict of benchmark name to TimingResult
        """
        results = {}

        for name, func in self._benchmarks.items():
            timer = Timer(name)

            # Warmup
            for _ in range(warmup):
                func()

            # Benchmark
            for _ in range(iterations):
                timer.start()
                func()
                timer.stop()

            results[name] = timer.result()

        self._results = results
        return results

    def get_results(self) -> dict[str, TimingResult]:
        """Get benchmark results."""
        return self._results.copy()

    def format_results(self) -> str:
        """Format results as a table."""
        if not self._results:
            return "No benchmark results available."

        lines = [
            f"Benchmark Results: {self.name}",
            "=" * 60,
            f"{'Name':<30} {'Avg (ms)':<12} {'Min (ms)':<12} {'Max (ms)':<12}",
            "-" * 60,
        ]

        for name, result in sorted(self._results.items()):
            lines.append(
                f"{name:<30} {result.avg_ms:<12.3f} {result.min_ms:<12.3f} {result.max_ms:<12.3f}"
            )

        return "\n".join(lines)
