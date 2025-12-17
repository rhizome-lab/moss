"""API Surface: Headless HTTP API types and handlers."""

from __future__ import annotations

import asyncio
import json
import uuid
from collections.abc import AsyncIterator
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum, auto
from typing import Any

from moss.agents import (
    Manager,
    TicketPriority,
)
from moss.events import Event, EventBus, EventType
from moss.handles import HandleRef


class RequestStatus(Enum):
    """Status of an API request."""

    PENDING = auto()
    PROCESSING = auto()
    WAITING_APPROVAL = auto()
    COMPLETED = auto()
    FAILED = auto()
    CANCELLED = auto()


@dataclass
class TaskRequest:
    """Request to create a new task."""

    task: str
    handles: list[dict[str, Any]] = field(default_factory=list)
    constraints: list[dict[str, str]] = field(default_factory=list)
    priority: str = "normal"
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class TaskResponse:
    """Response for a task request."""

    request_id: str
    status: RequestStatus
    ticket_id: str | None = None
    message: str | None = None
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))

    def to_dict(self) -> dict[str, Any]:
        return {
            "request_id": self.request_id,
            "status": self.status.name,
            "ticket_id": self.ticket_id,
            "message": self.message,
            "created_at": self.created_at.isoformat(),
        }


@dataclass
class TaskStatusResponse:
    """Response for task status query."""

    request_id: str
    status: RequestStatus
    ticket: dict[str, Any] | None = None
    result: dict[str, Any] | None = None
    progress: dict[str, Any] | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "request_id": self.request_id,
            "status": self.status.name,
            "ticket": self.ticket,
            "result": self.result,
            "progress": self.progress,
        }


@dataclass
class CheckpointRequest:
    """A checkpoint requiring user approval."""

    checkpoint_id: str
    request_id: str
    checkpoint_type: str  # "plan", "destructive_action", "merge"
    description: str
    options: list[str] = field(default_factory=lambda: ["approve", "reject"])
    data: dict[str, Any] = field(default_factory=dict)
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    expires_at: datetime | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "checkpoint_id": self.checkpoint_id,
            "request_id": self.request_id,
            "checkpoint_type": self.checkpoint_type,
            "description": self.description,
            "options": self.options,
            "data": self.data,
            "created_at": self.created_at.isoformat(),
            "expires_at": self.expires_at.isoformat() if self.expires_at else None,
        }


@dataclass
class CheckpointResponse:
    """Response to a checkpoint approval request."""

    checkpoint_id: str
    decision: str  # "approve", "reject", or custom option
    reason: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)


# Server-Sent Events (SSE) types


class SSEEventType(Enum):
    """Types of SSE events."""

    TASK_CREATED = "task_created"
    TASK_STARTED = "task_started"
    TASK_PROGRESS = "task_progress"
    TASK_COMPLETED = "task_completed"
    TASK_FAILED = "task_failed"
    CHECKPOINT_CREATED = "checkpoint_created"
    CHECKPOINT_RESOLVED = "checkpoint_resolved"
    VALIDATION_RESULT = "validation_result"
    COMMIT_CREATED = "commit_created"
    ERROR = "error"


@dataclass
class SSEEvent:
    """A Server-Sent Event."""

    event_type: SSEEventType
    data: dict[str, Any]
    id: str | None = None
    retry: int | None = None

    def to_sse(self) -> str:
        """Format as SSE wire format."""
        lines = []
        if self.id:
            lines.append(f"id: {self.id}")
        lines.append(f"event: {self.event_type.value}")
        lines.append(f"data: {json.dumps(self.data)}")
        if self.retry:
            lines.append(f"retry: {self.retry}")
        lines.append("")
        return "\n".join(lines) + "\n"


