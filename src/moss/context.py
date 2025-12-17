"""Context Host: View compilation pipeline and context management."""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from moss.views import (
    Intent,
    RawViewProvider,
    View,
    ViewOptions,
    ViewRegistry,
    ViewTarget,
    ViewType,
)

logger = logging.getLogger(__name__)


@dataclass
class StaticContext:
    """Static context files to inject into compilation."""

    architecture_docs: list[Path] = field(default_factory=list)
    style_guides: list[Path] = field(default_factory=list)
    pinned_files: list[Path] = field(default_factory=list)


@dataclass
class CompiledContext:
    """Result of context compilation."""

    views: list[View]
    static_context: dict[str, str]  # filename -> content
    metadata: dict[str, Any] = field(default_factory=dict)

    @property
    def total_tokens(self) -> int:
        """Estimate total tokens in compiled context."""
        view_tokens = sum(v.token_estimate for v in self.views)
        static_tokens = sum(
            int(len(content.split()) / 0.75) for content in self.static_context.values()
        )
        return view_tokens + static_tokens

    def to_prompt(self, separator: str = "\n\n---\n\n") -> str:
        """Format compiled context as a single prompt string."""
        parts = []

        # Add static context first
        for name, content in self.static_context.items():
            parts.append(f"# {name}\n{content}")

        # Add views
        for view in self.views:
            header = f"# {view.target.path.name} ({view.view_type.name})"
            parts.append(f"{header}\n{view.content}")

        return separator.join(parts)


