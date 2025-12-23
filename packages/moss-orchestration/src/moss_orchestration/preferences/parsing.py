"""Session log parsing for preference extraction.

Supports multiple agent log formats via the parser plugin system:
- Claude Code (JSONL with Anthropic message format)
- Gemini CLI (JSONL)
- GitHub Copilot (VSCode extension logs)
- Cline (VSCode extension, JSONL)
- Roo Code (JSONL)
- Aider (markdown-based chat logs)
- Generic/custom formats

The parser auto-detects format or can be explicitly specified.
Parsers are registered as plugins and can be extended via entry points.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any


class LogFormat(Enum):
    """Supported log formats."""

    CLAUDE_CODE = "claude_code"
    GEMINI_CLI = "gemini_cli"
    GITHUB_COPILOT = "github_copilot"
    CLINE = "cline"
    ROO_CODE = "roo_code"
    AIDER = "aider"
    GENERIC_JSONL = "generic_jsonl"
    GENERIC_CHAT = "generic_chat"
    AUTO = "auto"


# Tool name normalization - map various tool names to canonical forms
TOOL_NAME_MAP = {
    # File operations
    "write": "Write",
    "edit": "Edit",
    "read": "Read",
    "create_file": "Write",
    "update_file": "Edit",
    "read_file": "Read",
    "write_to_file": "Write",
    "replace_in_file": "Edit",
    "insert_code_block": "Edit",
    # Search operations
    "glob": "Glob",
    "grep": "Grep",
    "search": "Grep",
    "find_files": "Glob",
    "search_files": "Grep",
    "list_files": "Glob",
    "list_code_definition_names": "Glob",
    # Shell operations
    "bash": "Bash",
    "execute_command": "Bash",
    "run_terminal_command": "Bash",
    "terminal": "Bash",
    # Browser operations
    "browser_action": "Browser",
    "web_search": "WebSearch",
}


def normalize_tool_name(name: str) -> str:
    """Normalize tool names across different agents."""
    lower = name.lower().replace("-", "_").replace(" ", "_")
    return TOOL_NAME_MAP.get(lower, name)


@dataclass
class ToolCall:
    """A tool call from the assistant."""

    name: str
    input: dict[str, Any]
    id: str
    timestamp: str | None = None

    def __post_init__(self) -> None:
        # Normalize tool name
        self.name = normalize_tool_name(self.name)

    @property
    def is_file_write(self) -> bool:
        """Check if this is a file write operation."""
        return self.name in ("Write", "Edit", "NotebookEdit")

    @property
    def is_file_read(self) -> bool:
        """Check if this is a file read operation."""
        return self.name in ("Read", "Glob", "Grep")

    @property
    def target_file(self) -> str | None:
        """Get the target file path if this is a file operation."""
        return self.input.get("file_path") or self.input.get("path") or self.input.get("filename")


@dataclass
class ToolResult:
    """Result of a tool call."""

    tool_use_id: str
    content: str
    is_error: bool = False


@dataclass
class Turn:
    """A single turn in a conversation."""

    role: str  # "user" or "assistant"
    content: str
    tool_calls: list[ToolCall] = field(default_factory=list)
    tool_results: list[ToolResult] = field(default_factory=list)
    timestamp: str | None = None
    request_id: str | None = None

    @property
    def has_tool_calls(self) -> bool:
        return bool(self.tool_calls)

    @property
    def files_written(self) -> list[str]:
        """Get list of files written in this turn."""
        return [tc.target_file for tc in self.tool_calls if tc.is_file_write and tc.target_file]


@dataclass
class TurnPair:
    """An assistant turn followed by a user turn.

    Used to detect corrections: if user edits the same file after assistant writes it.
    """

    assistant: Turn
    user: Turn | None

    @property
    def is_correction(self) -> bool:
        """Check if the user turn appears to be a correction."""
        if not self.user:
            return False

        user_text = self.user.content.lower()

        # Look for correction signals in user response
        correction_signals = [
            "no",
            "wrong",
            "actually",
            "instead",
            "but",
            "that's not",
            "don't",
            "shouldn't",
            "change",
            "fix",
            "undo",
            "revert",
        ]
        return any(signal in user_text for signal in correction_signals)

    @property
    def files_overlap(self) -> bool:
        """Check if user edited same files assistant wrote."""
        assistant_files = set(self.assistant.files_written)
        if not self.user:
            return False
        user_files = set(self.user.files_written)
        return bool(assistant_files & user_files)


@dataclass
class ParsedSession:
    """A parsed session log with structured data."""

    path: Path = field(default_factory=Path)
    format: LogFormat = LogFormat.AUTO
    turns: list[Turn] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def turn_pairs(self) -> list[TurnPair]:
        """Get assistant-user turn pairs for correction detection."""
        pairs: list[TurnPair] = []
        i = 0
        while i < len(self.turns):
            turn = self.turns[i]
            if turn.role == "assistant":
                # Find next user turn
                user_turn = None
                if i + 1 < len(self.turns) and self.turns[i + 1].role == "user":
                    user_turn = self.turns[i + 1]
                pairs.append(TurnPair(assistant=turn, user=user_turn))
            i += 1
        return pairs

    def user_messages(self) -> list[Turn]:
        """Get all user turns."""
        return [t for t in self.turns if t.role == "user"]

    def assistant_messages(self) -> list[Turn]:
        """Get all assistant turns."""
        return [t for t in self.turns if t.role == "assistant"]


def parse_session(
    path: str | Path,
    format: LogFormat = LogFormat.AUTO,
) -> ParsedSession:
    """Parse a session log file.

    Args:
        path: Path to the session file
        format: Log format (auto-detect if AUTO)

    Returns:
        ParsedSession with structured turn data
    """
    from moss_orchestration.preferences.parsers import detect_parser, get_parser

    path = Path(path)

    if format == LogFormat.AUTO:
        parser_class = detect_parser(path)
    else:
        # Map LogFormat to parser name
        format_to_name = {
            LogFormat.CLAUDE_CODE: "claude_code",
            LogFormat.CLINE: "cline",
            LogFormat.ROO_CODE: "roo_code",
            LogFormat.GEMINI_CLI: "gemini_cli",
            LogFormat.AIDER: "aider",
            LogFormat.GENERIC_JSONL: "generic_jsonl",
            LogFormat.GENERIC_CHAT: "generic_chat",
        }
        parser_name = format_to_name.get(format, "generic_chat")
        parser_class = get_parser(parser_name)

    parser = parser_class(path)
    return parser.parse()


def parse_sessions(
    paths: list[str | Path],
    format: LogFormat = LogFormat.AUTO,
) -> list[ParsedSession]:
    """Parse multiple session files.

    Args:
        paths: List of paths to session files
        format: Log format (auto-detect if AUTO)

    Returns:
        List of ParsedSession objects
    """
    return [parse_session(p, format=format) for p in paths]
