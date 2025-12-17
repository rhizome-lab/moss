"""Tests for profiling module."""

import time

from moss.profiling import (
    BenchmarkSuite,
    Profiler,
    Timer,
    measure_time,
    profile,
    profile_function,
    timed_function,
)


class TestTimer:
    """Tests for Timer."""

    def test_basic_timing(self):
        timer = Timer("test")
        timer.start()
        time.sleep(0.01)
        duration = timer.stop()

        assert duration >= 10  # At least 10ms
        assert timer.duration_ms >= 10

    def test_multiple_measurements(self):
        timer = Timer("test")

        for _ in range(3):
            timer.start()
            time.sleep(0.01)
            timer.stop()

        assert len(timer._durations) == 3
        assert timer.total_ms >= 30
        assert timer.average_ms >= 10

    def test_min_max(self):
        timer = Timer("test")

        timer.start()
        time.sleep(0.01)
        timer.stop()

        timer.start()
        time.sleep(0.02)
        timer.stop()

        assert timer.min_ms < timer.max_ms

    def test_reset(self):
        timer = Timer("test")
        timer.start()
        timer.stop()

        timer.reset()

        assert timer.start_time is None
        assert timer.end_time is None
        assert len(timer._durations) == 0

    def test_result(self):
        timer = Timer("my_timer")
        timer.start()
        time.sleep(0.01)
        timer.stop()

        result = timer.result()
        assert result.name == "my_timer"
        assert result.duration_ms >= 10
        assert result.calls == 1


class TestMeasureTime:
    """Tests for measure_time context manager."""

    def test_basic_measurement(self):
        with measure_time("test") as timer:
            time.sleep(0.01)

        assert timer.duration_ms >= 10

    def test_measurement_with_exception(self):
        try:
            with measure_time("test") as timer:
                time.sleep(0.01)
                raise ValueError("test")
        except ValueError:
            pass

        # Timer should still have recorded the duration
        assert timer.duration_ms >= 10


class TestTimedFunction:
    """Tests for timed_function decorator."""

    def test_basic_timing(self):
        @timed_function("my_func")
        def slow_function():
            time.sleep(0.01)
            return 42

        result = slow_function()
        assert result == 42

        timings = slow_function.get_timings()
        assert len(timings) == 1
        assert timings[0] >= 10

    def test_multiple_calls(self):
        @timed_function()
        def fast_function():
            return "done"

        for _ in range(5):
            fast_function()

        timings = fast_function.get_timings()
        assert len(timings) == 5

    def test_clear_timings(self):
        @timed_function()
        def test_func():
            pass

        test_func()
        test_func()
        test_func.clear_timings()

        assert len(test_func.get_timings()) == 0


class TestProfiler:
    """Tests for Profiler."""

    def test_enable_disable(self):
        profiler = Profiler()
        assert not profiler.is_enabled()

        profiler.enable()
        assert profiler.is_enabled()

        profiler.disable()
        assert not profiler.is_enabled()

    def test_get_stats(self):
        profiler = Profiler()
        profiler.enable()

        # Run some code
        for i in range(100):
            _ = i * 2

        profiler.disable()

        stats = profiler.get_stats()
        assert isinstance(stats, str)

    def test_get_top_functions(self):
        profiler = Profiler()
        profiler.enable()

        # Run some code
        for i in range(100):
            _ = i * 2

        profiler.disable()

        top = profiler.get_top_functions()
        assert isinstance(top, list)

    def test_reset(self):
        profiler = Profiler()
        profiler.enable()
        profiler.disable()
        profiler.reset()

        assert not profiler.is_enabled()


class TestProfile:
    """Tests for profile context manager."""

    def test_basic_profiling(self):
        with profile("test") as profiler:
            for i in range(100):
                _ = i * 2

        stats = profiler.get_stats()
        assert isinstance(stats, str)


class TestProfileFunction:
    """Tests for profile_function decorator."""

    def test_basic_profiling(self):
        @profile_function
        def sample_function():
            for i in range(100):
                _ = i * 2
            return "done"

        result = sample_function()
        assert result == "done"

        profiler = sample_function.get_last_profile()
        assert profiler is not None
        stats = profiler.get_stats()
        assert isinstance(stats, str)


class TestBenchmarkSuite:
    """Tests for BenchmarkSuite."""

    def test_add_benchmark(self):
        suite = BenchmarkSuite("test_suite")

        def func1():
            pass

        def func2():
            pass

        suite.add("func1", func1)
        suite.add("func2", func2)

        assert "func1" in suite._benchmarks
        assert "func2" in suite._benchmarks

    def test_benchmark_decorator(self):
        suite = BenchmarkSuite()

        @suite.benchmark("my_benchmark")
        def test_func():
            pass

        assert "my_benchmark" in suite._benchmarks

    def test_run_benchmarks(self):
        suite = BenchmarkSuite("test")

        suite.add("fast", lambda: None)
        suite.add("slow", lambda: time.sleep(0.001))

        results = suite.run(iterations=10, warmup=2)

        assert "fast" in results
        assert "slow" in results
        assert results["slow"].avg_ms > results["fast"].avg_ms

    def test_format_results(self):
        suite = BenchmarkSuite("test")
        suite.add("test_func", lambda: None)
        suite.run(iterations=5)

        output = suite.format_results()
        assert "test_func" in output
        assert "Benchmark Results" in output

    def test_format_results_empty(self):
        suite = BenchmarkSuite()
        output = suite.format_results()
        assert "No benchmark results" in output
