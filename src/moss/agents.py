"""Multi-Agent: Ticket-based agent orchestration."""

from __future__ import annotations

import asyncio
import uuid
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum, auto
from typing import Any

from moss.events import EventBus, EventType
from moss.handles import HandleRef
from moss.shadow_git import CommitHandle, ShadowBranch, ShadowGit


class TicketStatus(Enum):
    """Status of a ticket."""

    PENDING = auto()
    ASSIGNED = auto()
    IN_PROGRESS = auto()
    COMPLETED = auto()
    FAILED = auto()
    CANCELLED = auto()


class TicketPriority(Enum):
    """Priority levels for tickets."""

    LOW = 1
    NORMAL = 2
    HIGH = 3
    CRITICAL = 4


@dataclass
class Constraint:
    """A constraint that must be respected by the worker."""

    name: str
    description: str
    check: str | None = None  # Optional validation command

    def to_prompt(self) -> str:
        """Convert constraint to prompt text."""
        return f"- {self.name}: {self.description}"


@dataclass
class Ticket:
    """A task ticket for an agent.

    Tickets are the communication protocol between manager and workers.
    They contain all context needed to complete a task without sharing
    full chat history.
    """

    id: str
    task: str  # High-level objective
    handles: list[HandleRef]  # References to relevant files/artifacts
    constraints: list[Constraint] = field(default_factory=list)
    priority: TicketPriority = TicketPriority.NORMAL
    status: TicketStatus = TicketStatus.PENDING
    parent_id: str | None = None  # For sub-tickets
    metadata: dict[str, Any] = field(default_factory=dict)
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    assigned_at: datetime | None = None
    completed_at: datetime | None = None

    @classmethod
    def create(
        cls,
        task: str,
        handles: list[HandleRef] | None = None,
        constraints: list[Constraint] | None = None,
        priority: TicketPriority = TicketPriority.NORMAL,
        parent_id: str | None = None,
        **metadata: Any,
    ) -> Ticket:
        """Create a new ticket."""
        return cls(
            id=str(uuid.uuid4())[:8],
            task=task,
            handles=handles or [],
            constraints=constraints or [],
            priority=priority,
            parent_id=parent_id,
            metadata=metadata,
        )

    def to_prompt(self) -> str:
        """Convert ticket to prompt text for the worker."""
        parts = [f"# Task: {self.task}", ""]

        if self.handles:
            parts.append("## Relevant Files")
            for handle in self.handles:
                parts.append(f"- {handle.location}")
            parts.append("")

        if self.constraints:
            parts.append("## Constraints")
            for constraint in self.constraints:
                parts.append(constraint.to_prompt())
            parts.append("")

        if self.metadata:
            parts.append("## Additional Context")
            for key, value in self.metadata.items():
                parts.append(f"- {key}: {value}")
            parts.append("")

        return "\n".join(parts)


@dataclass
class TicketResult:
    """Result returned by a worker after completing a ticket."""

    ticket_id: str
    success: bool
    summary: str
    commit: CommitHandle | None = None
    artifacts: list[HandleRef] = field(default_factory=list)
    error: str | None = None
    duration_ms: int = 0
    metadata: dict[str, Any] = field(default_factory=dict)


class WorkerStatus(Enum):
    """Status of a worker."""

    IDLE = auto()
    WORKING = auto()
    COMPLETED = auto()
    FAILED = auto()
    TERMINATED = auto()


@dataclass
class WorkerState:
    """State of a worker agent."""

    id: str
    status: WorkerStatus = WorkerStatus.IDLE
    current_ticket: Ticket | None = None
    branch: ShadowBranch | None = None
    started_at: datetime | None = None
    completed_at: datetime | None = None
    results: list[TicketResult] = field(default_factory=list)


