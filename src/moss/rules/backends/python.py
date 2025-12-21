"""Python backend for arbitrary custom checks.

This is the "escape hatch" backend - when you need to do something
that regex or ast-grep can't handle, use pure Python.

Rules using this backend don't match patterns - they receive the
full source and AST and can do arbitrary analysis:

    @rule(backend="python")
    def check_docstrings(ctx: RuleContext) -> list[Violation]:
        '''Check that all public functions have docstrings.'''
        violations = []
        result = ctx.backend("python")

        for func in result.metadata.get("functions", []):
            if not func["docstring"] and not func["name"].startswith("_"):
                violations.append(ctx.violation(
                    f"Missing docstring for public function: {func['name']}",
                    ctx.location(func["line"]),
                ))

        return violations

The python backend provides rich metadata:
- ast: The parsed AST
- functions: List of function info
- classes: List of class info
- imports: List of import info
"""

from __future__ import annotations

import ast
from pathlib import Path
from typing import Any

from ..base import BackendResult, BaseBackend
from . import register_backend


@register_backend
class PythonBackend(BaseBackend):
    """Python AST analysis backend.

    Provides structured access to Python code via the ast module.
    Use this when you need custom analysis logic.
    """

    @property
    def name(self) -> str:
        return "python"

    def analyze(
        self,
        file_path: Path,
        pattern: str | None = None,
        **options: Any,
    ) -> BackendResult:
        """Analyze a Python file and extract structural information.

        Args:
            file_path: File to analyze
            pattern: Ignored for python backend
            **options:
                - include_bodies: bool = False (include function bodies)

        Returns:
            BackendResult with metadata containing:
                - ast: The parsed AST
                - source: Raw source code
                - functions: List of function info
                - classes: List of class info
                - imports: List of import info
        """
        try:
            source = file_path.read_text()
        except (OSError, UnicodeDecodeError) as e:
            return BackendResult(
                backend_name=self.name,
                errors=[f"Could not read file: {e}"],
            )

        try:
            tree = ast.parse(source, filename=str(file_path))
        except SyntaxError as e:
            return BackendResult(
                backend_name=self.name,
                errors=[f"Syntax error: {e}"],
            )

        include_bodies = options.get("include_bodies", False)

        metadata = {
            "ast": tree,
            "source": source,
            "functions": self._extract_functions(tree, source, include_bodies),
            "classes": self._extract_classes(tree, source, include_bodies),
            "imports": self._extract_imports(tree),
            "file_path": file_path,
        }

        return BackendResult(
            backend_name=self.name,
            matches=[],  # Python backend doesn't produce matches
            metadata=metadata,
        )

    def _extract_functions(
        self, tree: ast.AST, source: str, include_bodies: bool
    ) -> list[dict[str, Any]]:
        """Extract function information from AST."""
        functions: list[dict[str, Any]] = []
        lines = source.splitlines()

        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                func_info: dict[str, Any] = {
                    "name": node.name,
                    "line": node.lineno,
                    "end_line": node.end_lineno,
                    "column": node.col_offset,
                    "is_async": isinstance(node, ast.AsyncFunctionDef),
                    "decorators": [self._get_decorator_name(d) for d in node.decorator_list],
                    "docstring": ast.get_docstring(node),
                    "args": self._extract_args(node.args),
                    "returns": self._get_annotation(node.returns),
                }

                if include_bodies and node.end_lineno:
                    func_info["body"] = "\n".join(lines[node.lineno - 1 : node.end_lineno])

                functions.append(func_info)

        return functions

    def _extract_classes(
        self, tree: ast.AST, source: str, include_bodies: bool
    ) -> list[dict[str, Any]]:
        """Extract class information from AST."""
        classes: list[dict[str, Any]] = []
        lines = source.splitlines()

        for node in ast.walk(tree):
            if isinstance(node, ast.ClassDef):
                # Get methods
                methods = []
                for item in node.body:
                    if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        methods.append(
                            {
                                "name": item.name,
                                "line": item.lineno,
                                "is_async": isinstance(item, ast.AsyncFunctionDef),
                            }
                        )

                class_info: dict[str, Any] = {
                    "name": node.name,
                    "line": node.lineno,
                    "end_line": node.end_lineno,
                    "column": node.col_offset,
                    "decorators": [self._get_decorator_name(d) for d in node.decorator_list],
                    "bases": [self._get_base_name(b) for b in node.bases],
                    "docstring": ast.get_docstring(node),
                    "methods": methods,
                }

                if include_bodies and node.end_lineno:
                    class_info["body"] = "\n".join(lines[node.lineno - 1 : node.end_lineno])

                classes.append(class_info)

        return classes

    def _extract_imports(self, tree: ast.AST) -> list[dict[str, Any]]:
        """Extract import information from AST."""
        imports: list[dict[str, Any]] = []

        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    imports.append(
                        {
                            "type": "import",
                            "module": alias.name,
                            "alias": alias.asname,
                            "line": node.lineno,
                        }
                    )
            elif isinstance(node, ast.ImportFrom):
                for alias in node.names:
                    imports.append(
                        {
                            "type": "from",
                            "module": node.module or "",
                            "name": alias.name,
                            "alias": alias.asname,
                            "line": node.lineno,
                            "level": node.level,
                        }
                    )

        return imports

    def _extract_args(self, args: ast.arguments) -> dict[str, Any]:
        """Extract function argument information."""
        return {
            "args": [
                {
                    "name": arg.arg,
                    "annotation": self._get_annotation(arg.annotation),
                }
                for arg in args.args
            ],
            "vararg": args.vararg.arg if args.vararg else None,
            "kwarg": args.kwarg.arg if args.kwarg else None,
            "kwonly": [arg.arg for arg in args.kwonlyargs],
            "defaults_count": len(args.defaults),
        }

    def _get_annotation(self, node: ast.expr | None) -> str | None:
        """Get string representation of a type annotation."""
        if node is None:
            return None
        try:
            return ast.unparse(node)
        except ValueError:
            return None

    def _get_decorator_name(self, node: ast.expr) -> str:
        """Get string representation of a decorator."""
        try:
            return ast.unparse(node)
        except ValueError:
            if isinstance(node, ast.Name):
                return node.id
            elif isinstance(node, ast.Attribute):
                return f"...{node.attr}"
            return "?"

    def _get_base_name(self, node: ast.expr) -> str:
        """Get string representation of a base class."""
        try:
            return ast.unparse(node)
        except ValueError:
            if isinstance(node, ast.Name):
                return node.id
            return "?"

    def supports_pattern(self, pattern: str) -> bool:
        """Python backend doesn't use patterns."""
        return True