class ContextHost:
    """Manages view compilation and context injection."""

    def __init__(self, registry: ViewRegistry | None = None):
        self.registry = registry or self._create_default_registry()
        self.static_context = StaticContext()
        self._token_budget: int | None = None

    @staticmethod
    def _create_default_registry() -> ViewRegistry:
        """Create registry with default providers via plugin discovery.

        Uses the PluginRegistry to discover available plugins and registers
        their underlying providers with the ViewRegistry. This enables
        automatic discovery of installed plugins while maintaining the
        ViewType-based interface.
        """
        from moss.plugins import get_registry as get_plugin_registry

        registry = ViewRegistry()

        # Always register the raw view provider
        registry.register(RawViewProvider())

        # Use plugin registry to discover providers
        plugin_registry = get_plugin_registry()

        # Map plugin view types to ViewType enum
        view_type_map = {
            "skeleton": ViewType.SKELETON,
            "dependency": ViewType.DEPENDENCY,
            "cfg": ViewType.CFG,
        }

        # Register plugins that have matching providers
        for plugin in plugin_registry.get_all_plugins():
            meta = plugin.metadata
            view_type = view_type_map.get(meta.view_type)

            if view_type is None:
                logger.debug(
                    "Skipping plugin %s: no ViewType mapping for %s",
                    meta.name,
                    meta.view_type,
                )
                continue

            # Create a wrapper that adapts the plugin to ViewProvider interface
            wrapper = _PluginViewProviderWrapper(plugin, view_type)
            try:
                registry.register(wrapper)
                logger.debug("Registered plugin %s for %s", meta.name, view_type.name)
            except Exception as e:
                logger.warning("Failed to register plugin %s: %s", meta.name, e)

        return registry

    def set_token_budget(self, budget: int | None) -> None:
        """Set maximum tokens for compiled context."""
        self._token_budget = budget

    def add_architecture_doc(self, path: Path) -> None:
        """Add an architecture document to static context."""
        self.static_context.architecture_docs.append(path)

    def add_style_guide(self, path: Path) -> None:
        """Add a style guide to static context."""
        self.static_context.style_guides.append(path)

    def add_pinned_file(self, path: Path) -> None:
        """Add a pinned file that's always included."""
        self.static_context.pinned_files.append(path)

    def _load_static_context(self) -> dict[str, str]:
        """Load all static context files."""
        result = {}

        for doc in self.static_context.architecture_docs:
            if doc.exists():
                result[f"arch/{doc.name}"] = doc.read_text()

        for guide in self.static_context.style_guides:
            if guide.exists():
                result[f"style/{guide.name}"] = guide.read_text()

        for pinned in self.static_context.pinned_files:
            if pinned.exists():
                result[f"pinned/{pinned.name}"] = pinned.read_text()

        return result

    async def compile(
        self,
        targets: list[ViewTarget],
        *,
        intent: Intent | None = None,
        view_types: list[ViewType] | None = None,
        options: ViewOptions | None = None,
    ) -> CompiledContext:
        """Compile context for given targets.

        Args:
            targets: Files/symbols to include in context
            intent: User intent for automatic view selection
            view_types: Explicit view types to use (overrides intent)
            options: Options for view rendering

        Returns:
            CompiledContext with rendered views and static context
        """
        # Determine view types to use
        if view_types is None:
            if intent is not None:
                view_types = self.registry.suggest_views(intent)
            else:
                view_types = [ViewType.RAW]

        # Load static context
        static = self._load_static_context()

        # Render views for each target
        views: list[View] = []
        for target in targets:
            for vtype in view_types:
                view = await self.registry.render(target, vtype, options)
                if view:
                    views.append(view)
                    break  # Use first successful view type

        # Apply token budget if set
        if self._token_budget is not None:
            views, static = self._apply_budget(views, static)

        return CompiledContext(
            views=views,
            static_context=static,
            metadata={
                "intent": intent.name if intent else None,
                "view_types": [vt.name for vt in view_types],
                "target_count": len(targets),
            },
        )

    def _apply_budget(
        self,
        views: list[View],
        static: dict[str, str],
    ) -> tuple[list[View], dict[str, str]]:
        """Trim views and static context to fit budget."""
        if self._token_budget is None:
            return views, static

        budget = self._token_budget

        # Prioritize static context
        trimmed_static = {}
        for name, content in static.items():
            tokens = int(len(content.split()) / 0.75)
            if tokens <= budget:
                trimmed_static[name] = content
                budget -= tokens

        # Then add views
        trimmed_views = []
        for view in views:
            if view.token_estimate <= budget:
                trimmed_views.append(view)
                budget -= view.token_estimate

        return trimmed_views, trimmed_static

    async def compile_for_intent(
        self,
        targets: list[Path],
        intent: Intent,
        options: ViewOptions | None = None,
    ) -> CompiledContext:
        """Convenience method to compile with intent-based view selection."""
        view_targets = [ViewTarget(path=p) for p in targets]
        return await self.compile(view_targets, intent=intent, options=options)

    async def get_skeleton(
        self,
        path: Path,
        options: ViewOptions | None = None,
    ) -> View | None:
        """Get skeleton view for a file."""
        target = ViewTarget(path=path)
        return await self.registry.render(target, ViewType.SKELETON, options)

    async def get_dependencies(
        self,
        path: Path,
        options: ViewOptions | None = None,
    ) -> View | None:
        """Get dependency view for a file."""
        target = ViewTarget(path=path)
        return await self.registry.render(target, ViewType.DEPENDENCY, options)

    async def get_raw(
        self,
        path: Path,
        options: ViewOptions | None = None,
    ) -> View | None:
        """Get raw view for a file."""
        target = ViewTarget(path=path)
        return await self.registry.render(target, ViewType.RAW, options)


# =============================================================================
# Plugin Adapter
# =============================================================================


class _PluginViewProviderWrapper:
    """Adapts a ViewPlugin to the ViewProvider interface.

    This wrapper allows plugins discovered via PluginRegistry to be used
    with the ViewRegistry, which expects the ViewProvider ABC interface.
    """

    def __init__(self, plugin: Any, view_type: ViewType) -> None:
        """Initialize wrapper with a plugin and target ViewType.

        Args:
            plugin: A ViewPlugin instance
            view_type: The ViewType enum value this wrapper provides
        """
        self._plugin = plugin
        self._view_type = view_type

    @property
    def view_type(self) -> ViewType:
        """The type of view this provider produces."""
        return self._view_type

    @property
    def supported_languages(self) -> set[str]:
        """Languages this provider supports."""
        return set(self._plugin.metadata.languages)

    def supports(self, target: ViewTarget) -> bool:
        """Check if this provider can render the target."""
        return self._plugin.supports(target)

    async def render(
        self,
        target: ViewTarget,
        options: ViewOptions | None = None,
    ) -> View:
        """Render the view for the target."""
        return await self._plugin.render(target, options)
