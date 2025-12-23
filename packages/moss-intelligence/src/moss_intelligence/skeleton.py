"""Skeleton Provider: AST-based code signature extraction."""

from __future__ import annotations

import ast
from dataclasses import dataclass
from typing import TYPE_CHECKING

from .views import View, ViewOptions, ViewProvider, ViewTarget, ViewType

if TYPE_CHECKING:
    from moss.plugins import PluginMetadata


@dataclass
class Symbol:
    """A code symbol extracted from AST."""

    name: str
    kind: str  # class, function, method, variable
    signature: str  # Full signature line
    docstring: str | None
    lineno: int
    end_lineno: int | None  # End line for size calculation
    children: list[Symbol]

    @property
    def line_count(self) -> int | None:
        """Number of lines in this symbol, or None if end_lineno unknown."""
        if self.end_lineno is None:
            return None
        return self.end_lineno - self.lineno + 1

    def to_dict(self) -> dict:
        """Convert to a serializable dictionary."""
        result = {
            "name": self.name,
            "kind": self.kind,
            "line": self.lineno,
        }
        if self.end_lineno is not None:
            result["end_line"] = self.end_lineno
            result["line_count"] = self.line_count
        if self.signature:
            result["signature"] = self.signature
        if self.docstring:
            result["docstring"] = self.docstring
        if self.children:
            result["children"] = [c.to_dict() for c in self.children]
        return result


