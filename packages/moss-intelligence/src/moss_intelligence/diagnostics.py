"""Signal-only diagnostics: parse structured errors, discard noise.

Parses compiler/linter output into structured diagnostics.
Extracts signal (error code, message, file, line, suggestion).
Discards noise (ASCII art, color codes, formatting).

Supports:
- Rust/Cargo: --message-format=json
- TypeScript/tsc: --pretty false
- ESLint: --format json
- Python/ruff: --output-format json
- GCC/Clang: -fdiagnostics-format=json
- Generic: line-based parsing with heuristics
"""

from __future__ import annotations

import json
import re
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum, auto
from pathlib import Path
from typing import Any


class Severity(Enum):
    """Severity level of a diagnostic."""

    ERROR = auto()
    WARNING = auto()
    INFO = auto()
    HINT = auto()

    @classmethod
    def from_string(cls, s: str) -> Severity:
        """Parse severity from string."""
        s = s.lower().strip()
        if s in ("error", "err", "e", "fatal"):
            return cls.ERROR
        if s in ("warning", "warn", "w"):
            return cls.WARNING
        if s in ("info", "information", "i", "note"):
            return cls.INFO
        return cls.HINT


@dataclass
class Location:
    """Source code location."""

    file: Path
    line: int
    column: int = 0
    end_line: int | None = None
    end_column: int | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "file": str(self.file),
            "line": self.line,
            "column": self.column,
            "end_line": self.end_line,
            "end_column": self.end_column,
        }

    def __str__(self) -> str:
        if self.column:
            return f"{self.file}:{self.line}:{self.column}"
        return f"{self.file}:{self.line}"


@dataclass
class Suggestion:
    """A suggested fix for a diagnostic."""

    message: str
    replacement: str | None = None
    location: Location | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "message": self.message,
            "replacement": self.replacement,
            "location": self.location.to_dict() if self.location else None,
        }


@dataclass
class Diagnostic:
    """A single diagnostic (error, warning, etc.)."""

    severity: Severity
    message: str
    location: Location | None = None
    code: str | None = None  # e.g., "E0001", "unused-variable"
    source: str | None = None  # e.g., "rustc", "eslint"
    suggestions: list[Suggestion] = field(default_factory=list)
    related: list[Diagnostic] = field(default_factory=list)
    raw: str | None = None  # Original text (for debugging)

    def to_dict(self) -> dict[str, Any]:
        return {
            "severity": self.severity.name.lower(),
            "message": self.message,
            "location": self.location.to_dict() if self.location else None,
            "code": self.code,
            "source": self.source,
            "suggestions": [s.to_dict() for s in self.suggestions],
            "related": [r.to_dict() for r in self.related],
        }

    def to_compact(self) -> str:
        """Format as compact single-line summary."""
        parts = []
        if self.location:
            parts.append(str(self.location))
        parts.append(f"[{self.severity.name}]")
        if self.code:
            parts.append(f"({self.code})")
        parts.append(self.message)
        return " ".join(parts)


