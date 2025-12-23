"""Test coverage integration.

Wraps pytest-cov to collect and report test coverage statistics.
"""

from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class FileCoverage:
    """Coverage data for a single file."""

    path: Path
    statements: int = 0
    missing: int = 0
    excluded: int = 0

    @property
    def covered(self) -> int:
        return self.statements - self.missing

    @property
    def coverage_percent(self) -> float:
        if self.statements == 0:
            return 0.0
        return (self.covered / self.statements) * 100

    def to_dict(self) -> dict[str, Any]:
        return {
            "path": str(self.path),
            "statements": self.statements,
            "missing": self.missing,
            "covered": self.covered,
            "coverage_percent": round(self.coverage_percent, 1),
        }


@dataclass
class CoverageReport:
    """Complete coverage report."""

    root: Path
    files: list[FileCoverage] = field(default_factory=list)
    total_statements: int = 0
    total_missing: int = 0
    total_covered: int = 0
    error: str | None = None

    @property
    def coverage_percent(self) -> float:
        if self.total_statements == 0:
            return 0.0
        return (self.total_covered / self.total_statements) * 100

    def to_dict(self) -> dict[str, Any]:
        return {
            "root": str(self.root),
            "total_statements": self.total_statements,
            "total_missing": self.total_missing,
            "total_covered": self.total_covered,
            "coverage_percent": round(self.coverage_percent, 1),
            "files": [f.to_dict() for f in self.files],
            "error": self.error,
        }

    def to_compact(self) -> str:
        """Format as compact summary."""
        if self.error:
            return f"coverage: error - {self.error}"

        if not self.files:
            return "coverage: no data"

        # Find lowest coverage files
        worst = sorted(self.files, key=lambda f: f.coverage_percent)[:3]
        worst_str = ", ".join(f"{f.path.name}:{f.coverage_percent:.0f}%" for f in worst)
        return f"coverage: {self.coverage_percent:.0f}% total | lowest: {worst_str}"

    def to_markdown(self) -> str:
        """Format as markdown report."""
        lines = ["# Test Coverage Report", ""]

        if self.error:
            lines.append(f"**Error**: {self.error}")
            return "\n".join(lines)

        if not self.files:
            lines.append("No coverage data available.")
            lines.append("")
            lines.append("Run `pytest --cov` to generate coverage data.")
            return "\n".join(lines)

        # Summary
        lines.append("## Summary")
        lines.append("")
        lines.append(f"- **Total coverage**: {self.coverage_percent:.1f}%")
        lines.append(f"- **Statements**: {self.total_statements}")
        lines.append(f"- **Covered**: {self.total_covered}")
        lines.append(f"- **Missing**: {self.total_missing}")
        lines.append("")

        # Files by coverage (lowest first)
        lines.append("## Files by Coverage")
        lines.append("")
        lines.append("| File | Statements | Covered | Missing | Coverage |")
        lines.append("|------|------------|---------|---------|----------|")

        for f in sorted(self.files, key=lambda x: x.coverage_percent):
            lines.append(
                f"| {f.path} | {f.statements} | {f.covered} | "
                f"{f.missing} | {f.coverage_percent:.1f}% |"
            )

        lines.append("")

        # Highlight low coverage
        low_coverage = [f for f in self.files if f.coverage_percent < 50]
        if low_coverage:
            lines.append("## Low Coverage Files (< 50%)")
            lines.append("")
            for f in sorted(low_coverage, key=lambda x: x.coverage_percent):
                lines.append(f"- `{f.path}`: {f.coverage_percent:.1f}%")
            lines.append("")

        return "\n".join(lines)


class CoverageAnalyzer:
    """Analyze test coverage using pytest-cov."""

    def __init__(self, root: Path):
        self.root = Path(root).resolve()

    def analyze(self, run_tests: bool = False) -> CoverageReport:
        """Collect coverage data.

        Args:
            run_tests: If True, run pytest with coverage. If False, use existing data.

        Returns:
            CoverageReport with coverage statistics
        """
        report = CoverageReport(root=self.root)

        try:
            if run_tests:
                self._run_coverage()

            # Try to read existing coverage data
            coverage_file = self.root / ".coverage"
            coverage_json = self.root / "coverage.json"

            if coverage_json.exists():
                report = self._parse_coverage_json(coverage_json)
            elif coverage_file.exists():
                # Generate JSON from .coverage file
                self._generate_json_report()
                if coverage_json.exists():
                    report = self._parse_coverage_json(coverage_json)
            else:
                report.error = "No coverage data found. Run pytest --cov first."

        except (OSError, subprocess.SubprocessError, json.JSONDecodeError) as e:
            report.error = str(e)

        return report

    def _run_coverage(self) -> None:
        """Run pytest with coverage."""
        cmd = [
            "python",
            "-m",
            "pytest",
            "--cov=src",
            "--cov-report=json",
            "-q",
        ]
        subprocess.run(cmd, cwd=self.root, capture_output=True)

    def _generate_json_report(self) -> None:
        """Generate JSON report from .coverage file."""
        cmd = ["python", "-m", "coverage", "json"]
        subprocess.run(cmd, cwd=self.root, capture_output=True)

    def _parse_coverage_json(self, path: Path) -> CoverageReport:
        """Parse coverage.json file."""
        report = CoverageReport(root=self.root)

        try:
            with open(path) as f:
                data = json.load(f)
        except (OSError, json.JSONDecodeError) as e:
            report.error = f"Failed to parse coverage.json: {e}"
            return report

        # Parse totals
        totals = data.get("totals", {})
        report.total_statements = totals.get("num_statements", 0)
        report.total_missing = totals.get("missing_lines", 0)
        report.total_covered = totals.get("covered_lines", 0)

        # Parse per-file data
        files = data.get("files", {})
        for file_path, file_data in files.items():
            summary = file_data.get("summary", {})
            # Make path relative to root if possible
            try:
                rel_path = Path(file_path).relative_to(self.root)
            except ValueError:
                rel_path = Path(file_path)

            fc = FileCoverage(
                path=rel_path,
                statements=summary.get("num_statements", 0),
                missing=summary.get("missing_lines", 0),
                excluded=summary.get("excluded_lines", 0),
            )
            report.files.append(fc)

        return report


def analyze_coverage(root: str | Path, run_tests: bool = False) -> CoverageReport:
    """Convenience function to analyze test coverage.

    Args:
        root: Path to the project root
        run_tests: If True, run pytest with coverage first

    Returns:
        CoverageReport with coverage statistics
    """
    analyzer = CoverageAnalyzer(Path(root))
    return analyzer.analyze(run_tests=run_tests)
