"""Session log parser plugins.

Plugin-based architecture for parsing different agent log formats.
Parsers can be registered via entry points or programmatically.

Entry point group: moss.preferences.parsers

Example plugin registration in pyproject.toml:
    [project.entry-points."moss.preferences.parsers"]
    my_parser = "my_package.parsers:MyParser"
"""

from __future__ import annotations

from importlib.metadata import entry_points
from pathlib import Path
from typing import TYPE_CHECKING, Protocol, runtime_checkable

if TYPE_CHECKING:
    from moss.preferences.parsing import LogFormat, ParsedSession

# Parser plugin registry
_PARSERS: dict[str, type[ParserPlugin]] = {}


@runtime_checkable
class ParserPlugin(Protocol):
    """Protocol for session log parsers.

    Parsers convert agent session logs into structured ParsedSession objects.
    Each parser handles a specific log format (Claude Code, Gemini CLI, etc.).
    """

    format: LogFormat

    def __init__(self, path: Path) -> None:
        """Initialize parser with file path."""
        ...

    def parse(self) -> ParsedSession:
        """Parse the session file into structured data.

        Returns:
            ParsedSession with extracted turns and metadata
        """
        ...

    @classmethod
    def can_parse(cls, path: Path, sample: str) -> bool:
        """Check if this parser can handle the given file.

        Args:
            path: Path to the file
            sample: First ~4KB of file content for detection

        Returns:
            True if this parser can handle the file
        """
        ...


def register_parser(name: str, parser_class: type[ParserPlugin]) -> None:
    """Register a parser plugin.

    Args:
        name: Unique name for the parser
        parser_class: Parser class implementing ParserPlugin protocol
    """
    _PARSERS[name] = parser_class


def get_parser(name: str) -> type[ParserPlugin]:
    """Get a parser class by name.

    Args:
        name: Parser name

    Returns:
        Parser class

    Raises:
        ValueError: If parser not found
    """
    if name not in _PARSERS:
        available = ", ".join(_PARSERS.keys()) or "none"
        raise ValueError(f"Parser '{name}' not found. Available: {available}")
    return _PARSERS[name]


def list_parsers() -> list[str]:
    """List all registered parser names."""
    return list(_PARSERS.keys())


def get_all_parsers() -> list[type[ParserPlugin]]:
    """Get all registered parser classes in priority order."""
    # Return in registration order (specific parsers first, generic last)
    return list(_PARSERS.values())


def detect_parser(path: Path) -> type[ParserPlugin]:
    """Auto-detect the appropriate parser for a file.

    Args:
        path: Path to the session log file

    Returns:
        Parser class that can handle the file
    """
    from moss.preferences.parsers.generic import GenericChatParser

    try:
        with open(path, encoding="utf-8") as f:
            sample = f.read(4096)
    except OSError:
        return GenericChatParser

    for parser_class in get_all_parsers():
        if parser_class.can_parse(path, sample):
            return parser_class

    return GenericChatParser


def _discover_entry_points() -> None:
    """Discover and register parsers from entry points."""
    try:
        eps = entry_points(group="moss.preferences.parsers")
        for ep in eps:
            try:
                parser_class = ep.load()
                register_parser(ep.name, parser_class)
            except (ImportError, AttributeError, TypeError):
                pass  # Skip failed imports
    except (TypeError, StopIteration):
        pass


def _register_builtin_parsers() -> None:
    """Register built-in parsers."""
    from moss.preferences.parsers.aider import AiderParser
    from moss.preferences.parsers.claude_code import ClaudeCodeParser
    from moss.preferences.parsers.cline import ClineParser
    from moss.preferences.parsers.gemini_cli import GeminiCLIParser
    from moss.preferences.parsers.generic import GenericChatParser, GenericJSONLParser
    from moss.preferences.parsers.roo_code import RooCodeParser

    # Register in order of specificity (most specific first)
    register_parser("claude_code", ClaudeCodeParser)
    register_parser("cline", ClineParser)
    register_parser("roo_code", RooCodeParser)
    register_parser("gemini_cli", GeminiCLIParser)
    register_parser("aider", AiderParser)
    register_parser("generic_jsonl", GenericJSONLParser)
    register_parser("generic_chat", GenericChatParser)


# Auto-register on import
_register_builtin_parsers()
_discover_entry_points()


__all__ = [
    "ParserPlugin",
    "detect_parser",
    "get_all_parsers",
    "get_parser",
    "list_parsers",
    "register_parser",
]