@dataclass
class DiagnosticSet:
    """Collection of diagnostics from a tool run."""

    diagnostics: list[Diagnostic] = field(default_factory=list)
    source: str | None = None
    raw_output: str | None = None

    @property
    def errors(self) -> list[Diagnostic]:
        return [d for d in self.diagnostics if d.severity == Severity.ERROR]

    @property
    def warnings(self) -> list[Diagnostic]:
        return [d for d in self.diagnostics if d.severity == Severity.WARNING]

    @property
    def error_count(self) -> int:
        return len(self.errors)

    @property
    def warning_count(self) -> int:
        return len(self.warnings)

    def to_compact(self) -> str:
        """Format as compact summary for LLM consumption."""
        if not self.diagnostics:
            return "No diagnostics"

        lines = [f"{self.error_count} errors, {self.warning_count} warnings"]
        for d in self.diagnostics[:10]:  # Limit to 10
            lines.append(f"  {d.to_compact()}")
        if len(self.diagnostics) > 10:
            lines.append(f"  ... and {len(self.diagnostics) - 10} more")
        return "\n".join(lines)

    def to_dict(self) -> dict[str, Any]:
        return {
            "source": self.source,
            "error_count": self.error_count,
            "warning_count": self.warning_count,
            "diagnostics": [d.to_dict() for d in self.diagnostics],
        }

    def localize_bug(self) -> list[Path]:
        """Heuristically identify potential bug locations from diagnostics.

        Analyzes diagnostic messages and stack traces to find relevant
        source files, prioritizing direct error locations.
        """
        locations = []
        for diag in self.diagnostics:
            if diag.severity == Severity.ERROR and diag.location:
                locations.append(diag.location.file)

            # Extract paths from message/raw text (e.g. stack traces)
            text = f"{diag.message} {diag.raw or ''}"
            # Pattern for common file paths in stack traces
            path_pattern = r"(?:/|\\|[a-zA-Z]:)[^:\s\"']+\.(?:py|rs|ts|js|go|c|cpp|h)"
            matches = re.findall(path_pattern, text)
            for m in matches:
                try:
                    path = Path(m)
                    if path.exists():
                        locations.append(path)
                except (OSError, ValueError):
                    pass

        # Deduplicate and return
        return sorted(list(set(locations)))

    def analyze_test_failure(self) -> list[tuple[Path, int]]:
        """Extract suspected bug locations (file, line) from test failures.

        Prioritizes implementation files over test files in stack traces.
        """
        locations = []
        for diag in self.diagnostics:
            text = f"{diag.message} {diag.raw or ''}"
            # Match file:line patterns common in pytest/unittest output
            # e.g. "src/moss/api.py:123: in function_name"
            pattern = r"([^:\s\"']+)\:(\d+)"
            matches = re.findall(pattern, text)
            for file_str, line_str in matches:
                try:
                    path = Path(file_str)
                    if path.exists() and path.suffix in (".py", ".rs", ".ts", ".js"):
                        # Heuristic: implementation files are better than test files
                        is_test_file = "test" in path.name.lower() or "/tests/" in str(path)
                        score = 1 if is_test_file else 10
                        locations.append((path, int(line_str), score))
                except (OSError, ValueError):
                    pass

        # Sort by score descending then line (to group files)
        locations.sort(key=lambda x: (-x[2], x[0], x[1]))
        return [(p, line) for p, line, s in locations]


# ============================================================================
# ANSI/Noise Stripping
# ============================================================================

# ANSI escape sequence pattern
_ANSI_PATTERN = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]")

# Common ASCII art patterns (box drawing, arrows, etc.)
_ASCII_ART_PATTERNS = [
    re.compile(r"^[\s│├└┌┐┘┬┴┼─]+$"),  # Box drawing
    re.compile(r"^\s*[─=]{3,}\s*$"),  # Horizontal lines
    re.compile(r"^\s*[\^~]+\s*$"),  # Underlines/carets
    re.compile(r"^\s*\|\s*$"),  # Single pipe
]


def strip_ansi(text: str) -> str:
    """Remove ANSI escape sequences from text."""
    return _ANSI_PATTERN.sub("", text)


def is_noise_line(line: str) -> bool:
    """Check if a line is visual noise (ASCII art, decorations)."""
    stripped = line.strip()
    if not stripped:
        return True
    for pattern in _ASCII_ART_PATTERNS:
        if pattern.match(stripped):
            return True
    return False


def clean_output(text: str) -> str:
    """Clean compiler output by removing noise."""
    text = strip_ansi(text)
    lines = [line for line in text.splitlines() if not is_noise_line(line)]
    return "\n".join(lines)


# ============================================================================
# Parsers
# ============================================================================


class DiagnosticParser(ABC):
    """Abstract base for diagnostic parsers."""

    @property
    @abstractmethod
    def name(self) -> str:
        """Parser name (e.g., 'cargo', 'eslint')."""
        ...

    @abstractmethod
    def parse(self, output: str) -> DiagnosticSet:
        """Parse output into diagnostics."""
        ...

    @abstractmethod
    def can_parse(self, output: str) -> bool:
        """Check if this parser can handle the output."""
        ...


