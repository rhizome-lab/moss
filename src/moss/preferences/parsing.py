"""Session log parsing for preference extraction.

Supports multiple agent log formats:
- Claude Code (JSONL with Anthropic message format)
- Gemini CLI (JSONL)
- GitHub Copilot (VSCode extension logs)
- Cline (VSCode extension, JSONL)
- Roo Code (JSONL)
- Aider (markdown-based chat logs)
- Generic/custom formats

The parser auto-detects format or can be explicitly specified.
"""

from __future__ import annotations

import json
import re
from abc import ABC, abstractmethod
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
        """Get the file path this tool operates on."""
        # Try various field names used by different agents
        for key in ["file_path", "path", "filePath", "file", "filename"]:
            if key in self.input:
                return str(self.input[key])
        return None


@dataclass
class ToolResult:
    """Result of a tool call."""

    tool_use_id: str
    content: str
    is_error: bool = False


@dataclass
class Turn:
    """A single turn in the conversation."""

    role: str  # "user" or "assistant"
    content: str  # Text content
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
    user: Turn | None = None

    @property
    def is_correction(self) -> bool:
        """Check if user turn appears to be a correction."""
        if not self.user:
            return False

        # Check if user edited same files
        assistant_files = set(self.assistant.files_written)
        user_files = set(self.user.files_written) if self.user else set()

        if assistant_files & user_files:
            return True

        # Check for correction language in user message
        correction_phrases = [
            "no,",
            "actually",
            "that's not",
            "wrong",
            "should be",
            "change it to",
            "fix",
            "instead",
            "not what i",
            "don't",
        ]
        user_text = self.user.content.lower()
        return any(phrase in user_text for phrase in correction_phrases)


@dataclass
class ParsedSession:
    """A fully parsed session with structured turn data."""

    path: Path
    format: LogFormat = LogFormat.AUTO
    turns: list[Turn] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def turn_pairs(self) -> list[TurnPair]:
        """Get assistant turns paired with following user turns."""
        pairs = []
        i = 0
        while i < len(self.turns):
            turn = self.turns[i]
            if turn.role == "assistant":
                next_user = None
                if i + 1 < len(self.turns) and self.turns[i + 1].role == "user":
                    next_user = self.turns[i + 1]
                pairs.append(TurnPair(assistant=turn, user=next_user))
            i += 1
        return pairs

    def user_messages(self) -> list[Turn]:
        """Get all user turns."""
        return [t for t in self.turns if t.role == "user"]

    def assistant_messages(self) -> list[Turn]:
        """Get all assistant turns."""
        return [t for t in self.turns if t.role == "assistant"]


