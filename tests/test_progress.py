"""Tests for progress indicator module."""

import io
import time

import pytest


class TestProgressConfig:
    """Tests for ProgressConfig."""

    def test_default_config(self):
        from moss.progress import ProgressConfig

        config = ProgressConfig()

        assert config.show_percentage is True
        assert config.show_count is True
        assert config.show_rate is True
        assert config.show_eta is True
        assert config.bar_width == 30

    def test_custom_config(self):
        from moss.progress import ProgressConfig

        output = io.StringIO()
        config = ProgressConfig(
            show_percentage=False,
            bar_width=50,
            output=output,
            use_colors=False,
        )

        assert config.show_percentage is False
        assert config.bar_width == 50
        assert config.output is output


class TestProgressTracker:
    """Tests for ProgressTracker."""

    def test_create_tracker(self):
        from moss.progress import ProgressTracker

        tracker = ProgressTracker("Test", total=100)

        assert tracker.description == "Test"
        assert tracker.total == 100
        assert tracker.current == 0

    def test_advance(self):
        from moss.progress import ProgressConfig, ProgressTracker

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)
        tracker = ProgressTracker("Test", total=10, config=config)
        tracker.start()

        tracker.advance()
        assert tracker.current == 1

        tracker.advance(5)
        assert tracker.current == 6

    def test_update(self):
        from moss.progress import ProgressConfig, ProgressTracker

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)
        tracker = ProgressTracker("Test", total=100, config=config)
        tracker.start()

        tracker.update(50)
        assert tracker.current == 50

    def test_percentage(self):
        from moss.progress import ProgressTracker

        tracker = ProgressTracker("Test", total=100)

        assert tracker.percentage == 0.0

        tracker._current = 50
        assert tracker.percentage == 50.0

        tracker._current = 100
        assert tracker.percentage == 100.0

    def test_percentage_no_total(self):
        from moss.progress import ProgressTracker

        tracker = ProgressTracker("Test")
        assert tracker.percentage is None

    def test_elapsed(self):
        from moss.progress import ProgressConfig, ProgressTracker

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)
        tracker = ProgressTracker("Test", config=config)
        tracker.start()

        time.sleep(0.1)

        assert tracker.elapsed >= 0.1

    def test_rate(self):
        from moss.progress import ProgressConfig, ProgressTracker

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)
        tracker = ProgressTracker("Test", config=config)
        tracker.start()

        time.sleep(0.1)
        tracker._current = 10

        rate = tracker.rate
        assert rate is not None
        assert rate > 0

    def test_eta(self):
        from moss.progress import ProgressConfig, ProgressTracker

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)
        tracker = ProgressTracker("Test", total=100, config=config)
        tracker.start()

        time.sleep(0.1)
        tracker._current = 50

        eta = tracker.eta
        assert eta is not None
        assert eta > 0

    def test_context_manager(self):
        from moss.progress import ProgressConfig, ProgressTracker

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)

        with ProgressTracker("Test", total=10, config=config) as tracker:
            for _ in range(10):
                tracker.advance()

        assert tracker._finished is True

    def test_format_time(self):
        from moss.progress import ProgressTracker

        tracker = ProgressTracker("Test")

        assert "s" in tracker._format_time(30)
        assert "m" in tracker._format_time(90)
        assert "h" in tracker._format_time(3700)


class TestProgressContext:
    """Tests for progress_context."""

    def test_context_basic(self):
        from moss.progress import ProgressConfig, progress_context

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)

        with progress_context("Test", total=5, config=config) as progress:
            for _ in range(5):
                progress.advance()

        assert progress.current == 5


class TestTrackIterable:
    """Tests for track_iterable."""

    def test_track_list(self):
        from moss.progress import ProgressConfig, track_iterable

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)

        items = list(range(10))
        result = list(track_iterable(items, "Test", config=config))

        assert result == items

    def test_track_with_total(self):
        from moss.progress import ProgressConfig, track_iterable

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)

        items = range(5)
        count = 0
        for _ in track_iterable(items, "Test", total=5, config=config):
            count += 1

        assert count == 5


class TestFileProgress:
    """Tests for FileProgress."""

    def test_create_file_progress(self):
        from moss.progress import FileProgress

        progress = FileProgress("Processing", total=10)

        assert progress.description == "Processing"
        assert progress.total == 10

    def test_set_current_file(self):
        from moss.progress import FileProgress, ProgressConfig

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)
        progress = FileProgress("Processing", total=10)
        progress.config = config
        progress.start()

        progress.set_current_file("test.py")
        assert progress._current_file == "test.py"


class TestMultiStageProgress:
    """Tests for MultiStageProgress."""

    def test_create_multi_stage(self):
        from moss.progress import MultiStageProgress

        stages = ["Scan", "Analyze", "Report"]
        progress = MultiStageProgress(stages)

        assert progress.stages == stages
        assert progress.current_stage == 0

    def test_start_stages(self):
        from moss.progress import MultiStageProgress, ProgressConfig

        output = io.StringIO()
        config = ProgressConfig(output=output, use_colors=False)

        stages = ["Stage 1", "Stage 2"]
        progress = MultiStageProgress(stages)

        tracker1 = progress.start_stage(total=5)
        tracker1.config = config
        assert progress.current_stage == 1
        tracker1.finish()

        tracker2 = progress.start_stage(total=3)
        tracker2.config = config
        assert progress.current_stage == 2
        tracker2.finish()

    def test_all_stages_complete_raises(self):
        from moss.progress import MultiStageProgress

        progress = MultiStageProgress(["Only"])
        tracker = progress.start_stage()
        tracker.finish()

        with pytest.raises(RuntimeError, match="All stages complete"):
            progress.start_stage()


class TestCreateProgress:
    """Tests for create_progress."""

    def test_create_normal(self):
        from moss.progress import create_progress

        progress = create_progress("Test", total=10)

        assert progress.description == "Test"
        assert progress.total == 10
        assert progress.config.show_percentage is True

    def test_create_quiet(self):
        from moss.progress import create_progress

        progress = create_progress("Test", total=10, quiet=True)

        assert progress.config.show_percentage is False
        assert progress.config.show_count is False
        assert progress.config.show_spinner is False