class CargoParser(DiagnosticParser):
    """Parse Cargo/rustc JSON output.

    Expects output from: cargo check --message-format=json
    """

    @property
    def name(self) -> str:
        return "cargo"

    def can_parse(self, output: str) -> bool:
        # Look for cargo-specific JSON markers
        has_reason = '"reason":' in output
        has_message = '"compiler-message"' in output or '"$message_type"' in output
        return has_reason and has_message

    def parse(self, output: str) -> DiagnosticSet:
        diagnostics = []

        for line in output.splitlines():
            if not line.strip():
                continue
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                continue

            # Skip non-message entries
            if msg.get("reason") != "compiler-message":
                continue

            message = msg.get("message", {})
            if not message:
                continue

            diag = self._parse_message(message)
            if diag:
                diagnostics.append(diag)

        return DiagnosticSet(diagnostics=diagnostics, source="cargo", raw_output=output)

    def _parse_message(self, message: dict[str, Any]) -> Diagnostic | None:
        """Parse a single rustc message."""
        level = message.get("level", "")
        if level == "failure-note":
            return None

        severity = Severity.from_string(level)
        msg_text = message.get("message", "")
        code = None
        if message.get("code"):
            code = message["code"].get("code", "")

        # Get primary span for location
        location = None
        spans = message.get("spans", [])
        for span in spans:
            if span.get("is_primary"):
                location = Location(
                    file=Path(span.get("file_name", "")),
                    line=span.get("line_start", 0),
                    column=span.get("column_start", 0),
                    end_line=span.get("line_end"),
                    end_column=span.get("column_end"),
                )
                break

        # Extract suggestions
        suggestions = []
        for span in spans:
            if span.get("suggested_replacement") is not None:
                suggestions.append(
                    Suggestion(
                        message=span.get("label", ""),
                        replacement=span.get("suggested_replacement"),
                    )
                )

        # Related diagnostics (children)
        related = []
        for child in message.get("children", []):
            child_diag = self._parse_message(child)
            if child_diag:
                related.append(child_diag)

        return Diagnostic(
            severity=severity,
            message=msg_text,
            location=location,
            code=code,
            source="rustc",
            suggestions=suggestions,
            related=related,
        )


class TypeScriptParser(DiagnosticParser):
    """Parse TypeScript/tsc output.

    Expects output from: tsc --pretty false
    Format: file(line,col): error TSxxxx: message
    """

    @property
    def name(self) -> str:
        return "typescript"

    def can_parse(self, output: str) -> bool:
        return "error TS" in output or "warning TS" in output

    def parse(self, output: str) -> DiagnosticSet:
        diagnostics = []
        # Pattern: file(line,col): error TSxxxx: message
        pattern = re.compile(
            r"^(.+?)\((\d+),(\d+)\):\s*(error|warning)\s+(TS\d+):\s*(.+)$",
            re.MULTILINE,
        )

        for match in pattern.finditer(strip_ansi(output)):
            file_path, line, col, severity, code, message = match.groups()
            diagnostics.append(
                Diagnostic(
                    severity=Severity.from_string(severity),
                    message=message,
                    location=Location(
                        file=Path(file_path),
                        line=int(line),
                        column=int(col),
                    ),
                    code=code,
                    source="tsc",
                )
            )

        return DiagnosticSet(diagnostics=diagnostics, source="tsc", raw_output=output)


class ESLintParser(DiagnosticParser):
    """Parse ESLint JSON output.

    Expects output from: eslint --format json
    """

    @property
    def name(self) -> str:
        return "eslint"

    def can_parse(self, output: str) -> bool:
        try:
            data = json.loads(output)
            return isinstance(data, list) and data and "filePath" in data[0]
        except (json.JSONDecodeError, KeyError, IndexError):
            return False

    def parse(self, output: str) -> DiagnosticSet:
        diagnostics = []

        try:
            results = json.loads(output)
        except json.JSONDecodeError:
            return DiagnosticSet(source="eslint", raw_output=output)

        for file_result in results:
            file_path = Path(file_result.get("filePath", ""))
            for msg in file_result.get("messages", []):
                severity = Severity.ERROR if msg.get("severity") == 2 else Severity.WARNING

                suggestions = []
                if msg.get("fix"):
                    suggestions.append(
                        Suggestion(
                            message="Apply fix",
                            replacement=msg["fix"].get("text"),
                        )
                    )

                diagnostics.append(
                    Diagnostic(
                        severity=severity,
                        message=msg.get("message", ""),
                        location=Location(
                            file=file_path,
                            line=msg.get("line", 0),
                            column=msg.get("column", 0),
                            end_line=msg.get("endLine"),
                            end_column=msg.get("endColumn"),
                        ),
                        code=msg.get("ruleId"),
                        source="eslint",
                        suggestions=suggestions,
                    )
                )

        return DiagnosticSet(diagnostics=diagnostics, source="eslint", raw_output=output)


