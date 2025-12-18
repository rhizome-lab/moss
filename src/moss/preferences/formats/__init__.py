"""Output format adapters for preferences.

Converts PreferenceSet to various agent instruction formats:
- claude: CLAUDE.md for Claude Code
- gemini: GEMINI.md for Gemini CLI
- antigravity: .agent/rules/*.md for Google Antigravity
- cursor: .cursorrules for Cursor
- generic: Plain markdown
- json: Structured JSON
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Protocol

from moss.preferences.formats.antigravity import AntigravityAdapter
from moss.preferences.formats.claude import ClaudeAdapter
from moss.preferences.formats.cursor import CursorAdapter
from moss.preferences.formats.gemini import GeminiAdapter
from moss.preferences.formats.generic import GenericAdapter, JSONAdapter

if TYPE_CHECKING:
    from moss.preferences.models import PreferenceSet


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
ADAPTERS: dict[str, type[FormatAdapter]] = {
    "claude": ClaudeAdapter,
    "gemini": GeminiAdapter,
    "antigravity": AntigravityAdapter,
    "cursor": CursorAdapter,
    "generic": GenericAdapter,
    "json": JSONAdapter,
}


def get_adapter(format_name: str) -> FormatAdapter:
    """Get a format adapter by name.

    Args:
        format_name: Name of the format

    Returns:
        FormatAdapter instance

    Raises:
        ValueError: If format not found
    """
    if format_name not in ADAPTERS:
        available = ", ".join(ADAPTERS.keys())
        raise ValueError(f"Unknown format '{format_name}'. Available: {available}")

    return ADAPTERS[format_name]()


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
]
