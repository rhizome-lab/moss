"""Multi-file refactoring support.

This module provides tools for coordinated refactoring across multiple files,
including symbol renaming, code moves, and import updates.

Usage:
    from moss_orchestration.refactoring import Refactorer, RenameRefactoring

    refactorer = Refactorer(workspace)
    refactoring = RenameRefactoring(
        old_name="old_func",
        new_name="new_func",
        scope=RefactoringScope.WORKSPACE,
    )
    result = await refactorer.apply(refactoring)
"""

from __future__ import annotations

import ast
import re
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    pass


# =============================================================================
# Configuration & Types
# =============================================================================


class RefactoringScope(Enum):
    """Scope of refactoring operation."""

    FILE = "file"  # Single file
    DIRECTORY = "directory"  # Directory and subdirectories
    WORKSPACE = "workspace"  # Entire workspace


class RefactoringKind(Enum):
    """Kind of refactoring operation."""

    RENAME = "rename"
    MOVE = "move"
    EXTRACT = "extract"
    INLINE = "inline"


@dataclass
class FileChange:
    """A change to apply to a single file."""

    path: Path
    original_content: str
    new_content: str
    description: str = ""

    @property
    def has_changes(self) -> bool:
        """Check if there are actual changes."""
        return self.original_content != self.new_content

    def to_diff(self) -> str:
        """Generate unified diff of changes."""
        import difflib

        original_lines = self.original_content.splitlines(keepends=True)
        new_lines = self.new_content.splitlines(keepends=True)

        diff = difflib.unified_diff(
            original_lines,
            new_lines,
            fromfile=f"a/{self.path}",
            tofile=f"b/{self.path}",
        )
        return "".join(diff)


@dataclass
class RefactoringResult:
    """Result of a refactoring operation."""

    success: bool
    changes: list[FileChange] = field(default_factory=list)
    affected_files: list[Path] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    @property
    def total_changes(self) -> int:
        """Total number of files with changes."""
        return sum(1 for c in self.changes if c.has_changes)


# =============================================================================
# Refactoring Operations
# =============================================================================


@dataclass
class Refactoring(ABC):
    """Base class for refactoring operations."""

    scope: RefactoringScope = RefactoringScope.FILE
    file_patterns: list[str] = field(default_factory=lambda: ["**/*.py"])

    @property
    @abstractmethod
    def kind(self) -> RefactoringKind:
        """The kind of refactoring."""
        ...

    @abstractmethod
    def apply_to_file(self, path: Path, content: str) -> str | None:
        """Apply refactoring to a single file.

        Args:
            path: Path to the file
            content: File content

        Returns:
            New content if changed, None otherwise
        """
        ...


@dataclass
class RenameRefactoring(Refactoring):
    """Rename a symbol across files."""

    old_name: str = ""
    new_name: str = ""
    symbol_type: str | None = None  # class, function, variable, module

    @property
    def kind(self) -> RefactoringKind:
        return RefactoringKind.RENAME

    def apply_to_file(self, path: Path, content: str) -> str | None:
        """Rename occurrences in file."""
        if not self.old_name or not self.new_name:
            return None

        # Use AST to find and rename symbols
        try:
            tree = ast.parse(content)
            transformer = _RenameTransformer(self.old_name, self.new_name, self.symbol_type)
            new_tree = transformer.visit(tree)

            if transformer.changed:
                return ast.unparse(new_tree)
        except SyntaxError:
            # Fall back to text-based replacement for non-Python files
            pass

        # Text-based fallback
        pattern = rf"\b{re.escape(self.old_name)}\b"
        new_content = re.sub(pattern, self.new_name, content)

        return new_content if new_content != content else None


class _RenameTransformer(ast.NodeTransformer):
    """AST transformer for renaming symbols."""

    def __init__(self, old_name: str, new_name: str, symbol_type: str | None = None):
        self.old_name = old_name
        self.new_name = new_name
        self.symbol_type = symbol_type
        self.changed = False

    def visit_Name(self, node: ast.Name) -> ast.Name:
        if node.id == self.old_name:
            self.changed = True
            return ast.Name(id=self.new_name, ctx=node.ctx)
        return node

    def visit_FunctionDef(self, node: ast.FunctionDef) -> ast.FunctionDef:
        self.generic_visit(node)
        if node.name == self.old_name and self.symbol_type in (None, "function"):
            self.changed = True
            node.name = self.new_name
        return node

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> ast.AsyncFunctionDef:
        self.generic_visit(node)
        if node.name == self.old_name and self.symbol_type in (None, "function"):
            self.changed = True
            node.name = self.new_name
        return node

    def visit_ClassDef(self, node: ast.ClassDef) -> ast.ClassDef:
        self.generic_visit(node)
        if node.name == self.old_name and self.symbol_type in (None, "class"):
            self.changed = True
            node.name = self.new_name
        return node

    def visit_alias(self, node: ast.alias) -> ast.alias:
        if node.name == self.old_name:
            self.changed = True
            node.name = self.new_name
        if node.asname == self.old_name:
            self.changed = True
            node.asname = self.new_name
        return node


