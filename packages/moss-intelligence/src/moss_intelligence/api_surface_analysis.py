"""API surface analysis for project health.

Analyzes public interface quality:
- Public exports inventory (__all__, non-underscore names)
- Public/private ratio per module
- Breaking change risk (widely-imported exports)
- Undocumented public APIs
- Inconsistent naming patterns
"""

from __future__ import annotations

import ast
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .dependency_analysis import find_source_dir


@dataclass
class PublicExport:
    """A public export from a module."""

    module: str
    name: str
    kind: str  # function, class, constant, type_alias
    file: Path
    lineno: int
    has_docstring: bool = False
    in_all: bool = False  # Listed in __all__
    import_count: int = 0  # How many modules import this


@dataclass
class NamingIssue:
    """A naming convention inconsistency."""

    module: str
    name: str
    kind: str
    issue: str
    file: Path
    lineno: int


@dataclass
class ModuleAPIStats:
    """API statistics for a single module."""

    module: str
    file: Path
    public_count: int = 0
    private_count: int = 0
    documented_count: int = 0
    has_all: bool = False
    all_count: int = 0  # Items in __all__

    @property
    def public_ratio(self) -> float:
        total = self.public_count + self.private_count
        if total == 0:
            return 0.0
        return self.public_count / total

    @property
    def doc_coverage(self) -> float:
        if self.public_count == 0:
            return 1.0
        return self.documented_count / self.public_count


