"""View Provider Protocol: Dynamic context views for code understanding."""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum, auto
from pathlib import Path
from typing import Any


class ViewType(Enum):
    """Standard view types."""

    SKELETON = auto()  # Class/function signatures only
    CFG = auto()  # Control flow graph
    DEPENDENCY = auto()  # Import/export relationships
    ELIDED = auto()  # Literals replaced with placeholders
    SNIPPET = auto()  # Intelligently elided snippets
    RAW = auto()  # Full source text


class Intent(Enum):
    """User intent for view selection heuristics."""

    EXPLORE = auto()  # Understanding codebase structure
    DEBUG = auto()  # Finding bugs
    REFACTOR = auto()  # Restructuring code
    EDIT = auto()  # Making specific changes


@dataclass(frozen=True)
class ViewOptions:
    """Options for rendering a view."""

    max_depth: int | None = None  # For hierarchical views
    include_private: bool = False  # Include _private members
    include_docstrings: bool = True  # Include documentation
    line_range: tuple[int, int] | None = None  # Restrict to line range
    extra: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class ViewTarget:
    """Target for a view render operation."""

    path: Path
    language: str | None = None  # Auto-detect if None
    symbol: str | None = None  # Specific symbol to focus on


@dataclass
class View:
    """Rendered view of a target."""

    target: ViewTarget
    view_type: ViewType
    content: str
    metadata: dict[str, Any] = field(default_factory=dict)

    @property
    def token_estimate(self) -> int:
        """Rough estimate of tokens (words / 0.75)."""
        return int(len(self.content.split()) / 0.75)


class ViewProvider(ABC):
    """Abstract base for view providers."""

    @property
    @abstractmethod
    def view_type(self) -> ViewType:
        """The type of view this provider produces."""
        ...

    @property
    @abstractmethod
    def supported_languages(self) -> set[str]:
        """Languages this provider supports. Empty means all."""
        ...

    def supports(self, target: ViewTarget) -> bool:
        """Check if this provider can render the target."""
        if not target.path.exists():
            return False
        if not self.supported_languages:
            return True
        if target.language:
            return target.language in self.supported_languages
        # Try to detect language from extension
        return self._detect_language(target.path) in self.supported_languages

    def _detect_language(self, path: Path) -> str:
        """Detect language from file extension."""
        ext_map = {
            ".py": "python",
            ".js": "javascript",
            ".ts": "typescript",
            ".tsx": "typescript",
            ".jsx": "javascript",
            ".rs": "rust",
            ".go": "go",
            ".java": "java",
            ".c": "c",
            ".cpp": "cpp",
            ".h": "c",
            ".hpp": "cpp",
            ".rb": "ruby",
        }
        return ext_map.get(path.suffix, "unknown")

    @abstractmethod
    async def render(self, target: ViewTarget, options: ViewOptions | None = None) -> View:
        """Render the view for the target."""
        ...


class RawViewProvider(ViewProvider):
    """Provider that returns raw file content."""

    @property
    def view_type(self) -> ViewType:
        return ViewType.RAW

    @property
    def supported_languages(self) -> set[str]:
        return set()  # All languages

    async def render(self, target: ViewTarget, options: ViewOptions | None = None) -> View:
        """Read and return raw file content."""
        opts = options or ViewOptions()
        content = target.path.read_text()

        # Apply line range if specified
        if opts.line_range:
            lines = content.splitlines()
            start, end = opts.line_range
            content = "\n".join(lines[start - 1 : end])

        return View(
            target=target,
            view_type=ViewType.RAW,
            content=content,
            metadata={"lines": len(content.splitlines())},
        )


