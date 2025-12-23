"""Clone detection via structural hashing.

Identifies structurally similar code by normalizing AST subtrees and comparing hashes.
This helps find code that could potentially be abstracted into shared functions.

Elision levels control what gets normalized:
- Level 0: Names only - replace variable/parameter names with positional placeholders
- Level 1: + Literals - also normalize string/number constants
- Level 2: + Call targets - also normalize function/method names being called
- Level 3: + Expressions - reduce to control flow skeleton only
"""

from __future__ import annotations

import ast
import hashlib
from collections import defaultdict
from dataclasses import dataclass, field
from enum import IntEnum
from pathlib import Path
from typing import Any


class ElisionLevel(IntEnum):
    """Levels of structural normalization for clone detection."""

    NAMES = 0  # Replace variable names with placeholders
    LITERALS = 1  # Also replace literals (strings, numbers)
    CALLS = 2  # Also replace call targets (function names)
    EXPRESSIONS = 3  # Reduce to control flow skeleton


@dataclass
class Clone:
    """A code fragment that may be a clone."""

    file: Path
    name: str  # Function/method name
    lineno: int
    end_lineno: int | None
    source: str  # Original source
    normalized: str  # Normalized representation
    hash: str  # Hash of normalized form


@dataclass
class CloneGroup:
    """A group of structurally similar code fragments."""

    hash: str
    level: ElisionLevel
    clones: list[Clone] = field(default_factory=list)

    @property
    def count(self) -> int:
        return len(self.clones)

    def files(self) -> set[Path]:
        """Unique files containing these clones."""
        return {c.file for c in self.clones}


