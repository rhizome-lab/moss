"""Regex-based pattern matching backend.

The simplest backend - matches patterns using Python regex.
Fast but not AST-aware.

Usage:
    @rule(backend="regex")
    def no_print(ctx: RuleContext) -> list[Violation]:
        for match in ctx.backend("regex").matches:
            ...

The pattern is stored in the rule spec's _pattern attribute.
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any

from ..base import BackendResult, BaseBackend, Location, Match
from . import register_backend


@register_backend
class RegexBackend(BaseBackend):
    """Simple regex pattern matching backend."""

    @property
    def name(self) -> str:
        return "regex"

    def analyze(
        self,
        file_path: Path,
        pattern: str | None = None,
        **options: Any,
    ) -> BackendResult:
        """Find regex matches in a file.

        Args:
            file_path: File to analyze
            pattern: Regex pattern to match
            **options:
                - case_sensitive: bool = True
                - multiline: bool = False

        Returns:
            BackendResult with Match objects for each hit
        """
        matches: list[Match] = []
        errors: list[str] = []

        if pattern is None:
            return BackendResult(backend_name=self.name, matches=matches)

        try:
            source = file_path.read_text()
        except (OSError, UnicodeDecodeError) as e:
            return BackendResult(
                backend_name=self.name,
                matches=[],
                errors=[f"Could not read file: {e}"],
            )

        # Compile pattern with options
        flags = 0
        if not options.get("case_sensitive", True):
            flags |= re.IGNORECASE
        if options.get("multiline", False):
            flags |= re.MULTILINE

        try:
            compiled = re.compile(pattern, flags)
        except re.error as e:
            return BackendResult(
                backend_name=self.name,
                matches=[],
                errors=[f"Invalid regex pattern: {e}"],
            )

        # Find all matches with line/column info
        lines = source.splitlines(keepends=True)
        line_offsets = self._compute_line_offsets(lines)

        for m in compiled.finditer(source):
            start = m.start()
            end = m.end()

            line_num, col = self._offset_to_line_col(start, line_offsets)
            end_line, end_col = self._offset_to_line_col(end, line_offsets)

            matches.append(
                Match(
                    location=Location(
                        file_path=file_path,
                        line=line_num,
                        column=col,
                        end_line=end_line,
                        end_column=end_col,
                    ),
                    text=m.group(),
                    metadata={
                        "groups": m.groups(),
                        "groupdict": m.groupdict(),
                    },
                )
            )

        return BackendResult(
            backend_name=self.name,
            matches=matches,
            errors=errors,
        )

    def _compute_line_offsets(self, lines: list[str]) -> list[int]:
        """Compute byte offset of each line start."""
        offsets = [0]
        for line in lines:
            offsets.append(offsets[-1] + len(line))
        return offsets

    def _offset_to_line_col(self, offset: int, line_offsets: list[int]) -> tuple[int, int]:
        """Convert byte offset to line and column (1-indexed)."""
        for i, line_offset in enumerate(line_offsets):
            if i + 1 < len(line_offsets) and offset < line_offsets[i + 1]:
                return i + 1, offset - line_offset + 1
        # Last line
        return len(line_offsets) - 1, offset - line_offsets[-2] + 1

    def supports_pattern(self, pattern: str) -> bool:
        """Check if pattern is valid regex."""
        try:
            re.compile(pattern)
            return True
        except re.error:
            return False