class Worker(ABC):
    """Abstract base for worker agents.

    Workers are isolated agents that process tickets without access to
    the full conversation history. They receive structured context via
    Handles and return structured results.
    """

    def __init__(
        self,
        shadow_git: ShadowGit,
        event_bus: EventBus | None = None,
    ):
        self.shadow_git = shadow_git
        self.event_bus = event_bus
        self._state = WorkerState(id=str(uuid.uuid4())[:8])

    @property
    def id(self) -> str:
        return self._state.id

    @property
    def status(self) -> WorkerStatus:
        return self._state.status

    @property
    def current_ticket(self) -> Ticket | None:
        return self._state.current_ticket

    async def _emit(self, event_type: EventType, payload: dict[str, Any]) -> None:
        """Emit an event if event bus is configured."""
        if self.event_bus:
            await self.event_bus.emit(event_type, payload)

    async def spawn(self, ticket: Ticket) -> None:
        """Initialize the worker with a ticket.

        Creates a fresh context (shadow branch) for isolated execution.
        """
        self._state.status = WorkerStatus.IDLE
        self._state.current_ticket = ticket
        self._state.started_at = datetime.now(UTC)

        # Create isolated branch for this worker
        branch_name = f"worker-{self.id}-{ticket.id}"
        self._state.branch = await self.shadow_git.create_shadow_branch(branch_name)

        # Mark ticket as assigned
        ticket.status = TicketStatus.ASSIGNED
        ticket.assigned_at = datetime.now(UTC)

        await self._emit(
            EventType.TOOL_CALL,
            {
                "action": "worker_spawn",
                "worker_id": self.id,
                "ticket_id": ticket.id,
                "branch": branch_name,
            },
        )

    @abstractmethod
    async def execute(self) -> TicketResult:
        """Execute the current ticket.

        Subclasses implement the actual work logic.
        """
        ...

    async def run(self, ticket: Ticket) -> TicketResult:
        """Full lifecycle: spawn → execute → die."""
        await self.spawn(ticket)

        try:
            self._state.status = WorkerStatus.WORKING
            ticket.status = TicketStatus.IN_PROGRESS

            start_time = datetime.now(UTC)
            result = await self.execute()
            end_time = datetime.now(UTC)

            result.duration_ms = int((end_time - start_time).total_seconds() * 1000)

            self._state.status = (
                WorkerStatus.COMPLETED if result.success else WorkerStatus.FAILED
            )
            ticket.status = (
                TicketStatus.COMPLETED if result.success else TicketStatus.FAILED
            )
            ticket.completed_at = end_time

            self._state.results.append(result)
            self._state.completed_at = end_time

            await self._emit(
                EventType.TOOL_CALL,
                {
                    "action": "worker_complete",
                    "worker_id": self.id,
                    "ticket_id": ticket.id,
                    "success": result.success,
                    "duration_ms": result.duration_ms,
                },
            )

            return result

        except Exception as e:
            self._state.status = WorkerStatus.FAILED
            ticket.status = TicketStatus.FAILED

            result = TicketResult(
                ticket_id=ticket.id,
                success=False,
                summary="Worker failed with exception",
                error=str(e),
            )
            self._state.results.append(result)

            await self._emit(
                EventType.TOOL_CALL,
                {
                    "action": "worker_error",
                    "worker_id": self.id,
                    "ticket_id": ticket.id,
                    "error": str(e),
                },
            )

            return result

    async def terminate(self) -> None:
        """Terminate the worker."""
        self._state.status = WorkerStatus.TERMINATED

        await self._emit(
            EventType.TOOL_CALL,
            {"action": "worker_terminate", "worker_id": self.id},
        )


class SimpleWorker(Worker):
    """A simple worker that executes a callback function."""

    def __init__(
        self,
        shadow_git: ShadowGit,
        executor: Any,  # Callable[[Ticket, ShadowBranch], TicketResult]
        event_bus: EventBus | None = None,
    ):
        super().__init__(shadow_git, event_bus)
        self._executor = executor

    async def execute(self) -> TicketResult:
        """Execute using the provided callback."""
        if not self._state.current_ticket or not self._state.branch:
            return TicketResult(
                ticket_id="unknown",
                success=False,
                summary="Worker not properly initialized",
                error="No ticket or branch available",
            )

        return await self._executor(self._state.current_ticket, self._state.branch)


class MergeStrategy(Enum):
    """Strategy for merging worker results."""

    SQUASH = auto()  # Squash all commits into one
    REBASE = auto()  # Rebase onto target
    MERGE = auto()  # Standard merge commit
    FAST_FORWARD = auto()  # Fast-forward if possible


@dataclass
class MergeResult:
    """Result of merging a worker's changes."""

    success: bool
    commit: CommitHandle | None = None
    conflicts: list[str] = field(default_factory=list)
    error: str | None = None


