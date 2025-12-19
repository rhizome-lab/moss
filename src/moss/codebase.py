"""Unified codebase tree: filesystem + AST merged.

See docs/codebase-tree.md for design.
"""

from __future__ import annotations

import ast
from collections.abc import Iterator
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path


class NodeKind(Enum):
    """Kind of tree node."""

    ROOT = "root"
    DIRECTORY = "directory"
    FILE = "file"
    CLASS = "class"
    FUNCTION = "function"
    METHOD = "method"
    CONSTANT = "constant"


@dataclass
class Node:
    """A node in the codebase tree."""

    kind: NodeKind
    name: str
    path: Path  # Filesystem path (for file/dir) or logical path (for symbols)
    parent: Node | None = None
    children: list[Node] = field(default_factory=list)

    # Metadata (populated lazily)
    description: str = ""  # First line of docstring or inferred
    signature: str = ""  # For functions/methods
    lineno: int = 0  # Line number in file (for symbols)
    end_lineno: int = 0

    def __repr__(self) -> str:
        return f"Node({self.kind.value}, {self.name!r})"

    @property
    def full_path(self) -> str:
        """Full path from root, e.g., 'src/moss/dwim.py:ToolRouter.analyze_intent'."""
        parts = []
        node: Node | None = self
        while node and node.kind != NodeKind.ROOT:
            parts.append(node.name)
            node = node.parent
        parts.reverse()

        # Join with appropriate separators
        result: list[str] = []
        for i, part in enumerate(parts):
            if i == 0:
                result.append(part)
            elif result and (result[-1].endswith(".py") or ":" in "".join(result)):
                # After a file, use colon; within symbols, use dot
                if result[-1].endswith(".py"):
                    result.append(":")
                else:
                    result.append(".")
                result.append(part)
            else:
                result.append("/")
                result.append(part)
        return "".join(result)

    def add_child(self, child: Node) -> Node:
        """Add a child node."""
        child.parent = self
        self.children.append(child)
        return child

    def find(self, name: str) -> Node | None:
        """Find immediate child by name."""
        for child in self.children:
            if child.name == name:
                return child
        return None

    def walk(self) -> Iterator[Node]:
        """Walk all descendants depth-first."""
        yield self
        for child in self.children:
            yield from child.walk()


