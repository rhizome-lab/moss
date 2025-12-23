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
    memory_bytes: int = 0
    context_tokens: int = 0
    memory_breakdown: dict[str, int] = field(default_factory=dict)

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
            "memory_bytes": self.memory_bytes,
            "context_tokens": self.context_tokens,
            "memory_breakdown": self.memory_breakdown,
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
            memory_bytes=data.get("memory_bytes", 0),
            context_tokens=data.get("context_tokens", 0),
            memory_breakdown=data.get("memory_breakdown", {}),
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


# Driver is a string, not enum - allows plugin drivers
# See docs/driver-architecture.md for the plugin design
# Common values: "user", "llm", "workflow", "state_machine"


@dataclass
class Session:
    """A resumable, observable work unit (unified Task model).

    Sessions (Tasks) track all agent activity including tool calls, file changes,
    and checkpoints. They can be saved and resumed later. Every task gets its own
    shadow branch for tracking changes.

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

    # Task hierarchy (unified model)
    parent_id: str | None = None
    children: list[str] = field(default_factory=list)
    shadow_branch: str | None = None  # shadow/task-{id}
    driver: str = "user"  # Plugin driver name (user, llm, workflow, etc.)
    driver_config: dict[str, Any] = field(default_factory=dict)  # Driver-specific config

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
        parent_id: str | None = None,
        driver: str = "user",
        driver_config: dict[str, Any] | None = None,
        event_bus: EventBus | None = None,
        on_update: Callable[[Session], None] | None = None,
        **metadata: Any,
    ) -> Session:
        """Create a new session (task).

        Args:
            workspace: Working directory for the session
            task: Description of the task
            session_id: Optional ID (auto-generated if not provided)
            parent_id: Optional parent task ID for subtasks
            driver: Driver name (user, llm, workflow, etc.)
            driver_config: Driver-specific configuration
            event_bus: Optional event bus for emitting events
            on_update: Optional callback on session updates
            **metadata: Additional metadata
        """
        task_id = session_id or str(uuid.uuid4())[:8]
        return cls(
            id=task_id,
            workspace=workspace or Path.cwd(),
            task=task,
            parent_id=parent_id,
            driver=driver,
            driver_config=driver_config or {},
            shadow_branch=f"shadow/task-{task_id}",
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

    def start(self, create_shadow_branch: bool = True) -> None:
        """Mark session as running.

        Args:
            create_shadow_branch: If True, creates shadow/task-{id} branch
        """
        self.status = SessionStatus.RUNNING

        # Create shadow branch if not already set
        if create_shadow_branch and not self.shadow_branch:
            self._create_shadow_branch()

        self._emit(EventType.LOOP_STARTED, {"session_id": self.id, "task": self.task})
        self._notify_update()

    def _create_shadow_branch(self) -> bool:
        """Create a shadow branch for this task.

        Returns True if branch was created, False on error.
        """
        import subprocess

        branch_name = f"shadow/task-{self.id}"
        try:
            # Get current branch as base
            result = subprocess.run(
                ["git", "rev-parse", "--abbrev-ref", "HEAD"],
                cwd=self.workspace,
                capture_output=True,
                text=True,
                check=True,
            )
            base_branch = result.stdout.strip()

            # Create and checkout shadow branch
            subprocess.run(
                ["git", "checkout", "-b", branch_name],
                cwd=self.workspace,
                capture_output=True,
                check=True,
            )
            self.shadow_branch = branch_name
            self.metadata["base_branch"] = base_branch
            return True
        except subprocess.CalledProcessError:
            # Git not available or not a repo - continue without shadow branch
            return False

    def _checkout_shadow_branch(self) -> bool:
        """Checkout the shadow branch for this task.

        Returns True if checked out, False on error.
        """
        import subprocess

        if not self.shadow_branch:
            return False
        try:
            subprocess.run(
                ["git", "checkout", self.shadow_branch],
                cwd=self.workspace,
                capture_output=True,
                check=True,
            )
            return True
        except subprocess.CalledProcessError:
            return False

    def get_diff(self) -> str:
        """Get the diff for this task's shadow branch.

        Returns diff between base branch and shadow branch,
        or empty string if not available.
        """
        import subprocess

        base_branch = self.metadata.get("base_branch")
        if not self.shadow_branch or not base_branch:
            return ""
        try:
            result = subprocess.run(
                ["git", "diff", f"{base_branch}...{self.shadow_branch}"],
                cwd=self.workspace,
                capture_output=True,
                text=True,
                check=True,
            )
            return result.stdout
        except subprocess.CalledProcessError:
            return ""

    def pause(self, reason: str = "") -> None:
        """Pause the session."""
        self.status = SessionStatus.PAUSED
        self.metadata["pause_reason"] = reason
        self._emit(EventType.STEP_COMPLETED, {"session_id": self.id, "action": "paused"})
        self._notify_update()

    def resume(self, checkout_shadow_branch: bool = True) -> None:
        """Resume a paused session.

        Args:
            checkout_shadow_branch: If True, checks out the task's shadow branch
        """
        if self.status == SessionStatus.PAUSED:
            self.status = SessionStatus.RUNNING

            # Checkout shadow branch if it exists
            if checkout_shadow_branch and self.shadow_branch:
                self._checkout_shadow_branch()

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

    def add_child(self, child_id: str) -> None:
        """Add a child task ID."""
        if child_id not in self.children:
            self.children.append(child_id)
            self._notify_update()

    def record_tool_call(
        self,
        tool_name: str,
        parameters: dict[str, Any],
        result: Any = None,
        error: str | None = None,
        memory_bytes: int = 0,
        context_tokens: int = 0,
        memory_breakdown: dict[str, int] | None = None,
    ) -> ToolCall:
        """Record a tool call."""
        call = ToolCall(
            tool_name=tool_name,
            parameters=parameters,
            memory_bytes=memory_bytes,
            context_tokens=context_tokens,
            memory_breakdown=memory_breakdown or {},
        )
        call.complete(result=result, error=error)
        self.tool_calls.append(call)

        self._emit(
            EventType.TOOL_CALL,
            {
                "session_id": self.id,
                "tool_name": tool_name,
                "success": error is None,
                "duration_ms": call.duration_ms,
                "memory_bytes": memory_bytes,
                "context_tokens": context_tokens,
                "memory_breakdown": memory_breakdown or {},
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
            # Task hierarchy (unified model)
            "parent_id": self.parent_id,
            "children": self.children,
            "shadow_branch": self.shadow_branch,
            "driver": self.driver,
            "driver_config": self.driver_config,
            # Work records
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
            # Task hierarchy
            parent_id=data.get("parent_id"),
            children=data.get("children", []),
            shadow_branch=data.get("shadow_branch"),
            driver=data.get("driver", "user"),
            driver_config=data.get("driver_config", {}),
            # Work records
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
        parent_id: str | None = None,
        driver: str = "user",
        driver_config: dict[str, Any] | None = None,
        tags: list[str] | None = None,
        **metadata: Any,
    ) -> Session:
        """Create a new task (session).

        Args:
            task: Description of the task
            workspace: Working directory
            session_id: Optional ID (auto-generated if not provided)
            parent_id: Optional parent task ID for subtasks
            driver: Driver name (user, llm, workflow, etc.)
            driver_config: Driver-specific configuration
            tags: Optional tags
            **metadata: Additional metadata
        """
        session = Session.create(
            workspace=workspace,
            task=task,
            session_id=session_id,
            parent_id=parent_id,
            driver=driver,
            driver_config=driver_config,
            event_bus=self.event_bus,
            **metadata,
        )
        if tags:
            session.tags = tags
        self._active_sessions[session.id] = session

        # Update parent's children list
        if parent_id:
            parent = self.get(parent_id)
            if parent:
                parent.add_child(session.id)
                self.save(parent)

        return session

    def create_subtask(
        self,
        parent: Session,
        task: str,
        driver: str = "llm",
        driver_config: dict[str, Any] | None = None,
        **metadata: Any,
    ) -> Session:
        """Create a subtask under a parent task.

        Args:
            parent: Parent task
            task: Description of the subtask
            driver: Driver name (default: llm for automated subtasks)
            driver_config: Driver-specific configuration
            **metadata: Additional metadata
        """
        return self.create(
            task=task,
            workspace=parent.workspace,
            parent_id=parent.id,
            driver=driver,
            driver_config=driver_config,
            **metadata,
        )

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

    def list_root_tasks(
        self,
        status: SessionStatus | None = None,
        limit: int = 50,
    ) -> list[Session]:
        """List root tasks (tasks with no parent)."""
        all_sessions = self.list_sessions(status=status, limit=limit * 2)
        return [s for s in all_sessions if s.parent_id is None][:limit]

    def get_children(self, parent_id: str) -> list[Session]:
        """Get all child tasks of a parent."""
        parent = self.get(parent_id)
        if not parent:
            return []
        return [s for s in (self.get(cid) for cid in parent.children) if s is not None]

    def get_task_tree(self, root_id: str) -> dict[str, Any]:
        """Get a task and all its descendants as a tree structure.

        Returns:
            Dict with 'task' (Session) and 'children' (list of subtrees)
        """
        root = self.get(root_id)
        if not root:
            return {}

        def build_tree(task: Session) -> dict[str, Any]:
            children = self.get_children(task.id)
            return {
                "task": task,
                "children": [build_tree(c) for c in children],
            }

        return build_tree(root)

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


# Type aliases for unified task model
# Session and Task are the same thing - use whichever name fits context
Task = Session
TaskManager = SessionManager
TaskStatus = SessionStatus
