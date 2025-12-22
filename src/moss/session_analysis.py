"""Session log analysis with multi-format support.

Parses session logs from various agents to extract:
- Tool call frequency and success rates
- Error patterns and retry loops
- Token usage and context growth
- Parallelization opportunities

Supported formats:
- Claude Code JSONL
- Gemini CLI JSONL (planned)
- Cline/Roo formats (planned)
- Moss internal sessions (via session.py)
"""

from __future__ import annotations

import json
from collections import Counter
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any, ClassVar, Protocol, runtime_checkable


class LogFormat(Enum):
    """Supported session log formats."""

    CLAUDE_CODE = "claude"
    GEMINI = "gemini"
    MOSS = "moss"
    UNKNOWN = "unknown"


@runtime_checkable
class LogParser(Protocol):
    """Protocol for session log parsers."""

    def analyze(self) -> SessionAnalysis:
        """Parse and analyze the session log."""
        ...


def detect_log_format(path: Path) -> LogFormat:
    """Auto-detect the log format from file content.

    Args:
        path: Path to the session log file

    Returns:
        Detected LogFormat enum value
    """
    if not path.exists():
        return LogFormat.UNKNOWN

    # Check file extension hints
    suffix = path.suffix.lower()

    # Check for JSON formats (not JSONL)
    if suffix == ".json":
        try:
            with open(path, encoding="utf-8") as f:
                data = json.load(f)
                # Moss internal sessions
                if "tool_calls" in data and "llm_calls" in data:
                    return LogFormat.MOSS
                # Gemini CLI sessions have sessionId and messages array
                if "sessionId" in data and "messages" in data:
                    messages = data.get("messages", [])
                    if any(m.get("type") == "gemini" for m in messages):
                        return LogFormat.GEMINI
        except (json.JSONDecodeError, OSError):
            pass

    # JSONL formats - peek at first few lines
    if suffix in (".jsonl", ".json"):
        try:
            with open(path, encoding="utf-8") as f:
                for i, line in enumerate(f):
                    if i > 5:
                        break
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        entry = json.loads(line)
                        # Claude Code has 'type' field with 'user', 'assistant', 'summary'
                        if entry.get("type") in ("user", "assistant", "summary"):
                            return LogFormat.CLAUDE_CODE
                        # Gemini CLI has different structure (future)
                        if "gemini" in str(entry).lower():
                            return LogFormat.GEMINI
                    except json.JSONDecodeError:
                        continue
        except OSError:
            pass

    return LogFormat.UNKNOWN


def get_parser_for_format(path: Path, fmt: LogFormat) -> LogParser:
    """Get the appropriate parser for a log format.

    Args:
        path: Path to the session log file
        fmt: The detected LogFormat

    Returns:
        LogParser instance for the format
    """
    if fmt == LogFormat.CLAUDE_CODE:
        return ClaudeCodeAnalyzer(path)
    if fmt == LogFormat.MOSS:
        return MossSessionAnalyzer(path)
    if fmt == LogFormat.GEMINI:
        return GeminiCliAnalyzer(path)
    # Default to Claude Code parser for unknown formats
    return ClaudeCodeAnalyzer(path)


def analyze_log(path: str | Path) -> SessionAnalysis:
    """Analyze a session log with auto-format detection.

    This is the recommended entry point for analyzing any session log.
    It auto-detects the format and uses the appropriate parser.

    Args:
        path: Path to the session log file

    Returns:
        SessionAnalysis with all statistics
    """
    path = Path(path)
    fmt = detect_log_format(path)
    parser = get_parser_for_format(path, fmt)
    return parser.analyze()


@dataclass
class ToolStats:
    """Statistics for a single tool."""

    name: str
    calls: int = 0
    errors: int = 0

    @property
    def success_rate(self) -> float:
        if self.calls == 0:
            return 0.0
        return (self.calls - self.errors) / self.calls

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "calls": self.calls,
            "errors": self.errors,
            "success_rate": round(self.success_rate * 100, 1),
        }


@dataclass
class TokenStats:
    """Token usage statistics."""

    total_input: int = 0
    total_output: int = 0
    cache_read: int = 0
    cache_create: int = 0
    min_context: int = 0
    max_context: int = 0
    api_calls: int = 0

    @property
    def avg_context(self) -> int:
        if self.api_calls == 0:
            return 0
        # Context = new input + cache read
        total_context = self.total_input + self.cache_read
        return total_context // self.api_calls

    def to_dict(self) -> dict[str, Any]:
        return {
            "total_input": self.total_input,
            "total_output": self.total_output,
            "cache_read": self.cache_read,
            "cache_create": self.cache_create,
            "min_context": self.min_context,
            "max_context": self.max_context,
            "avg_context": self.avg_context,
            "api_calls": self.api_calls,
        }


