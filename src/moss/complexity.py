"""Cyclomatic complexity analysis.

Calculates McCabe cyclomatic complexity for Python functions.
Complexity = E - N + 2P where E=edges, N=nodes, P=connected components.

For Python, we count decision points:
- if, elif, else
- for, while
- except, with
- and, or (short-circuit)
- assert (counts as branch)
- comprehensions with conditions
"""

from __future__ import annotations

import ast
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class FunctionComplexity:
    """Complexity data for a single function."""

    name: str
    file: Path
    lineno: int
    complexity: int
    is_method: bool = False
    class_name: str | None = None

    @property
    def qualified_name(self) -> str:
        if self.class_name:
            return f"{self.class_name}.{self.name}"
        return self.name

    @property
    def risk_level(self) -> str:
        """Categorize complexity risk."""
        if self.complexity <= 5:
            return "low"
        elif self.complexity <= 10:
            return "moderate"
        elif self.complexity <= 20:
            return "high"
        else:
            return "very-high"

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "qualified_name": self.qualified_name,
            "file": str(self.file),
            "lineno": self.lineno,
            "complexity": self.complexity,
            "risk_level": self.risk_level,
        }


@dataclass
class ComplexityReport:
    """Complete complexity analysis report."""

    root: Path
    functions: list[FunctionComplexity] = field(default_factory=list)
    total_functions: int = 0
    avg_complexity: float = 0.0
    max_complexity: int = 0
    error: str | None = None

    @property
    def high_complexity_count(self) -> int:
        return sum(1 for f in self.functions if f.complexity > 10)

    @property
    def very_high_complexity_count(self) -> int:
        return sum(1 for f in self.functions if f.complexity > 20)

    def to_dict(self) -> dict[str, Any]:
        return {
            "root": str(self.root),
            "total_functions": self.total_functions,
            "avg_complexity": round(self.avg_complexity, 1),
            "max_complexity": self.max_complexity,
            "high_complexity_count": self.high_complexity_count,
            "very_high_complexity_count": self.very_high_complexity_count,
            "functions": [f.to_dict() for f in self.functions],
            "error": self.error,
        }

    def to_compact(self) -> str:
        """Format as compact summary."""
        if self.error:
            return f"complexity: error - {self.error}"

        if not self.functions:
            return "complexity: no functions analyzed"

        high = self.high_complexity_count
        vhigh = self.very_high_complexity_count

        parts = [f"complexity: avg {self.avg_complexity:.1f}, max {self.max_complexity}"]
        if vhigh:
            parts.append(f"{vhigh} very-high risk")
        elif high:
            parts.append(f"{high} high risk")
        else:
            parts.append("all low/moderate")

        return " | ".join(parts)

    def to_markdown(self) -> str:
        """Format as markdown report."""
        lines = ["# Cyclomatic Complexity Report", ""]

        if self.error:
            lines.append(f"**Error**: {self.error}")
            return "\n".join(lines)

        if not self.functions:
            lines.append("No functions analyzed.")
            return "\n".join(lines)

        # Summary
        lines.append("## Summary")
        lines.append("")
        lines.append(f"- **Functions analyzed**: {self.total_functions}")
        lines.append(f"- **Average complexity**: {self.avg_complexity:.1f}")
        lines.append(f"- **Maximum complexity**: {self.max_complexity}")
        lines.append(f"- **High risk (>10)**: {self.high_complexity_count}")
        lines.append(f"- **Very high risk (>20)**: {self.very_high_complexity_count}")
        lines.append("")

        # Risk levels
        lines.append("## Complexity Thresholds")
        lines.append("")
        lines.append("| Complexity | Risk | Action |")
        lines.append("|------------|------|--------|")
        lines.append("| 1-5 | Low | Easily testable |")
        lines.append("| 6-10 | Moderate | Review recommended |")
        lines.append("| 11-20 | High | Consider refactoring |")
        lines.append("| 21+ | Very High | Refactor immediately |")
        lines.append("")

        # High complexity functions
        high_funcs = [f for f in self.functions if f.complexity > 10]
        if high_funcs:
            lines.append("## High Complexity Functions")
            lines.append("")
            lines.append("| Function | File | Line | Complexity | Risk |")
            lines.append("|----------|------|------|------------|------|")

            for f in sorted(high_funcs, key=lambda x: -x.complexity):
                lines.append(
                    f"| `{f.qualified_name}` | {f.file} | {f.lineno} | "
                    f"{f.complexity} | {f.risk_level} |"
                )
            lines.append("")

        # All functions by complexity (top 20)
        lines.append("## All Functions (sorted by complexity)")
        lines.append("")
        lines.append("| Function | File | Complexity |")
        lines.append("|----------|------|------------|")

        for f in sorted(self.functions, key=lambda x: -x.complexity)[:20]:
            lines.append(f"| `{f.qualified_name}` | {f.file.name}:{f.lineno} | {f.complexity} |")

        if len(self.functions) > 20:
            lines.append(f"| ... | ({len(self.functions) - 20} more) | ... |")

        lines.append("")

        return "\n".join(lines)