@dataclass
class CloneAnalysis:
    """Results of clone detection analysis."""

    level: ElisionLevel
    groups: list[CloneGroup] = field(default_factory=list)
    functions_analyzed: int = 0
    files_analyzed: int = 0

    @property
    def total_clones(self) -> int:
        return sum(g.count for g in self.groups)

    @property
    def clone_groups_count(self) -> int:
        return len(self.groups)

    def to_compact(self) -> str:
        """Format as compact text for LLM consumption."""
        lines = [
            f"Clone Analysis (level={self.level.name.lower()}): "
            f"{self.clone_groups_count} groups, {self.total_clones} clones "
            f"({self.functions_analyzed} functions in {self.files_analyzed} files)"
        ]
        for g in self.groups[:5]:  # Limit to top 5 groups
            files = ", ".join(str(f.name) for f in list(g.files())[:3])
            if len(g.files()) > 3:
                files += f" +{len(g.files()) - 3} more"
            lines.append(f"  - {g.count}x clones in: {files}")
        if len(self.groups) > 5:
            lines.append(f"  ... and {len(self.groups) - 5} more groups")
        return "\n".join(lines)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON output."""
        return {
            "level": self.level.name.lower(),
            "stats": {
                "files_analyzed": self.files_analyzed,
                "functions_analyzed": self.functions_analyzed,
                "clone_groups": self.clone_groups_count,
                "total_clones": self.total_clones,
            },
            "groups": [
                {
                    "hash": g.hash[:12],
                    "count": g.count,
                    "files": [str(f) for f in g.files()],
                    "clones": [
                        {
                            "file": str(c.file),
                            "name": c.name,
                            "line": c.lineno,
                            "end_line": c.end_lineno,
                        }
                        for c in g.clones
                    ],
                }
                for g in self.groups
            ],
        }


class ASTNormalizer(ast.NodeTransformer):
    """Normalize AST by replacing names/literals with placeholders.

    This creates a canonical form for structural comparison.
    """

    def __init__(self, level: ElisionLevel = ElisionLevel.NAMES):
        self.level = level
        self.name_map: dict[str, str] = {}
        self.name_counter = 0

    def _get_placeholder(self, name: str) -> str:
        """Get or create a placeholder for a name."""
        if name not in self.name_map:
            self.name_counter += 1
            self.name_map[name] = f"${self.name_counter}"
        return self.name_map[name]

    def visit_Name(self, node: ast.Name) -> ast.Name:
        """Replace variable names with placeholders."""
        return ast.Name(id=self._get_placeholder(node.id), ctx=node.ctx)

    def visit_arg(self, node: ast.arg) -> ast.arg:
        """Replace argument names with placeholders."""
        return ast.arg(
            arg=self._get_placeholder(node.arg),
            annotation=self.visit(node.annotation) if node.annotation else None,
        )

    def visit_FunctionDef(self, node: ast.FunctionDef) -> ast.FunctionDef:
        """Normalize function definition."""
        # Don't normalize the function name itself (we track it separately)
        new_node = ast.FunctionDef(
            name="$func",
            args=self.visit(node.args),
            body=[self.visit(stmt) for stmt in node.body],
            decorator_list=[self.visit(d) for d in node.decorator_list],
            returns=self.visit(node.returns) if node.returns else None,
        )
        return new_node

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> ast.AsyncFunctionDef:
        """Normalize async function definition."""
        new_node = ast.AsyncFunctionDef(
            name="$func",
            args=self.visit(node.args),
            body=[self.visit(stmt) for stmt in node.body],
            decorator_list=[self.visit(d) for d in node.decorator_list],
            returns=self.visit(node.returns) if node.returns else None,
        )
        return new_node

    def visit_Constant(self, node: ast.Constant) -> ast.Constant:
        """Replace literals with placeholders at level >= 1."""
        if self.level >= ElisionLevel.LITERALS:
            if isinstance(node.value, str):
                return ast.Constant(value="$str")
            elif isinstance(node.value, int | float | complex):
                return ast.Constant(value=0)
            elif isinstance(node.value, bytes):
                return ast.Constant(value=b"$bytes")
        return node

    def visit_Call(self, node: ast.Call) -> ast.Call:
        """Normalize function calls at level >= 2."""
        if self.level >= ElisionLevel.CALLS:
            # Replace call target with placeholder
            new_func: ast.expr
            if isinstance(node.func, ast.Name):
                new_func = ast.Name(id="$call", ctx=ast.Load())
            elif isinstance(node.func, ast.Attribute):
                new_func = ast.Attribute(
                    value=self.visit(node.func.value),
                    attr="$method",
                    ctx=ast.Load(),
                )
            else:
                new_func = self.visit(node.func)
            return ast.Call(
                func=new_func,
                args=[self.visit(a) for a in node.args],
                keywords=[self.visit(k) for k in node.keywords],
            )
        else:
            return ast.Call(
                func=self.visit(node.func),
                args=[self.visit(a) for a in node.args],
                keywords=[self.visit(k) for k in node.keywords],
            )

    def visit_Attribute(self, node: ast.Attribute) -> ast.Attribute:
        """Normalize attribute access."""
        if self.level >= ElisionLevel.CALLS:
            return ast.Attribute(
                value=self.visit(node.value),
                attr="$attr",
                ctx=node.ctx,
            )
        return ast.Attribute(
            value=self.visit(node.value),
            attr=node.attr,
            ctx=node.ctx,
        )


class ControlFlowNormalizer(ast.NodeTransformer):
    """Reduce AST to control flow skeleton (level 3).

    Keeps only control flow structure, replaces all expressions with placeholders.
    """

    def visit_If(self, node: ast.If) -> ast.If:
        return ast.If(
            test=ast.Name(id="$cond", ctx=ast.Load()),
            body=[self.visit(stmt) for stmt in node.body],
            orelse=[self.visit(stmt) for stmt in node.orelse],
        )

    def visit_For(self, node: ast.For) -> ast.For:
        return ast.For(
            target=ast.Name(id="$target", ctx=ast.Store()),
            iter=ast.Name(id="$iter", ctx=ast.Load()),
            body=[self.visit(stmt) for stmt in node.body],
            orelse=[self.visit(stmt) for stmt in node.orelse],
        )

    def visit_While(self, node: ast.While) -> ast.While:
        return ast.While(
            test=ast.Name(id="$cond", ctx=ast.Load()),
            body=[self.visit(stmt) for stmt in node.body],
            orelse=[self.visit(stmt) for stmt in node.orelse],
        )

    def visit_With(self, node: ast.With) -> ast.With:
        return ast.With(
            items=[
                ast.withitem(
                    context_expr=ast.Name(id="$ctx", ctx=ast.Load()),
                    optional_vars=(
                        ast.Name(id="$var", ctx=ast.Store()) if item.optional_vars else None
                    ),
                )
                for item in node.items
            ],
            body=[self.visit(stmt) for stmt in node.body],
        )

    def visit_Try(self, node: ast.Try) -> ast.Try:
        return ast.Try(
            body=[self.visit(stmt) for stmt in node.body],
            handlers=[
                ast.ExceptHandler(
                    type=ast.Name(id="$exc", ctx=ast.Load()) if h.type else None,
                    name="$e" if h.name else None,
                    body=[self.visit(stmt) for stmt in h.body],
                )
                for h in node.handlers
            ],
            orelse=[self.visit(stmt) for stmt in node.orelse],
            finalbody=[self.visit(stmt) for stmt in node.finalbody],
        )

    def visit_Match(self, node: ast.Match) -> ast.Match:
        return ast.Match(
            subject=ast.Name(id="$match", ctx=ast.Load()),
            cases=[
                ast.match_case(
                    pattern=ast.MatchAs(pattern=None, name="$pat"),
                    guard=None,
                    body=[self.visit(stmt) for stmt in c.body],
                )
                for c in node.cases
            ],
        )

    def visit_Return(self, node: ast.Return) -> ast.Return:
        return ast.Return(value=ast.Name(id="$ret", ctx=ast.Load()) if node.value else None)

    def visit_Assign(self, node: ast.Assign) -> ast.Assign:
        return ast.Assign(
            targets=[ast.Name(id="$var", ctx=ast.Store())],
            value=ast.Name(id="$val", ctx=ast.Load()),
        )

    def visit_AugAssign(self, node: ast.AugAssign) -> ast.AugAssign:
        return ast.AugAssign(
            target=ast.Name(id="$var", ctx=ast.Store()),
            op=node.op,  # Keep the operator type
            value=ast.Name(id="$val", ctx=ast.Load()),
        )

    def visit_Expr(self, node: ast.Expr) -> ast.Expr:
        # Expression statement - reduce to placeholder
        return ast.Expr(value=ast.Name(id="$expr", ctx=ast.Load()))

    def visit_FunctionDef(self, node: ast.FunctionDef) -> ast.FunctionDef:
        return ast.FunctionDef(
            name="$func",
            args=ast.arguments(
                posonlyargs=[],
                args=[ast.arg(arg="$arg")] * len(node.args.args),
                vararg=ast.arg(arg="$args") if node.args.vararg else None,
                kwonlyargs=[],
                kw_defaults=[],
                kwarg=ast.arg(arg="$kwargs") if node.args.kwarg else None,
                defaults=[],
            ),
            body=[self.visit(stmt) for stmt in node.body],
            decorator_list=[],
            returns=None,
        )

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> ast.AsyncFunctionDef:
        return ast.AsyncFunctionDef(
            name="$func",
            args=ast.arguments(
                posonlyargs=[],
                args=[ast.arg(arg="$arg")] * len(node.args.args),
                vararg=ast.arg(arg="$args") if node.args.vararg else None,
                kwonlyargs=[],
                kw_defaults=[],
                kwarg=ast.arg(arg="$kwargs") if node.args.kwarg else None,
                defaults=[],
            ),
            body=[self.visit(stmt) for stmt in node.body],
            decorator_list=[],
            returns=None,
        )


def normalize_ast(node: ast.AST, level: ElisionLevel) -> ast.AST:
    """Normalize an AST node at the given elision level."""
    if level == ElisionLevel.EXPRESSIONS:
        # Level 3: control flow skeleton
        normalizer = ControlFlowNormalizer()
        return normalizer.visit(node)
    else:
        # Levels 0-2: progressive name/literal/call normalization
        normalizer = ASTNormalizer(level)
        return normalizer.visit(node)


def hash_ast(node: ast.AST) -> str:
    """Compute a hash of an AST node's structure."""
    # Use ast.dump for a canonical string representation
    try:
        dumped = ast.dump(node, annotate_fields=False)
    except (ValueError, AttributeError):
        # Fallback for malformed nodes
        dumped = str(node)

    return hashlib.sha256(dumped.encode()).hexdigest()