@dataclass
class ErrorPattern:
    """A recurring error pattern."""

    category: str
    count: int
    examples: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "category": self.category,
            "count": self.count,
            "examples": self.examples[:3],
        }


@dataclass
class SessionAnalysis:
    """Complete analysis of a Claude Code session."""

    session_path: Path
    message_counts: dict[str, int] = field(default_factory=dict)
    tool_stats: dict[str, ToolStats] = field(default_factory=dict)
    token_stats: TokenStats = field(default_factory=TokenStats)
    error_patterns: list[ErrorPattern] = field(default_factory=list)
    file_tokens: dict[str, int] = field(default_factory=dict)
    parallel_opportunities: int = 0
    total_turns: int = 0

    @property
    def total_tool_calls(self) -> int:
        return sum(t.calls for t in self.tool_stats.values())

    @property
    def total_errors(self) -> int:
        return sum(t.errors for t in self.tool_stats.values())

    @property
    def overall_success_rate(self) -> float:
        if self.total_tool_calls == 0:
            return 0.0
        return (self.total_tool_calls - self.total_errors) / self.total_tool_calls

    def to_dict(self) -> dict[str, Any]:
        return {
            "session_path": str(self.session_path),
            "message_counts": self.message_counts,
            "tool_stats": {k: v.to_dict() for k, v in self.tool_stats.items()},
            "token_stats": self.token_stats.to_dict(),
            "error_patterns": [e.to_dict() for e in self.error_patterns],
            "file_tokens": dict(sorted(self.file_tokens.items(), key=lambda x: -x[1])[:20]),
            "summary": {
                "total_tool_calls": self.total_tool_calls,
                "total_errors": self.total_errors,
                "success_rate": round(self.overall_success_rate * 100, 1),
                "total_turns": self.total_turns,
                "parallel_opportunities": self.parallel_opportunities,
            },
        }

    def to_compact(self) -> str:
        """Format as compact summary."""
        lines = []
        lines.append(
            f"session: {self.total_tool_calls} tool calls, {self.overall_success_rate:.0%} success"
        )

        # Top tools
        top_tools = sorted(self.tool_stats.values(), key=lambda t: t.calls, reverse=True)[:5]
        tool_summary = ", ".join(f"{t.name}:{t.calls}" for t in top_tools)
        lines.append(f"tools: {tool_summary}")

        # Errors
        if self.total_errors:
            lines.append(f"errors: {self.total_errors}")

        # Token stats
        if self.token_stats.api_calls:
            ctx_k = self.token_stats.avg_context / 1000
            lines.append(f"context: avg {ctx_k:.0f}K tokens")

        return "\n".join(lines)

    def to_markdown(self) -> str:
        """Format as markdown report."""
        lines = ["# Session Analysis", ""]

        # Summary
        lines.append("## Summary")
        lines.append("")
        lines.append(f"- **Tool calls**: {self.total_tool_calls}")
        lines.append(f"- **Success rate**: {self.overall_success_rate:.1%}")
        lines.append(f"- **Total turns**: {self.total_turns}")
        lines.append(f"- **Parallel opportunities**: {self.parallel_opportunities}")
        lines.append("")

        # Message types
        if self.message_counts:
            lines.append("## Message Types")
            lines.append("")
            lines.append("| Type | Count |")
            lines.append("|------|-------|")
            for msg_type, count in sorted(self.message_counts.items(), key=lambda x: -x[1]):
                lines.append(f"| {msg_type} | {count} |")
            lines.append("")

        # Tool usage
        if self.tool_stats:
            lines.append("## Tool Usage")
            lines.append("")
            lines.append("| Tool | Calls | Errors | Success Rate |")
            lines.append("|------|-------|--------|--------------|")
            for tool in sorted(self.tool_stats.values(), key=lambda t: t.calls, reverse=True):
                lines.append(
                    f"| {tool.name} | {tool.calls} | {tool.errors} | {tool.success_rate:.0%} |"
                )
            lines.append("")

        # Token usage
        if self.token_stats.api_calls:
            ts = self.token_stats
            lines.append("## Token Usage")
            lines.append("")
            lines.append(f"- **API calls**: {ts.api_calls}")
            lines.append(f"- **Avg context**: {ts.avg_context:,} tokens")
            lines.append(f"- **Context range**: {ts.min_context:,} - {ts.max_context:,}")
            if ts.cache_read:
                lines.append(f"- **Cache read**: {ts.cache_read:,} tokens")
            if ts.cache_create:
                lines.append(f"- **Cache create**: {ts.cache_create:,} tokens")
            lines.append("")

        # Path token hotspots (files and symbols)
        if self.file_tokens:
            lines.append("## Token Hotspots")
            lines.append("")
            lines.append("| Path | Tokens |")
            lines.append("|------|--------|")
            sorted_paths = sorted(self.file_tokens.items(), key=lambda x: -x[1])[:10]
            for path, tokens in sorted_paths:
                lines.append(f"| {path} | {tokens:,} |")
            lines.append("")

        # Error patterns
        if self.error_patterns:
            lines.append("## Error Patterns")
            lines.append("")
            for pattern in self.error_patterns:
                lines.append(f"### {pattern.category} ({pattern.count})")
                for ex in pattern.examples[:3]:
                    lines.append(f"- {ex}")
                lines.append("")

        return "\n".join(lines)