class ComplexityVisitor(ast.NodeVisitor):
    """AST visitor to calculate cyclomatic complexity."""

    def __init__(self):
        self.complexity = 1  # Base complexity

    def visit_If(self, node: ast.If) -> None:
        self.complexity += 1
        self.generic_visit(node)

    def visit_For(self, node: ast.For) -> None:
        self.complexity += 1
        self.generic_visit(node)

    def visit_While(self, node: ast.While) -> None:
        self.complexity += 1
        self.generic_visit(node)

    def visit_ExceptHandler(self, node: ast.ExceptHandler) -> None:
        self.complexity += 1
        self.generic_visit(node)

    def visit_With(self, node: ast.With) -> None:
        self.complexity += 1
        self.generic_visit(node)

    def visit_Assert(self, node: ast.Assert) -> None:
        self.complexity += 1
        self.generic_visit(node)

    def visit_BoolOp(self, node: ast.BoolOp) -> None:
        # and/or add complexity for each additional operand
        self.complexity += len(node.values) - 1
        self.generic_visit(node)

    def visit_comprehension(self, node: ast.comprehension) -> None:
        # List/dict/set comprehensions with conditions
        self.complexity += len(node.ifs)
        self.generic_visit(node)

    def visit_IfExp(self, node: ast.IfExp) -> None:
        # Ternary expression
        self.complexity += 1
        self.generic_visit(node)


class ComplexityAnalyzer:
    """Analyze cyclomatic complexity of Python code."""

    def __init__(self, root: Path):
        self.root = Path(root).resolve()

    def analyze(self, pattern: str = "**/*.py") -> ComplexityReport:
        """Analyze all Python files matching pattern."""
        report = ComplexityReport(root=self.root)

        try:
            files = list(self.root.glob(pattern))
            # Exclude common non-source directories
            files = [f for f in files if "__pycache__" not in str(f) and ".venv" not in str(f)]

            for file_path in files:
                self._analyze_file(file_path, report)

            # Calculate summary stats
            report.total_functions = len(report.functions)
            if report.functions:
                total = sum(f.complexity for f in report.functions)
                report.avg_complexity = total / len(report.functions)
                report.max_complexity = max(f.complexity for f in report.functions)

        except Exception as e:
            report.error = str(e)

        return report

    def _analyze_file(self, path: Path, report: ComplexityReport) -> None:
        """Analyze a single file."""
        try:
            source = path.read_text()
            tree = ast.parse(source)
        except Exception:
            return

        # Make path relative
        try:
            rel_path = path.relative_to(self.root)
        except ValueError:
            rel_path = path

        # Visit all functions and methods
        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                visitor = ComplexityVisitor()
                visitor.visit(node)

                # Determine if it's a method
                is_method = False
                class_name = None
                for parent in ast.walk(tree):
                    if isinstance(parent, ast.ClassDef):
                        if node in ast.walk(parent):
                            is_method = True
                            class_name = parent.name
                            break

                fc = FunctionComplexity(
                    name=node.name,
                    file=rel_path,
                    lineno=node.lineno,
                    complexity=visitor.complexity,
                    is_method=is_method,
                    class_name=class_name,
                )
                report.functions.append(fc)


def analyze_complexity(root: str | Path, pattern: str = "**/*.py") -> ComplexityReport:
    """Convenience function to analyze cyclomatic complexity.

    Args:
        root: Path to the project root
        pattern: Glob pattern for files to analyze

    Returns:
        ComplexityReport with complexity data
    """
    analyzer = ComplexityAnalyzer(Path(root))
    return analyzer.analyze(pattern=pattern)
