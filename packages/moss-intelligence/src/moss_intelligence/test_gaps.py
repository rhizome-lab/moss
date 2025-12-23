"""Test Coverage Heuristics: detect missing tests without execution.

This module provides static analysis of test coverage by:
1. Detecting test file naming patterns in a codebase
2. Finding source files without corresponding tests
3. Detecting in-file tests (e.g., Rust's #[cfg(test)] modules)
4. Reporting test coverage gaps

Unlike runtime coverage tools, this is cheap and fast - no execution needed.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

# Pattern to detect Rust's in-file test modules
RUST_INLINE_TEST_PATTERN = re.compile(r'#\[cfg\(test\)\]')


@dataclass
class DetectedTestPattern:
    """A detected test file naming pattern."""

    pattern: str  # e.g., "test_*.py", "*_test.go"
    language: str  # e.g., "python", "go"
    count: int  # How many files match this pattern
    examples: list[str] = field(default_factory=list)  # Example file paths

    def matches(self, filename: str) -> bool:
        """Check if a filename matches this pattern."""
        if self.pattern.startswith("test_"):
            return filename.startswith("test_")
        elif self.pattern.endswith("_test.py"):
            return filename.endswith("_test.py")
        elif self.pattern.endswith("_test.go"):
            return filename.endswith("_test.go")
        elif self.pattern.endswith("_test.rs"):
            return filename.endswith("_test.rs")
        elif self.pattern.endswith(".test.ts"):
            return filename.endswith(".test.ts") or filename.endswith(".spec.ts")
        elif self.pattern.endswith(".test.js"):
            return filename.endswith(".test.js") or filename.endswith(".spec.js")
        elif self.pattern == "#[cfg(test)]":
            # Inline tests - this pattern is for detection, not matching test files
            return False
        return False

    def source_name(self, test_filename: str) -> str | None:
        """Extract the source file name from a test file name."""
        if self.pattern == "test_*.py" and test_filename.startswith("test_"):
            return test_filename[5:]  # Remove "test_" prefix
        elif self.pattern == "*_test.py" and test_filename.endswith("_test.py"):
            return test_filename[:-8] + ".py"  # Remove "_test" suffix
        elif self.pattern == "*_test.go" and test_filename.endswith("_test.go"):
            return test_filename[:-8] + ".go"
        elif self.pattern == "*.test.ts":
            if test_filename.endswith(".test.ts"):
                return test_filename[:-8] + ".ts"
            elif test_filename.endswith(".spec.ts"):
                return test_filename[:-8] + ".ts"
        elif self.pattern == "*.test.js":
            if test_filename.endswith(".test.js"):
                return test_filename[:-8] + ".js"
            elif test_filename.endswith(".spec.js"):
                return test_filename[:-8] + ".js"
        return None


@dataclass
class CoverageGap:
    """A source file without corresponding tests."""

    source_file: Path
    expected_test: str  # Expected test file name based on pattern
    language: str


@dataclass
class CoverageReport:
    """Report of test coverage gaps in a codebase."""

    patterns: list[DetectedTestPattern]
    gaps: list[CoverageGap]
    tested_count: int
    untested_count: int
    total_source_files: int

    @property
    def coverage_percent(self) -> float:
        """Percentage of source files with tests."""
        if self.total_source_files == 0:
            return 100.0
        return (self.tested_count / self.total_source_files) * 100

    def to_compact(self) -> str:
        """Return compact format for display."""
        lines = [
            f"Test coverage: {self.coverage_percent:.1f}% "
            f"({self.tested_count}/{self.total_source_files} files)"
        ]

        if self.patterns:
            patterns_str = ", ".join(p.pattern for p in self.patterns[:3])
            lines.append(f"Patterns: {patterns_str}")

        if self.gaps:
            lines.append(f"Gaps: {len(self.gaps)} files without tests")
            for gap in self.gaps[:5]:
                lines.append(f"  - {gap.source_file.name}")
            if len(self.gaps) > 5:
                lines.append(f"  ... and {len(self.gaps) - 5} more")

        return "\n".join(lines)


# Common test patterns by language
KNOWN_PATTERNS = {
    "python": [
        ("test_*.py", re.compile(r"^test_.*\.py$")),
        ("*_test.py", re.compile(r"^.*_test\.py$")),
    ],
    "go": [
        ("*_test.go", re.compile(r"^.*_test\.go$")),
    ],
    "rust": [
        ("*_test.rs", re.compile(r"^.*_test\.rs$")),
    ],
    "javascript": [
        ("*.test.js", re.compile(r"^.*\.(test|spec)\.js$")),
    ],
    "typescript": [
        ("*.test.ts", re.compile(r"^.*\.(test|spec)\.ts$")),
    ],
}

# Directories to skip when scanning
SKIP_DIRS = {
    ".git",
    ".venv",
    "venv",
    "node_modules",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    "target",
    "build",
    "dist",
    ".tox",
}


def detect_test_patterns(root: Path) -> list[DetectedTestPattern]:
    """Detect test file naming patterns in a codebase.

    Scans the codebase for test files and identifies which naming
    patterns are in use. Also detects inline tests (e.g., Rust's #[cfg(test)]).

    Args:
        root: Project root directory

    Returns:
        List of detected test patterns, sorted by count (most common first)
    """
    pattern_counts: dict[str, dict] = {}
    inline_test_count = 0
    inline_test_examples: list[str] = []

    for file_path in _walk_files(root):
        filename = file_path.name

        # Check for external test file patterns
        for lang, patterns in KNOWN_PATTERNS.items():
            for pattern_name, regex in patterns:
                if regex.match(filename):
                    key = f"{lang}:{pattern_name}"
                    if key not in pattern_counts:
                        pattern_counts[key] = {
                            "pattern": pattern_name,
                            "language": lang,
                            "count": 0,
                            "examples": [],
                        }
                    pattern_counts[key]["count"] += 1
                    if len(pattern_counts[key]["examples"]) < 3:
                        rel_path = file_path.relative_to(root)
                        pattern_counts[key]["examples"].append(str(rel_path))
                    break

        # Check for Rust inline tests
        if filename.endswith(".rs") and _has_inline_tests(file_path):
            inline_test_count += 1
            if len(inline_test_examples) < 3:
                rel_path = file_path.relative_to(root)
                inline_test_examples.append(str(rel_path))

    # Convert to DetectedTestPattern objects
    patterns = [
        DetectedTestPattern(
            pattern=data["pattern"],
            language=data["language"],
            count=data["count"],
            examples=data["examples"],
        )
        for data in pattern_counts.values()
        if data["count"] > 0
    ]

    # Add inline test pattern if detected
    if inline_test_count > 0:
        patterns.append(
            DetectedTestPattern(
                pattern="#[cfg(test)]",
                language="rust",
                count=inline_test_count,
                examples=inline_test_examples,
            )
        )

    # Sort by count (most common first)
    patterns.sort(key=lambda p: p.count, reverse=True)
    return patterns


def find_untested_files(
    root: Path,
    patterns: list[DetectedTestPattern] | None = None,
) -> list[CoverageGap]:
    """Find source files without corresponding test files.

    Args:
        root: Project root directory
        patterns: Test patterns to use (auto-detected if None)

    Returns:
        List of source files missing tests
    """
    if patterns is None:
        patterns = detect_test_patterns(root)

    if not patterns:
        return []  # No test patterns detected

    # Build set of test files and their corresponding source names
    test_files: set[str] = set()
    source_names_with_tests: set[str] = set()

    for file_path in _walk_files(root):
        filename = file_path.name

        for pattern in patterns:
            if pattern.matches(filename):
                test_files.add(str(file_path.relative_to(root)))
                source_name = pattern.source_name(filename)
                if source_name:
                    source_names_with_tests.add(source_name)
                break

    # Find source files without tests
    gaps: list[CoverageGap] = []

    for file_path in _walk_files(root):
        filename = file_path.name

        # Skip test files themselves
        is_test = any(p.matches(filename) for p in patterns)
        if is_test:
            continue

        # Check if this is a source file in a language we have patterns for
        matched = False
        for pattern in patterns:
            ext = _get_extension(pattern.language)
            if not filename.endswith(ext):
                continue

            matched = True

            # Skip private/internal files
            if filename.startswith("_") and filename != "__init__.py":
                break

            # Check if there's a corresponding test OR inline tests
            has_test = filename in source_names_with_tests
            if not has_test and pattern.language == "rust":
                has_test = _has_inline_tests(file_path)

            if not has_test:
                expected_test = _expected_test_name(filename, pattern)
                gaps.append(
                    CoverageGap(
                        source_file=file_path,
                        expected_test=expected_test,
                        language=pattern.language,
                    )
                )
            break

        # Handle Rust files when only inline pattern exists (no *_test.rs pattern)
        if not matched and filename.endswith(".rs"):
            rust_inline_pattern = any(
                p.pattern == "#[cfg(test)]" and p.language == "rust"
                for p in patterns
            )
            if rust_inline_pattern:
                if not _has_inline_tests(file_path):
                    gaps.append(
                        CoverageGap(
                            source_file=file_path,
                            expected_test=f"{filename[:-3]}_test.rs or #[cfg(test)]",
                            language="rust",
                        )
                    )

    return gaps


def analyze_test_coverage(root: Path) -> CoverageReport:
    """Analyze test coverage in a codebase.

    Args:
        root: Project root directory

    Returns:
        TestCoverageReport with patterns, gaps, and statistics
    """
    patterns = detect_test_patterns(root)
    gaps = find_untested_files(root, patterns)

    # Check if we have Rust inline test pattern
    has_rust_inline = any(
        p.pattern == "#[cfg(test)]" and p.language == "rust"
        for p in patterns
    )

    # Count source files
    total = 0
    untested = len(gaps)

    for file_path in _walk_files(root):
        filename = file_path.name

        # Skip test files
        is_test = any(p.matches(filename) for p in patterns)
        if is_test:
            continue

        # Check if this is a source file
        matched = False
        for pattern in patterns:
            ext = _get_extension(pattern.language)
            if filename.endswith(ext):
                if filename.startswith("_") and filename != "__init__.py":
                    matched = True
                    break
                total += 1
                matched = True
                break

        # Handle Rust files when only inline pattern exists
        if not matched and has_rust_inline and filename.endswith(".rs"):
            total += 1

    tested = total - untested

    return CoverageReport(
        patterns=patterns,
        gaps=gaps,
        tested_count=tested,
        untested_count=untested,
        total_source_files=total,
    )


def _walk_files(root: Path):
    """Walk files in a directory, skipping common non-source directories."""
    for path in root.rglob("*"):
        if path.is_file():
            # Skip files in excluded directories
            parts = path.relative_to(root).parts
            if any(part in SKIP_DIRS for part in parts):
                continue
            yield path


def _get_extension(language: str) -> str:
    """Get file extension for a language."""
    return {
        "python": ".py",
        "go": ".go",
        "rust": ".rs",
        "javascript": ".js",
        "typescript": ".ts",
    }.get(language, "")


def _expected_test_name(source_name: str, pattern: DetectedTestPattern) -> str:
    """Generate expected test file name for a source file."""
    base = source_name.rsplit(".", 1)[0]

    if pattern.pattern.startswith("test_"):
        return f"test_{source_name}"
    elif pattern.pattern == "*_test.py":
        return f"{base}_test.py"
    elif pattern.pattern == "*_test.go":
        return f"{base}_test.go"
    elif pattern.pattern == "*_test.rs":
        return f"{base}_test.rs"
    elif pattern.pattern == "#[cfg(test)]":
        return f"{base}_test.rs or #[cfg(test)]"
    elif pattern.pattern == "*.test.ts":
        return f"{base}.test.ts"
    elif pattern.pattern == "*.test.js":
        return f"{base}.test.js"

    return f"test_{source_name}"


def _has_inline_tests(file_path: Path) -> bool:
    """Check if a file contains inline tests (e.g., Rust's #[cfg(test)]).

    Args:
        file_path: Path to the source file

    Returns:
        True if the file contains inline test modules
    """
    if not file_path.suffix == ".rs":
        return False

    try:
        content = file_path.read_text(errors="ignore")
        return bool(RUST_INLINE_TEST_PATTERN.search(content))
    except OSError:
        return False
