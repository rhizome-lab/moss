"""Tree-sitter integration for multi-language AST parsing.

This module provides:
- TreeSitterParser: Generic parser for any tree-sitter supported language
- Language-specific skeleton extractors (TypeScript, JavaScript, Go, Rust)
- Query-based code navigation
- ParseResult: Result type with fallback to text window mode on parse failure

Requires: pip install tree-sitter tree-sitter-python tree-sitter-typescript etc.

Usage:
    parser = TreeSitterParser("typescript")
    result = parser.parse_safe(source_code)

    if result.is_ok:
        tree = result.tree
        symbols = parser.extract_symbols(tree)
    else:
        # Fallback: use text window around error location
        context = result.text_window(line=10, context_lines=5)

    # Find specific nodes
    functions = parser.query(tree, "(function_declaration) @fn")
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum, auto
from pathlib import Path
from typing import TYPE_CHECKING, Any, ClassVar

if TYPE_CHECKING:
    pass


@dataclass
class ParseError:
    """Error from tree-sitter parsing.

    Contains error details and the original source for fallback text mode.
    """

    message: str
    source: str
    error_line: int | None = None
    error_column: int | None = None

    def text_window(self, line: int, context_lines: int = 5) -> str:
        """Get a window of text around a line (fallback for failed parse).

        Args:
            line: Center line number (0-indexed)
            context_lines: Number of lines before/after to include

        Returns:
            Text window with line numbers
        """
        lines = self.source.splitlines()
        start = max(0, line - context_lines)
        end = min(len(lines), line + context_lines + 1)

        result = []
        for i in range(start, end):
            marker = ">" if i == line else " "
            result.append(f"{marker}{i + 1:4d} | {lines[i]}")

        return "\n".join(result)

    def error_context(self, context_lines: int = 3) -> str:
        """Get text window around the error location.

        Returns:
            Text window centered on error, or first few lines if no location
        """
        if self.error_line is not None:
            return self.text_window(self.error_line, context_lines)
        return self.text_window(0, context_lines)


@dataclass
class ParseResult:
    """Result of tree-sitter parsing, with fallback to text mode.

    Degraded Mode: Never block access due to parse failures.
    On failure, provides text window fallback for basic code navigation.
    """

    tree: TreeNode | None
    error: ParseError | None
    source: str

    @property
    def is_ok(self) -> bool:
        """True if parsing succeeded."""
        return self.tree is not None

    @property
    def is_err(self) -> bool:
        """True if parsing failed."""
        return self.tree is None

    def text_window(self, line: int, context_lines: int = 5) -> str:
        """Get a window of text around a line (works regardless of parse status).

        Args:
            line: Center line number (0-indexed)
            context_lines: Number of lines before/after to include

        Returns:
            Text window with line numbers
        """
        lines = self.source.splitlines()
        start = max(0, line - context_lines)
        end = min(len(lines), line + context_lines + 1)

        result = []
        for i in range(start, end):
            marker = ">" if i == line else " "
            result.append(f"{marker}{i + 1:4d} | {lines[i]}")

        return "\n".join(result)

    def unwrap(self) -> TreeNode:
        """Get the tree, raising if parse failed.

        Raises:
            ValueError: If parsing failed
        """
        if self.tree is None:
            raise ValueError(f"Parse failed: {self.error.message if self.error else 'unknown'}")
        return self.tree

    def unwrap_or_text(self, line: int, context_lines: int = 5) -> TreeNode | str:
        """Get tree if OK, or text window fallback if failed.

        Args:
            line: Center line for text window fallback
            context_lines: Context lines for fallback

        Returns:
            TreeNode if parse succeeded, text window string if failed
        """
        if self.tree is not None:
            return self.tree
        return self.text_window(line, context_lines)


class LanguageType(Enum):
    """Supported programming languages."""

    PYTHON = auto()
    TYPESCRIPT = auto()
    JAVASCRIPT = auto()
    GO = auto()
    RUST = auto()
    JAVA = auto()
    C = auto()
    CPP = auto()


@dataclass
class TreeNode:
    """Represents a node in the syntax tree."""

    type: str
    text: str
    start_line: int
    end_line: int
    start_column: int
    end_column: int
    children: list[TreeNode] = field(default_factory=list)
    named: bool = True

    @property
    def range(self) -> tuple[tuple[int, int], tuple[int, int]]:
        """Get the (start, end) positions as ((line, col), (line, col))."""
        return ((self.start_line, self.start_column), (self.end_line, self.end_column))


@dataclass
class TSSymbol:
    """Symbol extracted from tree-sitter parse."""

    name: str
    kind: str  # function, class, method, variable, etc.
    line: int
    end_line: int
    signature: str | None = None
    docstring: str | None = None
    visibility: str | None = None  # public, private, protected
    parent: str | None = None
    children: list[TSSymbol] = field(default_factory=list)


@dataclass
class QueryMatch:
    """Result of a tree-sitter query."""

    pattern_index: int
    captures: dict[str, TreeNode]


class TreeSitterParser:
    """Parser using tree-sitter for multi-language AST.

    Supports parsing source code and extracting structural information
    for many programming languages.
    """

    # Map of language to tree-sitter module name
    LANGUAGE_MODULES: ClassVar[dict[LanguageType, str]] = {
        LanguageType.PYTHON: "tree_sitter_python",
        LanguageType.TYPESCRIPT: "tree_sitter_typescript",
        LanguageType.JAVASCRIPT: "tree_sitter_javascript",
        LanguageType.GO: "tree_sitter_go",
        LanguageType.RUST: "tree_sitter_rust",
        LanguageType.JAVA: "tree_sitter_java",
        LanguageType.C: "tree_sitter_c",
        LanguageType.CPP: "tree_sitter_cpp",
    }

    # File extension to language mapping
    EXTENSION_MAP: ClassVar[dict[str, LanguageType]] = {
        ".py": LanguageType.PYTHON,
        ".ts": LanguageType.TYPESCRIPT,
        ".tsx": LanguageType.TYPESCRIPT,
        ".js": LanguageType.JAVASCRIPT,
        ".jsx": LanguageType.JAVASCRIPT,
        ".go": LanguageType.GO,
        ".rs": LanguageType.RUST,
        ".java": LanguageType.JAVA,
        ".c": LanguageType.C,
        ".h": LanguageType.C,
        ".cpp": LanguageType.CPP,
        ".cc": LanguageType.CPP,
        ".hpp": LanguageType.CPP,
    }

    def __init__(self, language: LanguageType | str) -> None:
        """Initialize parser for a specific language.

        Args:
            language: LanguageType enum or string (e.g., "typescript")
        """
        if isinstance(language, str):
            language = LanguageType[language.upper()]
        self._language_type = language
        self._parser: Any = None
        self._language: Any = None

    @classmethod
    def from_file(cls, path: Path) -> TreeSitterParser:
        """Create parser based on file extension.

        Args:
            path: Path to source file

        Returns:
            Configured TreeSitterParser

        Raises:
            ValueError: If extension not recognized
        """
        ext = path.suffix.lower()
        if ext not in cls.EXTENSION_MAP:
            raise ValueError(f"Unsupported file extension: {ext}")
        return cls(cls.EXTENSION_MAP[ext])

    def _ensure_initialized(self) -> None:
        """Lazily initialize tree-sitter parser."""
        if self._parser is not None:
            return

        try:
            import tree_sitter
        except ImportError as e:
            raise ImportError(
                "tree-sitter not installed. Install with: pip install tree-sitter"
            ) from e

        # Get the language module
        module_name = self.LANGUAGE_MODULES[self._language_type]
        try:
            lang_module = __import__(module_name)
            # Handle different naming conventions for language functions
            # Most use .language(), but some (like typescript) use language_<name>()
            if hasattr(lang_module, "language"):
                lang_fn = lang_module.language
            elif hasattr(lang_module, f"language_{self._language_type.value}"):
                lang_fn = getattr(lang_module, f"language_{self._language_type.value}")
            else:
                # Fallback: find any language_* function
                lang_fns = [n for n in dir(lang_module) if n.startswith("language_")]
                if lang_fns:
                    lang_fn = getattr(lang_module, lang_fns[0])
                else:
                    raise AttributeError(f"No language function found in {module_name}")
            self._language = tree_sitter.Language(lang_fn())
        except ImportError as e:
            raise ImportError(
                f"Language module {module_name} not installed. "
                f"Install with: pip install {module_name.replace('_', '-')}"
            ) from e

        self._parser = tree_sitter.Parser(self._language)

    def parse(self, source: str | bytes) -> TreeNode:
        """Parse source code into a tree.

        Args:
            source: Source code as string or bytes

        Returns:
            Root TreeNode of the parse tree
        """
        self._ensure_initialized()

        if isinstance(source, str):
            source = source.encode("utf-8")

        tree = self._parser.parse(source)
        return self._convert_node(tree.root_node, source)

    def parse_safe(self, source: str | bytes) -> ParseResult:
        """Parse source code, returning Result with fallback on failure.

        Degraded Mode: Never blocks access due to parse failures.
        On failure, ParseResult provides text window fallback.

        Args:
            source: Source code as string or bytes

        Returns:
            ParseResult with tree on success, or error with text fallback
        """
        source_str = source if isinstance(source, str) else source.decode("utf-8", errors="replace")

        try:
            self._ensure_initialized()

            source_bytes = source if isinstance(source, bytes) else source.encode("utf-8")
            tree = self._parser.parse(source_bytes)
            tree_node = self._convert_node(tree.root_node, source_bytes)

            # Check if tree has errors (tree-sitter marks error nodes)
            if self._has_error_nodes(tree_node):
                error_loc = self._find_first_error(tree_node)
                return ParseResult(
                    tree=tree_node,  # Still return tree, but with error info
                    error=ParseError(
                        message="Parse completed with errors",
                        source=source_str,
                        error_line=error_loc[0] if error_loc else None,
                        error_column=error_loc[1] if error_loc else None,
                    ),
                    source=source_str,
                )

            return ParseResult(tree=tree_node, error=None, source=source_str)

        except ImportError as e:
            return ParseResult(
                tree=None,
                error=ParseError(message=str(e), source=source_str),
                source=source_str,
            )
        except Exception as e:
            return ParseResult(
                tree=None,
                error=ParseError(message=str(e), source=source_str),
                source=source_str,
            )

    def parse_file_safe(self, path: Path) -> ParseResult:
        """Parse a source file, returning Result with fallback on failure.

        Args:
            path: Path to source file

        Returns:
            ParseResult with tree on success, or error with text fallback
        """
        try:
            source = path.read_bytes()
            return self.parse_safe(source)
        except OSError as e:
            return ParseResult(
                tree=None,
                error=ParseError(message=f"Failed to read file: {e}", source=""),
                source="",
            )

    def _has_error_nodes(self, tree: TreeNode) -> bool:
        """Check if tree contains ERROR nodes."""
        if tree.type == "ERROR":
            return True
        return any(self._has_error_nodes(child) for child in tree.children)

    def _find_first_error(self, tree: TreeNode) -> tuple[int, int] | None:
        """Find location of first ERROR node."""
        if tree.type == "ERROR":
            return (tree.start_line, tree.start_column)
        for child in tree.children:
            loc = self._find_first_error(child)
            if loc:
                return loc
        return None

    def parse_file(self, path: Path) -> TreeNode:
        """Parse a source file.

        Args:
            path: Path to source file

        Returns:
            Root TreeNode of the parse tree
        """
        source = path.read_bytes()
        return self.parse(source)

    def _convert_node(self, node: Any, source: bytes) -> TreeNode:
        """Convert tree-sitter node to TreeNode."""
        children = [self._convert_node(child, source) for child in node.children]

        return TreeNode(
            type=node.type,
            text=source[node.start_byte : node.end_byte].decode("utf-8", errors="replace"),
            start_line=node.start_point[0],
            end_line=node.end_point[0],
            start_column=node.start_point[1],
            end_column=node.end_point[1],
            children=children,
            named=node.is_named,
        )

    def query(self, tree: TreeNode, query_str: str) -> list[QueryMatch]:
        """Run a tree-sitter query on the parsed tree.

        Args:
            tree: Parsed tree from parse()
            query_str: Tree-sitter query string

        Returns:
            List of QueryMatch results
        """
        self._ensure_initialized()

        # Re-parse to get the raw tree-sitter tree
        # (we need this for querying)

        raw_tree = self._parser.parse(tree.text.encode("utf-8"))
        query = self._language.query(query_str)
        captures = query.captures(raw_tree.root_node)

        # Group captures by pattern
        results: list[QueryMatch] = []
        current_match: dict[str, TreeNode] = {}

        for node, name in captures:
            tree_node = TreeNode(
                type=node.type,
                text=tree.text[node.start_byte : node.end_byte]
                if hasattr(node, "start_byte")
                else "",
                start_line=node.start_point[0],
                end_line=node.end_point[0],
                start_column=node.start_point[1],
                end_column=node.end_point[1],
            )
            current_match[name] = tree_node

        if current_match:
            results.append(QueryMatch(pattern_index=0, captures=current_match))

        return results

    def extract_symbols(self, tree: TreeNode) -> list[TSSymbol]:
        """Extract symbols from parsed tree.

        Args:
            tree: Parsed tree from parse()

        Returns:
            List of symbols found in the tree
        """
        if self._language_type == LanguageType.TYPESCRIPT:
            return self._extract_typescript_symbols(tree)
        elif self._language_type == LanguageType.JAVASCRIPT:
            return self._extract_javascript_symbols(tree)
        elif self._language_type == LanguageType.PYTHON:
            return self._extract_python_symbols(tree)
        elif self._language_type == LanguageType.GO:
            return self._extract_go_symbols(tree)
        elif self._language_type == LanguageType.RUST:
            return self._extract_rust_symbols(tree)
        else:
            # Generic extraction
            return self._extract_generic_symbols(tree)

    def _extract_typescript_symbols(self, tree: TreeNode) -> list[TSSymbol]:
        """Extract symbols from TypeScript/JavaScript AST."""
        symbols = []

        def visit(node: TreeNode, parent_name: str | None = None) -> None:
            if node.type == "function_declaration":
                name = self._find_child_text(node, "identifier")
                params = self._find_child_text(node, "formal_parameters")
                symbols.append(
                    TSSymbol(
                        name=name or "<anonymous>",
                        kind="function",
                        line=node.start_line,
                        end_line=node.end_line,
                        signature=f"{name}{params}" if params else name,
                        parent=parent_name,
                    )
                )

            elif node.type == "class_declaration":
                name = self._find_child_text(node, "type_identifier") or self._find_child_text(
                    node, "identifier"
                )
                symbol = TSSymbol(
                    name=name or "<anonymous>",
                    kind="class",
                    line=node.start_line,
                    end_line=node.end_line,
                    parent=parent_name,
                )
                symbols.append(symbol)
                # Visit class body for methods
                for child in node.children:
                    if child.type == "class_body":
                        for member in child.children:
                            visit(member, name)

            elif node.type == "method_definition":
                name = self._find_child_text(node, "property_identifier")
                params = self._find_child_text(node, "formal_parameters")
                symbols.append(
                    TSSymbol(
                        name=name or "<anonymous>",
                        kind="method",
                        line=node.start_line,
                        end_line=node.end_line,
                        signature=f"{name}{params}" if params else name,
                        parent=parent_name,
                    )
                )

            elif node.type == "interface_declaration":
                name = self._find_child_text(node, "type_identifier")
                symbols.append(
                    TSSymbol(
                        name=name or "<anonymous>",
                        kind="interface",
                        line=node.start_line,
                        end_line=node.end_line,
                        parent=parent_name,
                    )
                )

            elif node.type == "type_alias_declaration":
                name = self._find_child_text(node, "type_identifier")
                symbols.append(
                    TSSymbol(
                        name=name or "<anonymous>",
                        kind="type",
                        line=node.start_line,
                        end_line=node.end_line,
                        parent=parent_name,
                    )
                )

            # Recurse
            for child in node.children:
                if child.type not in ("class_body", "statement_block"):
                    visit(child, parent_name)

        visit(tree)
        return symbols

    def _extract_javascript_symbols(self, tree: TreeNode) -> list[TSSymbol]:
        """Extract symbols from JavaScript AST (same as TypeScript minus types)."""
        return self._extract_typescript_symbols(tree)

    def _extract_python_symbols(self, tree: TreeNode) -> list[TSSymbol]:
        """Extract symbols from Python AST."""
        symbols = []

        def visit(node: TreeNode, parent_name: str | None = None) -> None:
            if node.type == "function_definition":
                name = self._find_child_text(node, "identifier")
                params = self._find_child_text(node, "parameters")
                symbols.append(
                    TSSymbol(
                        name=name or "<anonymous>",
                        kind="function",
                        line=node.start_line,
                        end_line=node.end_line,
                        signature=f"def {name}{params}" if params else f"def {name}()",
                        parent=parent_name,
                    )
                )

            elif node.type == "class_definition":
                name = self._find_child_text(node, "identifier")
                symbol = TSSymbol(
                    name=name or "<anonymous>",
                    kind="class",
                    line=node.start_line,
                    end_line=node.end_line,
                    parent=parent_name,
                )
                symbols.append(symbol)
                # Visit body for methods
                for child in node.children:
                    if child.type == "block":
                        for member in child.children:
                            visit(member, name)

            # Recurse
            for child in node.children:
                if child.type != "block":
                    visit(child, parent_name)

        visit(tree)
        return symbols

    def _extract_go_symbols(self, tree: TreeNode) -> list[TSSymbol]:
        """Extract symbols from Go AST."""
        symbols = []

        def visit(node: TreeNode, parent_name: str | None = None) -> None:
            if node.type == "function_declaration":
                name = self._find_child_text(node, "identifier")
                symbols.append(
                    TSSymbol(
                        name=name or "<anonymous>",
                        kind="function",
                        line=node.start_line,
                        end_line=node.end_line,
                        parent=parent_name,
                    )
                )

            elif node.type == "method_declaration":
                name = self._find_child_text(node, "field_identifier")
                symbols.append(
                    TSSymbol(
                        name=name or "<anonymous>",
                        kind="method",
                        line=node.start_line,
                        end_line=node.end_line,
                        parent=parent_name,
                    )
                )

            elif node.type == "type_declaration":
                for child in node.children:
                    if child.type == "type_spec":
                        name = self._find_child_text(child, "type_identifier")
                        child_types = [c.type for c in child.children]
                        kind = "struct" if "struct_type" in child_types else "type"
                        symbols.append(
                            TSSymbol(
                                name=name or "<anonymous>",
                                kind=kind,
                                line=node.start_line,
                                end_line=node.end_line,
                                parent=parent_name,
                            )
                        )

            for child in node.children:
                visit(child, parent_name)

        visit(tree)
        return symbols

    def _extract_rust_symbols(self, tree: TreeNode) -> list[TSSymbol]:
        """Extract symbols from Rust AST."""
        symbols = []

        def visit(node: TreeNode, parent_name: str | None = None) -> None:
            if node.type == "function_item":
                name = self._find_child_text(node, "identifier")
                symbols.append(
                    TSSymbol(
                        name=name or "<anonymous>",
                        kind="function",
                        line=node.start_line,
                        end_line=node.end_line,
                        parent=parent_name,
                    )
                )

            elif node.type == "struct_item":
                name = self._find_child_text(node, "type_identifier")
                symbols.append(
                    TSSymbol(
                        name=name or "<anonymous>",
                        kind="struct",
                        line=node.start_line,
                        end_line=node.end_line,
                        parent=parent_name,
                    )
                )

            elif node.type == "impl_item":
                type_name = self._find_child_text(node, "type_identifier")
                for child in node.children:
                    if child.type == "declaration_list":
                        for member in child.children:
                            visit(member, type_name)

            elif node.type == "trait_item":
                name = self._find_child_text(node, "type_identifier")
                symbols.append(
                    TSSymbol(
                        name=name or "<anonymous>",
                        kind="trait",
                        line=node.start_line,
                        end_line=node.end_line,
                        parent=parent_name,
                    )
                )

            for child in node.children:
                if child.type != "declaration_list":
                    visit(child, parent_name)

        visit(tree)
        return symbols

    def _extract_generic_symbols(self, tree: TreeNode) -> list[TSSymbol]:
        """Generic symbol extraction for unsupported languages."""
        symbols = []

        def visit(node: TreeNode) -> None:
            # Look for common patterns
            if "function" in node.type or "method" in node.type:
                name = self._find_child_text(node, "identifier") or self._find_child_text(
                    node, "name"
                )
                if name:
                    symbols.append(
                        TSSymbol(
                            name=name,
                            kind="function",
                            line=node.start_line,
                            end_line=node.end_line,
                        )
                    )

            elif "class" in node.type or "struct" in node.type:
                name = self._find_child_text(node, "identifier") or self._find_child_text(
                    node, "name"
                )
                if name:
                    symbols.append(
                        TSSymbol(
                            name=name,
                            kind="class",
                            line=node.start_line,
                            end_line=node.end_line,
                        )
                    )

            for child in node.children:
                visit(child)

        visit(tree)
        return symbols

    def _find_child_text(self, node: TreeNode, child_type: str) -> str | None:
        """Find first child of given type and return its text."""
        for child in node.children:
            if child.type == child_type:
                return child.text
        return None


class TreeSitterSkeletonProvider:
    """Skeleton provider using tree-sitter for multi-language support."""

    def __init__(self, language: LanguageType | str) -> None:
        """Initialize skeleton provider.

        Args:
            language: Target language
        """
        self._parser = TreeSitterParser(language)

    @classmethod
    def from_file(cls, path: Path) -> TreeSitterSkeletonProvider:
        """Create provider based on file extension."""
        parser = TreeSitterParser.from_file(path)
        provider = cls.__new__(cls)
        provider._parser = parser
        return provider

    def extract_skeleton(self, source: str) -> list[TSSymbol]:
        """Extract skeleton from source code.

        Args:
            source: Source code

        Returns:
            List of symbols representing the code skeleton
        """
        tree = self._parser.parse(source)
        return self._parser.extract_symbols(tree)

    def format_skeleton(self, symbols: list[TSSymbol], indent: int = 0) -> str:
        """Format symbols as readable skeleton.

        Args:
            symbols: List of TSSymbol
            indent: Base indentation level

        Returns:
            Formatted skeleton string
        """
        lines = []
        prefix = "  " * indent

        for sym in symbols:
            if sym.signature:
                lines.append(f"{prefix}{sym.kind}: {sym.signature} (L{sym.line}-{sym.end_line})")
            else:
                lines.append(f"{prefix}{sym.kind}: {sym.name} (L{sym.line}-{sym.end_line})")

            if sym.children:
                lines.append(self.format_skeleton(sym.children, indent + 1))

        return "\n".join(lines)


def get_language_for_extension(ext: str) -> LanguageType | None:
    """Get language type for a file extension.

    Args:
        ext: File extension (e.g., ".ts")

    Returns:
        LanguageType or None if not recognized
    """
    return TreeSitterParser.EXTENSION_MAP.get(ext.lower())


def is_supported_extension(ext: str) -> bool:
    """Check if a file extension is supported.

    Args:
        ext: File extension (e.g., ".ts")

    Returns:
        True if extension is supported
    """
    return ext.lower() in TreeSitterParser.EXTENSION_MAP


def get_symbols_at_line(file_path: Path | str, line: int) -> list[str]:
    """Find symbols (functions, classes) that contain a given line.

    Used for mapping diff hunks to AST nodes.

    Args:
        file_path: Path to the source file
        line: 1-indexed line number

    Returns:
        List of symbol names containing the line, from innermost to outermost.
        Empty list if no containing symbol or parsing fails.
    """
    from pathlib import Path

    file_path = Path(file_path)
    if not file_path.exists():
        return []

    ext = file_path.suffix
    if not is_supported_extension(ext):
        return []

    try:
        source = file_path.read_text()
        skeleton = TreeSitterSkeletonProvider()
        symbols = skeleton.extract_skeleton(source)
    except Exception:
        return []

    def find_containing(syms: list[TSSymbol], target_line: int) -> list[str]:
        """Recursively find symbols containing the line."""
        result = []
        for sym in syms:
            if sym.line <= target_line <= sym.end_line:
                # Check children first (inner symbols)
                child_matches = find_containing(sym.children, target_line)
                if child_matches:
                    result.extend(child_matches)
                result.append(sym.name)
        return result

    return find_containing(symbols, line)