class PythonSkeletonExtractor(ast.NodeVisitor):
    """Extract skeleton from Python AST."""

    def __init__(self, source: str, include_private: bool = False):
        self.source = source
        self.lines = source.splitlines()
        self.include_private = include_private
        self.symbols: list[Symbol] = []
        self._stack: list[list[Symbol]] = [self.symbols]

    def _should_include(self, name: str) -> bool:
        """Check if a name should be included based on privacy settings."""
        if self.include_private:
            return True
        return not name.startswith("_") or (name.startswith("__") and name.endswith("__"))

    def _get_signature(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> str:
        """Get function signature."""
        args = []

        # Regular args
        for i, arg in enumerate(node.args.args):
            default_offset = len(node.args.args) - len(node.args.defaults)
            arg_str = arg.arg
            if arg.annotation:
                arg_str += f": {ast.unparse(arg.annotation)}"
            if i >= default_offset:
                default = node.args.defaults[i - default_offset]
                arg_str += f" = {ast.unparse(default)}"
            args.append(arg_str)

        # *args
        if node.args.vararg:
            arg_str = f"*{node.args.vararg.arg}"
            if node.args.vararg.annotation:
                arg_str += f": {ast.unparse(node.args.vararg.annotation)}"
            args.append(arg_str)

        # **kwargs
        if node.args.kwarg:
            arg_str = f"**{node.args.kwarg.arg}"
            if node.args.kwarg.annotation:
                arg_str += f": {ast.unparse(node.args.kwarg.annotation)}"
            args.append(arg_str)

        prefix = "async def" if isinstance(node, ast.AsyncFunctionDef) else "def"
        sig = f"{prefix} {node.name}({', '.join(args)})"

        if node.returns:
            sig += f" -> {ast.unparse(node.returns)}"

        return sig

    def _get_docstring(self, node: ast.AST) -> str | None:
        """Extract docstring from a node."""
        return ast.get_docstring(node)

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        if not self._should_include(node.name):
            return

        bases = [ast.unparse(b) for b in node.bases]
        signature = f"class {node.name}"
        if bases:
            signature += f"({', '.join(bases)})"

        symbol = Symbol(
            name=node.name,
            kind="class",
            signature=signature,
            docstring=self._get_docstring(node),
            lineno=node.lineno,
            end_lineno=node.end_lineno,
            children=[],
        )

        self._stack[-1].append(symbol)
        self._stack.append(symbol.children)
        self.generic_visit(node)
        self._stack.pop()

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        if not self._should_include(node.name):
            return

        kind = "method" if len(self._stack) > 1 else "function"
        symbol = Symbol(
            name=node.name,
            kind=kind,
            signature=self._get_signature(node),
            docstring=self._get_docstring(node),
            lineno=node.lineno,
            end_lineno=node.end_lineno,
            children=[],
        )

        self._stack[-1].append(symbol)
        # Don't visit children of functions (no nested function extraction)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        if not self._should_include(node.name):
            return

        kind = "method" if len(self._stack) > 1 else "function"
        symbol = Symbol(
            name=node.name,
            kind=kind,
            signature=self._get_signature(node),
            docstring=self._get_docstring(node),
            lineno=node.lineno,
            end_lineno=node.end_lineno,
            children=[],
        )

        self._stack[-1].append(symbol)


def format_skeleton(
    symbols: list[Symbol],
    include_docstrings: bool = True,
    indent: int = 0,
) -> str:
    """Format symbols as skeleton text."""
    lines = []
    prefix = "    " * indent

    for sym in symbols:
        lines.append(f"{prefix}{sym.signature}:")

        if include_docstrings and sym.docstring:
            # Format docstring (first line only for brevity)
            first_line = sym.docstring.split("\n")[0].strip()
            if first_line:
                lines.append(f'{prefix}    """{first_line}"""')

        if sym.children:
            child_text = format_skeleton(sym.children, include_docstrings, indent + 1)
            lines.append(child_text)
        else:
            lines.append(f"{prefix}    ...")

        lines.append("")  # Blank line between symbols

    return "\n".join(lines).rstrip()


class PythonSkeletonProvider(ViewProvider):
    """Skeleton provider for Python files."""

    @property
    def view_type(self) -> ViewType:
        return ViewType.SKELETON

    @property
    def supported_languages(self) -> set[str]:
        return {"python"}

    async def render(self, target: ViewTarget, options: ViewOptions | None = None) -> View:
        """Extract and format Python skeleton."""
        opts = options or ViewOptions()
        source = target.path.read_text()

        try:
            tree = ast.parse(source)
        except SyntaxError as e:
            return View(
                target=target,
                view_type=ViewType.SKELETON,
                content=f"# Parse error: {e}",
                metadata={"error": str(e)},
            )

        extractor = PythonSkeletonExtractor(source, include_private=opts.include_private)
        extractor.visit(tree)

        content = format_skeleton(extractor.symbols, include_docstrings=opts.include_docstrings)

        return View(
            target=target,
            view_type=ViewType.SKELETON,
            content=content,
            metadata={
                "symbol_count": len(extractor.symbols),
                "symbols": [_symbol_to_dict(s) for s in extractor.symbols],
                "language": "python",
            },
        )


def _symbol_to_dict(symbol: Symbol) -> dict:
    """Convert a Symbol to a serializable dictionary."""
    result = {
        "name": symbol.name,
        "kind": symbol.kind,
        "line": symbol.lineno,
    }
    if symbol.end_lineno is not None:
        result["end_line"] = symbol.end_lineno
        result["line_count"] = symbol.line_count
    if symbol.signature:
        result["signature"] = symbol.signature
    if symbol.docstring:
        result["docstring"] = symbol.docstring
    if symbol.children:
        result["children"] = [_symbol_to_dict(c) for c in symbol.children]
    return result


def extract_python_skeleton(source: str, include_private: bool = False) -> list[Symbol]:
    """Convenience function to extract skeleton from Python source."""
    tree = ast.parse(source)
    extractor = PythonSkeletonExtractor(source, include_private)
    extractor.visit(tree)
    return extractor.symbols


def expand_symbol(source: str, symbol_name: str) -> str | None:
    """Get the full source code of a named symbol.

    Useful for getting complete enum definitions, class bodies, or function
    implementations when the skeleton isn't enough.

    Args:
        source: Python source code
        symbol_name: Name of the symbol to expand (e.g., "StepType", "my_function")

    Returns:
        Full source code of the symbol, or None if not found

    Example:
        # Get full enum definition
        content = expand_symbol(source, "StepType")
        # Returns:
        # class StepType(Enum):
        #     TOOL = auto()
        #     LLM = auto()
        #     HYBRID = auto()
    """
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return None

    lines = source.splitlines()

    for node in ast.walk(tree):
        if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            if node.name == symbol_name:
                start = node.lineno - 1
                end = node.end_lineno or node.lineno
                return "\n".join(lines[start:end])

    return None


def get_enum_values(source: str, enum_name: str) -> list[str] | None:
    """Extract enum member names from an Enum class.

    Args:
        source: Python source code
        enum_name: Name of the Enum class

    Returns:
        List of enum member names, or None if not found

    Example:
        values = get_enum_values(source, "StepType")
        # Returns: ["TOOL", "LLM", "HYBRID"]
    """
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return None

    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef) and node.name == enum_name:
            # Check if it's an Enum (inherits from Enum)
            is_enum = any(
                (isinstance(b, ast.Name) and b.id == "Enum")
                or (isinstance(b, ast.Attribute) and b.attr == "Enum")
                for b in node.bases
            )
            if not is_enum:
                return None

            # Extract enum values (assignments in class body)
            values = []
            for item in node.body:
                if isinstance(item, ast.Assign):
                    for target in item.targets:
                        if isinstance(target, ast.Name):
                            values.append(target.id)
                elif isinstance(item, ast.AnnAssign) and isinstance(item.target, ast.Name):
                    values.append(item.target.id)
            return values

    return None


# =============================================================================
# Plugin Wrapper
# =============================================================================


class PythonSkeletonPlugin:
    """Plugin wrapper for PythonSkeletonProvider.

    This wraps the ViewProvider implementation as a ViewPlugin for use
    with the plugin registry.
    """

    def __init__(self) -> None:
        self._provider = PythonSkeletonProvider()

    @property
    def metadata(self) -> PluginMetadata:
        from moss.plugins import PluginMetadata

        return PluginMetadata(
            name="python-skeleton",
            view_type="skeleton",
            languages=frozenset(["python"]),
            priority=5,
            version="0.1.0",
            description="Python skeleton extraction via AST",
        )

    def supports(self, target: ViewTarget) -> bool:
        """Check if this plugin can handle the target."""
        return self._provider.supports(target)

    async def render(
        self,
        target: ViewTarget,
        options: ViewOptions | None = None,
    ) -> View:
        """Render a skeleton view for the target."""
        return await self._provider.render(target, options)