class CodebaseTree:
    """Unified view of a codebase: filesystem + AST."""

    def __init__(self, root: Path):
        self.root_path = root.resolve()
        self._root = Node(NodeKind.ROOT, root.name, root)
        self._cache: dict[Path, Node] = {}
        self._parsed: set[Path] = set()  # Track which files have been parsed

    @property
    def root(self) -> Node:
        return self._root

    def get(self, path: str | Path) -> Node | None:
        """Get a node by path (file path or symbol path).

        Examples:
            tree.get("src/moss/dwim.py")
            tree.get("src/moss/dwim.py:ToolRouter")
            tree.get("ToolRouter.analyze_intent")
        """
        if isinstance(path, Path):
            path = str(path)

        # Handle symbol paths (file:symbol or just symbol)
        if ":" in path:
            file_part, symbol_part = path.split(":", 1)
            file_node = self._get_file_node(Path(file_part))
            if not file_node:
                return None
            return self._find_symbol(file_node, symbol_part)

        # Try as file path first
        p = Path(path)
        if p.suffix or (self.root_path / p).exists():
            return self._get_file_node(p)

        # Try as symbol name (search)
        return self._find_symbol_globally(path)

    def _get_file_node(self, path: Path) -> Node | None:
        """Get or create node for a file/directory path."""
        if not path.is_absolute():
            path = self.root_path / path

        if path in self._cache:
            return self._cache[path]

        if not path.exists():
            return None

        # Build path from root
        try:
            rel = path.relative_to(self.root_path)
        except ValueError:
            return None

        current = self._root
        current_path = self.root_path

        for part in rel.parts:
            current_path = current_path / part
            child = current.find(part)
            if not child:
                kind = NodeKind.DIRECTORY if current_path.is_dir() else NodeKind.FILE
                child = current.add_child(Node(kind, part, current_path))
                self._cache[current_path] = child

                # If it's a Python file, parse it
                if kind == NodeKind.FILE and current_path.suffix == ".py":
                    self._parse_python_file(child)

            current = child

        return current

    def _parse_python_file(self, file_node: Node) -> None:
        """Parse a Python file and add symbol children."""
        # Skip if already parsed
        if file_node.path in self._parsed:
            return
        self._parsed.add(file_node.path)

        try:
            source = file_node.path.read_text()
            tree = ast.parse(source)
        except (SyntaxError, OSError):
            return

        # Get module docstring
        file_node.description = ast.get_docstring(tree) or ""
        if file_node.description:
            file_node.description = file_node.description.split("\n")[0]

        for node in tree.body:
            self._add_ast_node(file_node, node)

    def _add_ast_node(self, parent: Node, node: ast.AST) -> None:
        """Add an AST node to the tree."""
        if isinstance(node, ast.ClassDef):
            class_node = parent.add_child(
                Node(
                    kind=NodeKind.CLASS,
                    name=node.name,
                    path=parent.path,
                    description=self._get_docstring_first_line(node),
                    lineno=node.lineno,
                    end_lineno=node.end_lineno or node.lineno,
                )
            )
            # Add methods
            for item in node.body:
                if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    class_node.add_child(
                        Node(
                            kind=NodeKind.METHOD,
                            name=item.name,
                            path=parent.path,
                            description=self._get_docstring_first_line(item),
                            signature=self._get_signature(item),
                            lineno=item.lineno,
                            end_lineno=item.end_lineno or item.lineno,
                        )
                    )

        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            parent.add_child(
                Node(
                    kind=NodeKind.FUNCTION,
                    name=node.name,
                    path=parent.path,
                    description=self._get_docstring_first_line(node),
                    signature=self._get_signature(node),
                    lineno=node.lineno,
                    end_lineno=node.end_lineno or node.lineno,
                )
            )

        elif isinstance(node, ast.Assign):
            # Top-level constants (UPPER_CASE)
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id.isupper():
                    parent.add_child(
                        Node(
                            kind=NodeKind.CONSTANT,
                            name=target.id,
                            path=parent.path,
                            lineno=node.lineno,
                            end_lineno=node.end_lineno or node.lineno,
                        )
                    )

    def _get_docstring_first_line(self, node: ast.AST) -> str:
        """Get first line of docstring."""
        doc = ast.get_docstring(node)
        if doc:
            return doc.split("\n")[0]
        return ""

    def _get_signature(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> str:
        """Get function signature."""
        args = []
        for arg in node.args.args:
            arg_str = arg.arg
            if arg.annotation:
                arg_str += f": {ast.unparse(arg.annotation)}"
            args.append(arg_str)

        ret = ""
        if node.returns:
            ret = f" -> {ast.unparse(node.returns)}"

        prefix = "async " if isinstance(node, ast.AsyncFunctionDef) else ""
        return f"{prefix}def {node.name}({', '.join(args)}){ret}"

    def _find_symbol(self, file_node: Node, symbol_path: str) -> Node | None:
        """Find a symbol within a file node.

        symbol_path can be "ClassName" or "ClassName.method_name"
        """
        parts = symbol_path.split(".")
        current = file_node

        for part in parts:
            found = current.find(part)
            if not found:
                return None
            current = found

        return current

    def _find_symbol_globally(self, name: str) -> Node | None:
        """Search for a symbol by name across all loaded files."""
        for node in self._root.walk():
            if node.name == name and node.kind in (
                NodeKind.CLASS,
                NodeKind.FUNCTION,
                NodeKind.METHOD,
            ):
                return node
        return None

    def _scan_file_structure(self) -> None:
        """Scan filesystem structure without parsing (fast).

        Uses os.walk with in-place pruning for speed.
        """
        import os

        skip_dirs = {"__pycache__", "node_modules", ".venv", "venv", ".git"}
        for dirpath, dirnames, filenames in os.walk(self.root_path):
            # Prune excluded directories in-place (prevents descent)
            dirnames[:] = [d for d in dirnames if not d.startswith(".") and d not in skip_dirs]

            # Add this directory and its files
            dir_path = Path(dirpath)
            if dir_path != self.root_path:
                self._add_file_node_fast(dir_path)
            for name in filenames:
                self._add_file_node_fast(dir_path / name)

    def _add_file_node_fast(self, path: Path) -> Node | None:
        """Add a file/dir node without parsing contents."""
        if path in self._cache:
            return self._cache[path]

        try:
            rel = path.relative_to(self.root_path)
        except ValueError:
            return None

        current = self._root
        current_path = self.root_path

        for part in rel.parts:
            current_path = current_path / part
            child = current.find(part)
            if not child:
                kind = NodeKind.DIRECTORY if current_path.is_dir() else NodeKind.FILE
                child = current.add_child(Node(kind, part, current_path))
                self._cache[current_path] = child
                # NO parsing here - just structure
            current = child

        return current

    def _scan_all_files(self) -> None:
        """Parse all Python files in the codebase for symbols.

        If files are already in cache (from fast scan), just parse them.
        """
        # First ensure file structure is scanned
        self._scan_file_structure()

        # Now parse all Python files
        for node in self._root.walk():
            if node.kind == NodeKind.FILE and node.path.suffix == ".py":
                self._parse_python_file(node)

    def resolve(self, query: str, scan_all: bool = True) -> list[Node]:
        """Resolve a fuzzy query to matching nodes.

        Handles:
        - Exact paths: src/moss/dwim.py
        - Partial filenames: dwim.py, dwim
        - Symbols: ToolRouter, resolve_tool
        - Scoped: dwim:ToolRouter, dwim.py:ToolRouter

        Returns list of matches (may be empty).
        """
        matches: list[Node] = []

        # If contains colon, it's file:symbol
        if ":" in query:
            file_part, symbol_part = query.split(":", 1)
            file_matches = self.resolve(file_part, scan_all=scan_all)
            for file_node in file_matches:
                if file_node.kind == NodeKind.FILE:
                    sym = self._find_symbol(file_node, symbol_part)
                    if sym:
                        matches.append(sym)
            return matches

        # Try exact path first
        exact = self.get(query)
        if exact:
            return [exact]

        query_lower = query.lower()

        # Fast scan filesystem structure (no parsing)
        self._scan_file_structure()

        # Match by filename/dirname first
        for node in self._root.walk():
            if node.kind in (NodeKind.FILE, NodeKind.DIRECTORY):
                name_lower = node.name.lower()
                stem_lower = Path(node.name).stem.lower()
                if name_lower == query_lower or stem_lower == query_lower:
                    matches.append(node)

        # If no file matches, try symbol names (requires parsing)
        if not matches and scan_all:
            self._scan_all_files()
            for node in self._root.walk():
                if node.kind in (NodeKind.CLASS, NodeKind.FUNCTION, NodeKind.METHOD):
                    if node.name.lower() == query_lower:
                        matches.append(node)

        return matches

    def search(self, query: str, scope: str | None = None, scan_all: bool = True) -> list[Node]:
        """Search for nodes matching a query within a scope.

        Args:
            query: Search term (matches name or description)
            scope: Optional scope (file, directory, or symbol path)
            scan_all: Whether to scan all files first

        Returns:
            List of matching nodes
        """
        if scan_all:
            self._scan_all_files()

        query_lower = query.lower()
        matches: list[Node] = []

        # Determine search root
        if scope:
            scope_nodes = self.resolve(scope, scan_all=False)
            if not scope_nodes:
                return []
            search_roots = scope_nodes
        else:
            search_roots = [self._root]

        # Search within each root
        for root in search_roots:
            for node in root.walk():
                # Skip the root itself
                if node == root and scope:
                    continue
                # Match name or description
                if query_lower in node.name.lower():
                    matches.append(node)
                elif query_lower in node.description.lower():
                    matches.append(node)

        return matches

    def find_callees(self, symbol: Node) -> list[str]:
        """Find symbols that a function/method calls.

        Returns list of symbol names (not full nodes, since they may be external).
        """
        if symbol.lineno == 0:
            return []

        try:
            source = symbol.path.read_text()
            lines = source.splitlines()
            func_source = "\n".join(lines[symbol.lineno - 1 : symbol.end_lineno])

            tree = ast.parse(func_source)
            calls: set[str] = set()

            for node in ast.walk(tree):
                if isinstance(node, ast.Call):
                    if isinstance(node.func, ast.Name):
                        calls.add(node.func.id)
                    elif isinstance(node.func, ast.Attribute):
                        calls.add(node.func.attr)

            return sorted(calls)
        except (SyntaxError, OSError):
            return []

    def find_references(self, symbol_name: str, scan_all: bool = True) -> list[Node]:
        """Find nodes that reference a symbol by name.

        Simple implementation: searches for symbol name usage in all files.
        Returns files/functions that appear to call/use the symbol.
        """
        if scan_all:
            self._scan_all_files()

        refs: list[Node] = []

        for node in self._root.walk():
            if node.kind != NodeKind.FILE:
                continue

            # Check if file contains the symbol name
            try:
                source = node.path.read_text()
                if symbol_name in source:
                    # Find which functions/methods use it
                    try:
                        tree = ast.parse(source)
                        for ast_node in ast.walk(tree):
                            if isinstance(ast_node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                                # Check if this function uses the symbol
                                func_source = ast.get_source_segment(source, ast_node)
                                if func_source and symbol_name in func_source:
                                    # Find the corresponding Node
                                    func_node = self._find_symbol(node, ast_node.name)
                                    if func_node and func_node.name != symbol_name:
                                        refs.append(func_node)
                            elif isinstance(ast_node, ast.ClassDef):
                                for item in ast_node.body:
                                    if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                                        method_source = ast.get_source_segment(source, item)
                                        if method_source and symbol_name in method_source:
                                            method_node = self._find_symbol(
                                                node, f"{ast_node.name}.{item.name}"
                                            )
                                            if method_node and method_node.name != symbol_name:
                                                refs.append(method_node)
                    except SyntaxError:
                        # If we can't parse, just add the file
                        refs.append(node)
            except OSError:
                continue

        return refs


def build_tree(root: Path) -> CodebaseTree:
    """Build a codebase tree for a directory."""
    return CodebaseTree(root)


@dataclass
class PathMatch:
    """Result of resolving a path."""

    node: Node
    full_path: str
    kind: str

    def __str__(self) -> str:
        return f"{self.full_path} ({self.kind})"


def resolve_path(query: str, root: Path | None = None) -> list[PathMatch]:
    """Resolve a fuzzy path query to matching nodes.

    Args:
        query: Path or symbol to find (supports fuzzy matching)
        root: Root directory (defaults to cwd)

    Returns:
        List of PathMatch results
    """
    if root is None:
        root = Path.cwd()

    tree = build_tree(root)
    nodes = tree.resolve(query)

    return [
        PathMatch(
            node=n,
            full_path=n.full_path,
            kind=n.kind.value,
        )
        for n in nodes
    ]
