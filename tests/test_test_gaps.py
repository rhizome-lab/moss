"""Tests for Test Coverage Heuristics."""

from pathlib import Path

import pytest

from moss_intelligence.test_gaps import (
    CoverageGap,
    CoverageReport,
    DetectedTestPattern,
    analyze_test_coverage,
    detect_test_patterns,
    find_untested_files,
)


class TestDetectedTestPattern:
    """Tests for DetectedTestPattern dataclass."""

    def test_matches_test_prefix(self):
        pattern = DetectedTestPattern(pattern="test_*.py", language="python", count=1, examples=[])

        assert pattern.matches("test_foo.py")
        assert not pattern.matches("foo_test.py")
        assert not pattern.matches("foo.py")

    def test_matches_test_suffix(self):
        pattern = DetectedTestPattern(pattern="*_test.py", language="python", count=1, examples=[])

        assert pattern.matches("foo_test.py")
        assert not pattern.matches("test_foo.py")
        assert not pattern.matches("foo.py")

    def test_matches_go_test(self):
        pattern = DetectedTestPattern(pattern="*_test.go", language="go", count=1, examples=[])

        assert pattern.matches("foo_test.go")
        assert not pattern.matches("foo.go")

    def test_matches_ts_test(self):
        pattern = DetectedTestPattern(
            pattern="*.test.ts", language="typescript", count=1, examples=[]
        )

        assert pattern.matches("foo.test.ts")
        assert pattern.matches("foo.spec.ts")
        assert not pattern.matches("foo.ts")

    def test_source_name_prefix(self):
        pattern = DetectedTestPattern(pattern="test_*.py", language="python", count=1, examples=[])

        assert pattern.source_name("test_foo.py") == "foo.py"

    def test_source_name_suffix(self):
        pattern = DetectedTestPattern(pattern="*_test.py", language="python", count=1, examples=[])

        assert pattern.source_name("foo_test.py") == "foo.py"

    def test_source_name_ts(self):
        pattern = DetectedTestPattern(
            pattern="*.test.ts", language="typescript", count=1, examples=[]
        )

        assert pattern.source_name("foo.test.ts") == "foo.ts"
        assert pattern.source_name("foo.spec.ts") == "foo.ts"


class TestDetectTestPatterns:
    """Tests for detect_test_patterns."""

    @pytest.fixture
    def python_project(self, tmp_path: Path) -> Path:
        """Create a Python project with test files."""
        src = tmp_path / "src"
        src.mkdir()
        (src / "foo.py").write_text("def foo(): pass")
        (src / "bar.py").write_text("def bar(): pass")

        tests = tmp_path / "tests"
        tests.mkdir()
        (tests / "test_foo.py").write_text("def test_foo(): pass")
        (tests / "test_bar.py").write_text("def test_bar(): pass")

        return tmp_path

    @pytest.fixture
    def mixed_project(self, tmp_path: Path) -> Path:
        """Create a project with multiple test patterns."""
        src = tmp_path / "src"
        src.mkdir()
        (src / "foo.py").write_text("def foo(): pass")
        (src / "bar_test.py").write_text("def test_bar(): pass")  # suffix pattern

        tests = tmp_path / "tests"
        tests.mkdir()
        (tests / "test_foo.py").write_text("def test_foo(): pass")  # prefix pattern

        return tmp_path

    def test_detects_prefix_pattern(self, python_project: Path):
        patterns = detect_test_patterns(python_project)

        assert len(patterns) >= 1
        pattern = patterns[0]
        assert pattern.pattern == "test_*.py"
        assert pattern.language == "python"
        assert pattern.count == 2

    def test_detects_multiple_patterns(self, mixed_project: Path):
        patterns = detect_test_patterns(mixed_project)

        pattern_names = [p.pattern for p in patterns]
        assert "test_*.py" in pattern_names
        assert "*_test.py" in pattern_names

    def test_empty_project(self, tmp_path: Path):
        patterns = detect_test_patterns(tmp_path)

        assert patterns == []

    def test_skips_venv(self, tmp_path: Path):
        """Test files in venv should be ignored."""
        venv = tmp_path / "venv" / "lib"
        venv.mkdir(parents=True)
        (venv / "test_something.py").write_text("pass")

        patterns = detect_test_patterns(tmp_path)

        assert patterns == []