@dataclass
class MoveRefactoring(Refactoring):
    """Move a symbol to a different file."""

    source_file: Path | None = None
    target_file: Path | None = None
    symbol_name: str = ""
    update_imports: bool = True

    @property
    def kind(self) -> RefactoringKind:
        return RefactoringKind.MOVE

    def apply_to_file(self, path: Path, content: str) -> str | None:
        """Update imports in files when a symbol moves."""
        if not self.source_file or not self.target_file or not self.symbol_name:
            return None

        # Get module names from paths
        source_module = _path_to_module(self.source_file)
        target_module = _path_to_module(self.target_file)

        if not source_module or not target_module:
            return None

        # Update imports
        try:
            tree = ast.parse(content)
            transformer = _ImportUpdater(self.symbol_name, source_module, target_module)
            new_tree = transformer.visit(tree)

            if transformer.changed:
                return ast.unparse(new_tree)
        except SyntaxError:
            pass

        return None


class _ImportUpdater(ast.NodeTransformer):
    """AST transformer for updating imports."""

    def __init__(self, symbol: str, old_module: str, new_module: str):
        self.symbol = symbol
        self.old_module = old_module
        self.new_module = new_module
        self.changed = False

    def visit_ImportFrom(self, node: ast.ImportFrom) -> ast.ImportFrom:
        if node.module == self.old_module:
            for alias in node.names:
                if alias.name == self.symbol:
                    self.changed = True
                    node.module = self.new_module
        return node


@dataclass
class ExtractRefactoring(Refactoring):
    """Extract code to a new function or method."""

    start_line: int = 0
    end_line: int = 0
    new_name: str = ""
    extract_to: str = "function"  # function, method, class

    @property
    def kind(self) -> RefactoringKind:
        return RefactoringKind.EXTRACT

    def apply_to_file(self, path: Path, content: str) -> str | None:
        """Extract selected code to a new function."""
        if not self.start_line or not self.end_line or not self.new_name:
            return None

        lines = content.splitlines(keepends=True)
        if self.start_line < 1 or self.end_line > len(lines):
            return None

        # Extract selected lines
        selected = lines[self.start_line - 1 : self.end_line]
        extracted_code = "".join(selected)

        # Analyze selected code for variables
        try:
            used_vars = _analyze_used_variables(extracted_code)
            returned_vars = _analyze_assigned_variables(extracted_code)
        except SyntaxError:
            return None

        # Build new function
        params = ", ".join(used_vars)
        returns = ", ".join(returned_vars) if returned_vars else "None"

        indent = _get_indent(selected[0]) if selected else ""
        new_func = f"\n{indent}def {self.new_name}({params}):\n"
        for line in selected:
            new_func += f"{indent}    {line.lstrip()}"
        new_func += f"\n{indent}    return {returns}\n"

        # Replace selected code with call
        call_args = ", ".join(used_vars)
        if returned_vars:
            assignment = ", ".join(returned_vars) + " = "
        else:
            assignment = ""
        replacement = f"{indent}{assignment}{self.new_name}({call_args})\n"

        # Build new content
        new_lines = [
            *lines[: self.start_line - 1],
            replacement,
            *lines[self.end_line :],
        ]
        new_content = "".join(new_lines) + new_func

        return new_content


def _analyze_used_variables(code: str) -> list[str]:
    """Analyze code to find used but not defined variables."""
    try:
        tree = ast.parse(code)
    except SyntaxError:
        return []

    assigned = set()
    used = set()

    for node in ast.walk(tree):
        if isinstance(node, ast.Name):
            if isinstance(node.ctx, ast.Store):
                assigned.add(node.id)
            elif isinstance(node.ctx, ast.Load):
                used.add(node.id)

    # Variables used but not assigned within the code
    return sorted(used - assigned)