class RuffParser(DiagnosticParser):
    """Parse Ruff JSON output.

    Expects output from: ruff check --output-format json
    """

    @property
    def name(self) -> str:
        return "ruff"

    def can_parse(self, output: str) -> bool:
        try:
            data = json.loads(output)
            return isinstance(data, list) and data and "code" in data[0]
        except (json.JSONDecodeError, KeyError, IndexError):
            return False

    def parse(self, output: str) -> DiagnosticSet:
        diagnostics = []

        try:
            results = json.loads(output)
        except json.JSONDecodeError:
            return DiagnosticSet(source="ruff", raw_output=output)

        for item in results:
            location = Location(
                file=Path(item.get("filename", "")),
                line=item.get("location", {}).get("row", 0),
                column=item.get("location", {}).get("column", 0),
                end_line=item.get("end_location", {}).get("row"),
                end_column=item.get("end_location", {}).get("column"),
            )

            suggestions = []
            if item.get("fix"):
                suggestions.append(
                    Suggestion(
                        message=item["fix"].get("message", "Apply fix"),
                        replacement=None,  # Ruff fix is an edit, not simple replacement
                    )
                )

            diagnostics.append(
                Diagnostic(
                    severity=Severity.ERROR,  # Ruff treats all as errors
                    message=item.get("message", ""),
                    location=location,
                    code=item.get("code"),
                    source="ruff",
                    suggestions=suggestions,
                )
            )

        return DiagnosticSet(diagnostics=diagnostics, source="ruff", raw_output=output)


class GCCParser(DiagnosticParser):
    """Parse GCC/Clang output.

    Handles both JSON format (-fdiagnostics-format=json) and text format.
    """

    @property
    def name(self) -> str:
        return "gcc"

    def can_parse(self, output: str) -> bool:
        # Check for GCC/Clang text format
        return bool(re.search(r":\d+:\d+:\s*(error|warning):", output))

    def parse(self, output: str) -> DiagnosticSet:
        diagnostics = []

        # Try JSON format first
        try:
            data = json.loads(output)
            if isinstance(data, list):
                return self._parse_json(data, output)
        except json.JSONDecodeError:
            pass

        # Fall back to text format
        # Pattern: file:line:col: error/warning: message
        pattern = re.compile(
            r"^(.+?):(\d+):(\d+):\s*(error|warning|note):\s*(.+)$",
            re.MULTILINE,
        )

        for match in pattern.finditer(strip_ansi(output)):
            file_path, line, col, severity, message = match.groups()
            diagnostics.append(
                Diagnostic(
                    severity=Severity.from_string(severity),
                    message=message,
                    location=Location(
                        file=Path(file_path),
                        line=int(line),
                        column=int(col),
                    ),
                    source="gcc",
                )
            )

        return DiagnosticSet(diagnostics=diagnostics, source="gcc", raw_output=output)

    def _parse_json(self, data: list[Any], raw: str) -> DiagnosticSet:
        """Parse GCC JSON diagnostic format."""
        diagnostics = []

        for item in data:
            if item.get("kind") not in ("error", "warning", "note"):
                continue

            locations = item.get("locations", [])
            location = None
            if locations:
                loc = locations[0].get("caret", {})
                location = Location(
                    file=Path(loc.get("file", "")),
                    line=loc.get("line", 0),
                    column=loc.get("column", 0),
                )

            diagnostics.append(
                Diagnostic(
                    severity=Severity.from_string(item.get("kind", "")),
                    message=item.get("message", ""),
                    location=location,
                    source="gcc",
                )
            )

        return DiagnosticSet(diagnostics=diagnostics, source="gcc", raw_output=raw)