class BaseParser(ABC):
    """Base class for session log parsers."""

    format: LogFormat

    def __init__(self, path: Path):
        self.path = Path(path)

    @abstractmethod
    def parse(self) -> ParsedSession:
        """Parse the session file into structured data."""
        ...

    @classmethod
    @abstractmethod
    def can_parse(cls, path: Path, sample: str) -> bool:
        """Check if this parser can handle the given file."""
        ...

    def _read_file(self) -> str:
        """Read file contents."""
        try:
            return self.path.read_text(encoding="utf-8")
        except Exception:
            return ""

    def _read_jsonl(self) -> list[dict[str, Any]]:
        """Read JSONL file."""
        entries = []
        try:
            with open(self.path, encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if line:
                        try:
                            entries.append(json.loads(line))
                        except json.JSONDecodeError:
                            continue
        except Exception:
            pass
        return entries


class ClaudeCodeParser(BaseParser):
    """Parse Claude Code JSONL session logs."""

    format = LogFormat.CLAUDE_CODE

    def parse(self) -> ParsedSession:
        session = ParsedSession(path=self.path, format=self.format)

        if not self.path.exists():
            return session

        entries = self._read_jsonl()
        session.turns = self._extract_turns(entries)
        session.metadata = self._extract_metadata(entries)

        return session

    @classmethod
    def can_parse(cls, path: Path, sample: str) -> bool:
        """Check for Claude Code format markers."""
        # Look for Claude Code specific fields
        if '"type": "assistant"' in sample and '"requestId"' in sample:
            return True
        if '"type": "user"' in sample and '"message"' in sample:
            return True
        return False

    def _extract_turns(self, entries: list[dict]) -> list[Turn]:
        turns: list[Turn] = []
        seen_request_ids: set[str] = set()

        for entry in entries:
            entry_type = entry.get("type")

            if entry_type == "user":
                message = entry.get("message", {})
                content = self._extract_text_content(message.get("content", []))
                turn = Turn(
                    role="user",
                    content=content,
                    timestamp=entry.get("timestamp"),
                )
                turns.append(turn)

            elif entry_type == "assistant":
                request_id = entry.get("requestId")
                if request_id and request_id in seen_request_ids:
                    for turn in reversed(turns):
                        if turn.request_id == request_id:
                            self._update_turn_from_entry(turn, entry)
                            break
                else:
                    if request_id:
                        seen_request_ids.add(request_id)
                    turn = self._create_assistant_turn(entry)
                    turns.append(turn)

        return turns

    def _create_assistant_turn(self, entry: dict) -> Turn:
        message = entry.get("message", {})
        content_blocks = message.get("content", [])

        text_content = self._extract_text_content(content_blocks)
        tool_calls = self._extract_tool_calls(content_blocks)
        tool_results = self._extract_tool_results(content_blocks)

        return Turn(
            role="assistant",
            content=text_content,
            tool_calls=tool_calls,
            tool_results=tool_results,
            timestamp=entry.get("timestamp"),
            request_id=entry.get("requestId"),
        )

    def _update_turn_from_entry(self, turn: Turn, entry: dict) -> None:
        message = entry.get("message", {})
        content_blocks = message.get("content", [])

        text = self._extract_text_content(content_blocks)
        if text:
            turn.content = text

        new_calls = self._extract_tool_calls(content_blocks)
        existing_ids = {tc.id for tc in turn.tool_calls}
        for tc in new_calls:
            if tc.id not in existing_ids:
                turn.tool_calls.append(tc)

        new_results = self._extract_tool_results(content_blocks)
        existing_result_ids = {tr.tool_use_id for tr in turn.tool_results}
        for tr in new_results:
            if tr.tool_use_id not in existing_result_ids:
                turn.tool_results.append(tr)

    def _extract_text_content(self, content_blocks: list | str) -> str:
        if isinstance(content_blocks, str):
            return content_blocks

        parts = []
        for block in content_blocks:
            if isinstance(block, str):
                parts.append(block)
            elif isinstance(block, dict):
                if block.get("type") == "text":
                    parts.append(block.get("text", ""))
        return "\n".join(parts)

    def _extract_tool_calls(self, content_blocks: list) -> list[ToolCall]:
        calls = []
        if not isinstance(content_blocks, list):
            return calls

        for block in content_blocks:
            if isinstance(block, dict) and block.get("type") == "tool_use":
                calls.append(
                    ToolCall(
                        name=block.get("name", "unknown"),
                        input=block.get("input", {}),
                        id=block.get("id", ""),
                    )
                )
        return calls

    def _extract_tool_results(self, content_blocks: list) -> list[ToolResult]:
        results = []
        if not isinstance(content_blocks, list):
            return results

        for block in content_blocks:
            if isinstance(block, dict) and block.get("type") == "tool_result":
                content = block.get("content", "")
                if isinstance(content, list):
                    content = str(content)
                results.append(
                    ToolResult(
                        tool_use_id=block.get("tool_use_id", ""),
                        content=str(content),
                        is_error=block.get("is_error", False),
                    )
                )
        return results

    def _extract_metadata(self, entries: list[dict]) -> dict[str, Any]:
        metadata: dict[str, Any] = {"format": "claude_code"}

        for entry in entries:
            if entry.get("type") == "system":
                if "cwd" in entry:
                    metadata["cwd"] = entry["cwd"]
                if "model" in entry:
                    metadata["model"] = entry["model"]

        user_count = sum(1 for e in entries if e.get("type") == "user")
        assistant_count = len({e.get("requestId") for e in entries if e.get("type") == "assistant"})

        metadata["user_messages"] = user_count
        metadata["assistant_messages"] = assistant_count

        return metadata


class ClineParser(BaseParser):
    """Parse Cline (VSCode extension) JSONL logs."""

    format = LogFormat.CLINE

    def parse(self) -> ParsedSession:
        session = ParsedSession(path=self.path, format=self.format)

        if not self.path.exists():
            return session

        entries = self._read_jsonl()
        session.turns = self._extract_turns(entries)
        session.metadata = {"format": "cline"}

        return session

    @classmethod
    def can_parse(cls, path: Path, sample: str) -> bool:
        # Look for Cline-specific markers
        if '"role": "user"' in sample or '"role": "assistant"' in sample:
            if '"tool_calls"' in sample or '"function_call"' in sample:
                return True
            # Cline uses a messages array format
            if '"messages"' in sample:
                return True
        return False

    def _extract_turns(self, entries: list[dict]) -> list[Turn]:
        turns: list[Turn] = []

        for entry in entries:
            # Cline may have different structures
            if "messages" in entry:
                # Array of messages format
                for msg in entry.get("messages", []):
                    turn = self._parse_message(msg)
                    if turn:
                        turns.append(turn)
            elif "role" in entry:
                # Single message format
                turn = self._parse_message(entry)
                if turn:
                    turns.append(turn)

        return turns

    def _parse_message(self, msg: dict) -> Turn | None:
        role = msg.get("role")
        if role not in ("user", "assistant"):
            return None

        content = msg.get("content", "")
        if isinstance(content, list):
            # Content blocks
            parts = []
            for block in content:
                if isinstance(block, dict):
                    if block.get("type") == "text":
                        parts.append(block.get("text", ""))
                elif isinstance(block, str):
                    parts.append(block)
            content = "\n".join(parts)

        tool_calls = []
        if "tool_calls" in msg:
            for tc in msg.get("tool_calls", []):
                func = tc.get("function", {})
                tool_calls.append(
                    ToolCall(
                        name=func.get("name", "unknown"),
                        input=json.loads(func.get("arguments", "{}")),
                        id=tc.get("id", ""),
                    )
                )

        return Turn(
            role=role,
            content=content,
            tool_calls=tool_calls,
        )


class RooCodeParser(BaseParser):
    """Parse Roo Code JSONL logs."""

    format = LogFormat.ROO_CODE

    def parse(self) -> ParsedSession:
        session = ParsedSession(path=self.path, format=self.format)

        if not self.path.exists():
            return session

        entries = self._read_jsonl()
        session.turns = self._extract_turns(entries)
        session.metadata = {"format": "roo_code"}

        return session

    @classmethod
    def can_parse(cls, path: Path, sample: str) -> bool:
        # Roo Code specific markers
        if "roo" in sample.lower() or "roocode" in sample.lower():
            return True
        return False

    def _extract_turns(self, entries: list[dict]) -> list[Turn]:
        turns: list[Turn] = []

        for entry in entries:
            role = entry.get("role") or entry.get("type")
            if role == "human" or role == "user":
                role = "user"
            elif role == "ai" or role == "assistant":
                role = "assistant"
            else:
                continue

            content = entry.get("content", "") or entry.get("text", "")
            if isinstance(content, list):
                content = "\n".join(str(c) for c in content)

            turns.append(Turn(role=role, content=str(content)))

        return turns


class GeminiCLIParser(BaseParser):
    """Parse Gemini CLI logs."""

    format = LogFormat.GEMINI_CLI

    def parse(self) -> ParsedSession:
        session = ParsedSession(path=self.path, format=self.format)

        if not self.path.exists():
            return session

        entries = self._read_jsonl()
        session.turns = self._extract_turns(entries)
        session.metadata = {"format": "gemini_cli"}

        return session

    @classmethod
    def can_parse(cls, path: Path, sample: str) -> bool:
        # Look for Gemini-specific markers
        if "gemini" in sample.lower():
            return True
        if '"model": "gemini' in sample.lower():
            return True
        return False

    def _extract_turns(self, entries: list[dict]) -> list[Turn]:
        turns: list[Turn] = []

        for entry in entries:
            role = entry.get("role")
            if role == "user":
                content = entry.get("parts", [{}])[0].get("text", "")
                turns.append(Turn(role="user", content=content))
            elif role == "model":
                content = entry.get("parts", [{}])[0].get("text", "")
                # Extract function calls if present
                tool_calls = []
                for part in entry.get("parts", []):
                    if "functionCall" in part:
                        fc = part["functionCall"]
                        tool_calls.append(
                            ToolCall(
                                name=fc.get("name", "unknown"),
                                input=fc.get("args", {}),
                                id=str(hash(str(fc))),
                            )
                        )
                turns.append(Turn(role="assistant", content=content, tool_calls=tool_calls))

        return turns


class AiderParser(BaseParser):
    """Parse Aider chat logs (markdown format)."""

    format = LogFormat.AIDER

    def parse(self) -> ParsedSession:
        session = ParsedSession(path=self.path, format=self.format)

        if not self.path.exists():
            return session

        content = self._read_file()
        session.turns = self._extract_turns(content)
        session.metadata = {"format": "aider"}

        return session

    @classmethod
    def can_parse(cls, path: Path, sample: str) -> bool:
        # Aider uses markdown with specific markers
        if "#### " in sample or ">" in sample:
            # Look for user/assistant patterns
            if re.search(r"^####\s+", sample, re.MULTILINE):
                return True
            if re.search(r"^>\s+", sample, re.MULTILINE):
                return True
        return False

    def _extract_turns(self, content: str) -> list[Turn]:
        turns: list[Turn] = []

        # Aider format: "#### user message" and "> assistant response"
        # or variations depending on settings

        # Split by user markers
        user_pattern = re.compile(r"^####\s*(.+?)(?=^####|\Z)", re.MULTILINE | re.DOTALL)

        # Try to find user messages
        user_matches = user_pattern.findall(content)
        for match in user_matches:
            turns.append(Turn(role="user", content=match.strip()))

        # If no structured format found, try line-by-line
        if not turns:
            lines = content.split("\n")
            current_role = None
            current_content: list[str] = []

            for line in lines:
                if line.startswith("User:") or line.startswith("Human:"):
                    if current_role and current_content:
                        turns.append(Turn(role=current_role, content="\n".join(current_content)))
                    current_role = "user"
                    current_content = [line.split(":", 1)[1].strip()]
                elif line.startswith("Assistant:") or line.startswith("AI:"):
                    if current_role and current_content:
                        turns.append(Turn(role=current_role, content="\n".join(current_content)))
                    current_role = "assistant"
                    current_content = [line.split(":", 1)[1].strip()]
                elif current_role:
                    current_content.append(line)

            if current_role and current_content:
                turns.append(Turn(role=current_role, content="\n".join(current_content)))

        return turns


class GenericChatParser(BaseParser):
    """Parse generic chat logs with common patterns."""

    format = LogFormat.GENERIC_CHAT

    def parse(self) -> ParsedSession:
        session = ParsedSession(path=self.path, format=self.format)

        if not self.path.exists():
            return session

        # Try JSONL first, then markdown
        entries = self._read_jsonl()
        if entries:
            session.turns = self._extract_from_jsonl(entries)
        else:
            content = self._read_file()
            session.turns = self._extract_from_text(content)

        session.metadata = {"format": "generic"}
        return session

    @classmethod
    def can_parse(cls, path: Path, sample: str) -> bool:
        # Generic parser is the fallback
        return True

    def _extract_from_jsonl(self, entries: list[dict]) -> list[Turn]:
        turns: list[Turn] = []

        for entry in entries:
            role = (
                entry.get("role") or entry.get("type") or entry.get("sender") or entry.get("from")
            )

            if role in ("user", "human", "User", "Human"):
                role = "user"
            elif role in ("assistant", "ai", "bot", "Assistant", "AI", "model"):
                role = "assistant"
            else:
                continue

            content = entry.get("content") or entry.get("text") or entry.get("message") or ""

            if isinstance(content, list):
                parts = []
                for item in content:
                    if isinstance(item, dict):
                        parts.append(item.get("text", str(item)))
                    else:
                        parts.append(str(item))
                content = "\n".join(parts)

            turns.append(Turn(role=role, content=str(content)))

        return turns

    def _extract_from_text(self, content: str) -> list[Turn]:
        turns: list[Turn] = []

        # Common patterns
        patterns = [
            (r"^User:\s*(.+?)(?=^(?:User|Assistant|AI|Human):|$)", "user"),
            (r"^Human:\s*(.+?)(?=^(?:User|Assistant|AI|Human):|$)", "user"),
            (r"^Assistant:\s*(.+?)(?=^(?:User|Assistant|AI|Human):|$)", "assistant"),
            (r"^AI:\s*(.+?)(?=^(?:User|Assistant|AI|Human):|$)", "assistant"),
        ]

        for pattern, role in patterns:
            matches = re.findall(pattern, content, re.MULTILINE | re.DOTALL | re.IGNORECASE)
            for match in matches:
                turns.append(Turn(role=role, content=match.strip()))

        return turns


# Parser registry
PARSERS: list[type[BaseParser]] = [
    ClaudeCodeParser,
    ClineParser,
    RooCodeParser,
    GeminiCLIParser,
    AiderParser,
    GenericChatParser,  # Fallback
]


def detect_format(path: Path) -> type[BaseParser]:
    """Auto-detect the log format and return appropriate parser."""
    try:
        # Read sample of file
        with open(path, encoding="utf-8") as f:
            sample = f.read(4096)
    except Exception:
        return GenericChatParser

    for parser_class in PARSERS:
        if parser_class.can_parse(path, sample):
            return parser_class

    return GenericChatParser


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
    path = Path(path)

    if format == LogFormat.AUTO:
        parser_class = detect_format(path)
    else:
        # Find parser for specified format
        format_to_parser = {
            LogFormat.CLAUDE_CODE: ClaudeCodeParser,
            LogFormat.CLINE: ClineParser,
            LogFormat.ROO_CODE: RooCodeParser,
            LogFormat.GEMINI_CLI: GeminiCLIParser,
            LogFormat.AIDER: AiderParser,
            LogFormat.GENERIC_JSONL: GenericChatParser,
            LogFormat.GENERIC_CHAT: GenericChatParser,
        }
        parser_class = format_to_parser.get(format, GenericChatParser)

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