def _analyze_assigned_variables(code: str) -> list[str]:
    """Analyze code to find assigned variables."""
    try:
        tree = ast.parse(code)
    except SyntaxError:
        return []

    assigned = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Name) and isinstance(node.ctx, ast.Store):
            assigned.add(node.id)

    return sorted(assigned)


def _get_indent(line: str) -> str:
    """Get leading whitespace from a line."""
    return line[: len(line) - len(line.lstrip())]


def _path_to_module(path: Path) -> str | None:
    """Convert a file path to a module name."""
    if not path.suffix == ".py":
        return None

    # Remove .py extension and convert / to .
    module = str(path.with_suffix("")).replace("/", ".").replace("\\", ".")

    # Remove leading . if present
    return module.lstrip(".")


# =============================================================================
# Base Class
# =============================================================================

# Default patterns to exclude from workspace operations
DEFAULT_EXCLUDE_PATTERNS = [
    "**/node_modules/**",
    "**/.git/**",
    "**/__pycache__/**",
    "**/venv/**",
    "**/.venv/**",
]


class WorkspaceRunner:
    """Base class for running operations across a workspace."""

    def __init__(self, workspace: Path, exclude_patterns: list[str] | None = None):
        self.workspace = Path(workspace).resolve()
        self.exclude_patterns = exclude_patterns or DEFAULT_EXCLUDE_PATTERNS

    def _is_excluded(self, path: Path) -> bool:
        """Check if path matches exclusion patterns."""
        import fnmatch

        path_str = str(path)
        for pattern in self.exclude_patterns:
            if fnmatch.fnmatch(path_str, pattern):
                return True
        return False


# =============================================================================
# Refactorer
# =============================================================================


class Refactorer(WorkspaceRunner):
    """Applies refactoring operations across multiple files."""

    async def apply(self, refactoring: Refactoring, dry_run: bool = False) -> RefactoringResult:
        """Apply a refactoring operation.

        Args:
            refactoring: The refactoring to apply
            dry_run: If True, don't write changes to disk

        Returns:
            RefactoringResult with changes and status
        """
        result = RefactoringResult(success=True)

        # Find files to process
        files = self._find_files(refactoring.scope, refactoring.file_patterns)

        for path in files:
            try:
                content = path.read_text()
                new_content = refactoring.apply_to_file(path, content)

                if new_content is not None and new_content != content:
                    change = FileChange(
                        path=path,
                        original_content=content,
                        new_content=new_content,
                        description=f"{refactoring.kind.value} in {path.name}",
                    )
                    result.changes.append(change)
                    result.affected_files.append(path)

                    if not dry_run:
                        path.write_text(new_content)

            except (OSError, SyntaxError) as e:
                result.errors.append(f"Error processing {path}: {e}")

        if result.errors:
            result.success = False

        return result

    def _find_files(self, scope: RefactoringScope, patterns: list[str]) -> list[Path]:
        """Find files matching scope and patterns."""
        if scope == RefactoringScope.FILE:
            return []

        files = []
        for pattern in patterns:
            for path in self.workspace.glob(pattern):
                if path.is_file() and not self._is_excluded(path):
                    files.append(path)

        return files

    def preview(self, refactoring: Refactoring) -> RefactoringResult:
        """Preview changes without applying them."""
        import asyncio

        return asyncio.run(self.apply(refactoring, dry_run=True))

    def generate_diff(self, result: RefactoringResult) -> str:
        """Generate combined diff for all changes."""
        diffs = []
        for change in result.changes:
            if change.has_changes:
                diffs.append(change.to_diff())
        return "\n".join(diffs)


# =============================================================================
# Convenience Functions
# =============================================================================


async def rename_symbol(
    workspace: Path,
    old_name: str,
    new_name: str,
    symbol_type: str | None = None,
    scope: RefactoringScope = RefactoringScope.WORKSPACE,
    dry_run: bool = False,
) -> RefactoringResult:
    """Rename a symbol across the workspace.

    Args:
        workspace: Workspace root directory
        old_name: Current symbol name
        new_name: New symbol name
        symbol_type: Type of symbol (class, function, variable)
        scope: Refactoring scope
        dry_run: If True, don't write changes

    Returns:
        RefactoringResult
    """
    refactorer = Refactorer(workspace)
    refactoring = RenameRefactoring(
        old_name=old_name,
        new_name=new_name,
        symbol_type=symbol_type,
        scope=scope,
    )
    return await refactorer.apply(refactoring, dry_run=dry_run)