def get_function_source(source: str, node: ast.FunctionDef | ast.AsyncFunctionDef) -> str:
    """Extract source code for a function node."""
    lines = source.splitlines()
    start = node.lineno - 1
    end = node.end_lineno if node.end_lineno else start + 1
    return "\n".join(lines[start:end])


class CloneDetector:
    """Detect structural clones in Python code."""

    def __init__(
        self,
        root: Path,
        level: ElisionLevel = ElisionLevel.NAMES,
        min_lines: int = 3,
    ):
        self.root = root.resolve()
        self.level = level
        self.min_lines = min_lines

    def analyze(self) -> CloneAnalysis:
        """Run clone detection on all Python files."""
        result = CloneAnalysis(level=self.level)
        hash_to_clones: dict[str, list[Clone]] = defaultdict(list)

        # Find Python files
        for py_file in self._find_python_files():
            clones = self._analyze_file(py_file)
            result.files_analyzed += 1
            result.functions_analyzed += len(clones)

            for clone in clones:
                hash_to_clones[clone.hash].append(clone)

        # Build groups (only include groups with 2+ clones)
        for hash_val, clones in hash_to_clones.items():
            if len(clones) >= 2:
                group = CloneGroup(hash=hash_val, level=self.level, clones=clones)
                result.groups.append(group)

        # Sort by group size (largest first)
        result.groups.sort(key=lambda g: g.count, reverse=True)

        return result

    def _find_python_files(self) -> list[Path]:
        """Find Python files to analyze."""
        files = []

        # Check common source directories
        for candidate in [self.root / "src", self.root / "lib", self.root]:
            if candidate.exists():
                for py_file in candidate.rglob("*.py"):
                    # Skip test files, vendored code, virtual envs
                    path_str = str(py_file)
                    if any(
                        skip in path_str
                        for skip in ["/test", "/.venv/", "/venv/", "/vendor/", "/__pycache__/"]
                    ):
                        continue
                    files.append(py_file)
                if files:
                    break

        return files

    def _analyze_file(self, path: Path) -> list[Clone]:
        """Analyze a single file for potential clones."""
        clones = []

        try:
            source = path.read_text()
            tree = ast.parse(source)
        except (OSError, UnicodeDecodeError, SyntaxError):
            return clones

        for node in ast.walk(tree):
            if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
                # Skip very short functions
                if node.end_lineno and (node.end_lineno - node.lineno + 1) < self.min_lines:
                    continue

                # Normalize and hash
                try:
                    normalized_node = normalize_ast(node, self.level)
                    normalized_str = ast.dump(normalized_node, annotate_fields=False)
                    hash_val = hash_ast(normalized_node)

                    clone = Clone(
                        file=path,
                        name=node.name,
                        lineno=node.lineno,
                        end_lineno=node.end_lineno,
                        source=get_function_source(source, node),
                        normalized=normalized_str,
                        hash=hash_val,
                    )
                    clones.append(clone)
                except (ValueError, AttributeError, TypeError):
                    # Skip functions that fail to normalize
                    continue

        return clones


