"""Elided Literals view provider.

This module provides views with literal values (strings, numbers, lists, dicts)
replaced with placeholders. This is useful for:
- Focusing on code structure without data clutter
- Reducing token count for LLM processing
- Highlighting the "shape" of code

Usage:
    from moss.elided_literals import elide_literals, ElidedLiteralsProvider

    # Elide literals in source code
    elided, stats = elide_literals(source)

    # Or use the view provider
    from pathlib import Path
    provider = ElidedLiteralsProvider()
    view = provider.provide(Path("myfile.py"))
"""

from __future__ import annotations

import ast
import re
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .views import View, ViewOptions


@dataclass
class ElisionStats:
    """Statistics about literal elision."""

    strings: int = 0
    numbers: int = 0
    lists: int = 0
    dicts: int = 0
    sets: int = 0
    tuples: int = 0
    f_strings: int = 0
    bytes_literals: int = 0

    @property
    def total(self) -> int:
        """Total number of elisions."""
        return (
            self.strings
            + self.numbers
            + self.lists
            + self.dicts
            + self.sets
            + self.tuples
            + self.f_strings
            + self.bytes_literals
        )


@dataclass
class ElisionConfig:
    """Configuration for literal elision."""

    elide_strings: bool = True
    elide_numbers: bool = True
    elide_lists: bool = True
    elide_dicts: bool = True
    elide_sets: bool = True
    elide_tuples: bool = True
    elide_f_strings: bool = True
    elide_bytes: bool = True

    # Preserve certain patterns
    preserve_empty_strings: bool = True
    preserve_single_char_strings: bool = True
    preserve_small_ints: bool = True  # -10 to 10
    preserve_zero_one: bool = True
    preserve_docstrings: bool = True
    preserve_type_annotations: bool = True

    # Placeholder format
    string_placeholder: str = '"..."'
    number_placeholder: str = "..."
    list_placeholder: str = "[...]"
    dict_placeholder: str = "{...}"
    set_placeholder: str = "{...}"
    tuple_placeholder: str = "(...)"
    f_string_placeholder: str = 'f"..."'
    bytes_placeholder: str = 'b"..."'


