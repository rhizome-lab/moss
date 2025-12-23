"""Parallel file analysis utilities.

This module provides utilities for processing multiple files in parallel,
improving performance for large codebases.

Usage:
    from moss_orchestration.parallel import parallel_analyze, ParallelAnalyzer

    # Simple parallel processing
    results = await parallel_analyze(
        files,
        analyze_func,
        max_workers=4,
    )

    # With progress tracking
    async with ParallelAnalyzer(max_workers=4) as analyzer:
        async for result in analyzer.analyze_files(files, analyze_func):
            print(result)
"""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator, Callable, Coroutine, Iterable
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from typing import Any, TypeVar

T = TypeVar("T")
R = TypeVar("R")


@dataclass
class AnalysisResult[T]:
    """Result of analyzing a single file."""

    path: Path
    result: T | None = None
    error: str | None = None
    duration_ms: float = 0.0

    @property
    def success(self) -> bool:
        """Check if analysis succeeded."""
        return self.error is None


@dataclass
class BatchStats:
    """Statistics for a batch analysis."""

    total: int = 0
    completed: int = 0
    failed: int = 0
    duration_ms: float = 0.0

    @property
    def success_rate(self) -> float:
        """Calculate success rate."""
        if self.total == 0:
            return 0.0
        return (self.total - self.failed) / self.total

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "total": self.total,
            "completed": self.completed,
            "failed": self.failed,
            "duration_ms": self.duration_ms,
            "success_rate": f"{self.success_rate:.2%}",
        }


class ParallelAnalyzer:
    """Parallel analyzer for processing multiple files concurrently.

    Supports both async and sync analysis functions with optional progress
    tracking and rate limiting.
    """

    def __init__(
        self,
        max_workers: int | None = None,
        use_processes: bool = False,
        batch_size: int = 100,
    ) -> None:
        """Initialize the analyzer.

        Args:
            max_workers: Maximum number of concurrent workers (default: CPU count)
            use_processes: Use process pool instead of thread pool
            batch_size: Number of files to process in each batch
        """
        self.max_workers = max_workers
        self.use_processes = use_processes
        self.batch_size = batch_size
        self._executor: ThreadPoolExecutor | ProcessPoolExecutor | None = None
        self._semaphore: asyncio.Semaphore | None = None

    async def __aenter__(self) -> ParallelAnalyzer:
        """Enter async context."""
        workers = self.max_workers or 4
        self._semaphore = asyncio.Semaphore(workers)

        if self.use_processes:
            self._executor = ProcessPoolExecutor(max_workers=workers)
        else:
            self._executor = ThreadPoolExecutor(max_workers=workers)

        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        """Exit async context."""
        if self._executor is not None:
            self._executor.shutdown(wait=True)
            self._executor = None
        self._semaphore = None

    async def analyze_file(
        self,
        path: Path,
        analyze_func: Callable[[Path], T] | Callable[[Path], Coroutine[Any, Any, T]],
    ) -> AnalysisResult[T]:
        """Analyze a single file.

        Args:
            path: Path to the file
            analyze_func: Function to analyze the file (sync or async)

        Returns:
            AnalysisResult with the result or error
        """
        import time

        start = time.perf_counter()

        try:
            # Check if function is async
            if asyncio.iscoroutinefunction(analyze_func):
                result = await analyze_func(path)
            else:
                # Run sync function in thread pool
                loop = asyncio.get_event_loop()
                if self._executor is not None:
                    result = await loop.run_in_executor(self._executor, analyze_func, path)
                else:
                    result = analyze_func(path)

            duration = (time.perf_counter() - start) * 1000
            return AnalysisResult(path=path, result=result, duration_ms=duration)

        except Exception as e:
            duration = (time.perf_counter() - start) * 1000
            return AnalysisResult(path=path, error=str(e), duration_ms=duration)

    async def analyze_files(
        self,
        paths: Iterable[Path],
        analyze_func: Callable[[Path], T] | Callable[[Path], Coroutine[Any, Any, T]],
        on_progress: Callable[[int, int], None] | None = None,
    ) -> AsyncIterator[AnalysisResult[T]]:
        """Analyze multiple files concurrently.

        Args:
            paths: Paths to analyze
            analyze_func: Function to analyze each file
            on_progress: Optional callback for progress updates (completed, total)

        Yields:
            AnalysisResult for each file
        """
        path_list = list(paths)
        total = len(path_list)
        completed = 0

        if self._semaphore is None:
            self._semaphore = asyncio.Semaphore(self.max_workers or 4)

        async def analyze_with_semaphore(path: Path) -> AnalysisResult[T]:
            async with self._semaphore:
                return await self.analyze_file(path, analyze_func)

        # Process in batches
        for i in range(0, len(path_list), self.batch_size):
            batch = path_list[i : i + self.batch_size]
            tasks = [analyze_with_semaphore(path) for path in batch]

            for result in await asyncio.gather(*tasks, return_exceptions=True):
                completed += 1
                if on_progress:
                    on_progress(completed, total)

                if isinstance(result, Exception):
                    yield AnalysisResult(
                        path=batch[completed - 1 - i],
                        error=str(result),
                    )
                else:
                    yield result

    async def analyze_all(
        self,
        paths: Iterable[Path],
        analyze_func: Callable[[Path], T] | Callable[[Path], Coroutine[Any, Any, T]],
        on_progress: Callable[[int, int], None] | None = None,
    ) -> tuple[list[AnalysisResult[T]], BatchStats]:
        """Analyze all files and return results with stats.

        Args:
            paths: Paths to analyze
            analyze_func: Function to analyze each file
            on_progress: Optional callback for progress updates

        Returns:
            Tuple of (results list, batch statistics)
        """
        import time

        start = time.perf_counter()
        results: list[AnalysisResult[T]] = []
        stats = BatchStats()

        path_list = list(paths)
        stats.total = len(path_list)

        async for result in self.analyze_files(path_list, analyze_func, on_progress):
            results.append(result)
            stats.completed += 1
            if result.error:
                stats.failed += 1

        stats.duration_ms = (time.perf_counter() - start) * 1000
        return results, stats


