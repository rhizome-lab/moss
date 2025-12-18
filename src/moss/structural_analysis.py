"""Structural analysis for code quality hotspots.

Identifies:
- Functions with too many parameters
- Classes with too many methods
- Files that are too long
- Deep nesting in functions
- Long functions
- Complex conditionals
"""

from __future__ import annotations

import ast
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class StructuralThresholds:
    """Configurable thresholds for structural analysis."""

    max_params: int = 5  # Max parameters per function
    max_methods: int = 15  # Max methods per class
    max_file_lines: int = 500  # Max lines per file
    max_function_lines: int = 50  # Max lines per function
    max_nesting_depth: int = 4  # Max nesting levels
    max_branches: int = 10  # Max branches in a function (cyclomatic-like)


@dataclass
class FunctionHotspot:
    """A function with structural issues."""

    file: Path
    name: str
    lineno: int
    issues: list[str] = field(default_factory=list)

    # Metrics
    param_count: int = 0
    line_count: int = 0
    max_nesting: int = 0
    branch_count: int = 0


@dataclass
class ClassHotspot:
    """A class with structural issues."""

    file: Path
    name: str
    lineno: int
    issues: list[str] = field(default_factory=list)

    # Metrics
    method_count: int = 0
    line_count: int = 0


@dataclass
class FileHotspot:
    """A file with structural issues."""

    file: Path
    issues: list[str] = field(default_factory=list)

    # Metrics
    line_count: int = 0
    function_count: int = 0
    class_count: int = 0


