"""Plugin Architecture: Extensible view provider system.

This module provides the plugin infrastructure for Moss view providers,
enabling multi-language support and third-party extensions.

Key components:
- PluginMetadata: Describes a plugin's capabilities
- ViewPlugin: Protocol that plugins must implement
- PluginRegistry: Discovers and manages plugins
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Protocol, runtime_checkable

if TYPE_CHECKING:
    from moss.views import View, ViewOptions, ViewTarget

logger = logging.getLogger(__name__)


# =============================================================================
# Plugin Metadata
# =============================================================================


@dataclass(frozen=True)
class PluginMetadata:
    """Metadata describing a view plugin's capabilities.

    Attributes:
        name: Unique identifier for the plugin (e.g., "python-skeleton")
        view_type: Type of view produced (e.g., "skeleton", "cfg", "dependency")
        languages: Languages supported (empty frozenset means all languages)
        priority: Selection priority (higher = preferred when multiple match)
        version: Plugin version string
        description: Human-readable description
    """

    name: str
    view_type: str
    languages: frozenset[str] = field(default_factory=frozenset)
    priority: int = 0
    version: str = "0.1.0"
    description: str = ""


# =============================================================================
# Plugin Protocol
# =============================================================================


@runtime_checkable
class ViewPlugin(Protocol):
    """Protocol that view plugins must implement.

    Plugins provide views of code (skeletons, CFGs, dependencies, etc.)
    for one or more programming languages.
    """

    @property
    def metadata(self) -> PluginMetadata:
        """Plugin metadata describing capabilities."""
        ...

    def supports(self, target: ViewTarget) -> bool:
        """Check if this plugin can handle the given target.

        Args:
            target: The file/symbol to render

        Returns:
            True if this plugin can render a view for the target
        """
        ...

    async def render(
        self,
        target: ViewTarget,
        options: ViewOptions | None = None,
    ) -> View:
        """Render a view for the target.

        Args:
            target: The file/symbol to render
            options: Optional rendering options

        Returns:
            The rendered view
        """
        ...


# =============================================================================
# Plugin Registry
# =============================================================================


class PluginRegistry:
    """Registry for discovering and managing view plugins.

    The registry supports:
    - Manual plugin registration
    - Automatic discovery via entry points
    - Priority-based plugin selection
    - Language-aware provider matching
    """

    def __init__(self) -> None:
        """Initialize an empty registry."""
        self._plugins: dict[str, ViewPlugin] = {}
        self._by_view_type: dict[str, list[ViewPlugin]] = {}
        self._discovered = False

    def register(self, plugin: ViewPlugin) -> None:
        """Register a plugin.

        Args:
            plugin: The plugin to register

        Raises:
            ValueError: If a plugin with the same name is already registered
        """
        meta = plugin.metadata

        if meta.name in self._plugins:
            raise ValueError(f"Plugin '{meta.name}' is already registered")

        self._plugins[meta.name] = plugin

        # Index by view type
        if meta.view_type not in self._by_view_type:
            self._by_view_type[meta.view_type] = []
        self._by_view_type[meta.view_type].append(plugin)

        # Keep sorted by priority (descending)
        self._by_view_type[meta.view_type].sort(
            key=lambda p: p.metadata.priority,
            reverse=True,
        )

        logger.debug(
            "Registered plugin: %s (view_type=%s, priority=%d)",
            meta.name,
            meta.view_type,
            meta.priority,
        )

    def unregister(self, name: str) -> bool:
        """Unregister a plugin by name.

        Args:
            name: Plugin name to unregister

        Returns:
            True if plugin was found and removed, False otherwise
        """
        plugin = self._plugins.pop(name, None)
        if plugin is None:
            return False

        view_type = plugin.metadata.view_type
        if view_type in self._by_view_type:
            self._by_view_type[view_type] = [
                p for p in self._by_view_type[view_type] if p.metadata.name != name
            ]
        return True

    def get_plugin(self, name: str) -> ViewPlugin | None:
        """Get a plugin by name.

        Args:
            name: Plugin name

        Returns:
            The plugin, or None if not found
        """
        return self._plugins.get(name)

    def find_plugin(
        self,
        target: ViewTarget,
        view_type: str,
    ) -> ViewPlugin | None:
        """Find the best plugin for a target and view type.

        Selection is based on:
        1. View type match
        2. Plugin.supports() returning True
        3. Priority (highest wins)

        Args:
            target: The file/symbol to render
            view_type: The type of view requested

        Returns:
            The best matching plugin, or None if no match
        """
        candidates = self._by_view_type.get(view_type, [])

        for plugin in candidates:  # Already sorted by priority
            if plugin.supports(target):
                return plugin

        return None

    def get_plugins_for_view_type(self, view_type: str) -> list[ViewPlugin]:
        """Get all plugins that produce a given view type.

        Args:
            view_type: The view type to query

        Returns:
            List of plugins, sorted by priority (highest first)
        """
        return list(self._by_view_type.get(view_type, []))

    def get_all_plugins(self) -> list[ViewPlugin]:
        """Get all registered plugins.

        Returns:
            List of all plugins
        """
        return list(self._plugins.values())

    def get_supported_view_types(self) -> set[str]:
        """Get all view types that have registered plugins.

        Returns:
            Set of view type strings
        """
        return set(self._by_view_type.keys())

    def discover_plugins(self) -> int:
        """Discover and register plugins via entry points.

        Looks for entry points in the "moss.plugins" group.
        Each entry point should be a callable that returns a ViewPlugin.

        Returns:
            Number of plugins discovered and registered
        """
        if self._discovered:
            return 0

        count = 0

        try:
            from importlib.metadata import entry_points

            # Python 3.10+ returns SelectableGroups
            eps = entry_points(group="moss.plugins")

            for ep in eps:
                try:
                    plugin_factory = ep.load()
                    plugin = plugin_factory()

                    if isinstance(plugin, ViewPlugin):
                        self.register(plugin)
                        count += 1
                        logger.info("Discovered plugin: %s", ep.name)
                    else:
                        logger.warning(
                            "Entry point '%s' did not return a ViewPlugin",
                            ep.name,
                        )
                except (ImportError, AttributeError, TypeError) as e:
                    logger.warning("Failed to load plugin '%s': %s", ep.name, e)

        except ImportError:
            logger.debug("importlib.metadata not available, skipping discovery")

        self._discovered = True
        return count

    def register_builtins(self) -> None:
        """Register built-in plugins.

        This provides fallback registration for plugins that may not be
        installed via entry points (e.g., during development).
        """
        # Import here to avoid circular imports
        from moss.cfg import PythonCFGPlugin
        from moss.dependencies import PythonDependencyPlugin
        from moss.skeleton import PythonSkeletonPlugin

        builtins: list[ViewPlugin] = [
            PythonSkeletonPlugin(),
            PythonDependencyPlugin(),
            PythonCFGPlugin(),
        ]

        # Try to add tree-sitter plugin (optional dependency)
        try:
            from moss.plugins.tree_sitter import TreeSitterSkeletonPlugin

            builtins.append(TreeSitterSkeletonPlugin())
        except ImportError:
            logger.debug("Tree-sitter plugin not available")

        # Add non-code content plugins
        from moss.plugins.data_files import JSONSchemaPlugin, TOMLSchemaPlugin, YAMLSchemaPlugin
        from moss.plugins.markdown import MarkdownStructurePlugin

        builtins.extend(
            [
                MarkdownStructurePlugin(),
                JSONSchemaPlugin(),
                YAMLSchemaPlugin(),
                TOMLSchemaPlugin(),
            ]
        )

        for plugin in builtins:
            if plugin.metadata.name not in self._plugins:
                self.register(plugin)

    def ensure_initialized(self) -> None:
        """Ensure the registry has discovered plugins.

        Safe to call multiple times.
        """
        if not self._discovered:
            self.discover_plugins()
            self.register_builtins()


# =============================================================================
# Global Registry
# =============================================================================

_global_registry: PluginRegistry | None = None


def get_registry() -> PluginRegistry:
    """Get the global plugin registry.

    Creates and initializes the registry on first call.

    Returns:
        The global PluginRegistry instance
    """
    global _global_registry

    if _global_registry is None:
        _global_registry = PluginRegistry()
        _global_registry.ensure_initialized()

    return _global_registry


def reset_registry() -> None:
    """Reset the global registry (mainly for testing)."""
    global _global_registry
    _global_registry = None


# =============================================================================
# Helper Functions
# =============================================================================


def detect_language(path: Path) -> str:
    """Detect programming language from file extension.

    Args:
        path: File path

    Returns:
        Language identifier (e.g., "python", "typescript")
    """
    ext_map = {
        ".py": "python",
        ".pyi": "python",
        ".js": "javascript",
        ".mjs": "javascript",
        ".cjs": "javascript",
        ".jsx": "javascript",
        ".ts": "typescript",
        ".tsx": "typescript",
        ".mts": "typescript",
        ".cts": "typescript",
        ".go": "go",
        ".rs": "rust",
        ".java": "java",
        ".c": "c",
        ".h": "c",
        ".cpp": "cpp",
        ".cc": "cpp",
        ".cxx": "cpp",
        ".hpp": "cpp",
        ".hxx": "cpp",
        ".rb": "ruby",
        ".md": "markdown",
        ".json": "json",
        ".yaml": "yaml",
        ".yml": "yaml",
        ".toml": "toml",
    }
    return ext_map.get(path.suffix.lower(), "unknown")


# Export tree-sitter plugin for direct import
try:
    from moss.plugins.tree_sitter import TreeSitterSkeletonPlugin
except ImportError:
    TreeSitterSkeletonPlugin = None  # type: ignore

__all__ = [
    "PluginMetadata",
    "PluginRegistry",
    "TreeSitterSkeletonPlugin",
    "ViewPlugin",
    "detect_language",
    "get_registry",
    "reset_registry",
]