def format_clone_analysis(
    analysis: CloneAnalysis, show_source: bool = False, root: Path | None = None
) -> str:
    """Format clone analysis results as markdown.

    Args:
        analysis: Clone analysis results
        show_source: Whether to include source code snippets
        root: Project root for computing relative paths (defaults to showing file names only)
    """
    lines = ["## Clone Analysis", ""]

    # Summary
    level_desc = {
        ElisionLevel.NAMES: "variable names normalized",
        ElisionLevel.LITERALS: "names + literals normalized",
        ElisionLevel.CALLS: "names + literals + calls normalized",
        ElisionLevel.EXPRESSIONS: "control flow skeleton only",
    }
    lines.append(f"**Elision level:** {analysis.level.name.lower()} ({level_desc[analysis.level]})")
    lines.append(f"**Files analyzed:** {analysis.files_analyzed}")
    lines.append(f"**Functions analyzed:** {analysis.functions_analyzed}")
    lines.append(f"**Clone groups found:** {analysis.clone_groups_count}")
    lines.append(f"**Total clones:** {analysis.total_clones}")
    lines.append("")

    if not analysis.groups:
        lines.append("No structural clones detected at this elision level.")
        return "\n".join(lines)

    lines.append("### Clone Groups")
    lines.append("")

    for i, group in enumerate(analysis.groups[:20], 1):  # Limit to top 20
        files = list(group.files())
        file_desc = f"in `{files[0].name}`" if len(files) == 1 else f"across {len(files)} files"
        lines.append(f"**Group {i}** ({group.count} clones {file_desc}):")

        for clone in group.clones:
            if root:
                try:
                    rel_path = str(clone.file.relative_to(root))
                except ValueError:
                    rel_path = clone.file.name
            else:
                rel_path = clone.file.name
            line_range = (
                f"{clone.lineno}-{clone.end_lineno}" if clone.end_lineno else str(clone.lineno)
            )
            lines.append(f"  - `{clone.name}` in `{rel_path}:{line_range}`")

            if show_source:
                lines.append("    ```python")
                for src_line in clone.source.splitlines()[:10]:  # Limit preview
                    lines.append(f"    {src_line}")
                if clone.source.count("\n") > 10:
                    lines.append("    # ... (truncated)")
                lines.append("    ```")

        lines.append("")

    if len(analysis.groups) > 20:
        lines.append(f"*... and {len(analysis.groups) - 20} more groups*")

    return "\n".join(lines)


def detect_clones(
    root: Path | str,
    level: ElisionLevel | int = ElisionLevel.NAMES,
    min_lines: int = 3,
) -> CloneAnalysis:
    """Convenience function to detect clones.

    Args:
        root: Project root directory
        level: Elision level (0-3) for normalization
        min_lines: Minimum function lines to consider

    Returns:
        CloneAnalysis with detected clone groups
    """
    root_path = Path(root).resolve()
    elision_level = ElisionLevel(level) if isinstance(level, int) else level
    detector = CloneDetector(root_path, elision_level, min_lines)
    return detector.analyze()