async def parallel_analyze(
    paths: Iterable[Path],
    analyze_func: Callable[[Path], T] | Callable[[Path], Coroutine[Any, Any, T]],
    max_workers: int | None = None,
    on_progress: Callable[[int, int], None] | None = None,
) -> list[AnalysisResult[T]]:
    """Analyze files in parallel (convenience function).

    Args:
        paths: Paths to analyze
        analyze_func: Function to analyze each file
        max_workers: Maximum number of concurrent workers
        on_progress: Optional callback for progress updates

    Returns:
        List of AnalysisResults
    """
    async with ParallelAnalyzer(max_workers=max_workers) as analyzer:
        results, _ = await analyzer.analyze_all(paths, analyze_func, on_progress)
        return results


async def parallel_map(
    items: Iterable[T],
    func: Callable[[T], R] | Callable[[T], Coroutine[Any, Any, R]],
    max_workers: int | None = None,
) -> list[R]:
    """Map a function over items in parallel.

    Args:
        items: Items to process
        func: Function to apply to each item
        max_workers: Maximum number of concurrent workers

    Returns:
        List of results (in order)
    """
    semaphore = asyncio.Semaphore(max_workers or 4)

    async def process(item: T) -> R:
        async with semaphore:
            if asyncio.iscoroutinefunction(func):
                return await func(item)
            else:
                loop = asyncio.get_event_loop()
                return await loop.run_in_executor(None, func, item)

    return await asyncio.gather(*[process(item) for item in items])


def sync_parallel_analyze[T](
    paths: Iterable[Path],
    analyze_func: Callable[[Path], T],
    max_workers: int | None = None,
) -> list[AnalysisResult[T]]:
    """Synchronous parallel analysis using thread pool.

    Args:
        paths: Paths to analyze
        analyze_func: Function to analyze each file
        max_workers: Maximum number of concurrent workers

    Returns:
        List of AnalysisResults
    """
    from concurrent.futures import as_completed

    path_list = list(paths)
    results: list[AnalysisResult[T]] = []

    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        # Submit all tasks
        future_to_path = {}
        for path in path_list:
            future = executor.submit(_analyze_one, path, analyze_func)
            future_to_path[future] = path

        # Collect results
        for future in as_completed(future_to_path):
            path = future_to_path[future]
            try:
                result, duration = future.result()
                results.append(AnalysisResult(path=path, result=result, duration_ms=duration))
            except Exception as e:
                results.append(AnalysisResult(path=path, error=str(e)))

    return results


def _analyze_one[T](path: Path, func: Callable[[Path], T]) -> tuple[T, float]:
    """Analyze one file and return result with timing."""
    import time

    start = time.perf_counter()
    result = func(path)
    duration = (time.perf_counter() - start) * 1000
    return result, duration