class RequestTracker:
    """Track active requests and their state."""

    def __init__(self):
        self._requests: dict[str, dict[str, Any]] = {}
        self._checkpoints: dict[str, CheckpointRequest] = {}
        self._checkpoint_responses: dict[str, asyncio.Future[CheckpointResponse]] = {}

    def create_request(self, task: str) -> str:
        """Create a new request and return its ID."""
        request_id = str(uuid.uuid4())[:8]
        self._requests[request_id] = {
            "status": RequestStatus.PENDING,
            "task": task,
            "ticket_id": None,
            "result": None,
            "created_at": datetime.now(UTC),
        }
        return request_id

    def get_request(self, request_id: str) -> dict[str, Any] | None:
        """Get request state by ID."""
        return self._requests.get(request_id)

    def update_request(
        self,
        request_id: str,
        status: RequestStatus | None = None,
        ticket_id: str | None = None,
        result: dict[str, Any] | None = None,
    ) -> None:
        """Update request state."""
        if request_id not in self._requests:
            return
        if status:
            self._requests[request_id]["status"] = status
        if ticket_id:
            self._requests[request_id]["ticket_id"] = ticket_id
        if result:
            self._requests[request_id]["result"] = result

    def create_checkpoint(
        self,
        request_id: str,
        checkpoint_type: str,
        description: str,
        options: list[str] | None = None,
        data: dict[str, Any] | None = None,
    ) -> CheckpointRequest:
        """Create a checkpoint requiring approval."""
        checkpoint = CheckpointRequest(
            checkpoint_id=str(uuid.uuid4())[:8],
            request_id=request_id,
            checkpoint_type=checkpoint_type,
            description=description,
            options=options or ["approve", "reject"],
            data=data or {},
        )
        self._checkpoints[checkpoint.checkpoint_id] = checkpoint
        self._checkpoint_responses[checkpoint.checkpoint_id] = asyncio.Future()
        return checkpoint

    def get_checkpoint(self, checkpoint_id: str) -> CheckpointRequest | None:
        """Get a checkpoint by ID."""
        return self._checkpoints.get(checkpoint_id)

    def get_pending_checkpoints(self, request_id: str | None = None) -> list[CheckpointRequest]:
        """Get pending checkpoints, optionally filtered by request."""
        checkpoints = []
        for cp in self._checkpoints.values():
            if cp.checkpoint_id in self._checkpoint_responses:
                future = self._checkpoint_responses[cp.checkpoint_id]
                if not future.done():
                    if request_id is None or cp.request_id == request_id:
                        checkpoints.append(cp)
        return checkpoints

    async def wait_for_checkpoint(
        self, checkpoint_id: str, timeout: float | None = None
    ) -> CheckpointResponse | None:
        """Wait for a checkpoint response."""
        future = self._checkpoint_responses.get(checkpoint_id)
        if not future:
            return None
        try:
            if timeout:
                return await asyncio.wait_for(future, timeout)
            return await future
        except TimeoutError:
            return None

    def resolve_checkpoint(self, response: CheckpointResponse) -> bool:
        """Resolve a checkpoint with a response."""
        future = self._checkpoint_responses.get(response.checkpoint_id)
        if not future or future.done():
            return False
        future.set_result(response)
        return True


class EventStreamManager:
    """Manage SSE event streams for clients."""

    def __init__(self, event_bus: EventBus):
        self.event_bus = event_bus
        self._streams: dict[str, asyncio.Queue[SSEEvent]] = {}
        self._subscribed = False

    async def _handle_event(self, event: Event) -> None:
        """Handle events from the event bus."""
        sse_type = self._map_event_type(event.event_type)
        if sse_type:
            sse_event = SSEEvent(
                event_type=sse_type,
                data=event.payload,
                id=str(uuid.uuid4())[:8],
            )
            for queue in self._streams.values():
                await queue.put(sse_event)

    def _map_event_type(self, event_type: EventType) -> SSEEventType | None:
        """Map internal event types to SSE event types."""
        mapping = {
            EventType.TOOL_CALL: SSEEventType.TASK_PROGRESS,
            EventType.VALIDATION_FAILED: SSEEventType.VALIDATION_RESULT,
            EventType.SHADOW_COMMIT: SSEEventType.COMMIT_CREATED,
        }
        return mapping.get(event_type)

    def _ensure_subscribed(self) -> None:
        """Ensure we're subscribed to the event bus."""
        if not self._subscribed:
            self.event_bus.subscribe_all(self._handle_event)
            self._subscribed = True

    def create_stream(self) -> str:
        """Create a new event stream and return its ID."""
        self._ensure_subscribed()
        stream_id = str(uuid.uuid4())[:8]
        self._streams[stream_id] = asyncio.Queue()
        return stream_id

    def close_stream(self, stream_id: str) -> bool:
        """Close an event stream."""
        return self._streams.pop(stream_id, None) is not None

    async def get_events(self, stream_id: str) -> AsyncIterator[SSEEvent]:
        """Async iterator for SSE events."""
        queue = self._streams.get(stream_id)
        if not queue:
            return

        while True:
            try:
                event = await queue.get()
                yield event
            except asyncio.CancelledError:
                break

    def send_event(
        self,
        event_type: SSEEventType,
        data: dict[str, Any],
        stream_id: str | None = None,
    ) -> None:
        """Send an event to streams (or a specific stream)."""
        event = SSEEvent(
            event_type=event_type,
            data=data,
            id=str(uuid.uuid4())[:8],
        )
        if stream_id:
            queue = self._streams.get(stream_id)
            if queue:
                queue.put_nowait(event)
        else:
            for queue in self._streams.values():
                queue.put_nowait(event)


