"""Output format adapters for preferences.

Converts PreferenceSet to various agent instruction formats:
- claude: CLAUDE.md for Claude Code
- gemini: GEMINI.md for Gemini CLI
- antigravity: .agent/rules/*.md for Google Antigravity
- cursor: .cursorrules for Cursor
- generic: Plain markdown
- json: Structured JSON

Adapters can be registered via entry points or programmatically.

Entry point group: moss.preferences.formats

Example plugin registration in pyproject.toml:
    [project.entry-points."moss.preferences.formats"]
    my_format = "my_package.formats:MyAdapter"
"""

from __future__ import annotations

from importlib.metadata import entry_points
from typing import TYPE_CHECKING, Protocol

from moss_orchestration.preferences.formats.antigravity import AntigravityAdapter
from moss_orchestration.preferences.formats.claude import ClaudeAdapter
from moss_orchestration.preferences.formats.cursor import CursorAdapter
from moss_orchestration.preferences.formats.gemini import GeminiAdapter
from moss_orchestration.preferences.formats.generic import GenericAdapter, JSONAdapter

if TYPE_CHECKING:
    from moss_orchestration.preferences.models import PreferenceSet


class FormatAdapter(Protocol):
    """Protocol for format adapters."""

    def format(self, prefs: PreferenceSet) -> str:
        """Format preferences to the target format.

        Args:
            prefs: PreferenceSet to format

        Returns:
            Formatted string
        """
        ...


# Adapter registry
_ADAPTERS: dict[str, type[FormatAdapter]] = {}


def register_adapter(name: str, adapter_class: type[FormatAdapter]) -> None:
    """Register a format adapter.

    Args:
        name: Adapter name (e.g., "claude", "json")
        adapter_class: Adapter class implementing FormatAdapter protocol
    """
    _ADAPTERS[name] = adapter_class


def list_adapters() -> list[str]:
    """List all registered adapter names."""
    return list(_ADAPTERS.keys())


def get_adapter(format_name: str) -> FormatAdapter:
    """Get a format adapter by name.

    Args:
        format_name: Name of the format

    Returns:
        FormatAdapter instance

    Raises:
        ValueError: If format not found
    """
    if format_name not in _ADAPTERS:
        available = ", ".join(_ADAPTERS.keys())
        raise ValueError(f"Unknown format '{format_name}'. Available: {available}")

    return _ADAPTERS[format_name]()


def format_preferences(prefs: PreferenceSet, format_name: str = "generic") -> str:
    """Format preferences using the specified adapter.

    Args:
        prefs: PreferenceSet to format
        format_name: Output format name

    Returns:
        Formatted string
    """
    adapter = get_adapter(format_name)
    return adapter.format(prefs)


def _discover_entry_points() -> None:
    """Discover and register adapters from entry points."""
    try:
        eps = entry_points(group="moss.preferences.formats")
        for ep in eps:
            try:
                adapter_class = ep.load()
                if ep.name not in _ADAPTERS:
                    register_adapter(ep.name, adapter_class)
            except (ImportError, AttributeError, TypeError):
                pass
    except (TypeError, StopIteration):
        pass


def _register_builtin_adapters() -> None:
    """Register built-in format adapters."""
    register_adapter("claude", ClaudeAdapter)
    register_adapter("gemini", GeminiAdapter)
    register_adapter("antigravity", AntigravityAdapter)
    register_adapter("cursor", CursorAdapter)
    register_adapter("generic", GenericAdapter)
    register_adapter("json", JSONAdapter)


# Auto-register on import
_register_builtin_adapters()
_discover_entry_points()

# Backwards compatibility alias
ADAPTERS = _ADAPTERS

__all__ = [
    "ADAPTERS",
    "AntigravityAdapter",
    "ClaudeAdapter",
    "CursorAdapter",
    "FormatAdapter",
    "GeminiAdapter",
    "GenericAdapter",
    "JSONAdapter",
    "format_preferences",
    "get_adapter",
    "list_adapters",
    "register_adapter",
]