class TestFindUntestedFiles:
    """Tests for find_untested_files."""

    @pytest.fixture
    def project(self, tmp_path: Path) -> Path:
        """Create a project with some tested and untested files."""
        src = tmp_path / "src"
        src.mkdir()
        (src / "tested.py").write_text("def tested(): pass")
        (src / "untested.py").write_text("def untested(): pass")
        (src / "_private.py").write_text("def private(): pass")
        (src / "__init__.py").write_text("")

        tests = tmp_path / "tests"
        tests.mkdir()
        (tests / "test_tested.py").write_text("def test_tested(): pass")

        return tmp_path

    def test_finds_untested_files(self, project: Path):
        gaps = find_untested_files(project)

        gap_names = [g.source_file.name for g in gaps]
        assert "untested.py" in gap_names

    def test_excludes_tested_files(self, project: Path):
        gaps = find_untested_files(project)

        gap_names = [g.source_file.name for g in gaps]
        assert "tested.py" not in gap_names

    def test_excludes_private_files(self, project: Path):
        gaps = find_untested_files(project)

        gap_names = [g.source_file.name for g in gaps]
        assert "_private.py" not in gap_names

    def test_includes_init_files(self, project: Path):
        """__init__.py is not private."""
        gaps = find_untested_files(project)

        gap_names = [g.source_file.name for g in gaps]
        assert "__init__.py" in gap_names

    def test_generates_expected_test_name(self, project: Path):
        gaps = find_untested_files(project)

        untested_gap = next(g for g in gaps if g.source_file.name == "untested.py")
        assert untested_gap.expected_test == "test_untested.py"


class TestAnalyzeTestCoverage:
    """Tests for analyze_test_coverage."""

    @pytest.fixture
    def project(self, tmp_path: Path) -> Path:
        """Create a project with mixed coverage."""
        src = tmp_path / "src"
        src.mkdir()
        (src / "a.py").write_text("pass")
        (src / "b.py").write_text("pass")
        (src / "c.py").write_text("pass")

        tests = tmp_path / "tests"
        tests.mkdir()
        (tests / "test_a.py").write_text("pass")
        (tests / "test_b.py").write_text("pass")

        return tmp_path

    def test_counts_files(self, project: Path):
        report = analyze_test_coverage(project)

        assert report.total_source_files == 3
        assert report.tested_count == 2
        assert report.untested_count == 1

    def test_coverage_percent(self, project: Path):
        report = analyze_test_coverage(project)

        assert 66 < report.coverage_percent < 67  # ~66.7%

    def test_includes_gaps(self, project: Path):
        report = analyze_test_coverage(project)

        assert len(report.gaps) == 1
        assert report.gaps[0].source_file.name == "c.py"


class TestCoverageReportClass:
    """Tests for CoverageReport."""

    def test_coverage_percent_empty(self):
        report = CoverageReport(
            patterns=[],
            gaps=[],
            tested_count=0,
            untested_count=0,
            total_source_files=0,
        )

        assert report.coverage_percent == 100.0

    def test_to_compact(self, tmp_path: Path):
        report = CoverageReport(
            patterns=[DetectedTestPattern("test_*.py", "python", 5, [])],
            gaps=[
                CoverageGap(tmp_path / "a.py", "test_a.py", "python"),
                CoverageGap(tmp_path / "b.py", "test_b.py", "python"),
            ],
            tested_count=8,
            untested_count=2,
            total_source_files=10,
        )

        output = report.to_compact()

        assert "80.0%" in output
        assert "8/10" in output
        assert "test_*.py" in output
        assert "2 files without tests" in output
        assert "a.py" in output

    def test_to_compact_truncates_gaps(self, tmp_path: Path):
        gaps = [
            CoverageGap(tmp_path / f"file{i}.py", f"test_file{i}.py", "python") for i in range(10)
        ]
        report = CoverageReport(
            patterns=[DetectedTestPattern("test_*.py", "python", 5, [])],
            gaps=gaps,
            tested_count=0,
            untested_count=10,
            total_source_files=10,
        )

        output = report.to_compact()

        assert "... and 5 more" in output