class ClaudeCodeAnalyzer:
    """Analyze Claude Code session logs (JSONL format)."""

    def __init__(self, session_path: Path):
        self.session_path = Path(session_path)

    def analyze(self) -> SessionAnalysis:
        """Parse and analyze the session log."""
        result = SessionAnalysis(session_path=self.session_path)

        if not self.session_path.exists():
            return result

        # Read JSONL file
        entries = self._read_entries()

        # Count message types
        result.message_counts = self._count_message_types(entries)

        # Analyze tool usage
        result.tool_stats = self._analyze_tools(entries)

        # Analyze tokens
        result.token_stats = self._analyze_tokens(entries)

        # Find error patterns
        result.error_patterns = self._find_error_patterns(entries)

        # Analyze file token usage
        result.file_tokens = self._analyze_file_tokens(entries)

        # Count turns and parallel opportunities
        result.total_turns = self._count_turns(entries)
        result.parallel_opportunities = self._find_parallel_opportunities(entries)

        return result

    def _read_entries(self) -> list[dict[str, Any]]:
        """Read all JSONL entries."""
        entries = []
        try:
            with open(self.session_path, encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if line:
                        try:
                            entries.append(json.loads(line))
                        except json.JSONDecodeError:
                            continue
        except OSError:
            pass
        return entries

    def _count_message_types(self, entries: list[dict]) -> dict[str, int]:
        """Count occurrences of each message type."""
        counter: Counter[str] = Counter()
        for entry in entries:
            msg_type = entry.get("type", "unknown")
            counter[msg_type] += 1
        return dict(counter)

    def _analyze_tools(self, entries: list[dict]) -> dict[str, ToolStats]:
        """Analyze tool call frequency and success rates."""
        stats: dict[str, ToolStats] = {}

        for entry in entries:
            if entry.get("type") != "assistant":
                continue

            message = entry.get("message", {})
            content = message.get("content", [])

            if not isinstance(content, list):
                continue

            for block in content:
                if not isinstance(block, dict):
                    continue

                # Tool use blocks
                if block.get("type") == "tool_use":
                    tool_name = block.get("name", "unknown")
                    if tool_name not in stats:
                        stats[tool_name] = ToolStats(name=tool_name)
                    stats[tool_name].calls += 1

                # Tool result blocks (look for errors)
                if block.get("type") == "tool_result":
                    # Check if this is an error
                    is_error = block.get("is_error", False)
                    content_text = block.get("content", "")
                    if is_error or (
                        isinstance(content_text, str)
                        and any(
                            err in content_text.lower() for err in ["error", "failed", "exception"]
                        )
                    ):
                        # Tool error detected - would need to track tool_ids to match
                        # For now, errors are counted in _find_error_patterns
                        pass

        return stats

    def _analyze_tokens(self, entries: list[dict]) -> TokenStats:
        """Analyze token usage from API calls."""
        stats = TokenStats()
        request_data: dict[str, dict] = {}

        for entry in entries:
            if entry.get("type") != "assistant":
                continue

            message = entry.get("message", {})
            usage = message.get("usage", {})

            if not usage:
                continue

            # Deduplicate by request ID and take max values (streaming updates)
            request_id = entry.get("requestId", str(id(entry)))
            input_tokens = usage.get("input_tokens", 0)
            output_tokens = usage.get("output_tokens", 0)
            cache_read = usage.get("cache_read_input_tokens", 0)
            cache_create = usage.get("cache_creation_input_tokens", 0)

            if request_id not in request_data:
                request_data[request_id] = {
                    "input": 0,
                    "output": 0,
                    "cache_read": 0,
                    "cache_create": 0,
                }

            # Take max values for this request (streaming updates progressively)
            rd = request_data[request_id]
            rd["input"] = max(rd["input"], input_tokens)
            rd["output"] = max(rd["output"], output_tokens)
            rd["cache_read"] = max(rd["cache_read"], cache_read)
            rd["cache_create"] = max(rd["cache_create"], cache_create)

        # Aggregate
        for rd in request_data.values():
            if rd["input"] > 0 or rd["cache_read"] > 0:
                stats.api_calls += 1
                stats.total_input += rd["input"]
                stats.total_output += rd["output"]
                stats.cache_read += rd["cache_read"]
                stats.cache_create += rd["cache_create"]

                # Context = new input + cache read
                context_size = rd["input"] + rd["cache_read"]
                if stats.min_context == 0 or context_size < stats.min_context:
                    stats.min_context = context_size
                if context_size > stats.max_context:
                    stats.max_context = context_size

        return stats

    def _find_error_patterns(self, entries: list[dict]) -> list[ErrorPattern]:
        """Identify recurring error patterns."""
        error_categories: dict[str, list[str]] = {}

        for entry in entries:
            if entry.get("type") != "assistant":
                continue

            message = entry.get("message", {})
            content = message.get("content", [])

            if not isinstance(content, list):
                continue

            for block in content:
                if not isinstance(block, dict):
                    continue

                if block.get("type") == "tool_result" and block.get("is_error"):
                    error_text = str(block.get("content", ""))[:100]
                    category = self._categorize_error(error_text)
                    if category not in error_categories:
                        error_categories[category] = []
                    error_categories[category].append(error_text)

        patterns = []
        for category, examples in sorted(error_categories.items(), key=lambda x: -len(x[1])):
            patterns.append(ErrorPattern(category=category, count=len(examples), examples=examples))

        return patterns

    def _categorize_error(self, error_text: str) -> str:
        """Categorize an error by its content."""
        text = error_text.lower()
        if "exit code" in text:
            return "Command failure"
        if "not found" in text:
            return "File not found"
        if "permission" in text:
            return "Permission error"
        if "timeout" in text:
            return "Timeout"
        if "syntax" in text:
            return "Syntax error"
        if "import" in text:
            return "Import error"
        return "Other"

    def _extract_symbol_paths_from_bash(self, command: str) -> list[str]:
        """Extract symbol paths from bash commands like 'moss view cli.py/Foo'."""
        import re

        paths = []
        # Match: moss view <path> or uv run moss view <path>
        # Symbol paths look like: file.py/Symbol or file.py/Class/method
        pattern = r"(?:uv run )?moss (?:view|analyze)\s+([^\s]+)"
        for match in re.finditer(pattern, command):
            path = match.group(1)
            # Skip flags
            if path.startswith("-"):
                continue
            # Check if it looks like a symbol path (has / after file extension)
            if re.search(r"\.\w+/\w", path):
                paths.append(path)
        return paths

    def _analyze_file_tokens(self, entries: list[dict]) -> dict[str, int]:
        """Analyze token usage per file/symbol.

        For each assistant turn, identify files/symbols accessed via tool calls
        and distribute the output tokens among them.

        Tracks both file paths and symbol paths (e.g., cli.py/cmd_telemetry).
        """
        file_tokens: dict[str, int] = {}

        # Group entries by request ID
        requests: dict[str, dict] = {}
        for entry in entries:
            if entry.get("type") != "assistant":
                continue
            request_id = entry.get("requestId", str(id(entry)))
            message = entry.get("message", {})
            usage = message.get("usage", {})
            content = message.get("content", [])

            if request_id not in requests:
                requests[request_id] = {"paths": set(), "output_tokens": 0}

            # Track output tokens (take max due to streaming)
            output = usage.get("output_tokens", 0)
            requests[request_id]["output_tokens"] = max(
                requests[request_id]["output_tokens"], output
            )

            # Extract files/symbols from tool calls
            if isinstance(content, list):
                for block in content:
                    if not isinstance(block, dict):
                        continue
                    if block.get("type") == "tool_use":
                        tool_name = block.get("name", "")
                        tool_input = block.get("input", {})

                        # Bash: extract symbol paths from moss view/analyze commands
                        if tool_name == "Bash" and "command" in tool_input:
                            cmd = tool_input["command"]
                            if isinstance(cmd, str):
                                symbol_paths = self._extract_symbol_paths_from_bash(cmd)
                                requests[request_id]["paths"].update(symbol_paths)

                        # Read, Edit, Write have file_path
                        if "file_path" in tool_input:
                            fp = tool_input["file_path"]
                            if isinstance(fp, str):
                                requests[request_id]["paths"].add(fp)

                        # Glob has pattern (extract directory)
                        if tool_name == "Glob" and "pattern" in tool_input:
                            pat = tool_input["pattern"]
                            if isinstance(pat, str) and "/" in pat:
                                dir_part = pat.rsplit("/", 1)[0]
                                if not dir_part.startswith("*"):
                                    requests[request_id]["paths"].add(dir_part)

                        # Grep has path
                        if "path" in tool_input:
                            p = tool_input["path"]
                            if isinstance(p, str):
                                requests[request_id]["paths"].add(p)

        # Distribute tokens to paths
        for req in requests.values():
            paths = req["paths"]
            tokens = req["output_tokens"]
            if paths and tokens > 0:
                per_path = tokens // len(paths)
                for p in paths:
                    # Normalize path (remove absolute prefix if present)
                    norm_path = p
                    if norm_path.startswith("/"):
                        # Try to make relative
                        parts = norm_path.split("/")
                        # Find common project markers
                        for i, part in enumerate(parts):
                            if part in ("src", "lib", "crates", "tests", "docs"):
                                norm_path = "/".join(parts[i:])
                                break
                    file_tokens[norm_path] = file_tokens.get(norm_path, 0) + per_path

        return file_tokens

    def _count_turns(self, entries: list[dict]) -> int:
        """Count assistant turns."""
        seen_request_ids: set[str] = set()
        for entry in entries:
            if entry.get("type") == "assistant":
                request_id = entry.get("requestId", str(id(entry)))
                seen_request_ids.add(request_id)
        return len(seen_request_ids)

    def _find_parallel_opportunities(self, entries: list[dict]) -> int:
        """Count turns with only 1 tool call that could have parallelized."""
        single_tool_turns = 0

        for entry in entries:
            if entry.get("type") != "assistant":
                continue

            message = entry.get("message", {})
            content = message.get("content", [])

            if not isinstance(content, list):
                continue

            tool_uses = [b for b in content if isinstance(b, dict) and b.get("type") == "tool_use"]

            # If exactly 1 tool use, it's a potential parallel opportunity
            if len(tool_uses) == 1:
                single_tool_turns += 1

        return single_tool_turns


# Backward compatibility alias
SessionAnalyzer = ClaudeCodeAnalyzer


class MossSessionAnalyzer:
    """Analyze Moss internal session files (JSON format)."""

    def __init__(self, session_path: Path):
        self.session_path = Path(session_path)

    def analyze(self) -> SessionAnalysis:
        """Parse and analyze the Moss session."""
        result = SessionAnalysis(session_path=self.session_path)

        if not self.session_path.exists():
            return result

        try:
            with open(self.session_path, encoding="utf-8") as f:
                data = json.load(f)
        except (json.JSONDecodeError, OSError):
            return result

        # Extract tool stats
        tool_calls = data.get("tool_calls", [])
        for tc in tool_calls:
            tool_name = tc.get("tool_name", "unknown")
            if tool_name not in result.tool_stats:
                result.tool_stats[tool_name] = ToolStats(name=tool_name)
            result.tool_stats[tool_name].calls += 1
            if tc.get("error"):
                result.tool_stats[tool_name].errors += 1

        # Token stats
        result.token_stats.total_input = data.get("tokens_in", 0)
        result.token_stats.total_output = data.get("tokens_out", 0)
        result.token_stats.api_calls = data.get("llm_calls", 0)

        # Turns
        result.total_turns = data.get("llm_calls", 0)

        return result


class GeminiCliAnalyzer:
    """Analyze Gemini CLI session files (JSON format).

    Gemini CLI stores sessions as single JSON files with structure:
    - sessionId, projectHash, startTime, lastUpdated
    - messages: array of user/gemini messages with toolCalls, thoughts, tokens
    """

    def __init__(self, session_path: Path):
        self.session_path = Path(session_path)

    def analyze(self) -> SessionAnalysis:
        """Parse and analyze the Gemini CLI session."""
        result = SessionAnalysis(session_path=self.session_path)

        if not self.session_path.exists():
            return result

        try:
            with open(self.session_path, encoding="utf-8") as f:
                data = json.load(f)
        except (json.JSONDecodeError, OSError):
            return result

        messages = data.get("messages", [])
        result.message_counts = self._count_message_types(messages)
        result.tool_stats = self._analyze_tools(messages)
        result.token_stats = self._analyze_tokens(messages)
        result.file_tokens = self._analyze_file_tokens(messages)
        result.total_turns = self._count_turns(messages)

        return result

    def _count_message_types(self, messages: list[dict]) -> dict[str, int]:
        """Count message types."""
        counts: dict[str, int] = {}
        for msg in messages:
            msg_type = msg.get("type", "unknown")
            counts[msg_type] = counts.get(msg_type, 0) + 1
        return counts

    def _analyze_tools(self, messages: list[dict]) -> dict[str, ToolStats]:
        """Analyze tool call frequency and success rates."""
        stats: dict[str, ToolStats] = {}

        for msg in messages:
            if msg.get("type") != "gemini":
                continue

            tool_calls = msg.get("toolCalls", [])
            for tc in tool_calls:
                tool_name = tc.get("name", "unknown")
                if tool_name not in stats:
                    stats[tool_name] = ToolStats(name=tool_name)
                stats[tool_name].calls += 1

                # Check for errors
                if tc.get("status") == "error":
                    stats[tool_name].errors += 1

        return stats

    def _analyze_tokens(self, messages: list[dict]) -> TokenStats:
        """Analyze token usage from messages."""
        stats = TokenStats()

        for msg in messages:
            if msg.get("type") != "gemini":
                continue

            tokens = msg.get("tokens", {})
            if not tokens:
                continue

            stats.api_calls += 1
            stats.total_input += tokens.get("input", 0)
            stats.total_output += tokens.get("output", 0)
            stats.cache_read += tokens.get("cached", 0)

            # Track context (input + cached)
            context = tokens.get("input", 0) + tokens.get("cached", 0)
            if stats.min_context == 0 or context < stats.min_context:
                stats.min_context = context
            if context > stats.max_context:
                stats.max_context = context

        return stats

    def _analyze_file_tokens(self, messages: list[dict]) -> dict[str, int]:
        """Analyze token usage per file."""
        file_tokens: dict[str, int] = {}

        for msg in messages:
            if msg.get("type") != "gemini":
                continue

            tokens = msg.get("tokens", {})
            output_tokens = tokens.get("output", 0)
            if output_tokens == 0:
                continue

            # Extract files from tool calls
            files: set[str] = set()
            for tc in msg.get("toolCalls", []):
                args = tc.get("args", {})
                # read_file, write_file have file_path
                if "file_path" in args:
                    files.add(args["file_path"])
                # run_shell_command might have file references in command
                # (skip for now - complex to parse)

            # Distribute tokens to files
            if files:
                per_file = output_tokens // len(files)
                for f in files:
                    # Normalize path
                    norm_path = f
                    if norm_path.startswith("/"):
                        parts = norm_path.split("/")
                        for i, part in enumerate(parts):
                            if part in ("src", "lib", "crates", "tests", "docs"):
                                norm_path = "/".join(parts[i:])
                                break
                    file_tokens[norm_path] = file_tokens.get(norm_path, 0) + per_file

        return file_tokens

    def _count_turns(self, messages: list[dict]) -> int:
        """Count gemini turns."""
        return sum(1 for msg in messages if msg.get("type") == "gemini")


def analyze_session(path: str | Path) -> SessionAnalysis:
    """Convenience function to analyze a session log.

    Uses auto-format detection to support multiple log formats.

    Args:
        path: Path to the session file (JSONL or JSON)

    Returns:
        SessionAnalysis with all statistics
    """
    return analyze_log(path)


# =============================================================================
# Session Log Comparison Tool
# =============================================================================


@dataclass
class EditStats:
    """Statistics for edit tool usage."""

    tool_name: str
    total_attempts: int = 0
    successes: int = 0
    failures: int = 0
    retries: int = 0
    avg_old_string_len: float = 0.0
    avg_new_string_len: float = 0.0

    @property
    def success_rate(self) -> float:
        if self.total_attempts == 0:
            return 0.0
        return self.successes / self.total_attempts

    def to_dict(self) -> dict[str, Any]:
        return {
            "tool_name": self.tool_name,
            "total_attempts": self.total_attempts,
            "successes": self.successes,
            "failures": self.failures,
            "retries": self.retries,
            "success_rate": round(self.success_rate * 100, 1),
            "avg_old_string_len": round(self.avg_old_string_len, 1),
            "avg_new_string_len": round(self.avg_new_string_len, 1),
        }


@dataclass
class SessionComparison:
    """Comparison between two agent session logs."""

    agent_a: str
    agent_b: str
    edit_stats_a: EditStats
    edit_stats_b: EditStats
    tool_usage_a: dict[str, int] = field(default_factory=dict)
    tool_usage_b: dict[str, int] = field(default_factory=dict)
    total_tokens_a: int = 0
    total_tokens_b: int = 0
    total_turns_a: int = 0
    total_turns_b: int = 0

    def to_dict(self) -> dict[str, Any]:
        return {
            "agents": {
                "a": self.agent_a,
                "b": self.agent_b,
            },
            "edit_stats": {
                self.agent_a: self.edit_stats_a.to_dict(),
                self.agent_b: self.edit_stats_b.to_dict(),
            },
            "tool_usage": {
                self.agent_a: self.tool_usage_a,
                self.agent_b: self.tool_usage_b,
            },
            "tokens": {
                self.agent_a: self.total_tokens_a,
                self.agent_b: self.total_tokens_b,
            },
            "turns": {
                self.agent_a: self.total_turns_a,
                self.agent_b: self.total_turns_b,
            },
        }

    def to_markdown(self) -> str:
        """Format as markdown comparison report."""
        lines = [
            "# Session Comparison Report",
            "",
            f"Comparing **{self.agent_a}** vs **{self.agent_b}**",
            "",
            "## Edit Tool Performance",
            "",
            "| Metric | " + self.agent_a + " | " + self.agent_b + " |",
            "|--------|" + "-" * len(self.agent_a) + "--|" + "-" * len(self.agent_b) + "--|",
        ]

        # Edit stats comparison
        stats_a = self.edit_stats_a
        stats_b = self.edit_stats_b
        lines.append(f"| Attempts | {stats_a.total_attempts} | {stats_b.total_attempts} |")
        lines.append(f"| Successes | {stats_a.successes} | {stats_b.successes} |")
        lines.append(f"| Failures | {stats_a.failures} | {stats_b.failures} |")
        lines.append(f"| Success Rate | {stats_a.success_rate:.0%} | {stats_b.success_rate:.0%} |")
        lines.append(f"| Retries | {stats_a.retries} | {stats_b.retries} |")
        avg_a = f"{stats_a.avg_old_string_len:.0f}"
        avg_b = f"{stats_b.avg_old_string_len:.0f}"
        lines.append(f"| Avg old_string len | {avg_a} | {avg_b} |")

        # Overall stats
        lines.extend(
            [
                "",
                "## Overall Statistics",
                "",
                "| Metric | " + self.agent_a + " | " + self.agent_b + " |",
                "|--------|" + "-" * len(self.agent_a) + "--|" + "-" * len(self.agent_b) + "--|",
                f"| Total Turns | {self.total_turns_a} | {self.total_turns_b} |",
                f"| Total Tokens | {self.total_tokens_a:,} | {self.total_tokens_b:,} |",
            ]
        )

        # Top tools
        lines.extend(["", "## Top Tools Used", ""])
        all_tools = set(self.tool_usage_a.keys()) | set(self.tool_usage_b.keys())
        sorted_tools = sorted(all_tools, key=lambda t: self.tool_usage_a.get(t, 0), reverse=True)[
            :10
        ]

        lines.append("| Tool | " + self.agent_a + " | " + self.agent_b + " |")
        lines.append("|------|" + "-" * len(self.agent_a) + "--|" + "-" * len(self.agent_b) + "--|")
        for tool in sorted_tools:
            count_a = self.tool_usage_a.get(tool, 0)
            count_b = self.tool_usage_b.get(tool, 0)
            lines.append(f"| {tool} | {count_a} | {count_b} |")

        return "\n".join(lines)


class SessionComparer:
    """Compare session logs from different agents."""

    # Edit tool names for each agent
    EDIT_TOOLS: ClassVar[dict[str, list[str]]] = {
        "claude_code": ["Edit", "MultiEdit", "str_replace_editor"],
        "gemini_cli": ["edit", "edit_file", "replace_in_file"],
    }

    def __init__(self, log_a: Path, log_b: Path, agent_a: str = "Claude", agent_b: str = "Gemini"):
        self.log_a = log_a
        self.log_b = log_b
        self.agent_a = agent_a
        self.agent_b = agent_b

    def compare(self) -> SessionComparison:
        """Compare two session logs."""
        entries_a = self._read_entries(self.log_a)
        entries_b = self._read_entries(self.log_b)

        # Detect agent type from entries
        agent_type_a = self._detect_agent_type(entries_a)
        agent_type_b = self._detect_agent_type(entries_b)

        return SessionComparison(
            agent_a=self.agent_a,
            agent_b=self.agent_b,
            edit_stats_a=self._analyze_edits(entries_a, agent_type_a),
            edit_stats_b=self._analyze_edits(entries_b, agent_type_b),
            tool_usage_a=self._count_tool_usage(entries_a),
            tool_usage_b=self._count_tool_usage(entries_b),
            total_tokens_a=self._count_tokens(entries_a),
            total_tokens_b=self._count_tokens(entries_b),
            total_turns_a=self._count_turns(entries_a),
            total_turns_b=self._count_turns(entries_b),
        )

    def _read_entries(self, path: Path) -> list[dict[str, Any]]:
        """Read JSONL entries from a session log."""
        entries = []
        try:
            with path.open() as f:
                for line in f:
                    line = line.strip()
                    if line:
                        try:
                            entries.append(json.loads(line))
                        except json.JSONDecodeError:
                            continue
        except FileNotFoundError:
            pass
        return entries

    def _detect_agent_type(self, entries: list[dict[str, Any]]) -> str:
        """Detect agent type from log entries."""
        for entry in entries:
            # Check for tool usage patterns
            content = entry.get("content", [])
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict):
                        tool_name = block.get("name", "")
                        if tool_name in ["Edit", "MultiEdit", "str_replace_editor"]:
                            return "claude_code"
                        if tool_name in ["edit", "edit_file", "replace_in_file"]:
                            return "gemini_cli"
        return "unknown"

    def _analyze_edits(self, entries: list[dict[str, Any]], agent_type: str) -> EditStats:
        """Analyze edit tool usage from log entries."""
        edit_tools = self.EDIT_TOOLS.get(agent_type, [])
        stats = EditStats(tool_name=agent_type)

        old_string_lens: list[int] = []
        new_string_lens: list[int] = []
        last_edit_file: str | None = None
        last_edit_failed = False

        for entry in entries:
            content = entry.get("content", [])
            if not isinstance(content, list):
                continue

            for block in content:
                if not isinstance(block, dict):
                    continue

                # Check for tool use
                if block.get("type") == "tool_use":
                    tool_name = block.get("name", "")
                    if tool_name not in edit_tools:
                        continue

                    stats.total_attempts += 1
                    tool_input = block.get("input", {})

                    # Track string lengths
                    old_str = tool_input.get("old_string", "")
                    new_str = tool_input.get("new_string", "")
                    if old_str:
                        old_string_lens.append(len(old_str))
                    if new_str:
                        new_string_lens.append(len(new_str))

                    # Track retry pattern
                    file_path = tool_input.get("file_path", "")
                    if last_edit_failed and file_path == last_edit_file:
                        stats.retries += 1

                    last_edit_file = file_path

                # Check for tool result (success/failure)
                if block.get("type") == "tool_result":
                    is_error = block.get("is_error", False)
                    if is_error:
                        stats.failures += 1
                        last_edit_failed = True
                    else:
                        stats.successes += 1
                        last_edit_failed = False

        # Calculate averages
        if old_string_lens:
            stats.avg_old_string_len = sum(old_string_lens) / len(old_string_lens)
        if new_string_lens:
            stats.avg_new_string_len = sum(new_string_lens) / len(new_string_lens)

        return stats

    def _count_tool_usage(self, entries: list[dict[str, Any]]) -> dict[str, int]:
        """Count tool usage from log entries."""
        tool_counts: Counter[str] = Counter()

        for entry in entries:
            content = entry.get("content", [])
            if not isinstance(content, list):
                continue

            for block in content:
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    tool_name = block.get("name", "unknown")
                    tool_counts[tool_name] += 1

        return dict(tool_counts)

    def _count_tokens(self, entries: list[dict[str, Any]]) -> int:
        """Count total tokens from log entries."""
        total = 0
        for entry in entries:
            usage = entry.get("usage", {})
            total += usage.get("input_tokens", 0)
            total += usage.get("output_tokens", 0)
        return total

    def _count_turns(self, entries: list[dict[str, Any]]) -> int:
        """Count conversation turns."""
        turns = 0
        for entry in entries:
            if entry.get("role") == "assistant":
                turns += 1
        return turns


def compare_sessions(
    log_a: str | Path,
    log_b: str | Path,
    agent_a: str = "Claude Code",
    agent_b: str = "Gemini CLI",
) -> SessionComparison:
    """Compare two session logs from different agents.

    Args:
        log_a: Path to first session log (JSONL)
        log_b: Path to second session log (JSONL)
        agent_a: Name of first agent
        agent_b: Name of second agent

    Returns:
        SessionComparison with comparison metrics

    Example:
        >>> comparison = compare_sessions(
        ...     "claude_session.jsonl",
        ...     "gemini_session.jsonl"
        ... )
        >>> print(comparison.to_markdown())
    """
    comparer = SessionComparer(
        Path(log_a),
        Path(log_b),
        agent_a=agent_a,
        agent_b=agent_b,
    )
    return comparer.compare()
