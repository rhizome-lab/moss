"""First-class sessions: resumable, observable work units.

Sessions are the core abstraction for tracking agent work. They provide:
- Resumability: save/load session state across restarts
- Observability: emit events for all operations
- Traceability: record tool calls, file changes, decisions
- Integration: tie together todos, context, and loop execution
"""

from __future__ import annotations

import json
import uuid
from collections import defaultdict
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum, auto
from pathlib import Path
from typing import Any

from moss.events import EventBus, EventType


class SessionStatus(Enum):
    """Current status of a session."""

    CREATED = auto()
    RUNNING = auto()
    PAUSED = auto()
    COMPLETED = auto()
    FAILED = auto()
    CANCELLED = auto()


class MessageRole(Enum):
    """Role of a message sender."""

    USER = auto()
    AGENT = auto()
    SYSTEM = auto()


@dataclass
class Message:
    """A message in the session inbox/chat history."""

    role: MessageRole
    content: str
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))
    read: bool = False

    def to_dict(self) -> dict[str, Any]:
        return {
            "role": self.role.name.lower(),
            "content": self.content,
            "timestamp": self.timestamp.isoformat(),
            "read": self.read,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Message:
        return cls(
            role=MessageRole[data["role"].upper()],
            content=data["content"],
            timestamp=datetime.fromisoformat(data["timestamp"]),
            read=data.get("read", False),
        )


@dataclass
class ToolCall:
    """Record of a single tool call."""

    tool_name: str
    parameters: dict[str, Any]
    result: Any = None
    error: str | None = None
    started_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    completed_at: datetime | None = None
    duration_ms: int = 0

    def complete(self, result: Any = None, error: str | None = None) -> None:
        """Mark the tool call as complete."""
        self.completed_at = datetime.now(UTC)
        self.result = result
        self.error = error
        self.duration_ms = int((self.completed_at - self.started_at).total_seconds() * 1000)

    def to_dict(self) -> dict[str, Any]:
        return {
            "tool_name": self.tool_name,
            "parameters": self.parameters,
            "result": str(self.result)[:500] if self.result else None,
            "error": self.error,
            "started_at": self.started_at.isoformat(),
            "completed_at": self.completed_at.isoformat() if self.completed_at else None,
            "duration_ms": self.duration_ms,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ToolCall:
        call = cls(
            tool_name=data["tool_name"],
            parameters=data.get("parameters", {}),
            result=data.get("result"),
            error=data.get("error"),
            started_at=datetime.fromisoformat(data["started_at"]),
            duration_ms=data.get("duration_ms", 0),
        )
        if data.get("completed_at"):
            call.completed_at = datetime.fromisoformat(data["completed_at"])
        return call


@dataclass
class FileChange:
    """Record of a file modification."""

    path: Path
    action: str  # created, modified, deleted
    before_hash: str | None = None
    after_hash: str | None = None
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))

    def to_dict(self) -> dict[str, Any]:
        return {
            "path": str(self.path),
            "action": self.action,
            "before_hash": self.before_hash,
            "after_hash": self.after_hash,
            "timestamp": self.timestamp.isoformat(),
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> FileChange:
        return cls(
            path=Path(data["path"]),
            action=data["action"],
            before_hash=data.get("before_hash"),
            after_hash=data.get("after_hash"),
            timestamp=datetime.fromisoformat(data["timestamp"]),
        )


@dataclass
class Checkpoint:
    """A named checkpoint in the session for resumption."""

    name: str
    description: str
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    tool_call_index: int = 0
    file_change_index: int = 0
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "created_at": self.created_at.isoformat(),
            "tool_call_index": self.tool_call_index,
            "file_change_index": self.file_change_index,
            "metadata": self.metadata,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Checkpoint:
        return cls(
            name=data["name"],
            description=data["description"],
            created_at=datetime.fromisoformat(data["created_at"]),
            tool_call_index=data.get("tool_call_index", 0),
            file_change_index=data.get("file_change_index", 0),
            metadata=data.get("metadata", {}),
        )


@dataclass
class Session:
    """A resumable, observable work unit.

    Sessions track all agent activity including tool calls, file changes,
    and checkpoints. They can be saved and resumed later.

    Example:
        session = Session.create(workspace=Path.cwd(), task="Fix the login bug")

        # Record work
        session.record_tool_call("read_file", {"path": "auth.py"}, result="...")
        session.record_file_change(Path("auth.py"), "modified")
        session.checkpoint("fixed_validation", "Fixed input validation")

        # Save for later
        session.save()

        # Resume later
        session = Session.load(session_id)
    """

    id: str
    workspace: Path
    task: str
    status: SessionStatus = SessionStatus.CREATED
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    updated_at: datetime = field(default_factory=lambda: datetime.now(UTC))

    # Work records
    tool_calls: list[ToolCall] = field(default_factory=list)
    file_changes: list[FileChange] = field(default_factory=list)
    checkpoints: list[Checkpoint] = field(default_factory=list)
    messages: list[Message] = field(default_factory=list)
    access_patterns: dict[str, int] = field(default_factory=lambda: defaultdict(int))

    # Metrics
    llm_tokens_in: int = 0
    llm_tokens_out: int = 0
    llm_calls: int = 0

    # Flexible metadata
    metadata: dict[str, Any] = field(default_factory=dict)
    tags: list[str] = field(default_factory=list)

    # Event bus for observability
    _event_bus: EventBus | None = field(default=None, repr=False)
    _on_update: Callable[[Session], None] | None = field(default=None, repr=False)

    @classmethod
    def create(
        cls,
        workspace: Path | None = None,
        task: str = "",
        session_id: str | None = None,
        event_bus: EventBus | None = None,
        on_update: Callable[[Session], None] | None = None,
        **metadata: Any,
    ) -> Session:
        """Create a new session.

        Args:
            workspace: Working directory for the session
            task: Description of the task
            session_id: Optional ID (auto-generated if not provided)
            event_bus: Optional event bus for emitting events
            on_update: Optional callback on session updates
            **metadata: Additional metadata
        """
        return cls(
            id=session_id or str(uuid.uuid4())[:8],
            workspace=workspace or Path.cwd(),
            task=task,
            metadata=metadata,
            _event_bus=event_bus,
            _on_update=on_update,
        )

    def _emit(self, event_type: EventType, payload: dict[str, Any]) -> None:
        """Emit an event if event bus is configured."""
        if self._event_bus:
            import asyncio

            try:
                asyncio.get_running_loop()
                # Store reference to prevent garbage collection
                task = asyncio.create_task(self._event_bus.emit(event_type, payload))
                # Fire and forget - we don't need the result
                task.add_done_callback(lambda _: None)
            except RuntimeError:
                # No running loop - run synchronously
                asyncio.run(self._event_bus.emit(event_type, payload))

    def _notify_update(self) -> None:
        """Notify update callback if configured."""
        self.updated_at = datetime.now(UTC)
        if self._on_update:
            self._on_update(self)

    def start(self) -> None:
        """Mark session as running."""
        self.status = SessionStatus.RUNNING
        self._emit(EventType.LOOP_STARTED, {"session_id": self.id, "task": self.task})
        self._notify_update()

    def pause(self, reason: str = "") -> None:
        """Pause the session."""
        self.status = SessionStatus.PAUSED
        self.metadata["pause_reason"] = reason
        self._emit(EventType.STEP_COMPLETED, {"session_id": self.id, "action": "paused"})
        self._notify_update()

    def resume(self) -> None:
        """Resume a paused session."""
        if self.status == SessionStatus.PAUSED:
            self.status = SessionStatus.RUNNING
            self._emit(EventType.LOOP_STARTED, {"session_id": self.id, "action": "resumed"})
            self._notify_update()

    def complete(self, result: Any = None) -> None:
        """Mark session as completed."""
        self.status = SessionStatus.COMPLETED
        self.metadata["result"] = result
        self._emit(
            EventType.LOOP_COMPLETED,
            {"session_id": self.id, "status": "completed", "result": str(result)[:200]},
        )
        self._notify_update()

    def fail(self, error: str) -> None:
        """Mark session as failed."""
        self.status = SessionStatus.FAILED
        self.metadata["error"] = error
        self._emit(
            EventType.ERROR_OCCURRED,
            {"session_id": self.id, "error": error},
        )
        self._notify_update()

    def cancel(self, reason: str = "") -> None:
        """Cancel the session."""
        self.status = SessionStatus.CANCELLED
        self.metadata["cancel_reason"] = reason
        self._notify_update()

    def record_tool_call(
        self,
        tool_name: str,
        parameters: dict[str, Any],
        result: Any = None,
        error: str | None = None,
    ) -> ToolCall:
        """Record a tool call."""
        call = ToolCall(tool_name=tool_name, parameters=parameters)
        call.complete(result=result, error=error)
        self.tool_calls.append(call)

        self._emit(
            EventType.TOOL_CALL,
            {
                "session_id": self.id,
                "tool_name": tool_name,
                "success": error is None,
                "duration_ms": call.duration_ms,
            },
        )
        self._notify_update()
        return call

    def record_file_change(
        self,
        path: Path,
        action: str,
        before_hash: str | None = None,
        after_hash: str | None = None,
    ) -> FileChange:
        """Record a file modification."""
        # Update access patterns
        self.access_patterns[str(path)] += 1

        change = FileChange(
            path=path,
            action=action,
            before_hash=before_hash,
            after_hash=after_hash,
        )
        self.file_changes.append(change)

        self._emit(
            EventType.FILE_MODIFIED,
            {
                "session_id": self.id,
                "path": str(path),
                "action": action,
            },
        )
        self._notify_update()
        return change

    def record_llm_usage(self, tokens_in: int, tokens_out: int) -> None:
        """Record LLM token usage."""
        self.llm_tokens_in += tokens_in
        self.llm_tokens_out += tokens_out
        self.llm_calls += 1
        self._notify_update()

    def checkpoint(self, name: str, description: str = "", **metadata: Any) -> Checkpoint:
        """Create a checkpoint for resumption."""
        cp = Checkpoint(
            name=name,
            description=description,
            tool_call_index=len(self.tool_calls),
            file_change_index=len(self.file_changes),
            metadata=metadata,
        )
        self.checkpoints.append(cp)

        self._emit(
            EventType.STEP_COMPLETED,
            {
                "session_id": self.id,
                "checkpoint": name,
                "description": description,
            },
        )
        self._notify_update()
        return cp

    def get_checkpoint(self, name: str) -> Checkpoint | None:
        """Get a checkpoint by name."""
        for cp in self.checkpoints:
            if cp.name == name:
                return cp
        return None

    def send_message(self, content: str, role: MessageRole = MessageRole.USER) -> Message:
        """Send a message to the session inbox."""
        msg = Message(role=role, content=content)
        self.messages.append(msg)
        self._emit(
            EventType.STEP_COMPLETED,
            {"session_id": self.id, "action": "message_sent", "role": role.name.lower()},
        )
        self._notify_update()
        return msg

    def get_unread_messages(self, mark_as_read: bool = True) -> list[Message]:
        """Get all unread messages from the inbox."""
        unread = [m for m in self.messages if not m.read]
        if mark_as_read:
            for m in unread:
                m.read = True
            if unread:
                self._notify_update()
        return unread

    @property
    def duration_seconds(self) -> float:
        """Total session duration in seconds."""
        return (self.updated_at - self.created_at).total_seconds()

    @property
    def total_tokens(self) -> int:
        """Total LLM tokens used."""
        return self.llm_tokens_in + self.llm_tokens_out

    def to_dict(self) -> dict[str, Any]:
        """Serialize session to dictionary."""
        return {
            "id": self.id,
            "workspace": str(self.workspace),
            "task": self.task,
            "status": self.status.name.lower(),
            "created_at": self.created_at.isoformat(),
            "updated_at": self.updated_at.isoformat(),
            "tool_calls": [tc.to_dict() for tc in self.tool_calls],
            "file_changes": [fc.to_dict() for fc in self.file_changes],
            "checkpoints": [cp.to_dict() for cp in self.checkpoints],
            "messages": [m.to_dict() for m in self.messages],
            "llm_tokens_in": self.llm_tokens_in,
            "llm_tokens_out": self.llm_tokens_out,
            "llm_calls": self.llm_calls,
            "access_patterns": dict(self.access_patterns),
            "metadata": self.metadata,
            "tags": self.tags,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Session:
        """Deserialize session from dictionary."""
        status_name = data.get("status", "created").upper()
        return cls(
            id=data["id"],
            workspace=Path(data["workspace"]),
            task=data.get("task", ""),
            status=SessionStatus[status_name],
            created_at=datetime.fromisoformat(data["created_at"]),
            updated_at=datetime.fromisoformat(data["updated_at"]),
            tool_calls=[ToolCall.from_dict(tc) for tc in data.get("tool_calls", [])],
            file_changes=[FileChange.from_dict(fc) for fc in data.get("file_changes", [])],
            checkpoints=[Checkpoint.from_dict(cp) for cp in data.get("checkpoints", [])],
            messages=[Message.from_dict(m) for m in data.get("messages", [])],
            llm_tokens_in=data.get("llm_tokens_in", 0),
            llm_tokens_out=data.get("llm_tokens_out", 0),
            llm_calls=data.get("llm_calls", 0),
            access_patterns=defaultdict(int, data.get("access_patterns", {})),
            metadata=data.get("metadata", {}),
            tags=data.get("tags", []),
        )

    def to_compact(self) -> str:
        """Format as compact summary."""
        status_icon = {
            SessionStatus.CREATED: "○",
            SessionStatus.RUNNING: "●",
            SessionStatus.PAUSED: "◐",
            SessionStatus.COMPLETED: "✓",
            SessionStatus.FAILED: "✗",
            SessionStatus.CANCELLED: "⊘",
        }
        icon = status_icon.get(self.status, "?")
        return (
            f"{icon} [{self.id}] {self.task[:40]} | "
            f"tools: {len(self.tool_calls)}, files: {len(self.file_changes)}, "
            f"tokens: {self.total_tokens}, time: {self.duration_seconds:.1f}s"
        )


class SessionManager:
    """Manage session lifecycle and persistence.

    Provides:
    - Session creation and storage
    - Listing and searching sessions
    - Loading sessions for resumption
    """

    def __init__(
        self,
        storage_dir: Path | None = None,
        event_bus: EventBus | None = None,
    ):
        self.storage_dir = storage_dir or Path(".moss/sessions")
        self.event_bus = event_bus
        self._active_sessions: dict[str, Session] = {}

    def _session_path(self, session_id: str) -> Path:
        """Get the storage path for a session."""
        return self.storage_dir / f"{session_id}.json"

    def create(
        self,
        task: str = "",
        workspace: Path | None = None,
        session_id: str | None = None,
        tags: list[str] | None = None,
        **metadata: Any,
    ) -> Session:
        """Create a new session."""
        session = Session.create(
            workspace=workspace,
            task=task,
            session_id=session_id,
            event_bus=self.event_bus,
            **metadata,
        )
        if tags:
            session.tags = tags
        self._active_sessions[session.id] = session
        return session

    def save(self, session: Session) -> Path:
        """Save a session to storage."""
        self.storage_dir.mkdir(parents=True, exist_ok=True)
        path = self._session_path(session.id)
        path.write_text(json.dumps(session.to_dict(), indent=2))
        return path

    def load(self, session_id: str) -> Session | None:
        """Load a session from storage."""
        path = self._session_path(session_id)
        if not path.exists():
            return None

        data = json.loads(path.read_text())
        session = Session.from_dict(data)
        session._event_bus = self.event_bus
        self._active_sessions[session.id] = session
        return session

    def get(self, session_id: str) -> Session | None:
        """Get an active or stored session."""
        if session_id in self._active_sessions:
            return self._active_sessions[session_id]
        return self.load(session_id)

    def list_sessions(
        self,
        status: SessionStatus | None = None,
        tags: list[str] | None = None,
        limit: int = 50,
    ) -> list[Session]:
        """List stored sessions with optional filtering."""
        sessions = []
        if not self.storage_dir.exists():
            return sessions

        for path in sorted(self.storage_dir.glob("*.json"), reverse=True):
            if len(sessions) >= limit:
                break

            try:
                data = json.loads(path.read_text())
                session = Session.from_dict(data)

                # Filter by status
                if status and session.status != status:
                    continue

                # Filter by tags
                if tags and not all(t in session.tags for t in tags):
                    continue

                sessions.append(session)
            except (json.JSONDecodeError, KeyError):
                continue

        return sessions

    def resume_latest(
        self,
        status: SessionStatus | None = SessionStatus.PAUSED,
    ) -> Session | None:
        """Resume the most recent session with given status."""
        sessions = self.list_sessions(status=status, limit=1)
        if sessions:
            session = sessions[0]
            session.resume()
            return session
        return None

    def cleanup_old(self, max_age_days: int = 30) -> int:
        """Remove sessions older than max_age_days. Returns count removed."""
        if not self.storage_dir.exists():
            return 0

        cutoff = datetime.now(UTC).timestamp() - (max_age_days * 86400)
        removed = 0

        for path in self.storage_dir.glob("*.json"):
            if path.stat().st_mtime < cutoff:
                path.unlink()
                removed += 1

        return removed

    @property
    def active_sessions(self) -> list[Session]:
        """Get all currently active sessions."""
        return list(self._active_sessions.values())


def create_session(
    task: str = "",
    workspace: Path | None = None,
    storage_dir: Path | None = None,
    **metadata: Any,
) -> Session:
    """Convenience function to create a session.

    Args:
        task: Description of the task
        workspace: Working directory
        storage_dir: Where to store session data
        **metadata: Additional metadata

    Returns:
        A new Session instance
    """
    manager = SessionManager(storage_dir=storage_dir)
    return manager.create(task=task, workspace=workspace, **metadata)
