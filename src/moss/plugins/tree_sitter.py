"""Tree-sitter based plugins for multi-language support.

These plugins use tree-sitter for parsing, enabling support for
TypeScript, JavaScript, Go, Rust, and other languages.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from moss.plugins import PluginMetadata
    from moss.tree_sitter import TSSymbol
    from moss.views import View, ViewOptions, ViewTarget

logger = logging.getLogger(__name__)

# Languages supported by tree-sitter skeleton extraction
SUPPORTED_LANGUAGES = frozenset(
    [
        "typescript",
        "javascript",
        "python",
        "go",
        "rust",
    ]
)


class TreeSitterSkeletonPlugin:
    """Multi-language skeleton extraction using tree-sitter.

    This plugin provides skeleton views for multiple programming languages
    using tree-sitter for parsing. It has higher priority than language-specific
    AST-based providers, so it will be preferred when tree-sitter is available.
    """

    @property
    def metadata(self) -> PluginMetadata:
        from moss.plugins import PluginMetadata

        return PluginMetadata(
            name="tree-sitter-skeleton",
            view_type="skeleton",
            languages=SUPPORTED_LANGUAGES,
            priority=10,  # Higher than AST-based providers
            version="0.1.0",
            description="Multi-language skeleton extraction via tree-sitter",
        )

    def supports(self, target: ViewTarget) -> bool:
        """Check if this plugin can handle the target."""
        from moss.plugins import detect_language

        if not target.path.exists():
            return False

        # Check if tree-sitter is available
        if not self._is_tree_sitter_available():
            return False

        lang = target.language or detect_language(target.path)
        return lang in SUPPORTED_LANGUAGES

    def _is_tree_sitter_available(self) -> bool:
        """Check if tree-sitter dependencies are installed."""
        try:
            import tree_sitter  # noqa: F401

            return True
        except ImportError:
            return False

    async def render(
        self,
        target: ViewTarget,
        options: ViewOptions | None = None,
    ) -> View:
        """Render a skeleton view using tree-sitter."""
        from moss.plugins import detect_language
        from moss.views import View, ViewType

        source = target.path.read_text()
        lang = target.language or detect_language(target.path)

        try:
            from moss.tree_sitter import TreeSitterSkeletonProvider

            provider = TreeSitterSkeletonProvider(lang)
            symbols = provider.extract_skeleton(source)
            content = provider.format_skeleton(symbols)

            return View(
                target=target,
                view_type=ViewType.SKELETON,
                content=content,
                metadata={
                    "symbol_count": len(symbols),
                    "symbols": [_ts_symbol_to_dict(s) for s in symbols],
                    "language": lang,
                    "provider": "tree-sitter",
                },
            )

        except ImportError as e:
            # Tree-sitter not available, return error
            return View(
                target=target,
                view_type=ViewType.SKELETON,
                content=f"# Tree-sitter not available: {e}",
                metadata={"error": str(e)},
            )

        except (RuntimeError, OSError, ValueError) as e:
            # Parse or extraction error
            logger.warning("Tree-sitter extraction failed for %s: %s", target.path, e)
            return View(
                target=target,
                view_type=ViewType.SKELETON,
                content=f"# Parse error: {e}",
                metadata={"error": str(e)},
            )


def _ts_symbol_to_dict(symbol: TSSymbol) -> dict:
    """Convert a TSSymbol to a serializable dictionary."""
    result = {
        "name": symbol.name,
        "kind": symbol.kind,
        "line": symbol.line,
        "end_line": symbol.end_line,
    }
    if symbol.signature:
        result["signature"] = symbol.signature
    if symbol.docstring:
        result["docstring"] = symbol.docstring
    if symbol.visibility:
        result["visibility"] = symbol.visibility
    if symbol.parent:
        result["parent"] = symbol.parent
    if symbol.children:
        result["children"] = [_ts_symbol_to_dict(c) for c in symbol.children]
    return result
