"""TODO tracking and verification.

Cross-references TODOs with implementation status:
- Detect completed items still marked pending in TODO.md
- Find undocumented TODOs in code comments
- Track TODO/FIXME/HACK/XXX markers in source code
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any


class TodoStatus(Enum):
    """Status of a TODO item."""

    PENDING = "pending"
    DONE = "done"
    STALE = "stale"  # Marked done but item still exists in code
    ORPHAN = "orphan"  # In code but not tracked in TODO.md


@dataclass
class TodoItem:
    """A tracked TODO item."""

    text: str
    status: TodoStatus
    source: str  # "todo.md" or file path
    line: int
    category: str | None = None  # e.g., "Phase 22", "Future"
    marker: str = "TODO"  # TODO, FIXME, HACK, XXX

    def to_dict(self) -> dict[str, Any]:
        return {
            "text": self.text,
            "status": self.status.value,
            "source": self.source,
            "line": self.line,
            "category": self.category,
            "marker": self.marker,
        }


@dataclass
class TodoCheckResult:
    """Result of TODO verification."""

    tracked_items: list[TodoItem] = field(default_factory=list)
    code_todos: list[TodoItem] = field(default_factory=list)
    issues: list[str] = field(default_factory=list)

    @property
    def pending_count(self) -> int:
        return sum(1 for t in self.tracked_items if t.status == TodoStatus.PENDING)

    @property
    def done_count(self) -> int:
        return sum(1 for t in self.tracked_items if t.status == TodoStatus.DONE)

    @property
    def stale_count(self) -> int:
        """Items marked done but still exist as TODOs in code."""
        return sum(1 for t in self.tracked_items if t.status == TodoStatus.STALE)

    @property
    def orphan_count(self) -> int:
        """TODOs in code not tracked in TODO.md."""
        return sum(1 for t in self.code_todos if t.status == TodoStatus.ORPHAN)

    def to_dict(self) -> dict[str, Any]:
        return {
            "tracked": [t.to_dict() for t in self.tracked_items],
            "code_todos": [t.to_dict() for t in self.code_todos],
            "issues": self.issues,
            "stats": {
                "pending": self.pending_count,
                "done": self.done_count,
                "stale": self.stale_count,
                "orphan": self.orphan_count,
            },
        }

    def to_compact(self) -> str:
        """Format as compact single-line summary (token-efficient).

        Example: todos: 5 pending, 12 done | 2 stale, 1 orphan
        """
        parts = [f"todos: {self.pending_count} pending, {self.done_count} done"]
        issues = []
        if self.stale_count:
            issues.append(f"{self.stale_count} stale")
        if self.orphan_count:
            issues.append(f"{self.orphan_count} orphan")
        if issues:
            parts.append(", ".join(issues))
        return " | ".join(parts)

    def to_markdown(self) -> str:
        lines = ["# TODO Check Results", ""]

        # Stats
        total = len(self.tracked_items)
        lines.append(f"**Tracked items:** {total}")
        lines.append(f"  - Pending: {self.pending_count}")
        lines.append(f"  - Done: {self.done_count}")
        lines.append("")
        lines.append(f"**Code TODOs:** {len(self.code_todos)}")
        lines.append(f"  - Orphaned: {self.orphan_count}")
        lines.append("")

        # Issues
        if self.stale_count > 0:
            lines.append("## Stale Items")
            lines.append("")
            lines.append("Items marked done in TODO.md but still have TODOs in code:")
            lines.append("")
            for item in self.tracked_items:
                if item.status == TodoStatus.STALE:
                    lines.append(f"- [ ] {item.text}")
                    lines.append(f"  - Source: {item.source}:{item.line}")
            lines.append("")

        # Orphaned TODOs
        if self.orphan_count > 0:
            lines.append("## Orphaned TODOs")
            lines.append("")
            lines.append("TODOs in code not tracked in TODO.md:")
            lines.append("")
            for item in self.code_todos:
                if item.status == TodoStatus.ORPHAN:
                    lines.append(f"- {item.marker}: {item.text}")
                    lines.append(f"  - File: `{item.source}`:{item.line}")
            lines.append("")

        # Summary by category
        if self.tracked_items:
            by_category: dict[str, list[TodoItem]] = {}
            for item in self.tracked_items:
                cat = item.category or "Uncategorized"
                by_category.setdefault(cat, []).append(item)

            if by_category:
                lines.append("## By Category")
                lines.append("")
                for cat, items in sorted(by_category.items()):
                    pending = sum(1 for i in items if i.status == TodoStatus.PENDING)
                    done = sum(1 for i in items if i.status == TodoStatus.DONE)
                    lines.append(f"- **{cat}**: {pending} pending, {done} done")

        return "\n".join(lines)


class TodoChecker:
    """Check TODOs across codebase and documentation."""

    # Patterns for TODO markers in code
    TODO_PATTERN = re.compile(
        r"#\s*(TODO|FIXME|HACK|XXX|NOTE)[\s:]+(.+?)(?:\s*#.*)?$",
        re.IGNORECASE,
    )

    # Pattern for markdown checkbox items
    CHECKBOX_PATTERN = re.compile(r"^\s*-\s*\[([ xX])\]\s*(.+)$")

    def __init__(self, root: Path):
        self.root = root.resolve()

    def check(self) -> TodoCheckResult:
        """Run all TODO checks."""
        result = TodoCheckResult()

        # Parse TODO.md if it exists
        todo_file = self.root / "TODO.md"
        if todo_file.exists():
            tracked = self._parse_todo_md(todo_file)
            result.tracked_items.extend(tracked)

        # Scan code for TODOs
        code_todos = self._scan_code_todos()
        result.code_todos.extend(code_todos)

        # Cross-reference: find orphaned TODOs
        tracked_texts = {self._normalize(t.text) for t in result.tracked_items}
        for todo in result.code_todos:
            normalized = self._normalize(todo.text)
            if normalized not in tracked_texts:
                todo.status = TodoStatus.ORPHAN

        return result

    def _parse_todo_md(self, path: Path) -> list[TodoItem]:
        """Parse TODO.md for tracked items."""
        items = []
        current_category = None

        try:
            content = path.read_text()
        except Exception:
            return items

        for i, line in enumerate(content.splitlines(), 1):
            # Track category (## headers)
            if line.startswith("## "):
                current_category = line[3:].strip()
                continue
            if line.startswith("### "):
                current_category = line[4:].strip()
                continue

            # Match checkbox items
            match = self.CHECKBOX_PATTERN.match(line)
            if match:
                checked = match.group(1).lower() == "x"
                text = match.group(2).strip()

                status = TodoStatus.DONE if checked else TodoStatus.PENDING
                items.append(
                    TodoItem(
                        text=text,
                        status=status,
                        source=str(path.relative_to(self.root)),
                        line=i,
                        category=current_category,
                    )
                )

        return items

    def _scan_code_todos(self) -> list[TodoItem]:
        """Scan source files for TODO comments."""
        items = []

        # Find Python files
        for py_file in self.root.rglob("*.py"):
            # Skip common non-source directories
            parts = py_file.relative_to(self.root).parts
            if any(p in parts for p in [".git", "__pycache__", ".venv", "node_modules"]):
                continue

            try:
                content = py_file.read_text()
            except Exception:
                continue

            for i, line in enumerate(content.splitlines(), 1):
                match = self.TODO_PATTERN.search(line)
                if match:
                    marker = match.group(1).upper()
                    text = match.group(2).strip()

                    items.append(
                        TodoItem(
                            text=text,
                            status=TodoStatus.PENDING,
                            source=str(py_file.relative_to(self.root)),
                            line=i,
                            marker=marker,
                        )
                    )

        return items

    def _normalize(self, text: str) -> str:
        """Normalize text for comparison."""
        # Remove markdown formatting, extra whitespace
        text = re.sub(r"`[^`]+`", "", text)
        text = re.sub(r"\*\*[^*]+\*\*", "", text)
        text = re.sub(r"\s+", " ", text)
        return text.lower().strip()