async def move_symbol(
    workspace: Path,
    symbol_name: str,
    source_file: Path,
    target_file: Path,
    dry_run: bool = False,
) -> RefactoringResult:
    """Move a symbol to a different file and update imports.

    Args:
        workspace: Workspace root directory
        symbol_name: Name of the symbol to move
        source_file: Current file containing the symbol
        target_file: Target file to move to
        dry_run: If True, don't write changes

    Returns:
        RefactoringResult
    """
    refactorer = Refactorer(workspace)
    refactoring = MoveRefactoring(
        source_file=source_file,
        target_file=target_file,
        symbol_name=symbol_name,
        scope=RefactoringScope.WORKSPACE,
    )
    return await refactorer.apply(refactoring, dry_run=dry_run)


async def extract_function(
    path: Path,
    start_line: int,
    end_line: int,
    new_name: str,
    dry_run: bool = False,
) -> RefactoringResult:
    """Extract code to a new function.

    Args:
        path: File path
        start_line: Start line of code to extract
        end_line: End line of code to extract
        new_name: Name for the new function
        dry_run: If True, don't write changes

    Returns:
        RefactoringResult
    """
    workspace = path.parent
    refactorer = Refactorer(workspace)
    refactoring = ExtractRefactoring(
        start_line=start_line,
        end_line=end_line,
        new_name=new_name,
        scope=RefactoringScope.FILE,
        file_patterns=[path.name],
    )
    return await refactorer.apply(refactoring, dry_run=dry_run)


# =============================================================================
# Inline Refactoring
# =============================================================================


