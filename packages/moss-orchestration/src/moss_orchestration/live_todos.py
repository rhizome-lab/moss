"""Live TODO tracking for agent sessions.

Provides real-time task display and session persistence.

Usage:
    from moss_orchestration.live_todos import TodoTracker, TodoItem, TodoStatus

    # Create tracker
    tracker = TodoTracker()

    # Add tasks
    tracker.add("Implement feature X")
    tracker.add("Write tests for X")

    # Start a task
    tracker.start("Implement feature X")

    # Complete a task
    tracker.complete("Implement feature X")

    # Get display
    print(tracker.format())

    # Save session
    tracker.save()

    # Load session (resumes from last state)
    tracker = TodoTracker.load()
"""

from __future__ import annotations

import json
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from pathlib import Path
from typing import Any


class TodoStatus(Enum):
    """Status of a todo item."""

    PENDING = "pending"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"
    BLOCKED = "blocked"
    SKIPPED = "skipped"


@dataclass
class TodoItem:
    """A todo item with status tracking."""

    content: str
    status: TodoStatus = TodoStatus.PENDING
    created_at: float = field(default_factory=time.time)
    started_at: float | None = None
    completed_at: float | None = None
    notes: str = ""

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "content": self.content,
            "status": self.status.value,
            "created_at": self.created_at,
            "started_at": self.started_at,
            "completed_at": self.completed_at,
            "notes": self.notes,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> TodoItem:
        """Create from dictionary."""
        return cls(
            content=data["content"],
            status=TodoStatus(data["status"]),
            created_at=data.get("created_at", time.time()),
            started_at=data.get("started_at"),
            completed_at=data.get("completed_at"),
            notes=data.get("notes", ""),
        )

    @property
    def elapsed_time(self) -> float | None:
        """Time elapsed since start (or total if completed)."""
        if self.started_at is None:
            return None
        end = self.completed_at or time.time()
        return end - self.started_at

    def elapsed_str(self) -> str:
        """Human-readable elapsed time."""
        elapsed = self.elapsed_time
        if elapsed is None:
            return ""
        if elapsed < 60:
            return f"{elapsed:.0f}s"
        minutes = int(elapsed // 60)
        seconds = int(elapsed % 60)
        return f"{minutes}m {seconds}s"


@dataclass
class TodoSession:
    """A session of TODO tracking."""

    session_id: str
    items: list[TodoItem] = field(default_factory=list)
    created_at: float = field(default_factory=time.time)
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "session_id": self.session_id,
            "items": [item.to_dict() for item in self.items],
            "created_at": self.created_at,
            "metadata": self.metadata,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> TodoSession:
        """Create from dictionary."""
        return cls(
            session_id=data["session_id"],
            items=[TodoItem.from_dict(i) for i in data.get("items", [])],
            created_at=data.get("created_at", time.time()),
            metadata=data.get("metadata", {}),
        )


class TodoTracker:
    """Track TODO items with real-time updates and persistence."""

    def __init__(
        self,
        session_id: str | None = None,
        storage_dir: Path | None = None,
        on_update: Callable[[TodoItem], None] | None = None,
    ):
        """Initialize the tracker.

        Args:
            session_id: Unique session identifier (auto-generated if not provided)
            storage_dir: Directory for session persistence (default: .moss/sessions)
            on_update: Callback for item updates
        """
        self.session_id = session_id or datetime.now().strftime("%Y%m%d_%H%M%S")
        self.storage_dir = storage_dir or Path(".moss/sessions")
        self.on_update = on_update
        self._items: dict[str, TodoItem] = {}
        self._order: list[str] = []  # Preserve insertion order

    @property
    def items(self) -> list[TodoItem]:
        """Get all items in order."""
        return [self._items[key] for key in self._order if key in self._items]

    def add(
        self,
        content: str,
        status: TodoStatus = TodoStatus.PENDING,
        notes: str = "",
    ) -> TodoItem:
        """Add a new todo item.

        Args:
            content: Description of the todo
            status: Initial status
            notes: Optional notes

        Returns:
            The created TodoItem
        """
        item = TodoItem(content=content, status=status, notes=notes)
        self._items[content] = item
        if content not in self._order:
            self._order.append(content)
        if self.on_update:
            self.on_update(item)
        return item

    def get(self, content: str) -> TodoItem | None:
        """Get an item by content."""
        return self._items.get(content)

    def start(self, content: str) -> TodoItem | None:
        """Mark an item as in progress.

        Args:
            content: Item content to start

        Returns:
            The updated item, or None if not found
        """
        item = self._items.get(content)
        if item:
            item.status = TodoStatus.IN_PROGRESS
            item.started_at = time.time()
            if self.on_update:
                self.on_update(item)
        return item

    def complete(self, content: str, notes: str = "") -> TodoItem | None:
        """Mark an item as completed.

        Args:
            content: Item content to complete
            notes: Optional completion notes

        Returns:
            The updated item, or None if not found
        """
        item = self._items.get(content)
        if item:
            item.status = TodoStatus.COMPLETED
            item.completed_at = time.time()
            if notes:
                item.notes = notes
            if self.on_update:
                self.on_update(item)
        return item

    def block(self, content: str, reason: str = "") -> TodoItem | None:
        """Mark an item as blocked.

        Args:
            content: Item content to block
            reason: Why it's blocked

        Returns:
            The updated item, or None if not found
        """
        item = self._items.get(content)
        if item:
            item.status = TodoStatus.BLOCKED
            if reason:
                item.notes = reason
            if self.on_update:
                self.on_update(item)
        return item

    def skip(self, content: str, reason: str = "") -> TodoItem | None:
        """Mark an item as skipped.

        Args:
            content: Item content to skip
            reason: Why it's skipped

        Returns:
            The updated item, or None if not found
        """
        item = self._items.get(content)
        if item:
            item.status = TodoStatus.SKIPPED
            if reason:
                item.notes = reason
            if self.on_update:
                self.on_update(item)
        return item

    def remove(self, content: str) -> bool:
        """Remove an item.

        Args:
            content: Item content to remove

        Returns:
            True if removed, False if not found
        """
        if content in self._items:
            del self._items[content]
            self._order.remove(content)
            return True
        return False

    def clear(self) -> None:
        """Clear all items."""
        self._items.clear()
        self._order.clear()

    @property
    def current(self) -> TodoItem | None:
        """Get the current (in_progress) item."""
        for item in self.items:
            if item.status == TodoStatus.IN_PROGRESS:
                return item
        return None

    @property
    def pending(self) -> list[TodoItem]:
        """Get all pending items."""
        return [i for i in self.items if i.status == TodoStatus.PENDING]

    @property
    def completed(self) -> list[TodoItem]:
        """Get all completed items."""
        return [i for i in self.items if i.status == TodoStatus.COMPLETED]

    @property
    def stats(self) -> dict[str, int]:
        """Get statistics."""
        stats: dict[str, int] = {}
        for status in TodoStatus:
            stats[status.value] = sum(1 for i in self.items if i.status == status)
        stats["total"] = len(self.items)
        return stats

    def format(self, compact: bool = False) -> str:
        """Format the todo list for display.

        Args:
            compact: Use compact format (single line per item)

        Returns:
            Formatted string
        """
        if not self.items:
            return "No tasks"

        lines = []
        stats = self.stats

        # Header
        completed = stats["completed"]
        total = stats["total"]
        pct = (completed / total * 100) if total > 0 else 0
        lines.append(f"Tasks: {completed}/{total} ({pct:.0f}%)")
        lines.append("")

        # Status icons
        icons = {
            TodoStatus.PENDING: "○",
            TodoStatus.IN_PROGRESS: "●",
            TodoStatus.COMPLETED: "✓",
            TodoStatus.BLOCKED: "✗",
            TodoStatus.SKIPPED: "⊘",
        }

        for item in self.items:
            icon = icons.get(item.status, "?")
            elapsed = item.elapsed_str()
            if compact:
                suffix = f" ({elapsed})" if elapsed else ""
                lines.append(f"  {icon} {item.content}{suffix}")
            else:
                lines.append(f"  {icon} {item.content}")
                if elapsed:
                    lines.append(f"      {elapsed}")
                if item.notes:
                    lines.append(f"      {item.notes}")

        return "\n".join(lines)

    def format_compact(self) -> str:
        """Format for single-line display (status bar style)."""
        if not self.items:
            return ""

        current = self.current
        stats = self.stats
        completed = stats["completed"]
        total = stats["total"]

        if current:
            return f"[{completed}/{total}] {current.content}"
        elif stats["pending"] > 0:
            return f"[{completed}/{total}] {stats['pending']} pending"
        else:
            return f"[{completed}/{total}] Done"

    def session_path(self) -> Path:
        """Get the path for session storage."""
        return self.storage_dir / f"{self.session_id}.json"

    def save(self) -> Path:
        """Save the session to disk.

        Returns:
            Path to saved file
        """
        self.storage_dir.mkdir(parents=True, exist_ok=True)
        session = TodoSession(
            session_id=self.session_id,
            items=self.items,
        )
        path = self.session_path()
        path.write_text(json.dumps(session.to_dict(), indent=2))
        return path

    @classmethod
    def load(
        cls,
        session_id: str,
        storage_dir: Path | None = None,
    ) -> TodoTracker | None:
        """Load a session from disk.

        Args:
            session_id: Session ID to load
            storage_dir: Storage directory

        Returns:
            TodoTracker with loaded session, or None if not found
        """
        storage_dir = storage_dir or Path(".moss/sessions")
        path = storage_dir / f"{session_id}.json"

        if not path.exists():
            return None

        try:
            data = json.loads(path.read_text())
            session = TodoSession.from_dict(data)
            tracker = cls(session_id=session.session_id, storage_dir=storage_dir)
            for item in session.items:
                tracker._items[item.content] = item
                tracker._order.append(item.content)
            return tracker
        except (json.JSONDecodeError, KeyError):
            return None

    @classmethod
    def latest(cls, storage_dir: Path | None = None) -> TodoTracker | None:
        """Load the most recent session.

        Args:
            storage_dir: Storage directory

        Returns:
            TodoTracker with latest session, or None if no sessions
        """
        storage_dir = storage_dir or Path(".moss/sessions")

        if not storage_dir.exists():
            return None

        sessions = sorted(storage_dir.glob("*.json"), reverse=True)
        if not sessions:
            return None

        session_id = sessions[0].stem
        return cls.load(session_id, storage_dir)

    @classmethod
    def list_sessions(cls, storage_dir: Path | None = None) -> list[str]:
        """List all saved sessions.

        Args:
            storage_dir: Storage directory

        Returns:
            List of session IDs
        """
        storage_dir = storage_dir or Path(".moss/sessions")

        if not storage_dir.exists():
            return []

        return sorted([f.stem for f in storage_dir.glob("*.json")], reverse=True)


def create_tracker(
    session_id: str | None = None,
    resume: bool = False,
) -> TodoTracker:
    """Create a new tracker, optionally resuming the last session.

    Args:
        session_id: Specific session ID (auto-generates if None)
        resume: If True, resume the latest session

    Returns:
        TodoTracker instance
    """
    if resume:
        tracker = TodoTracker.latest()
        if tracker:
            return tracker

    return TodoTracker(session_id=session_id)


__all__ = [
    "TodoItem",
    "TodoSession",
    "TodoStatus",
    "TodoTracker",
    "create_tracker",
]