@dataclass
class APISurfaceAnalysis:
    """Results of API surface analysis."""

    # All exports
    exports: list[PublicExport] = field(default_factory=list)

    # Per-module stats
    module_stats: list[ModuleAPIStats] = field(default_factory=list)

    # Issues
    undocumented: list[PublicExport] = field(default_factory=list)
    naming_issues: list[NamingIssue] = field(default_factory=list)
    high_risk_exports: list[PublicExport] = field(default_factory=list)

    # Summary
    total_public: int = 0
    total_private: int = 0
    total_documented: int = 0
    modules_with_all: int = 0

    @property
    def overall_public_ratio(self) -> float:
        total = self.total_public + self.total_private
        if total == 0:
            return 0.0
        return self.total_public / total

    @property
    def overall_doc_coverage(self) -> float:
        if self.total_public == 0:
            return 1.0
        return self.total_documented / self.total_public

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON output."""
        return {
            "stats": {
                "total_public": self.total_public,
                "total_private": self.total_private,
                "total_documented": self.total_documented,
                "public_ratio": self.overall_public_ratio,
                "doc_coverage": self.overall_doc_coverage,
                "modules_with_all": self.modules_with_all,
            },
            "undocumented": [
                {
                    "module": e.module,
                    "name": e.name,
                    "kind": e.kind,
                    "file": str(e.file),
                    "line": e.lineno,
                }
                for e in self.undocumented[:20]
            ],
            "naming_issues": [
                {
                    "module": i.module,
                    "name": i.name,
                    "kind": i.kind,
                    "issue": i.issue,
                    "file": str(i.file),
                    "line": i.lineno,
                }
                for i in self.naming_issues[:20]
            ],
            "high_risk_exports": [
                {
                    "module": e.module,
                    "name": e.name,
                    "kind": e.kind,
                    "import_count": e.import_count,
                }
                for e in self.high_risk_exports[:20]
            ],
        }


class APISurfaceAnalyzer:
    """Analyze API surface of Python code."""

    # Naming patterns
    SNAKE_CASE = re.compile(r"^[a-z][a-z0-9_]*$")
    PASCAL_CASE = re.compile(r"^[A-Z][a-zA-Z0-9]*$")
    SCREAMING_SNAKE = re.compile(r"^[A-Z][A-Z0-9_]*$")

    def __init__(self, root: Path):
        self.root = root.resolve()
        self._import_counts: dict[str, int] = {}

    def analyze(self) -> APISurfaceAnalysis:
        """Run API surface analysis."""
        result = APISurfaceAnalysis()

        # Find source directory
        src_dir = find_source_dir(self.root)
        if not src_dir:
            return result

        # First pass: collect import counts
        self._collect_import_counts(src_dir)

        # Second pass: analyze each module
        for py_file in src_dir.rglob("*.py"):
            # Skip test files
            if "test" in py_file.name.lower():
                continue

            self._analyze_module(py_file, src_dir, result)

        # Calculate summaries
        result.total_public = sum(s.public_count for s in result.module_stats)
        result.total_private = sum(s.private_count for s in result.module_stats)
        result.total_documented = sum(s.documented_count for s in result.module_stats)
        result.modules_with_all = sum(1 for s in result.module_stats if s.has_all)

        # Find undocumented exports
        result.undocumented = [e for e in result.exports if not e.has_docstring]

        # Find high-risk exports (imported by many modules)
        for export in result.exports:
            export.import_count = self._import_counts.get(export.name, 0)
            if export.import_count >= 5:  # Threshold for "widely used"
                result.high_risk_exports.append(export)

        # Sort by import count
        result.high_risk_exports.sort(key=lambda x: x.import_count, reverse=True)

        return result

    def _collect_import_counts(self, src_dir: Path) -> None:
        """Count how many modules import each name."""
        for py_file in src_dir.rglob("*.py"):
            try:
                source = py_file.read_text()
                tree = ast.parse(source)
            except (OSError, UnicodeDecodeError, SyntaxError):
                continue

            for node in ast.walk(tree):
                if isinstance(node, ast.ImportFrom):
                    for alias in node.names:
                        name = alias.name
                        self._import_counts[name] = self._import_counts.get(name, 0) + 1

    def _analyze_module(
        self,
        path: Path,
        src_dir: Path,
        result: APISurfaceAnalysis,
    ) -> None:
        """Analyze a single module."""
        try:
            source = path.read_text()
            tree = ast.parse(source)
        except (OSError, UnicodeDecodeError, SyntaxError):
            return

        # Get module name
        rel_path = path.relative_to(src_dir)
        module_name = str(rel_path.with_suffix("")).replace("/", ".").replace("\\", ".")

        stats = ModuleAPIStats(module=module_name, file=path)

        # Check for __all__
        all_names: set[str] = set()
        for node in ast.iter_child_nodes(tree):
            if isinstance(node, ast.Assign):
                for target in node.targets:
                    if isinstance(target, ast.Name) and target.id == "__all__":
                        stats.has_all = True
                        all_names = self._extract_all_names(node.value)
                        stats.all_count = len(all_names)

        # Analyze top-level definitions
        for node in ast.iter_child_nodes(tree):
            self._analyze_definition(node, module_name, path, stats, all_names, result)

        result.module_stats.append(stats)

    def _extract_all_names(self, node: ast.expr) -> set[str]:
        """Extract names from __all__ = [...] assignment."""
        names = set()
        if isinstance(node, ast.List | ast.Tuple):
            for elt in node.elts:
                if isinstance(elt, ast.Constant) and isinstance(elt.value, str):
                    names.add(elt.value)
        return names

    def _analyze_definition(
        self,
        node: ast.AST,
        module_name: str,
        path: Path,
        stats: ModuleAPIStats,
        all_names: set[str],
        result: APISurfaceAnalysis,
    ) -> None:
        """Analyze a top-level definition."""
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
            is_public = not node.name.startswith("_")
            if is_public:
                stats.public_count += 1
                has_doc = ast.get_docstring(node) is not None
                if has_doc:
                    stats.documented_count += 1

                export = PublicExport(
                    module=module_name,
                    name=node.name,
                    kind="function",
                    file=path,
                    lineno=node.lineno,
                    has_docstring=has_doc,
                    in_all=node.name in all_names,
                )
                result.exports.append(export)

                # Check naming
                self._check_function_naming(node, module_name, path, result)
            else:
                stats.private_count += 1

        elif isinstance(node, ast.ClassDef):
            is_public = not node.name.startswith("_")
            if is_public:
                stats.public_count += 1
                has_doc = ast.get_docstring(node) is not None
                if has_doc:
                    stats.documented_count += 1

                export = PublicExport(
                    module=module_name,
                    name=node.name,
                    kind="class",
                    file=path,
                    lineno=node.lineno,
                    has_docstring=has_doc,
                    in_all=node.name in all_names,
                )
                result.exports.append(export)

                # Check naming
                self._check_class_naming(node, module_name, path, result)
            else:
                stats.private_count += 1

        elif isinstance(node, ast.Assign):
            # Module-level constants
            for target in node.targets:
                if isinstance(target, ast.Name):
                    name = target.id
                    if name.startswith("_") or name == "__all__":
                        stats.private_count += 1
                    else:
                        stats.public_count += 1
                        # Constants don't have docstrings, but check naming
                        export = PublicExport(
                            module=module_name,
                            name=name,
                            kind="constant",
                            file=path,
                            lineno=node.lineno,
                            has_docstring=True,  # Constants don't need docs
                            in_all=name in all_names,
                        )
                        result.exports.append(export)

                        self._check_constant_naming(name, module_name, path, node.lineno, result)

        elif isinstance(node, ast.AnnAssign):
            # Type annotations (e.g., x: int = 1)
            if isinstance(node.target, ast.Name):
                name = node.target.id
                if name.startswith("_"):
                    stats.private_count += 1
                else:
                    stats.public_count += 1

    def _check_function_naming(
        self,
        node: ast.FunctionDef | ast.AsyncFunctionDef,
        module_name: str,
        path: Path,
        result: APISurfaceAnalysis,
    ) -> None:
        """Check function naming conventions."""
        name = node.name
        if not self.SNAKE_CASE.match(name):
            result.naming_issues.append(
                NamingIssue(
                    module=module_name,
                    name=name,
                    kind="function",
                    issue=f"Function '{name}' should use snake_case",
                    file=path,
                    lineno=node.lineno,
                )
            )

    def _check_class_naming(
        self,
        node: ast.ClassDef,
        module_name: str,
        path: Path,
        result: APISurfaceAnalysis,
    ) -> None:
        """Check class naming conventions."""
        name = node.name
        if not self.PASCAL_CASE.match(name):
            result.naming_issues.append(
                NamingIssue(
                    module=module_name,
                    name=name,
                    kind="class",
                    issue=f"Class '{name}' should use PascalCase",
                    file=path,
                    lineno=node.lineno,
                )
            )

    def _check_constant_naming(
        self,
        name: str,
        module_name: str,
        path: Path,
        lineno: int,
        result: APISurfaceAnalysis,
    ) -> None:
        """Check constant naming conventions."""
        # Constants should be SCREAMING_SNAKE_CASE or snake_case (for module globals)
        # We only flag mixed case that's neither
        if not (self.SCREAMING_SNAKE.match(name) or self.SNAKE_CASE.match(name)):
            result.naming_issues.append(
                NamingIssue(
                    module=module_name,
                    name=name,
                    kind="constant",
                    issue=f"Constant '{name}' should use SCREAMING_SNAKE_CASE or snake_case",
                    file=path,
                    lineno=lineno,
                )
            )


def format_api_surface_analysis(analysis: APISurfaceAnalysis) -> str:
    """Format analysis results as markdown."""
    lines = ["## API Surface Analysis", ""]

    # Summary
    lines.append(f"**Public symbols:** {analysis.total_public}")
    lines.append(f"**Private symbols:** {analysis.total_private}")
    lines.append(f"**Public ratio:** {analysis.overall_public_ratio:.0%}")
    lines.append(f"**Doc coverage:** {analysis.overall_doc_coverage:.0%}")
    lines.append(f"**Modules with __all__:** {analysis.modules_with_all}")
    lines.append("")

    # Undocumented exports
    if analysis.undocumented:
        lines.append("### Undocumented Public APIs")
        lines.append("")
        for e in analysis.undocumented[:15]:
            lines.append(f"- `{e.module}.{e.name}` ({e.kind})")
        if len(analysis.undocumented) > 15:
            lines.append(f"- ... and {len(analysis.undocumented) - 15} more")
        lines.append("")

    # High-risk exports
    if analysis.high_risk_exports:
        lines.append("### High-Risk Exports (Breaking Change Risk)")
        lines.append("")
        for e in analysis.high_risk_exports[:10]:
            lines.append(f"- `{e.name}` ({e.kind}) - imported by {e.import_count} modules")
        lines.append("")

    # Naming issues
    if analysis.naming_issues:
        lines.append("### Naming Inconsistencies")
        lines.append("")
        for i in analysis.naming_issues[:10]:
            lines.append(f"- `{i.module}.{i.name}`: {i.issue}")
        if len(analysis.naming_issues) > 10:
            lines.append(f"- ... and {len(analysis.naming_issues) - 10} more")
        lines.append("")

    if not (analysis.undocumented or analysis.high_risk_exports or analysis.naming_issues):
        lines.append("No API surface issues found.")

    return "\n".join(lines)