@dataclass
class InlineRefactoring(Refactoring):
    """Inline a function or variable.

    For functions: replaces all calls with the function body.
    For variables: replaces all uses with the assigned value.
    """

    name: str = ""
    remove_definition: bool = True

    @property
    def kind(self) -> RefactoringKind:
        return RefactoringKind.INLINE

    def apply_to_file(self, path: Path, content: str) -> str | None:
        """Inline occurrences in file."""
        if not self.name:
            return None

        try:
            tree = ast.parse(content)
        except SyntaxError:
            return None

        # Find the definition
        definition = _find_definition(tree, self.name)
        if definition is None:
            return None

        # Inline based on type
        if isinstance(definition, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return self._inline_function(tree, definition, content)
        elif isinstance(definition, ast.Assign):
            return self._inline_variable(tree, definition, content)

        return None

    def _inline_function(
        self,
        tree: ast.Module,
        func: ast.FunctionDef | ast.AsyncFunctionDef,
        content: str,
    ) -> str | None:
        """Inline a function."""
        # Simple case: single-expression return
        if len(func.body) == 1 and isinstance(func.body[0], ast.Return):
            return_expr = func.body[0].value
            if return_expr is None:
                return None

            transformer = _InlineFunctionTransformer(
                self.name, func.args, return_expr, self.remove_definition
            )
            new_tree = transformer.visit(tree)

            if transformer.changed:
                return ast.unparse(new_tree)

        return None

    def _inline_variable(
        self,
        tree: ast.Module,
        assign: ast.Assign,
        content: str,
    ) -> str | None:
        """Inline a variable."""
        # Only handle simple assignments
        if len(assign.targets) != 1:
            return None

        target = assign.targets[0]
        if not isinstance(target, ast.Name):
            return None

        transformer = _InlineVariableTransformer(self.name, assign.value, self.remove_definition)
        new_tree = transformer.visit(tree)

        if transformer.changed:
            return ast.unparse(new_tree)

        return None


def _find_definition(tree: ast.Module, name: str) -> ast.stmt | None:
    """Find the definition of a symbol."""
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if node.name == name:
                return node
        elif isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id == name:
                    return node
    return None


class _InlineFunctionTransformer(ast.NodeTransformer):
    """AST transformer for inlining function calls."""

    def __init__(
        self,
        func_name: str,
        args: ast.arguments,
        body: ast.expr,
        remove_def: bool,
    ):
        self.func_name = func_name
        self.args = args
        self.body = body
        self.remove_def = remove_def
        self.changed = False

    def visit_Call(self, node: ast.Call) -> ast.expr:
        self.generic_visit(node)

        if isinstance(node.func, ast.Name) and node.func.id == self.func_name:
            # Build substitution map
            subs: dict[str, ast.expr] = {}
            for i, arg in enumerate(node.args):
                if i < len(self.args.args):
                    subs[self.args.args[i].arg] = arg

            # Substitute in body copy
            body_copy = _deep_copy_expr(self.body)
            substituted = _SubstituteVars(subs).visit(body_copy)

            self.changed = True
            return substituted

        return node

    def visit_FunctionDef(self, node: ast.FunctionDef) -> ast.FunctionDef | None:
        self.generic_visit(node)
        if self.remove_def and node.name == self.func_name:
            self.changed = True
            return None  # Remove the function definition
        return node

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> ast.AsyncFunctionDef | None:
        self.generic_visit(node)
        if self.remove_def and node.name == self.func_name:
            self.changed = True
            return None
        return node


class _InlineVariableTransformer(ast.NodeTransformer):
    """AST transformer for inlining variable uses."""

    def __init__(self, var_name: str, value: ast.expr, remove_def: bool):
        self.var_name = var_name
        self.value = value
        self.remove_def = remove_def
        self.changed = False
        self._in_definition = False

    def visit_Assign(self, node: ast.Assign) -> ast.Assign | None:
        # Check if this is the variable definition
        is_our_definition = False
        for target in node.targets:
            if isinstance(target, ast.Name) and target.id == self.var_name:
                is_our_definition = True
                break

        if is_our_definition:
            if self.remove_def:
                self.changed = True
                return None  # Remove the assignment
            # Keep the definition but don't inline in its value
            return node

        # Process other assignments normally
        self.generic_visit(node)
        return node

    def visit_Name(self, node: ast.Name) -> ast.expr:
        if node.id == self.var_name and isinstance(node.ctx, ast.Load):
            self.changed = True
            return _deep_copy_expr(self.value)
        return node


class _SubstituteVars(ast.NodeTransformer):
    """Substitute variables in an expression."""

    def __init__(self, subs: dict[str, ast.expr]):
        self.subs = subs

    def visit_Name(self, node: ast.Name) -> ast.expr:
        if node.id in self.subs:
            return _deep_copy_expr(self.subs[node.id])
        return node


def _deep_copy_expr(node: ast.expr) -> ast.expr:
    """Deep copy an AST expression."""
    import copy

    return copy.deepcopy(node)


async def inline_symbol(
    path: Path,
    name: str,
    remove_definition: bool = True,
    dry_run: bool = False,
) -> RefactoringResult:
    """Inline a function or variable.

    Args:
        path: File path
        name: Symbol name to inline
        remove_definition: Whether to remove the definition
        dry_run: If True, don't write changes

    Returns:
        RefactoringResult
    """
    result = RefactoringResult(success=True)
    refactoring = InlineRefactoring(
        name=name,
        remove_definition=remove_definition,
    )

    try:
        path = Path(path).resolve()
        content = path.read_text()
        new_content = refactoring.apply_to_file(path, content)

        if new_content is not None and new_content != content:
            change = FileChange(
                path=path,
                original_content=content,
                new_content=new_content,
                description=f"inline {name}",
            )
            result.changes.append(change)
            result.affected_files.append(path)

            if not dry_run:
                path.write_text(new_content)

    except (OSError, SyntaxError) as e:
        result.errors.append(f"Error inlining {name}: {e}")
        result.success = False

    return result


# =============================================================================
# Codemod DSL
# =============================================================================


@dataclass
class CodemodPattern:
    """A pattern to match in code."""

    # Pattern can be:
    # - A string with $var placeholders: "assert $x == $y"
    # - A regex pattern with named groups
    pattern: str
    is_regex: bool = False

    def match(self, code: str) -> list[dict[str, str]]:
        """Find all matches in code."""
        if self.is_regex:
            return self._match_regex(code)
        return self._match_pattern(code)

    def _match_regex(self, code: str) -> list[dict[str, str]]:
        """Match using regex."""
        matches = []
        for m in re.finditer(self.pattern, code):
            matches.append(m.groupdict())
        return matches

    def _match_pattern(self, code: str) -> list[dict[str, str]]:
        """Match using placeholder pattern."""
        # Convert pattern to regex
        regex = self._pattern_to_regex()
        matches = []
        for m in re.finditer(regex, code):
            matches.append(m.groupdict())
        return matches

    def _pattern_to_regex(self) -> str:
        """Convert $var pattern to regex."""
        # Escape regex special chars except $
        escaped = re.escape(self.pattern)
        # Replace escaped \$var with named groups
        regex = re.sub(r"\\\$(\w+)", r"(?P<\1>[^,)]+)", escaped)
        return regex


@dataclass
class CodemodRule:
    """A single codemod transformation rule."""

    name: str
    description: str
    pattern: CodemodPattern
    replacement: str  # Can use $var or \\1 references
    file_patterns: list[str] = field(default_factory=lambda: ["**/*.py"])

    def apply(self, content: str) -> tuple[str, int]:
        """Apply rule to content.

        Returns:
            Tuple of (new_content, num_replacements)
        """
        if self.pattern.is_regex:
            new_content, count = re.subn(self.pattern.pattern, self.replacement, content)
            return new_content, count

        # Pattern-based replacement
        regex = self.pattern._pattern_to_regex()

        def replacer(m: re.Match) -> str:
            result = self.replacement
            for name, value in m.groupdict().items():
                result = result.replace(f"${name}", value)
            return result

        new_content, count = re.subn(regex, replacer, content)
        return new_content, count


@dataclass
class Codemod:
    """A collection of codemod rules."""

    name: str
    description: str = ""
    rules: list[CodemodRule] = field(default_factory=list)

    def add_rule(
        self,
        name: str,
        pattern: str,
        replacement: str,
        description: str = "",
        is_regex: bool = False,
    ) -> Codemod:
        """Add a rule to the codemod.

        Returns self for chaining.
        """
        self.rules.append(
            CodemodRule(
                name=name,
                description=description,
                pattern=CodemodPattern(pattern=pattern, is_regex=is_regex),
                replacement=replacement,
            )
        )
        return self


class CodemodRunner(WorkspaceRunner):
    """Runs codemods across files."""

    async def run(self, codemod: Codemod, dry_run: bool = False) -> RefactoringResult:
        """Run a codemod across the workspace."""
        result = RefactoringResult(success=True)

        for rule in codemod.rules:
            for pattern in rule.file_patterns:
                for path in self.workspace.glob(pattern):
                    if path.is_file() and not self._is_excluded(path):
                        try:
                            content = path.read_text()
                            new_content, count = rule.apply(content)

                            if count > 0:
                                change = FileChange(
                                    path=path,
                                    original_content=content,
                                    new_content=new_content,
                                    description=f"{rule.name}: {count} replacements",
                                )
                                result.changes.append(change)
                                result.affected_files.append(path)

                                if not dry_run:
                                    path.write_text(new_content)

                        except (OSError, SyntaxError) as e:
                            result.errors.append(f"Error in {path}: {e}")

        if result.errors:
            result.success = False

        return result


# =============================================================================
# Built-in Codemods
# =============================================================================


def create_deprecation_codemod(
    old_import: str,
    new_import: str,
    old_name: str,
    new_name: str | None = None,
) -> Codemod:
    """Create a codemod for deprecating imports.

    Example:
        codemod = create_deprecation_codemod(
            "old_module", "new_module", "OldClass", "NewClass"
        )
    """
    new_name = new_name or old_name
    codemod = Codemod(
        name=f"deprecate_{old_name}",
        description=f"Replace {old_import}.{old_name} with {new_import}.{new_name}",
    )

    # Update imports
    codemod.add_rule(
        name="update_import",
        pattern=rf"from {re.escape(old_import)} import {re.escape(old_name)}",
        replacement=f"from {new_import} import {new_name}",
        is_regex=True,
    )

    # Update uses if name changed
    if new_name != old_name:
        codemod.add_rule(
            name="update_usage",
            pattern=rf"\b{re.escape(old_name)}\b",
            replacement=new_name,
            is_regex=True,
        )

    return codemod


def create_api_migration_codemod(
    old_pattern: str,
    new_pattern: str,
    name: str = "api_migration",
) -> Codemod:
    """Create a codemod for API migrations.

    Example:
        codemod = create_api_migration_codemod(
            "assert_equal($x, $y)",
            "assert $x == $y"
        )
    """
    return Codemod(
        name=name,
        description=f"Migrate {old_pattern} to {new_pattern}",
        rules=[
            CodemodRule(
                name="migrate",
                description="",
                pattern=CodemodPattern(pattern=old_pattern),
                replacement=new_pattern,
            )
        ],
    )