class LiteralElider(ast.NodeTransformer):
    """AST transformer that replaces literals with placeholders."""

    def __init__(self, config: ElisionConfig | None = None) -> None:
        self.config = config or ElisionConfig()
        self.stats = ElisionStats()
        self._in_annotation = False
        self._is_docstring_position = False

    def visit_Module(self, node: ast.Module) -> ast.Module:
        """Visit module, handling docstrings."""
        if node.body and isinstance(node.body[0], ast.Expr):
            if isinstance(node.body[0].value, ast.Constant) and isinstance(
                node.body[0].value.value, str
            ):
                self._is_docstring_position = True
                self.visit(node.body[0])
                self._is_docstring_position = False
                for child in node.body[1:]:
                    self.visit(child)
                return node

        self.generic_visit(node)
        return node

    def visit_FunctionDef(self, node: ast.FunctionDef) -> ast.FunctionDef:
        """Visit function definition, preserving docstrings."""
        # Handle docstring
        if node.body and isinstance(node.body[0], ast.Expr):
            if isinstance(node.body[0].value, ast.Constant) and isinstance(
                node.body[0].value.value, str
            ):
                self._is_docstring_position = True
                self.visit(node.body[0])
                self._is_docstring_position = False
                for child in node.body[1:]:
                    self.visit(child)
                # Visit other parts
                for child in node.decorator_list:
                    self.visit(child)
                if node.returns:
                    self._in_annotation = True
                    self.visit(node.returns)
                    self._in_annotation = False
                self.visit(node.args)
                return node

        self.generic_visit(node)
        return node

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> ast.AsyncFunctionDef:
        """Visit async function definition."""
        # Same as FunctionDef
        if node.body and isinstance(node.body[0], ast.Expr):
            if isinstance(node.body[0].value, ast.Constant) and isinstance(
                node.body[0].value.value, str
            ):
                self._is_docstring_position = True
                self.visit(node.body[0])
                self._is_docstring_position = False
                for child in node.body[1:]:
                    self.visit(child)
                for child in node.decorator_list:
                    self.visit(child)
                if node.returns:
                    self._in_annotation = True
                    self.visit(node.returns)
                    self._in_annotation = False
                self.visit(node.args)
                return node

        self.generic_visit(node)
        return node

    def visit_ClassDef(self, node: ast.ClassDef) -> ast.ClassDef:
        """Visit class definition, preserving docstrings."""
        if node.body and isinstance(node.body[0], ast.Expr):
            if isinstance(node.body[0].value, ast.Constant) and isinstance(
                node.body[0].value.value, str
            ):
                self._is_docstring_position = True
                self.visit(node.body[0])
                self._is_docstring_position = False
                for child in node.body[1:]:
                    self.visit(child)
                for child in node.decorator_list:
                    self.visit(child)
                for child in node.bases:
                    self.visit(child)
                for child in node.keywords:
                    self.visit(child)
                return node

        self.generic_visit(node)
        return node

    def visit_AnnAssign(self, node: ast.AnnAssign) -> ast.AnnAssign:
        """Visit annotated assignment, preserving type annotations."""
        if self.config.preserve_type_annotations:
            self._in_annotation = True
            self.visit(node.annotation)
            self._in_annotation = False
            if node.value:
                self.visit(node.value)
            return node
        self.generic_visit(node)
        return node

    def visit_arg(self, node: ast.arg) -> ast.arg:
        """Visit function argument, preserving type annotations."""
        if node.annotation and self.config.preserve_type_annotations:
            self._in_annotation = True
            self.visit(node.annotation)
            self._in_annotation = False
        return node

    def visit_Constant(self, node: ast.Constant) -> ast.Constant:
        """Visit constant literal."""
        if self._in_annotation and self.config.preserve_type_annotations:
            return node

        if self._is_docstring_position and self.config.preserve_docstrings:
            return node

        value = node.value

        # String
        if isinstance(value, str):
            if not self.config.elide_strings:
                return node
            if self.config.preserve_empty_strings and value == "":
                return node
            if self.config.preserve_single_char_strings and len(value) == 1:
                return node
            self.stats.strings += 1
            return ast.Constant(value="...")

        # Bytes
        if isinstance(value, bytes):
            if not self.config.elide_bytes:
                return node
            self.stats.bytes_literals += 1
            return ast.Constant(value=b"...")

        # Numbers
        if isinstance(value, (int, float, complex)):
            if not self.config.elide_numbers:
                return node
            if self.config.preserve_zero_one and value in (0, 1, 0.0, 1.0):
                return node
            if self.config.preserve_small_ints and isinstance(value, int) and -10 <= value <= 10:
                return node
            self.stats.numbers += 1
            return ast.Constant(value=0)  # Placeholder number

        return node

    def visit_JoinedStr(self, node: ast.JoinedStr) -> ast.JoinedStr | ast.Constant:
        """Visit f-string."""
        if not self.config.elide_f_strings:
            self.generic_visit(node)
            return node

        self.stats.f_strings += 1
        # Replace with simple string constant
        return ast.Constant(value="...")

    def visit_List(self, node: ast.List) -> ast.List:
        """Visit list literal."""
        if not self.config.elide_lists:
            self.generic_visit(node)
            return node

        if len(node.elts) == 0:
            return node  # Preserve empty lists

        self.stats.lists += 1
        # Keep first element as hint
        if node.elts:
            first = self.visit(node.elts[0])
            return ast.List(elts=[first, ast.Constant(value="...")], ctx=node.ctx)
        return ast.List(elts=[ast.Constant(value="...")], ctx=node.ctx)

    def visit_Dict(self, node: ast.Dict) -> ast.Dict:
        """Visit dict literal."""
        if not self.config.elide_dicts:
            self.generic_visit(node)
            return node

        if len(node.keys) == 0:
            return node  # Preserve empty dicts

        self.stats.dicts += 1
        # Keep first key-value as hint
        if node.keys and node.keys[0] is not None:
            key = self.visit(node.keys[0])
            value = self.visit(node.values[0])
            ellipsis = ast.Constant(value="...")
            return ast.Dict(keys=[key, ellipsis], values=[value, ellipsis])
        ellipsis = ast.Constant(value="...")
        return ast.Dict(keys=[ellipsis], values=[ellipsis])

    def visit_Set(self, node: ast.Set) -> ast.Set:
        """Visit set literal."""
        if not self.config.elide_sets:
            self.generic_visit(node)
            return node

        self.stats.sets += 1
        if node.elts:
            first = self.visit(node.elts[0])
            return ast.Set(elts=[first, ast.Constant(value="...")])
        return ast.Set(elts=[ast.Constant(value="...")])

    def visit_Tuple(self, node: ast.Tuple) -> ast.Tuple:
        """Visit tuple literal."""
        if not self.config.elide_tuples:
            self.generic_visit(node)
            return node

        if len(node.elts) == 0:
            return node  # Preserve empty tuples

        # Only elide long tuples
        if len(node.elts) <= 3:
            self.generic_visit(node)
            return node

        self.stats.tuples += 1
        first = self.visit(node.elts[0])
        return ast.Tuple(elts=[first, ast.Constant(value="...")], ctx=node.ctx)


