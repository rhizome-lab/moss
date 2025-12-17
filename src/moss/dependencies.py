"""Dependency Graph Provider: Import/export relationship extraction."""

from __future__ import annotations

import ast
from dataclasses import dataclass, field

from moss.views import View, ViewOptions, ViewProvider, ViewTarget, ViewType


@dataclass
class Import:
    """An import statement."""

    module: str  # The module being imported
    names: list[str]  # Names imported (empty for 'import x')
    alias: str | None  # Alias if 'as X' used
    lineno: int
    is_relative: bool  # True for relative imports (from . import)
    level: int  # Number of dots for relative imports


@dataclass
class Export:
    """An exported symbol."""

    name: str
    kind: str  # function, class, variable
    lineno: int


@dataclass
class DependencyInfo:
    """Extracted dependency information for a file."""

    imports: list[Import] = field(default_factory=list)
    exports: list[Export] = field(default_factory=list)
    all_exports: list[str] | None = None  # __all__ if defined


class PythonDependencyExtractor(ast.NodeVisitor):
    """Extract dependencies from Python AST."""

    def __init__(self):
        self.imports: list[Import] = []
        self.exports: list[Export] = []
        self.all_exports: list[str] | None = None
        self._in_class = False

    def visit_Import(self, node: ast.Import) -> None:
        for alias in node.names:
            self.imports.append(
                Import(
                    module=alias.name,
                    names=[],
                    alias=alias.asname,
                    lineno=node.lineno,
                    is_relative=False,
                    level=0,
                )
            )

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        module = node.module or ""
        names = [alias.name for alias in node.names]
        aliases = {alias.name: alias.asname for alias in node.names if alias.asname}

        self.imports.append(
            Import(
                module=module,
                names=names,
                alias=aliases.get(names[0]) if len(names) == 1 else None,
                lineno=node.lineno,
                is_relative=node.level > 0,
                level=node.level,
            )
        )

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        if not self._in_class and not node.name.startswith("_"):
            self.exports.append(Export(name=node.name, kind="class", lineno=node.lineno))
        old_in_class = self._in_class
        self._in_class = True
        self.generic_visit(node)
        self._in_class = old_in_class

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        if not self._in_class and not node.name.startswith("_"):
            self.exports.append(Export(name=node.name, kind="function", lineno=node.lineno))

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        if not self._in_class and not node.name.startswith("_"):
            self.exports.append(Export(name=node.name, kind="function", lineno=node.lineno))

    def visit_Assign(self, node: ast.Assign) -> None:
        # Check for __all__ definition
        for target in node.targets:
            if isinstance(target, ast.Name) and target.id == "__all__":
                if isinstance(node.value, ast.List | ast.Tuple):
                    self.all_exports = []
                    for elt in node.value.elts:
                        if isinstance(elt, ast.Constant) and isinstance(elt.value, str):
                            self.all_exports.append(elt.value)

        # Track module-level variable assignments
        if not self._in_class:
            for target in node.targets:
                if isinstance(target, ast.Name) and not target.id.startswith("_"):
                    self.exports.append(Export(name=target.id, kind="variable", lineno=node.lineno))


def extract_dependencies(source: str) -> DependencyInfo:
    """Extract dependencies from Python source."""
    tree = ast.parse(source)
    extractor = PythonDependencyExtractor()
    extractor.visit(tree)
    return DependencyInfo(
        imports=extractor.imports,
        exports=extractor.exports,
        all_exports=extractor.all_exports,
    )


@dataclass
class ReverseDependency:
    """A file that imports a target module."""

    file: str
    import_line: int
    import_type: str  # "import" or "from"
    names: list[str]  # Names imported (for "from X import Y")


def find_reverse_dependencies(
    target_module: str,
    search_path: str,
    pattern: str = "**/*.py",
) -> list[ReverseDependency]:
    """Find all files that import a target module.

    Args:
        target_module: Module name to search for (e.g., "moss.skeleton")
        search_path: Directory to search in
        pattern: Glob pattern for files to search

    Returns:
        List of ReverseDependency showing files that import the target
    """
    from pathlib import Path

    results: list[ReverseDependency] = []
    search_dir = Path(search_path)

    if not search_dir.exists():
        return results

    for file_path in search_dir.glob(pattern):
        try:
            source = file_path.read_text()
            deps = extract_dependencies(source)

            for imp in deps.imports:
                # Check if this import matches the target module
                # Handle both "import X" and "from X import Y"
                module = imp.module

                # Exact match
                if module == target_module:
                    results.append(
                        ReverseDependency(
                            file=str(file_path),
                            import_line=imp.lineno,
                            import_type="from" if imp.names else "import",
                            names=imp.names,
                        )
                    )
                # Prefix match (e.g., target="moss" matches "moss.skeleton")
                elif module.startswith(target_module + "."):
                    results.append(
                        ReverseDependency(
                            file=str(file_path),
                            import_line=imp.lineno,
                            import_type="from" if imp.names else "import",
                            names=imp.names,
                        )
                    )
                # Check if target is imported as a name (from X import target)
                elif target_module in imp.names:
                    results.append(
                        ReverseDependency(
                            file=str(file_path),
                            import_line=imp.lineno,
                            import_type="from",
                            names=[target_module],
                        )
                    )

        except (SyntaxError, OSError):
            continue

    return results


def format_dependencies(info: DependencyInfo, include_exports: bool = True) -> str:
    """Format dependency info as text."""
    lines = []

    if info.imports:
        lines.append("# Imports")
        for imp in info.imports:
            if imp.is_relative:
                prefix = "." * imp.level
                if imp.module:
                    prefix += imp.module
            else:
                prefix = imp.module

            if imp.names:
                names_str = ", ".join(imp.names)
                lines.append(f"from {prefix} import {names_str}")
            else:
                alias_str = f" as {imp.alias}" if imp.alias else ""
                lines.append(f"import {prefix}{alias_str}")
        lines.append("")

    if include_exports and info.exports:
        lines.append("# Exports")
        if info.all_exports is not None:
            lines.append(f"__all__ = {info.all_exports!r}")
        for exp in info.exports:
            if exp.kind == "variable":
                continue  # Skip variables in compact view
            lines.append(f"{exp.kind}: {exp.name}")

    return "\n".join(lines).strip()


class PythonDependencyProvider(ViewProvider):
    """Dependency graph provider for Python files."""

    @property
    def view_type(self) -> ViewType:
        return ViewType.DEPENDENCY

    @property
    def supported_languages(self) -> set[str]:
        return {"python"}

    async def render(self, target: ViewTarget, options: ViewOptions | None = None) -> View:
        """Extract and format Python dependencies."""
        source = target.path.read_text()

        try:
            info = extract_dependencies(source)
        except SyntaxError as e:
            return View(
                target=target,
                view_type=ViewType.DEPENDENCY,
                content=f"# Parse error: {e}",
                metadata={"error": str(e)},
            )

        content = format_dependencies(info)

        return View(
            target=target,
            view_type=ViewType.DEPENDENCY,
            content=content,
            metadata={
                "imports": len(info.imports),
                "exports": len(info.exports),
                "language": "python",
            },
        )