@dataclass
class StructuralAnalysis:
    """Results of structural analysis."""

    thresholds: StructuralThresholds = field(default_factory=StructuralThresholds)

    function_hotspots: list[FunctionHotspot] = field(default_factory=list)
    class_hotspots: list[ClassHotspot] = field(default_factory=list)
    file_hotspots: list[FileHotspot] = field(default_factory=list)

    # Summary stats
    files_analyzed: int = 0
    functions_analyzed: int = 0
    classes_analyzed: int = 0

    @property
    def has_issues(self) -> bool:
        return bool(self.function_hotspots or self.class_hotspots or self.file_hotspots)

    @property
    def total_issues(self) -> int:
        return (
            sum(len(h.issues) for h in self.function_hotspots)
            + sum(len(h.issues) for h in self.class_hotspots)
            + sum(len(h.issues) for h in self.file_hotspots)
        )

    def to_compact(self) -> str:
        """Return compact format for LLM consumption."""
        parts = [
            f"{self.files_analyzed} files",
            f"{self.functions_analyzed} functions",
            f"{self.classes_analyzed} classes",
        ]
        if self.has_issues:
            hotspot_parts = []
            if self.function_hotspots:
                hotspot_parts.append(f"{len(self.function_hotspots)} function")
            if self.class_hotspots:
                hotspot_parts.append(f"{len(self.class_hotspots)} class")
            if self.file_hotspots:
                hotspot_parts.append(f"{len(self.file_hotspots)} file")
            parts.append(f"hotspots: {', '.join(hotspot_parts)}")
        else:
            parts.append("no hotspots")
        return " | ".join(parts)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON output."""
        return {
            "thresholds": {
                "max_params": self.thresholds.max_params,
                "max_methods": self.thresholds.max_methods,
                "max_file_lines": self.thresholds.max_file_lines,
                "max_function_lines": self.thresholds.max_function_lines,
                "max_nesting_depth": self.thresholds.max_nesting_depth,
                "max_branches": self.thresholds.max_branches,
            },
            "stats": {
                "files_analyzed": self.files_analyzed,
                "functions_analyzed": self.functions_analyzed,
                "classes_analyzed": self.classes_analyzed,
                "total_issues": self.total_issues,
            },
            "function_hotspots": [
                {
                    "file": str(h.file),
                    "name": h.name,
                    "line": h.lineno,
                    "issues": h.issues,
                    "param_count": h.param_count,
                    "line_count": h.line_count,
                    "max_nesting": h.max_nesting,
                    "branch_count": h.branch_count,
                }
                for h in self.function_hotspots
            ],
            "class_hotspots": [
                {
                    "file": str(h.file),
                    "name": h.name,
                    "line": h.lineno,
                    "issues": h.issues,
                    "method_count": h.method_count,
                    "line_count": h.line_count,
                }
                for h in self.class_hotspots
            ],
            "file_hotspots": [
                {
                    "file": str(h.file),
                    "issues": h.issues,
                    "line_count": h.line_count,
                    "function_count": h.function_count,
                    "class_count": h.class_count,
                }
                for h in self.file_hotspots
            ],
        }


class NestingDepthVisitor(ast.NodeVisitor):
    """Calculate maximum nesting depth in a function."""

    def __init__(self):
        self.max_depth = 0
        self.current_depth = 0

    def _enter_block(self) -> None:
        self.current_depth += 1
        self.max_depth = max(self.max_depth, self.current_depth)

    def _exit_block(self) -> None:
        self.current_depth -= 1

    def visit_If(self, node: ast.If) -> None:
        self._enter_block()
        self.generic_visit(node)
        self._exit_block()

    def visit_For(self, node: ast.For) -> None:
        self._enter_block()
        self.generic_visit(node)
        self._exit_block()

    def visit_While(self, node: ast.While) -> None:
        self._enter_block()
        self.generic_visit(node)
        self._exit_block()

    def visit_With(self, node: ast.With) -> None:
        self._enter_block()
        self.generic_visit(node)
        self._exit_block()

    def visit_Try(self, node: ast.Try) -> None:
        self._enter_block()
        self.generic_visit(node)
        self._exit_block()

    def visit_Match(self, node: ast.Match) -> None:
        self._enter_block()
        self.generic_visit(node)
        self._exit_block()


class BranchCountVisitor(ast.NodeVisitor):
    """Count branches (if/elif/for/while/try/except) in a function."""

    def __init__(self):
        self.count = 0

    def visit_If(self, node: ast.If) -> None:
        self.count += 1
        self.generic_visit(node)

    def visit_For(self, node: ast.For) -> None:
        self.count += 1
        self.generic_visit(node)

    def visit_While(self, node: ast.While) -> None:
        self.count += 1
        self.generic_visit(node)

    def visit_Try(self, node: ast.Try) -> None:
        self.count += 1
        self.count += len(node.handlers)  # Each except is a branch
        self.generic_visit(node)

    def visit_Match(self, node: ast.Match) -> None:
        self.count += len(node.cases)
        self.generic_visit(node)

    def visit_BoolOp(self, node: ast.BoolOp) -> None:
        # and/or create implicit branches
        if isinstance(node.op, ast.And | ast.Or):
            self.count += len(node.values) - 1
        self.generic_visit(node)


class StructuralAnalyzer:
    """Analyze structural quality of Python code."""

    def __init__(self, root: Path, thresholds: StructuralThresholds | None = None):
        self.root = root.resolve()
        self.thresholds = thresholds or StructuralThresholds()

    def analyze(self) -> StructuralAnalysis:
        """Run structural analysis on all Python files."""
        result = StructuralAnalysis(thresholds=self.thresholds)

        # Find Python files
        src_dir = self._find_source_dir()
        if not src_dir:
            return result

        for py_file in src_dir.rglob("*.py"):
            # Skip test files
            if "test" in py_file.name.lower() or "/tests/" in str(py_file):
                continue

            self._analyze_file(py_file, result)

        return result

    def _find_source_dir(self) -> Path | None:
        """Find the main source directory."""
        for candidate in [self.root / "src", self.root / "lib", self.root]:
            if candidate.exists():
                for subdir in candidate.iterdir():
                    if subdir.is_dir() and (subdir / "__init__.py").exists():
                        return subdir
                if list(candidate.glob("*.py")):
                    return candidate
        return None

    def _analyze_file(self, path: Path, result: StructuralAnalysis) -> None:
        """Analyze a single Python file."""
        try:
            source = path.read_text()
            lines = source.splitlines()
            line_count = len(lines)
        except Exception:
            return

        result.files_analyzed += 1

        # Check file length
        file_hotspot = FileHotspot(file=path, line_count=line_count)
        if line_count > self.thresholds.max_file_lines:
            file_hotspot.issues.append(
                f"File too long: {line_count} lines (max {self.thresholds.max_file_lines})"
            )

        try:
            tree = ast.parse(source)
        except SyntaxError:
            return

        # Analyze functions and classes
        for node in ast.walk(tree):
            if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
                # Skip methods (handled with class)
                if self._is_top_level_function(tree, node):
                    result.functions_analyzed += 1
                    file_hotspot.function_count += 1
                    hotspot = self._analyze_function(path, node)
                    if hotspot and hotspot.issues:
                        result.function_hotspots.append(hotspot)

            elif isinstance(node, ast.ClassDef):
                result.classes_analyzed += 1
                file_hotspot.class_count += 1
                hotspot = self._analyze_class(path, node, result)
                if hotspot and hotspot.issues:
                    result.class_hotspots.append(hotspot)

        if file_hotspot.issues:
            result.file_hotspots.append(file_hotspot)

    def _is_top_level_function(self, tree: ast.Module, func: ast.AST) -> bool:
        """Check if function is at module level (not a method)."""
        for node in ast.iter_child_nodes(tree):
            if node is func:
                return True
        return False

    def _analyze_function(
        self,
        path: Path,
        node: ast.FunctionDef | ast.AsyncFunctionDef,
    ) -> FunctionHotspot | None:
        """Analyze a function for structural issues."""
        hotspot = FunctionHotspot(file=path, name=node.name, lineno=node.lineno)

        # Parameter count
        args = node.args
        param_count = (
            len(args.args)
            + len(args.posonlyargs)
            + len(args.kwonlyargs)
            + (1 if args.vararg else 0)
            + (1 if args.kwarg else 0)
        )
        # Don't count 'self' or 'cls'
        if args.args and args.args[0].arg in ("self", "cls"):
            param_count -= 1

        hotspot.param_count = param_count
        if param_count > self.thresholds.max_params:
            hotspot.issues.append(
                f"Too many parameters: {param_count} (max {self.thresholds.max_params})"
            )

        # Line count
        if node.end_lineno:
            line_count = node.end_lineno - node.lineno + 1
            hotspot.line_count = line_count
            if line_count > self.thresholds.max_function_lines:
                hotspot.issues.append(
                    f"Function too long: {line_count} lines "
                    f"(max {self.thresholds.max_function_lines})"
                )

        # Nesting depth
        nesting_visitor = NestingDepthVisitor()
        nesting_visitor.visit(node)
        hotspot.max_nesting = nesting_visitor.max_depth
        if nesting_visitor.max_depth > self.thresholds.max_nesting_depth:
            hotspot.issues.append(
                f"Deep nesting: {nesting_visitor.max_depth} levels "
                f"(max {self.thresholds.max_nesting_depth})"
            )

        # Branch count (cyclomatic-like)
        branch_visitor = BranchCountVisitor()
        branch_visitor.visit(node)
        hotspot.branch_count = branch_visitor.count
        if branch_visitor.count > self.thresholds.max_branches:
            hotspot.issues.append(
                f"High complexity: {branch_visitor.count} branches "
                f"(max {self.thresholds.max_branches})"
            )

        return hotspot

    def _analyze_class(
        self,
        path: Path,
        node: ast.ClassDef,
        result: StructuralAnalysis,
    ) -> ClassHotspot | None:
        """Analyze a class for structural issues."""
        hotspot = ClassHotspot(file=path, name=node.name, lineno=node.lineno)

        # Count methods
        methods = [n for n in node.body if isinstance(n, ast.FunctionDef | ast.AsyncFunctionDef)]
        hotspot.method_count = len(methods)
        if len(methods) > self.thresholds.max_methods:
            hotspot.issues.append(
                f"Too many methods: {len(methods)} (max {self.thresholds.max_methods})"
            )

        # Line count
        if node.end_lineno:
            hotspot.line_count = node.end_lineno - node.lineno + 1

        # Also analyze each method
        for method in methods:
            result.functions_analyzed += 1
            method_hotspot = self._analyze_function(path, method)
            if method_hotspot and method_hotspot.issues:
                # Prefix with class name
                method_hotspot.name = f"{node.name}.{method_hotspot.name}"
                result.function_hotspots.append(method_hotspot)

        return hotspot


def format_structural_analysis(analysis: StructuralAnalysis) -> str:
    """Format analysis results as markdown."""
    lines = ["## Structural Analysis", ""]

    # Summary
    lines.append(f"**Files analyzed:** {analysis.files_analyzed}")
    lines.append(f"**Functions analyzed:** {analysis.functions_analyzed}")
    lines.append(f"**Classes analyzed:** {analysis.classes_analyzed}")
    lines.append(f"**Issues found:** {analysis.total_issues}")
    lines.append("")

    if not analysis.has_issues:
        lines.append("No structural issues found.")
        return "\n".join(lines)

    # File hotspots
    if analysis.file_hotspots:
        lines.append("### Large Files")
        lines.append("")
        for h in sorted(analysis.file_hotspots, key=lambda x: x.line_count, reverse=True)[:10]:
            lines.append(f"- `{h.file.name}`: {h.line_count} lines")
            for issue in h.issues:
                lines.append(f"  - {issue}")
        lines.append("")

    # Function hotspots
    if analysis.function_hotspots:
        lines.append("### Function Hotspots")
        lines.append("")
        for h in sorted(analysis.function_hotspots, key=lambda x: len(x.issues), reverse=True)[:10]:
            lines.append(f"- `{h.name}` ({h.file.name}:{h.lineno})")
            for issue in h.issues:
                lines.append(f"  - {issue}")
        lines.append("")

    # Class hotspots
    if analysis.class_hotspots:
        lines.append("### Class Hotspots")
        lines.append("")
        for h in sorted(analysis.class_hotspots, key=lambda x: x.method_count, reverse=True)[:10]:
            lines.append(f"- `{h.name}` ({h.file.name}:{h.lineno})")
            for issue in h.issues:
                lines.append(f"  - {issue}")
        lines.append("")

    return "\n".join(lines)
