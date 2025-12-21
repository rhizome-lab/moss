"""Base parser with shared utilities."""

from __future__ import annotations

import json
from abc import ABC, abstractmethod
from pathlib import Path
from typing import Any

from moss.preferences.parsing import LogFormat, ParsedSession


class BaseParser(ABC):
    """Base class for session log parsers with common utilities."""

    format: LogFormat

    def __init__(self, path: Path) -> None:
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
        except (OSError, UnicodeDecodeError):
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
        except OSError:
            pass
        return entries