class APIHandler:
    """Framework-agnostic API handler.

    This class provides the business logic for API endpoints.
    It can be mounted on any web framework (FastAPI, Flask, etc.).
    """

    def __init__(
        self,
        manager: Manager,
        event_bus: EventBus,
    ):
        self.manager = manager
        self.event_bus = event_bus
        self.tracker = RequestTracker()
        self.streams = EventStreamManager(event_bus)

    def _parse_priority(self, priority: str) -> TicketPriority:
        """Parse priority string to enum."""
        mapping = {
            "low": TicketPriority.LOW,
            "normal": TicketPriority.NORMAL,
            "high": TicketPriority.HIGH,
            "critical": TicketPriority.CRITICAL,
        }
        return mapping.get(priority.lower(), TicketPriority.NORMAL)

    async def create_task(self, request: TaskRequest) -> TaskResponse:
        """Handle task creation request."""
        # Create request tracking
        request_id = self.tracker.create_request(request.task)

        # Convert to ticket
        handles: list[HandleRef] = []  # Would need proper conversion

        ticket = self.manager.create_ticket(
            task=request.task,
            handles=handles,
            priority=self._parse_priority(request.priority),
            **request.metadata,
        )

        self.tracker.update_request(
            request_id,
            status=RequestStatus.PENDING,
            ticket_id=ticket.id,
        )

        # Send SSE event
        self.streams.send_event(
            SSEEventType.TASK_CREATED,
            {"request_id": request_id, "ticket_id": ticket.id},
        )

        return TaskResponse(
            request_id=request_id,
            status=RequestStatus.PENDING,
            ticket_id=ticket.id,
            message="Task created successfully",
        )

    async def get_task_status(self, request_id: str) -> TaskStatusResponse | None:
        """Get status of a task."""
        req = self.tracker.get_request(request_id)
        if not req:
            return None

        ticket_data = None
        if req["ticket_id"]:
            ticket = self.manager.get_ticket(req["ticket_id"])
            if ticket:
                ticket_data = {
                    "id": ticket.id,
                    "task": ticket.task,
                    "status": ticket.status.name,
                    "priority": ticket.priority.name,
                }

        return TaskStatusResponse(
            request_id=request_id,
            status=req["status"],
            ticket=ticket_data,
            result=req["result"],
        )

    async def cancel_task(self, request_id: str) -> bool:
        """Cancel a task."""
        req = self.tracker.get_request(request_id)
        if not req:
            return False

        if req["status"] in (RequestStatus.COMPLETED, RequestStatus.CANCELLED):
            return False

        self.tracker.update_request(request_id, status=RequestStatus.CANCELLED)

        self.streams.send_event(
            SSEEventType.TASK_FAILED,
            {"request_id": request_id, "reason": "cancelled"},
        )

        return True

    async def create_checkpoint(
        self,
        request_id: str,
        checkpoint_type: str,
        description: str,
        options: list[str] | None = None,
        data: dict[str, Any] | None = None,
    ) -> CheckpointRequest:
        """Create a checkpoint requiring approval."""
        checkpoint = self.tracker.create_checkpoint(
            request_id=request_id,
            checkpoint_type=checkpoint_type,
            description=description,
            options=options,
            data=data,
        )

        self.tracker.update_request(request_id, status=RequestStatus.WAITING_APPROVAL)

        self.streams.send_event(
            SSEEventType.CHECKPOINT_CREATED,
            checkpoint.to_dict(),
        )

        return checkpoint

    async def get_checkpoints(self, request_id: str | None = None) -> list[CheckpointRequest]:
        """Get pending checkpoints."""
        return self.tracker.get_pending_checkpoints(request_id)

    async def resolve_checkpoint(self, response: CheckpointResponse) -> bool:
        """Resolve a checkpoint with a decision."""
        success = self.tracker.resolve_checkpoint(response)

        if success:
            checkpoint = self.tracker.get_checkpoint(response.checkpoint_id)
            if checkpoint:
                self.tracker.update_request(
                    checkpoint.request_id,
                    status=RequestStatus.PROCESSING,
                )

            self.streams.send_event(
                SSEEventType.CHECKPOINT_RESOLVED,
                {
                    "checkpoint_id": response.checkpoint_id,
                    "decision": response.decision,
                },
            )

        return success

    async def wait_for_checkpoint(
        self, checkpoint_id: str, timeout: float | None = None
    ) -> CheckpointResponse | None:
        """Wait for a checkpoint to be resolved."""
        return await self.tracker.wait_for_checkpoint(checkpoint_id, timeout)

    def create_event_stream(self) -> str:
        """Create a new SSE event stream."""
        return self.streams.create_stream()

    def close_event_stream(self, stream_id: str) -> bool:
        """Close an SSE event stream."""
        return self.streams.close_stream(stream_id)

    async def get_events(self, stream_id: str) -> AsyncIterator[SSEEvent]:
        """Get events from a stream."""
        async for event in self.streams.get_events(stream_id):
            yield event

    def get_stats(self) -> dict[str, Any]:
        """Get API statistics."""
        return {
            "active_requests": len(self.tracker._requests),
            "pending_checkpoints": len(self.tracker.get_pending_checkpoints()),
            "active_streams": len(self.streams._streams),
            "manager_stats": self.manager.stats(),
        }


def create_api_handler(manager: Manager, event_bus: EventBus) -> APIHandler:
    """Create an API handler."""
    return APIHandler(manager, event_bus)