class Manager:
    """Manages workers and coordinates ticket processing.

    The Manager acts as the orchestrator, delegating tasks to workers
    and handling merge conflicts.
    """

    def __init__(
        self,
        shadow_git: ShadowGit,
        event_bus: EventBus | None = None,
        merge_strategy: MergeStrategy = MergeStrategy.SQUASH,
    ):
        self.shadow_git = shadow_git
        self.event_bus = event_bus
        self.merge_strategy = merge_strategy

        self._tickets: dict[str, Ticket] = {}
        self._workers: dict[str, Worker] = {}
        self._results: dict[str, TicketResult] = {}

    async def _emit(self, event_type: EventType, payload: dict[str, Any]) -> None:
        """Emit an event if event bus is configured."""
        if self.event_bus:
            await self.event_bus.emit(event_type, payload)

    def create_ticket(
        self,
        task: str,
        handles: list[HandleRef] | None = None,
        constraints: list[Constraint] | None = None,
        priority: TicketPriority = TicketPriority.NORMAL,
        **metadata: Any,
    ) -> Ticket:
        """Create and register a new ticket."""
        ticket = Ticket.create(
            task=task,
            handles=handles,
            constraints=constraints,
            priority=priority,
            **metadata,
        )
        self._tickets[ticket.id] = ticket
        return ticket

    def get_ticket(self, ticket_id: str) -> Ticket | None:
        """Get a ticket by ID."""
        return self._tickets.get(ticket_id)

    def get_pending_tickets(self) -> list[Ticket]:
        """Get all pending tickets, sorted by priority."""
        pending = [t for t in self._tickets.values() if t.status == TicketStatus.PENDING]
        return sorted(pending, key=lambda t: -t.priority.value)

    async def delegate(
        self,
        ticket: Ticket,
        worker: Worker,
    ) -> TicketResult:
        """Delegate a ticket to a worker and wait for completion."""
        self._workers[worker.id] = worker

        await self._emit(
            EventType.TOOL_CALL,
            {
                "action": "manager_delegate",
                "ticket_id": ticket.id,
                "worker_id": worker.id,
            },
        )

        result = await worker.run(ticket)
        self._results[ticket.id] = result

        return result

    async def delegate_parallel(
        self,
        tickets: list[Ticket],
        worker_factory: Any,  # Callable[[], Worker]
    ) -> list[TicketResult]:
        """Delegate multiple tickets to workers in parallel."""
        tasks = []
        for ticket in tickets:
            worker = worker_factory()
            self._workers[worker.id] = worker
            tasks.append(self.delegate(ticket, worker))

        return await asyncio.gather(*tasks)

    async def merge(
        self,
        result: TicketResult,
        target_branch: str = "main",
    ) -> MergeResult:
        """Merge a worker's changes into the target branch."""
        if not result.success:
            return MergeResult(
                success=False,
                error="Cannot merge failed ticket result",
            )

        if not result.commit:
            return MergeResult(
                success=False,
                error="No commit to merge",
            )

        ticket = self._tickets.get(result.ticket_id)
        if not ticket:
            return MergeResult(
                success=False,
                error=f"Ticket {result.ticket_id} not found",
            )

        worker = None
        for w in self._workers.values():
            if w.current_ticket and w.current_ticket.id == result.ticket_id:
                worker = w
                break

        if not worker or not worker._state.branch:
            return MergeResult(
                success=False,
                error="Worker or branch not found",
            )

        try:
            if self.merge_strategy == MergeStrategy.SQUASH:
                commit = await self.shadow_git.squash_merge(
                    worker._state.branch,
                    f"Merge ticket {ticket.id}: {ticket.task}",
                )
            else:
                # For other strategies, just use squash for now
                # Full implementation would support all strategies
                commit = await self.shadow_git.squash_merge(
                    worker._state.branch,
                    f"Merge ticket {ticket.id}: {ticket.task}",
                )

            await self._emit(
                EventType.SHADOW_COMMIT,
                {
                    "action": "manager_merge",
                    "ticket_id": ticket.id,
                    "commit": commit.sha,
                },
            )

            return MergeResult(success=True, commit=commit)

        except Exception as e:
            error_msg = str(e)
            conflicts = []

            # Try to detect conflicts
            if "conflict" in error_msg.lower():
                conflicts = ["Unknown conflict - manual resolution required"]

            return MergeResult(
                success=False,
                conflicts=conflicts,
                error=error_msg,
            )

    async def resolve_conflict(
        self,
        ticket_id: str,
        resolution: str,  # "ours", "theirs", or path to resolution
    ) -> MergeResult:
        """Resolve a merge conflict.

        This is a simplified implementation. A full implementation would
        support interactive conflict resolution.
        """
        ticket = self._tickets.get(ticket_id)
        if not ticket:
            return MergeResult(
                success=False,
                error=f"Ticket {ticket_id} not found",
            )

        # In a real implementation, this would:
        # 1. Checkout the conflicting files
        # 2. Apply the resolution strategy
        # 3. Stage and commit the resolution

        await self._emit(
            EventType.TOOL_CALL,
            {
                "action": "manager_resolve_conflict",
                "ticket_id": ticket_id,
                "resolution": resolution,
            },
        )

        return MergeResult(
            success=True,
            error="Conflict resolution not fully implemented",
        )

    def stats(self) -> dict[str, Any]:
        """Get statistics about ticket processing."""
        tickets_by_status = {}
        for ticket in self._tickets.values():
            status = ticket.status.name
            tickets_by_status[status] = tickets_by_status.get(status, 0) + 1

        successful = sum(1 for r in self._results.values() if r.success)
        failed = sum(1 for r in self._results.values() if not r.success)

        return {
            "total_tickets": len(self._tickets),
            "tickets_by_status": tickets_by_status,
            "active_workers": len(
                [w for w in self._workers.values() if w.status == WorkerStatus.WORKING]
            ),
            "completed_results": len(self._results),
            "successful": successful,
            "failed": failed,
        }


def create_manager(
    shadow_git: ShadowGit,
    event_bus: EventBus | None = None,
) -> Manager:
    """Create a manager with default settings."""
    return Manager(shadow_git, event_bus)