def elide_literals(
    source: str,
    config: ElisionConfig | None = None,
) -> tuple[str, ElisionStats]:
    """Elide literals in Python source code.

    Args:
        source: Python source code
        config: Elision configuration

    Returns:
        Tuple of (elided source code, statistics)
    """
    try:
        tree = ast.parse(source)
    except SyntaxError:
        # If parsing fails, return original
        return source, ElisionStats()

    elider = LiteralElider(config)
    elided_tree = elider.visit(tree)
    ast.fix_missing_locations(elided_tree)

    try:
        elided_source = ast.unparse(elided_tree)
    except ValueError:
        return source, elider.stats

    return elided_source, elider.stats


def elide_literals_regex(source: str) -> str:
    """Quick regex-based literal elision (no AST parsing).

    This is faster but less accurate than AST-based elision.
    Useful for non-Python files or quick processing.

    Args:
        source: Source code

    Returns:
        Source with literals replaced
    """
    # Replace quoted strings
    source = re.sub(r'""".*?"""', '"""..."""', source, flags=re.DOTALL)
    source = re.sub(r"'''.*?'''", "'''...'''", source, flags=re.DOTALL)
    source = re.sub(r'"[^"\\]*(?:\\.[^"\\]*)*"', '"..."', source)
    source = re.sub(r"'[^'\\]*(?:\\.[^'\\]*)*'", "'...'", source)

    # Replace numbers (but not in identifiers)
    source = re.sub(r"\b\d+\.?\d*\b", "...", source)

    return source


class ElidedLiteralsProvider:
    """View provider that generates elided literals views.

    Note: This is a legacy interface. For the plugin-based architecture,
    use the appropriate plugin instead.
    """

    def __init__(self, config: ElisionConfig | None = None) -> None:
        self.config = config or ElisionConfig()

    @property
    def name(self) -> str:
        return "elided"

    def provide(self, path: Path, options: ViewOptions | None = None) -> View:
        """Provide an elided literals view.

        Args:
            path: Path to the source file
            options: View options (unused, for API compatibility)

        Returns:
            View with literals elided
        """
        from .views import View, ViewTarget, ViewType

        content = path.read_text()

        elided_content, stats = elide_literals(content, self.config)

        return View(
            target=ViewTarget(path=path),
            view_type=ViewType.ELIDED,
            content=elided_content,
            metadata={
                "elisions": stats.total,
                "strings_elided": stats.strings,
                "numbers_elided": stats.numbers,
                "lists_elided": stats.lists,
                "dicts_elided": stats.dicts,
            },
        )
