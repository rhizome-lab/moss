"""Multi-Agent: Ticket-based agent orchestration."""

from __future__ import annotations

import asyncio
import uuid
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum, auto
from typing import Any

from moss.events import EventBus, EventEmitterMixin, EventType
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


class Worker(EventEmitterMixin, ABC):
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

            self._state.status = WorkerStatus.COMPLETED if result.success else WorkerStatus.FAILED
            ticket.status = TicketStatus.COMPLETED if result.success else TicketStatus.FAILED
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


class Manager(EventEmitterMixin):
    """Manages workers and coordinates ticket processing.

    The Manager acts as the orchestrator, delegating tasks to workers
    and handling merge conflicts.

    Supports both synchronous (delegate) and asynchronous (spawn_async)
    execution patterns.
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
        # Background tasks for fire-and-forget execution
        self._background_tasks: dict[str, asyncio.Task[TicketResult]] = {}
        self._callbacks: dict[str, list] = {}  # ticket_id -> [callbacks]

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
        """Delegate multiple tickets to workers in parallel (waits for all)."""
        tasks = []
        for ticket in tickets:
            worker = worker_factory()
            self._workers[worker.id] = worker
            tasks.append(self.delegate(ticket, worker))

        return await asyncio.gather(*tasks)

    def spawn_async(
        self,
        ticket: Ticket,
        worker: Worker,
        on_complete: Any | None = None,  # Callable[[TicketResult], None]
    ) -> str:
        """Spawn an agent in the background without waiting.

        Returns the ticket ID for later result retrieval.
        Fire-and-forget execution - agent runs independently.
        """
        self._workers[worker.id] = worker

        async def run_and_callback() -> TicketResult:
            result = await worker.run(ticket)
            self._results[ticket.id] = result

            # Run callbacks
            for callback in self._callbacks.get(ticket.id, []):
                try:
                    callback(result)
                except Exception:
                    pass  # Don't let callback errors break the flow

            await self._emit(
                EventType.TOOL_CALL,
                {
                    "action": "async_worker_complete",
                    "ticket_id": ticket.id,
                    "worker_id": worker.id,
                    "success": result.success,
                },
            )
            return result

        task = asyncio.create_task(run_and_callback())
        self._background_tasks[ticket.id] = task

        if on_complete:
            self._callbacks.setdefault(ticket.id, []).append(on_complete)

        return ticket.id

    def spawn_many_async(
        self,
        tickets: list[Ticket],
        worker_factory: Any,  # Callable[[], Worker]
        on_complete: Any | None = None,  # Callable[[TicketResult], None]
    ) -> list[str]:
        """Spawn multiple agents in background without waiting.

        Returns list of ticket IDs for later result retrieval.
        """
        ticket_ids = []
        for ticket in tickets:
            worker = worker_factory()
            ticket_id = self.spawn_async(ticket, worker, on_complete)
            ticket_ids.append(ticket_id)
        return ticket_ids

    def get_result(self, ticket_id: str) -> TicketResult | None:
        """Get result for a completed ticket (non-blocking)."""
        return self._results.get(ticket_id)

    def is_running(self, ticket_id: str) -> bool:
        """Check if a background task is still running."""
        task = self._background_tasks.get(ticket_id)
        if task is None:
            return False
        return not task.done()

    async def wait_for(self, ticket_id: str, timeout: float | None = None) -> TicketResult | None:
        """Wait for a specific background task to complete."""
        task = self._background_tasks.get(ticket_id)
        if task is None:
            return self._results.get(ticket_id)

        try:
            return await asyncio.wait_for(task, timeout=timeout)
        except TimeoutError:
            return None

    async def wait_any(
        self,
        ticket_ids: list[str] | None = None,
        timeout: float | None = None,
    ) -> tuple[str, TicketResult] | None:
        """Wait for any of the specified tasks to complete.

        Returns (ticket_id, result) of first completed task.
        If ticket_ids is None, waits for any running task.
        """
        if ticket_ids is None:
            tasks_to_wait = dict(self._background_tasks)
        else:
            tasks_to_wait = {
                tid: self._background_tasks[tid]
                for tid in ticket_ids
                if tid in self._background_tasks
            }

        if not tasks_to_wait:
            return None

        # Reverse lookup: task -> ticket_id
        task_to_id = {v: k for k, v in tasks_to_wait.items()}

        try:
            done, _ = await asyncio.wait(
                tasks_to_wait.values(),
                timeout=timeout,
                return_when=asyncio.FIRST_COMPLETED,
            )
            if done:
                task = next(iter(done))
                ticket_id = task_to_id[task]
                return (ticket_id, task.result())
        except TimeoutError:
            pass

        return None

    async def wait_all(
        self,
        ticket_ids: list[str] | None = None,
        timeout: float | None = None,
    ) -> dict[str, TicketResult]:
        """Wait for all specified tasks to complete.

        If ticket_ids is None, waits for all running tasks.
        """
        if ticket_ids is None:
            tasks_to_wait = dict(self._background_tasks)
        else:
            tasks_to_wait = {
                tid: self._background_tasks[tid]
                for tid in ticket_ids
                if tid in self._background_tasks
            }

        if not tasks_to_wait:
            return {}

        try:
            await asyncio.wait(
                tasks_to_wait.values(),
                timeout=timeout,
                return_when=asyncio.ALL_COMPLETED,
            )
        except TimeoutError:
            pass

        # Return whatever completed
        results = {}
        for ticket_id, task in tasks_to_wait.items():
            if task.done() and not task.cancelled():
                try:
                    results[ticket_id] = task.result()
                except Exception:
                    pass
        return results

    def cancel(self, ticket_id: str) -> bool:
        """Cancel a running background task."""
        task = self._background_tasks.get(ticket_id)
        if task is None or task.done():
            return False

        task.cancel()
        return True

    def running_count(self) -> int:
        """Count of currently running background tasks."""
        return sum(1 for t in self._background_tasks.values() if not t.done())

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


# =============================================================================
# Swarm Coordination Patterns
# =============================================================================


class SwarmPattern(Enum):
    """Common swarm coordination patterns."""

    FORK_JOIN = auto()  # Split work, gather all results
    PIPELINE = auto()  # Chain workers, each processes previous output
    MAP_REDUCE = auto()  # Parallel map, then aggregate reduce
    VOTING = auto()  # Multiple workers same task, pick best
    RACE = auto()  # First completion wins
    SUPERVISED = auto()  # Monitor and restart failed workers


@dataclass
class SwarmResult:
    """Result of swarm execution."""

    pattern: SwarmPattern
    results: list[TicketResult]
    aggregated: Any = None  # Pattern-specific aggregated result
    duration_ms: int = 0
    success: bool = True
    error: str | None = None

    @property
    def successful_count(self) -> int:
        return sum(1 for r in self.results if r.success)

    @property
    def failed_count(self) -> int:
        return sum(1 for r in self.results if not r.success)


WorkerFactory = Any  # Callable[[], Worker]
ReducerFunc = Any  # Callable[[list[TicketResult]], Any]
VoterFunc = Any  # Callable[[list[TicketResult]], TicketResult]
TransformFunc = Any  # Callable[[TicketResult], Ticket]


class SwarmCoordinator(EventEmitterMixin):
    """Coordinator for common swarm patterns.

    Provides high-level APIs for:
    - fork_join: parallel execution, wait for all
    - pipeline: sequential chain with data flow
    - map_reduce: parallel map, aggregate reduce
    - voting: consensus from multiple workers
    - race: first completion wins
    """

    def __init__(
        self,
        manager: Manager,
        event_bus: EventBus | None = None,
    ):
        self.manager = manager
        self.event_bus = event_bus or manager.event_bus

    async def fork_join(
        self,
        tickets: list[Ticket],
        worker_factory: WorkerFactory,
        timeout: float | None = None,
    ) -> SwarmResult:
        """Execute tickets in parallel, wait for all to complete.

        Classic fork-join pattern:
        1. Fork: spawn N workers for N tickets
        2. Execute: all work in parallel
        3. Join: gather all results

        Args:
            tickets: Tasks to execute in parallel
            worker_factory: Creates workers for each ticket
            timeout: Max wait time in seconds

        Returns:
            SwarmResult with all individual results
        """
        start = datetime.now(UTC)

        await self._emit(
            EventType.TOOL_CALL,
            {"action": "swarm_fork_join_start", "ticket_count": len(tickets)},
        )

        results = await self.manager.delegate_parallel(tickets, worker_factory)

        duration = int((datetime.now(UTC) - start).total_seconds() * 1000)

        await self._emit(
            EventType.TOOL_CALL,
            {
                "action": "swarm_fork_join_complete",
                "success_count": sum(1 for r in results if r.success),
                "duration_ms": duration,
            },
        )

        return SwarmResult(
            pattern=SwarmPattern.FORK_JOIN,
            results=list(results),
            duration_ms=duration,
            success=all(r.success for r in results),
        )

    async def pipeline(
        self,
        initial_ticket: Ticket,
        stages: list[tuple[WorkerFactory, TransformFunc]],
        stop_on_failure: bool = True,
    ) -> SwarmResult:
        """Execute workers in sequence, each processing previous output.

        Pipeline pattern:
        1. Worker A processes initial ticket
        2. Transform A's result into ticket for worker B
        3. Worker B processes, transform for C, etc.

        Args:
            initial_ticket: Starting ticket
            stages: List of (worker_factory, transform_func) pairs
            stop_on_failure: If True, stop pipeline on first failure

        Returns:
            SwarmResult with results from each stage
        """
        start = datetime.now(UTC)
        results: list[TicketResult] = []
        current_ticket = initial_ticket

        await self._emit(
            EventType.TOOL_CALL,
            {"action": "swarm_pipeline_start", "stage_count": len(stages)},
        )

        for i, (worker_factory, transform) in enumerate(stages):
            worker = worker_factory()
            result = await self.manager.delegate(current_ticket, worker)
            results.append(result)

            if not result.success and stop_on_failure:
                await self._emit(
                    EventType.TOOL_CALL,
                    {"action": "swarm_pipeline_failed", "stage": i, "error": result.error},
                )
                break

            # Transform result for next stage (if not last)
            if i < len(stages) - 1:
                current_ticket = transform(result)

        duration = int((datetime.now(UTC) - start).total_seconds() * 1000)

        await self._emit(
            EventType.TOOL_CALL,
            {
                "action": "swarm_pipeline_complete",
                "stages_completed": len(results),
                "duration_ms": duration,
            },
        )

        return SwarmResult(
            pattern=SwarmPattern.PIPELINE,
            results=results,
            duration_ms=duration,
            success=all(r.success for r in results),
        )

    async def map_reduce(
        self,
        tickets: list[Ticket],
        worker_factory: WorkerFactory,
        reducer: ReducerFunc,
        timeout: float | None = None,
    ) -> SwarmResult:
        """Map work across workers, then reduce results.

        MapReduce pattern:
        1. Map: execute tickets in parallel
        2. Reduce: aggregate results with reducer function

        Args:
            tickets: Tasks to map
            worker_factory: Creates workers for each ticket
            reducer: Aggregates results into final value
            timeout: Max wait time

        Returns:
            SwarmResult with aggregated field set
        """
        start = datetime.now(UTC)

        await self._emit(
            EventType.TOOL_CALL,
            {"action": "swarm_map_reduce_start", "map_count": len(tickets)},
        )

        # Map phase
        results = await self.manager.delegate_parallel(tickets, worker_factory)

        # Reduce phase
        aggregated = reducer(list(results))

        duration = int((datetime.now(UTC) - start).total_seconds() * 1000)

        await self._emit(
            EventType.TOOL_CALL,
            {"action": "swarm_map_reduce_complete", "duration_ms": duration},
        )

        return SwarmResult(
            pattern=SwarmPattern.MAP_REDUCE,
            results=list(results),
            aggregated=aggregated,
            duration_ms=duration,
            success=all(r.success for r in results),
        )

    async def voting(
        self,
        ticket: Ticket,
        worker_factory: WorkerFactory,
        voter_count: int = 3,
        voter: VoterFunc | None = None,
    ) -> SwarmResult:
        """Run same task on multiple workers, pick best result.

        Voting/consensus pattern:
        1. Spawn N workers for same ticket
        2. All execute in parallel
        3. Voter function picks best result (default: majority success)

        Args:
            ticket: Task for all workers
            worker_factory: Creates worker instances
            voter_count: Number of workers to spawn
            voter: Picks best result (default: first successful)

        Returns:
            SwarmResult with aggregated set to winning result
        """
        start = datetime.now(UTC)

        await self._emit(
            EventType.TOOL_CALL,
            {"action": "swarm_voting_start", "voter_count": voter_count},
        )

        # Create copies of ticket for each worker
        tickets = [
            Ticket.create(
                task=ticket.task,
                handles=ticket.handles,
                constraints=ticket.constraints,
                priority=ticket.priority,
                parent_id=ticket.id,
            )
            for _ in range(voter_count)
        ]

        results = await self.manager.delegate_parallel(tickets, worker_factory)

        # Default voter: first successful result
        if voter is None:
            winner = next((r for r in results if r.success), results[0])
        else:
            winner = voter(list(results))

        duration = int((datetime.now(UTC) - start).total_seconds() * 1000)

        await self._emit(
            EventType.TOOL_CALL,
            {
                "action": "swarm_voting_complete",
                "winner_success": winner.success,
                "duration_ms": duration,
            },
        )

        return SwarmResult(
            pattern=SwarmPattern.VOTING,
            results=list(results),
            aggregated=winner,
            duration_ms=duration,
            success=winner is not None,
        )

    async def diffusion_refactor(
        self,
        contracts: list[str],
        worker_factory: WorkerFactory,
    ) -> SwarmResult:
        """Parallel implementation of multiple components based on contracts.

        Diffusion-like pattern:
        1. Fork: spawn implementation workers for each contract in parallel
        2. Join: gather all independent implementations

        Args:
            contracts: List of component contracts/specifications
            worker_factory: Creates implementation workers

        Returns:
            SwarmResult with all implementations
        """
        from moss.agents import Ticket

        tickets = [Ticket(task=f"Implement component with contract: {c}") for c in contracts]

        return await self.fork_join(tickets, worker_factory)

    async def race(
        self,
        tickets: list[Ticket],
        worker_factory: WorkerFactory,
        cancel_losers: bool = True,
    ) -> SwarmResult:
        """First worker to complete wins.

        Race pattern:
        1. Spawn workers for all tickets
        2. Return as soon as any completes
        3. Optionally cancel remaining workers

        Args:
            tickets: Tasks to race
            worker_factory: Creates workers
            cancel_losers: Cancel workers that didn't win

        Returns:
            SwarmResult with winner as first result
        """
        start = datetime.now(UTC)

        await self._emit(
            EventType.TOOL_CALL,
            {"action": "swarm_race_start", "racer_count": len(tickets)},
        )

        # Spawn all in background
        ticket_ids = self.manager.spawn_many_async(tickets, worker_factory)

        # Wait for first to complete
        winner_pair = await self.manager.wait_any(ticket_ids)

        # Cancel losers if requested
        if cancel_losers and winner_pair:
            winner_id, _winner_result = winner_pair
            for tid in ticket_ids:
                if tid != winner_id:
                    self.manager.cancel(tid)

        duration = int((datetime.now(UTC) - start).total_seconds() * 1000)

        results = [winner_pair[1]] if winner_pair else []

        await self._emit(
            EventType.TOOL_CALL,
            {
                "action": "swarm_race_complete",
                "winner_id": winner_pair[0] if winner_pair else None,
                "duration_ms": duration,
            },
        )

        return SwarmResult(
            pattern=SwarmPattern.RACE,
            results=results,
            aggregated=winner_pair[1] if winner_pair else None,
            duration_ms=duration,
            success=bool(results and results[0].success),
        )

    async def with_retry(
        self,
        ticket: Ticket,
        worker_factory: WorkerFactory,
        max_retries: int = 3,
        delay_ms: int = 1000,
    ) -> SwarmResult:
        """Retry failed tasks with exponential backoff.

        Args:
            ticket: Task to execute
            worker_factory: Creates workers
            max_retries: Max retry attempts
            delay_ms: Initial delay between retries (doubles each retry)

        Returns:
            SwarmResult with all attempt results
        """
        start = datetime.now(UTC)
        results: list[TicketResult] = []
        current_delay = delay_ms

        await self._emit(
            EventType.TOOL_CALL,
            {"action": "swarm_retry_start", "max_retries": max_retries},
        )

        for attempt in range(max_retries + 1):
            worker = worker_factory()
            result = await self.manager.delegate(ticket, worker)
            results.append(result)

            if result.success:
                break

            if attempt < max_retries:
                await asyncio.sleep(current_delay / 1000.0)
                current_delay *= 2  # Exponential backoff

        duration = int((datetime.now(UTC) - start).total_seconds() * 1000)
        final_success = results[-1].success if results else False

        await self._emit(
            EventType.TOOL_CALL,
            {
                "action": "swarm_retry_complete",
                "attempts": len(results),
                "success": final_success,
                "duration_ms": duration,
            },
        )

        return SwarmResult(
            pattern=SwarmPattern.SUPERVISED,
            results=results,
            aggregated=results[-1] if results else None,
            duration_ms=duration,
            success=final_success,
        )

    async def supervised(
        self,
        tickets: list[Ticket],
        worker_factory: WorkerFactory,
        max_failures: int = 3,
        restart_delay_ms: int = 500,
    ) -> SwarmResult:
        """Monitor workers and restart failed ones.

        Supervised pattern:
        1. Spawn workers in background
        2. Monitor for failures
        3. Restart failed workers up to max_failures total
        4. Return when all succeed or max failures reached

        Args:
            tickets: Tasks to supervise
            worker_factory: Creates workers
            max_failures: Max total failures before giving up
            restart_delay_ms: Delay before restarting failed worker

        Returns:
            SwarmResult with all attempt results
        """
        start = datetime.now(UTC)
        all_results: list[TicketResult] = []
        failure_count = 0

        await self._emit(
            EventType.TOOL_CALL,
            {"action": "swarm_supervised_start", "ticket_count": len(tickets)},
        )

        # Track pending tickets
        pending = list(tickets)
        completed: set[str] = set()

        while pending and failure_count < max_failures:
            # Spawn batch
            ticket_ids = self.manager.spawn_many_async(pending, worker_factory)
            pending.clear()

            # Wait for all to complete
            results = await self.manager.wait_all(ticket_ids)

            for tid, result in results.items():
                all_results.append(result)
                if result.success:
                    completed.add(tid)
                else:
                    failure_count += 1
                    if failure_count < max_failures:
                        # Retry this ticket
                        orig_ticket = self.manager.get_ticket(tid)
                        if orig_ticket:
                            pending.append(orig_ticket)
                            await asyncio.sleep(restart_delay_ms / 1000.0)

        duration = int((datetime.now(UTC) - start).total_seconds() * 1000)

        await self._emit(
            EventType.TOOL_CALL,
            {
                "action": "swarm_supervised_complete",
                "completed": len(completed),
                "failures": failure_count,
                "duration_ms": duration,
            },
        )

        error_msg = None
        if failure_count >= max_failures:
            error_msg = f"Max failures ({max_failures}) reached"

        return SwarmResult(
            pattern=SwarmPattern.SUPERVISED,
            results=all_results,
            duration_ms=duration,
            success=len(completed) == len(tickets),
            error=error_msg,
        )


def create_swarm_coordinator(
    manager: Manager,
    event_bus: EventBus | None = None,
) -> SwarmCoordinator:
    """Create a swarm coordinator with default settings."""
    return SwarmCoordinator(manager, event_bus)
