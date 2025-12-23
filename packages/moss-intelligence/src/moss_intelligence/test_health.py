"""Test Health Analysis: extract pytest markers and summarize test health.

This module analyzes test files to extract:
- @pytest.mark.skip (with reasons)
- @pytest.mark.xfail (expected failures)
- @pytest.mark.skipif (conditional skips)
- @pytest.mark.parametrize (parameterized tests)

Provides a health summary showing skip reasons, xfail counts, and test distribution.
"""

from __future__ import annotations

import ast
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class MarkerInfo:
    """Information about a pytest marker on a test."""

    marker: str  # skip, xfail, skipif, parametrize
    reason: str | None = None  # Reason if provided
    condition: str | None = None  # Condition for skipif
    params: int = 0  # Number of params for parametrize


@dataclass
class TestInfo:
    """Information about a single test function."""

    name: str
    file: Path
    lineno: int
    markers: list[MarkerInfo] = field(default_factory=list)

    @property
    def is_skipped(self) -> bool:
        return any(m.marker in ("skip", "skipif") for m in self.markers)

    @property
    def is_xfail(self) -> bool:
        return any(m.marker == "xfail" for m in self.markers)

    @property
    def is_parametrized(self) -> bool:
        return any(m.marker == "parametrize" for m in self.markers)