class GenericParser(DiagnosticParser):
    """Generic fallback parser using heuristics.

    Handles common formats like:
    - file:line: error: message
    - file:line:col: warning: message
    - file(line): error message
    - [ERROR] file:line message
    """

    @property
    def name(self) -> str:
        return "generic"

    def can_parse(self, output: str) -> bool:
        # Always available as fallback
        return True

    def parse(self, output: str) -> DiagnosticSet:
        diagnostics = []
        output = strip_ansi(output)

        # Pattern 1: file:line:col: level: message (GCC style)
        p1 = re.compile(r"^(.+?):(\d+):(\d+):\s*(error|warning|info|note):\s*(.+)$", re.IGNORECASE)

        # Pattern 2: file:line: level: message
        p2 = re.compile(r"^(.+?):(\d+):\s*(error|warning|info|note):\s*(.+)$", re.IGNORECASE)

        # Pattern 3: file(line,col): level message (MSVC style)
        p3 = re.compile(r"^(.+?)\((\d+),(\d+)\):\s*(error|warning)\s+\w+:\s*(.+)$", re.IGNORECASE)

        # Pattern 4: [LEVEL] file:line message
        p4 = re.compile(r"^\[(ERROR|WARNING|INFO)\]\s*(.+?):(\d+)\s+(.+)$", re.IGNORECASE)

        for line in output.splitlines():
            line = line.strip()
            if not line or is_noise_line(line):
                continue

            diag = None

            # Try patterns in order
            if m := p1.match(line):
                diag = Diagnostic(
                    severity=Severity.from_string(m.group(4)),
                    message=m.group(5),
                    location=Location(
                        file=Path(m.group(1)),
                        line=int(m.group(2)),
                        column=int(m.group(3)),
                    ),
                    raw=line,
                )
            elif m := p2.match(line):
                diag = Diagnostic(
                    severity=Severity.from_string(m.group(3)),
                    message=m.group(4),
                    location=Location(
                        file=Path(m.group(1)),
                        line=int(m.group(2)),
                    ),
                    raw=line,
                )
            elif m := p3.match(line):
                diag = Diagnostic(
                    severity=Severity.from_string(m.group(4)),
                    message=m.group(5),
                    location=Location(
                        file=Path(m.group(1)),
                        line=int(m.group(2)),
                        column=int(m.group(3)),
                    ),
                    raw=line,
                )
            elif m := p4.match(line):
                diag = Diagnostic(
                    severity=Severity.from_string(m.group(1)),
                    message=m.group(4),
                    location=Location(
                        file=Path(m.group(2)),
                        line=int(m.group(3)),
                    ),
                    raw=line,
                )

            if diag:
                diagnostics.append(diag)

        return DiagnosticSet(diagnostics=diagnostics, source="generic", raw_output=output)


# ============================================================================
# Parser Registry
# ============================================================================


class DiagnosticRegistry:
    """Registry of diagnostic parsers."""

    def __init__(self) -> None:
        self._parsers: list[DiagnosticParser] = []
        # Register built-in parsers in priority order
        self.register(CargoParser())
        self.register(RuffParser())
        self.register(ESLintParser())
        self.register(TypeScriptParser())
        self.register(GCCParser())
        self.register(GenericParser())  # Fallback

    def register(self, parser: DiagnosticParser) -> None:
        """Register a parser (earlier = higher priority)."""
        self._parsers.append(parser)

    def parse(self, output: str) -> DiagnosticSet:
        """Parse output using the first matching parser."""
        for parser in self._parsers:
            if parser.can_parse(output):
                return parser.parse(output)
        # Should never reach here (GenericParser always matches)
        return DiagnosticSet(raw_output=output)

    def parse_with(self, parser_name: str, output: str) -> DiagnosticSet:
        """Parse output with a specific parser."""
        for parser in self._parsers:
            if parser.name == parser_name:
                return parser.parse(output)
        raise ValueError(f"Unknown parser: {parser_name}")


# Global registry instance
_registry = DiagnosticRegistry()


def parse_diagnostics(output: str, parser: str | None = None) -> DiagnosticSet:
    """Parse diagnostic output.

    Args:
        output: Raw compiler/linter output
        parser: Optional parser name to force (e.g., 'cargo', 'eslint')

    Returns:
        DiagnosticSet with parsed diagnostics
    """
    if parser:
        return _registry.parse_with(parser, output)
    return _registry.parse(output)


def get_structured_command(tool: str) -> list[str]:
    """Get command-line flags for structured output.

    Args:
        tool: Tool name (e.g., 'cargo', 'eslint', 'ruff')

    Returns:
        List of flags to add for structured output
    """
    flags = {
        "cargo": ["--message-format=json"],
        "rustc": ["--error-format=json"],
        "tsc": ["--pretty", "false"],
        "eslint": ["--format", "json"],
        "ruff": ["--output-format", "json"],
        "gcc": ["-fdiagnostics-format=json"],
        "clang": ["-fdiagnostics-format=json"],
        "g++": ["-fdiagnostics-format=json"],
        "clang++": ["-fdiagnostics-format=json"],
        "mypy": ["--output", "json"],
        "pyright": ["--outputjson"],
    }
    return flags.get(tool, [])