class SnippetViewProvider(ViewProvider):
    """Provider that returns intelligently elided snippets.

    Preserves lines matching 'anchors' (definitions, search hits)
    and elides large unimportant blocks.
    """

    @property
    def view_type(self) -> ViewType:
        return ViewType.SNIPPET

    @property
    def supported_languages(self) -> set[str]:
        return set()  # All languages

    async def render(self, target: ViewTarget, options: ViewOptions | None = None) -> View:
        import re

        opts = options or ViewOptions()
        content = target.path.read_text()
        lines = content.splitlines()

        # Determine anchor lines to preserve
        anchor_patterns = [
            r"^(class|def|async\s+def)\s+\w+",  # Python definitions
            r"^(export\s+)?(function|class|interface|type|enum)\s+\w+",  # JS/TS
            r"^fn\s+\w+",  # Rust
        ]

        # Add search pattern if provided in extra
        search_pattern = opts.extra.get("search_pattern")
        if search_pattern:
            anchor_patterns.append(search_pattern)

        preserved_indices = set()
        context_lines = opts.extra.get("context_lines", 2)

        for i, line in enumerate(lines):
            if any(re.search(p, line) for p in anchor_patterns):
                # Preserve line plus context
                start = max(0, i - context_lines)
                end = min(len(lines), i + context_lines + 1)
                for j in range(start, end):
                    preserved_indices.add(j)

        # Construct snippet with elision markers
        result_lines = []
        last_idx = -1
        elided_count = 0

        for i in range(len(lines)):
            if i in preserved_indices:
                if last_idx != -1 and i > last_idx + 1:
                    skip = i - last_idx - 1
                    result_lines.append(f"... [{skip} lines elided] ...")
                    elided_count += skip
                result_lines.append(lines[i])
                last_idx = i

        # If no anchors found, fallback to raw or top of file
        if not result_lines:
            content = "\n".join(lines[:50]) + "\n... [truncated] ..."
        else:
            content = "\n".join(result_lines)

        return View(
            target=target,
            view_type=ViewType.SNIPPET,
            content=content,
            metadata={
                "original_lines": len(lines),
                "elided_lines": elided_count,
                "preserved_lines": len(result_lines),
            },
        )


# Intent -> ViewType heuristics
INTENT_DEFAULTS: dict[Intent, list[ViewType]] = {
    Intent.EXPLORE: [ViewType.SKELETON, ViewType.DEPENDENCY],
    Intent.DEBUG: [ViewType.CFG, ViewType.RAW],
    Intent.REFACTOR: [ViewType.DEPENDENCY, ViewType.SKELETON],
    Intent.EDIT: [ViewType.RAW],
}


class ViewRegistry:
    """Registry for view providers."""

    def __init__(self) -> None:
        self._providers: dict[ViewType, list[ViewProvider]] = {}

    def register(self, provider: ViewProvider) -> None:
        """Register a view provider."""
        vtype = provider.view_type
        if vtype not in self._providers:
            self._providers[vtype] = []
        self._providers[vtype].append(provider)

    def get_providers(self, view_type: ViewType) -> list[ViewProvider]:
        """Get all providers for a view type."""
        return self._providers.get(view_type, [])

    def find_provider(self, target: ViewTarget, view_type: ViewType) -> ViewProvider | None:
        """Find a provider that supports the target."""
        for provider in self.get_providers(view_type):
            if provider.supports(target):
                return provider
        return None

    def suggest_views(self, intent: Intent) -> list[ViewType]:
        """Suggest view types based on intent."""
        return INTENT_DEFAULTS.get(intent, [ViewType.RAW])

    async def render(
        self,
        target: ViewTarget,
        view_type: ViewType,
        options: ViewOptions | None = None,
    ) -> View | None:
        """Render a view using an appropriate provider."""
        provider = self.find_provider(target, view_type)
        if provider:
            return await provider.render(target, options)
        return None

    async def render_multi(
        self,
        target: ViewTarget,
        view_types: list[ViewType],
        options: ViewOptions | None = None,
    ) -> list[View]:
        """Render multiple views for a target."""
        views = []
        for vtype in view_types:
            view = await self.render(target, vtype, options)
            if view:
                views.append(view)
        return views


def create_default_registry() -> ViewRegistry:
    """Create a registry with default providers."""
    registry = ViewRegistry()
    registry.register(RawViewProvider())
    registry.register(SnippetViewProvider())
    return registry