@dataclass
class TestHealthReport:
    """Summary of test health for a project."""

    total_tests: int = 0
    skipped_tests: int = 0
    xfail_tests: int = 0
    parametrized_tests: int = 0
    total_params: int = 0  # Total parametrize cases

    skip_reasons: dict[str, int] = field(default_factory=dict)
    xfail_reasons: dict[str, int] = field(default_factory=dict)
    skipif_conditions: dict[str, int] = field(default_factory=dict)

    tests: list[TestInfo] = field(default_factory=list)

    @property
    def skip_rate(self) -> float:
        """Percentage of tests that are skipped."""
        if self.total_tests == 0:
            return 0.0
        return (self.skipped_tests / self.total_tests) * 100

    @property
    def xfail_rate(self) -> float:
        """Percentage of tests that are expected to fail."""
        if self.total_tests == 0:
            return 0.0
        return (self.xfail_tests / self.total_tests) * 100

    def to_compact(self) -> str:
        """Return compact format for display."""
        lines = [f"# Test Health: {self.total_tests} tests"]

        if self.skipped_tests:
            lines.append(f"Skipped: {self.skipped_tests} ({self.skip_rate:.1f}%)")
            if self.skip_reasons:
                top_reasons = sorted(
                    self.skip_reasons.items(), key=lambda x: x[1], reverse=True
                )[:3]
                for reason, count in top_reasons:
                    short_reason = reason[:50] + "..." if len(reason) > 50 else reason
                    lines.append(f"  - {short_reason}: {count}")

        if self.xfail_tests:
            lines.append(f"Expected failures: {self.xfail_tests} ({self.xfail_rate:.1f}%)")

        if self.parametrized_tests:
            lines.append(
                f"Parametrized: {self.parametrized_tests} tests, {self.total_params} cases"
            )

        if self.skipif_conditions:
            lines.append(f"Conditional skips: {len(self.skipif_conditions)} conditions")
            top_conds = sorted(
                self.skipif_conditions.items(), key=lambda x: x[1], reverse=True
            )[:3]
            for cond, count in top_conds:
                short_cond = cond[:40] + "..." if len(cond) > 40 else cond
                lines.append(f"  - {short_cond}: {count}")

        return "\n".join(lines)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON output."""
        return {
            "summary": {
                "total_tests": self.total_tests,
                "skipped_tests": self.skipped_tests,
                "xfail_tests": self.xfail_tests,
                "parametrized_tests": self.parametrized_tests,
                "total_params": self.total_params,
                "skip_rate": self.skip_rate,
                "xfail_rate": self.xfail_rate,
            },
            "skip_reasons": dict(
                sorted(self.skip_reasons.items(), key=lambda x: x[1], reverse=True)[:10]
            ),
            "xfail_reasons": dict(
                sorted(self.xfail_reasons.items(), key=lambda x: x[1], reverse=True)[:10]
            ),
            "skipif_conditions": dict(
                sorted(
                    self.skipif_conditions.items(), key=lambda x: x[1], reverse=True
                )[:10]
            ),
            "skipped_tests": [
                {"name": t.name, "file": str(t.file), "line": t.lineno}
                for t in self.tests
                if t.is_skipped
            ][:20],
        }


# Directories to skip
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


def analyze_test_health(root: Path) -> TestHealthReport:
    """Analyze test health by extracting pytest markers.

    Args:
        root: Project root directory

    Returns:
        TestHealthReport with marker statistics and skip reasons
    """
    report = TestHealthReport()

    # Find test files
    for path in root.rglob("*.py"):
        # Skip excluded directories
        parts = path.relative_to(root).parts
        if any(part in SKIP_DIRS for part in parts):
            continue

        # Only process test files
        if not (path.name.startswith("test_") or path.name.endswith("_test.py")):
            continue

        _analyze_test_file(path, report)

    return report


def _analyze_test_file(path: Path, report: TestHealthReport) -> None:
    """Analyze a single test file for pytest markers."""
    try:
        source = path.read_text()
        tree = ast.parse(source)
    except (OSError, SyntaxError, UnicodeDecodeError):
        return

    for node in ast.walk(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue

        # Only process test functions
        if not node.name.startswith("test_"):
            continue

        report.total_tests += 1

        test_info = TestInfo(
            name=node.name,
            file=path,
            lineno=node.lineno,
        )

        # Extract markers from decorators
        for decorator in node.decorator_list:
            marker_info = _extract_marker(decorator)
            if marker_info:
                test_info.markers.append(marker_info)

                if marker_info.marker == "skip":
                    report.skipped_tests += 1
                    reason = marker_info.reason or "(no reason)"
                    report.skip_reasons[reason] = report.skip_reasons.get(reason, 0) + 1

                elif marker_info.marker == "skipif":
                    report.skipped_tests += 1
                    cond = marker_info.condition or "(unknown)"
                    report.skipif_conditions[cond] = (
                        report.skipif_conditions.get(cond, 0) + 1
                    )

                elif marker_info.marker == "xfail":
                    report.xfail_tests += 1
                    reason = marker_info.reason or "(no reason)"
                    report.xfail_reasons[reason] = (
                        report.xfail_reasons.get(reason, 0) + 1
                    )

                elif marker_info.marker == "parametrize":
                    report.parametrized_tests += 1
                    report.total_params += marker_info.params

        if test_info.markers:
            report.tests.append(test_info)


def _extract_marker(decorator: ast.expr) -> MarkerInfo | None:
    """Extract marker info from a decorator node."""
    # Handle @pytest.mark.skip, @pytest.mark.xfail, etc.
    if isinstance(decorator, ast.Attribute):
        marker_name = decorator.attr
        if marker_name in ("skip", "xfail"):
            return MarkerInfo(marker=marker_name)
        return None

    if isinstance(decorator, ast.Call):
        func = decorator.func

        # @pytest.mark.skip(reason="...")
        if isinstance(func, ast.Attribute):
            marker_name = func.attr

            if marker_name == "skip":
                reason = _extract_reason(decorator)
                return MarkerInfo(marker="skip", reason=reason)

            elif marker_name == "xfail":
                reason = _extract_reason(decorator)
                return MarkerInfo(marker="xfail", reason=reason)

            elif marker_name == "skipif":
                condition = _extract_condition(decorator)
                reason = _extract_reason(decorator)
                return MarkerInfo(marker="skipif", condition=condition, reason=reason)

            elif marker_name == "parametrize":
                params = _count_params(decorator)
                return MarkerInfo(marker="parametrize", params=params)

        # Handle mark.skip as a simple call
        if isinstance(func, ast.Name) and func.id in ("skip", "xfail"):
            reason = _extract_reason(decorator)
            return MarkerInfo(marker=func.id, reason=reason)

    return None


def _extract_reason(call: ast.Call) -> str | None:
    """Extract reason argument from a marker call."""
    # Check keyword argument
    for kw in call.keywords:
        if kw.arg == "reason" and isinstance(kw.value, ast.Constant):
            return str(kw.value.value)

    # Check first positional argument (for skip)
    if call.args and isinstance(call.args[0], ast.Constant):
        return str(call.args[0].value)

    return None


def _extract_condition(call: ast.Call) -> str | None:
    """Extract condition from skipif marker."""
    if call.args:
        # Try to get the condition as source
        return ast.unparse(call.args[0])
    return None


def _count_params(call: ast.Call) -> int:
    """Count parameter cases in parametrize marker."""
    # @pytest.mark.parametrize("name", [val1, val2, val3])
    if len(call.args) >= 2:
        values_arg = call.args[1]
        if isinstance(values_arg, (ast.List, ast.Tuple)):
            return len(values_arg.elts)
    return 1
