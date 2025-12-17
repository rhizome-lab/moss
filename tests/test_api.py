"""Tests for API Surface."""

import asyncio
from pathlib import Path

import pytest

from moss.api import (
    APIHandler,
    CheckpointRequest,
    CheckpointResponse,
    EventStreamManager,
    RequestStatus,
    RequestTracker,
    SSEEvent,
    SSEEventType,
    TaskRequest,
    TaskResponse,
    create_api_handler,
)
from moss.events import EventBus
from moss.shadow_git import ShadowGit


@pytest.fixture
async def git_repo(tmp_path: Path):
    """Create a temporary git repository."""
    repo = tmp_path / "repo"
    repo.mkdir()

    proc = await asyncio.create_subprocess_exec(
        "git",
        "init",
        cwd=repo,
        stdout=asyncio.subprocess.DEVNULL,
        stderr=asyncio.subprocess.DEVNULL,
    )
    await proc.wait()

    proc = await asyncio.create_subprocess_exec(
        "git", "config", "user.email", "test@test.com", cwd=repo
    )
    await proc.wait()
    proc = await asyncio.create_subprocess_exec("git", "config", "user.name", "Test User", cwd=repo)
    await proc.wait()

    (repo / "README.md").write_text("# Test")
    proc = await asyncio.create_subprocess_exec("git", "add", "-A", cwd=repo)
    await proc.wait()
    proc = await asyncio.create_subprocess_exec("git", "commit", "-m", "Initial", cwd=repo)
    await proc.wait()

    return repo


@pytest.fixture
def shadow_git(git_repo: Path):
    return ShadowGit(git_repo)


@pytest.fixture
def event_bus():
    return EventBus()


class TestTaskRequest:
    """Tests for TaskRequest."""

    def test_create_request(self):
        request = TaskRequest(
            task="Implement feature X",
            priority="high",
            metadata={"deadline": "2024-01-01"},
        )

        assert request.task == "Implement feature X"
        assert request.priority == "high"
        assert request.metadata["deadline"] == "2024-01-01"

    def test_default_values(self):
        request = TaskRequest(task="Simple task")

        assert request.priority == "normal"
        assert request.handles == []
        assert request.constraints == []


class TestTaskResponse:
    """Tests for TaskResponse."""

    def test_to_dict(self):
        response = TaskResponse(
            request_id="abc123",
            status=RequestStatus.PENDING,
            ticket_id="ticket1",
            message="Created",
        )

        data = response.to_dict()

        assert data["request_id"] == "abc123"
        assert data["status"] == "PENDING"
        assert data["ticket_id"] == "ticket1"
        assert data["message"] == "Created"
        assert "created_at" in data


class TestCheckpointRequest:
    """Tests for CheckpointRequest."""

    def test_to_dict(self):
        checkpoint = CheckpointRequest(
            checkpoint_id="cp1",
            request_id="req1",
            checkpoint_type="plan",
            description="Approve the plan?",
            options=["approve", "reject", "modify"],
        )

        data = checkpoint.to_dict()

        assert data["checkpoint_id"] == "cp1"
        assert data["checkpoint_type"] == "plan"
        assert len(data["options"]) == 3


class TestSSEEvent:
    """Tests for SSEEvent."""

    def test_to_sse_format(self):
        event = SSEEvent(
            event_type=SSEEventType.TASK_CREATED,
            data={"task_id": "123"},
            id="event1",
        )

        sse = event.to_sse()

        assert "id: event1" in sse
        assert "event: task_created" in sse
        assert "data:" in sse
        assert '"task_id"' in sse

    def test_to_sse_with_retry(self):
        event = SSEEvent(
            event_type=SSEEventType.ERROR,
            data={"error": "Connection lost"},
            retry=5000,
        )

        sse = event.to_sse()

        assert "retry: 5000" in sse


class TestRequestTracker:
    """Tests for RequestTracker."""

    @pytest.fixture
    def tracker(self):
        return RequestTracker()

    def test_create_request(self, tracker: RequestTracker):
        request_id = tracker.create_request("Test task")

        assert len(request_id) == 8
        req = tracker.get_request(request_id)
        assert req is not None
        assert req["status"] == RequestStatus.PENDING

    def test_update_request(self, tracker: RequestTracker):
        request_id = tracker.create_request("Test task")

        tracker.update_request(
            request_id,
            status=RequestStatus.PROCESSING,
            ticket_id="ticket123",
        )

        req = tracker.get_request(request_id)
        assert req["status"] == RequestStatus.PROCESSING
        assert req["ticket_id"] == "ticket123"

    def test_create_checkpoint(self, tracker: RequestTracker):
        request_id = tracker.create_request("Test task")

        checkpoint = tracker.create_checkpoint(
            request_id=request_id,
            checkpoint_type="plan",
            description="Approve plan?",
        )

        assert checkpoint.checkpoint_id is not None
        assert checkpoint.request_id == request_id

    def test_get_pending_checkpoints(self, tracker: RequestTracker):
        request_id = tracker.create_request("Test task")

        tracker.create_checkpoint(request_id, "plan", "Checkpoint 1")
        tracker.create_checkpoint(request_id, "merge", "Checkpoint 2")

        pending = tracker.get_pending_checkpoints()

        assert len(pending) == 2

    def test_resolve_checkpoint(self, tracker: RequestTracker):
        request_id = tracker.create_request("Test task")
        checkpoint = tracker.create_checkpoint(request_id, "plan", "Approve?")

        response = CheckpointResponse(
            checkpoint_id=checkpoint.checkpoint_id,
            decision="approve",
        )

        success = tracker.resolve_checkpoint(response)

        assert success
        # Check it's no longer pending
        pending = tracker.get_pending_checkpoints()
        assert len(pending) == 0

    async def test_wait_for_checkpoint(self, tracker: RequestTracker):
        request_id = tracker.create_request("Test task")
        checkpoint = tracker.create_checkpoint(request_id, "plan", "Approve?")

        # Resolve in background
        async def resolve_later():
            await asyncio.sleep(0.01)
            tracker.resolve_checkpoint(
                CheckpointResponse(
                    checkpoint_id=checkpoint.checkpoint_id,
                    decision="approve",
                )
            )

        _task = asyncio.create_task(resolve_later())

        response = await tracker.wait_for_checkpoint(checkpoint.checkpoint_id, timeout=1.0)

        assert response is not None
        assert response.decision == "approve"
        assert _task.done()

    async def test_wait_for_checkpoint_timeout(self, tracker: RequestTracker):
        request_id = tracker.create_request("Test task")
        checkpoint = tracker.create_checkpoint(request_id, "plan", "Approve?")

        response = await tracker.wait_for_checkpoint(checkpoint.checkpoint_id, timeout=0.01)

        assert response is None


class TestEventStreamManager:
    """Tests for EventStreamManager."""

    @pytest.fixture
    def manager(self, event_bus: EventBus):
        return EventStreamManager(event_bus)

    def test_create_stream(self, manager: EventStreamManager):
        stream_id = manager.create_stream()

        assert len(stream_id) == 8

    def test_close_stream(self, manager: EventStreamManager):
        stream_id = manager.create_stream()

        assert manager.close_stream(stream_id)
        assert not manager.close_stream(stream_id)  # Already closed

    def test_send_event_to_all_streams(self, manager: EventStreamManager):
        stream1 = manager.create_stream()
        stream2 = manager.create_stream()

        manager.send_event(
            SSEEventType.TASK_CREATED,
            {"task_id": "123"},
        )

        # Events should be in both queues
        assert not manager._streams[stream1].empty()
        assert not manager._streams[stream2].empty()

    def test_send_event_to_specific_stream(self, manager: EventStreamManager):
        stream1 = manager.create_stream()
        stream2 = manager.create_stream()

        manager.send_event(
            SSEEventType.TASK_CREATED,
            {"task_id": "123"},
            stream_id=stream1,
        )

        # Event should only be in stream1
        assert not manager._streams[stream1].empty()
        assert manager._streams[stream2].empty()


class TestAPIHandler:
    """Tests for APIHandler."""

    @pytest.fixture
    def handler(self, shadow_git: ShadowGit, event_bus: EventBus):
        from moss.agents import create_manager

        manager = create_manager(shadow_git, event_bus)
        return create_api_handler(manager, event_bus)

    async def test_create_task(self, handler: APIHandler):
        request = TaskRequest(
            task="Implement feature",
            priority="high",
        )

        response = await handler.create_task(request)

        assert response.request_id is not None
        assert response.status == RequestStatus.PENDING
        assert response.ticket_id is not None

    async def test_get_task_status(self, handler: APIHandler):
        request = TaskRequest(task="Test task")
        create_response = await handler.create_task(request)

        status = await handler.get_task_status(create_response.request_id)

        assert status is not None
        assert status.request_id == create_response.request_id
        assert status.ticket is not None

    async def test_get_task_status_not_found(self, handler: APIHandler):
        status = await handler.get_task_status("nonexistent")

        assert status is None

    async def test_cancel_task(self, handler: APIHandler):
        request = TaskRequest(task="Test task")
        create_response = await handler.create_task(request)

        cancelled = await handler.cancel_task(create_response.request_id)

        assert cancelled
        status = await handler.get_task_status(create_response.request_id)
        assert status is not None
        assert status.status == RequestStatus.CANCELLED

    async def test_cancel_task_not_found(self, handler: APIHandler):
        cancelled = await handler.cancel_task("nonexistent")

        assert not cancelled

    async def test_create_and_resolve_checkpoint(self, handler: APIHandler):
        request = TaskRequest(task="Test task")
        create_response = await handler.create_task(request)

        checkpoint = await handler.create_checkpoint(
            request_id=create_response.request_id,
            checkpoint_type="plan",
            description="Approve the plan?",
        )

        assert checkpoint.checkpoint_id is not None

        # Resolve it
        response = CheckpointResponse(
            checkpoint_id=checkpoint.checkpoint_id,
            decision="approve",
        )
        resolved = await handler.resolve_checkpoint(response)

        assert resolved

    async def test_get_checkpoints(self, handler: APIHandler):
        request = TaskRequest(task="Test task")
        create_response = await handler.create_task(request)

        await handler.create_checkpoint(create_response.request_id, "plan", "Checkpoint 1")
        await handler.create_checkpoint(create_response.request_id, "merge", "Checkpoint 2")

        checkpoints = await handler.get_checkpoints()

        assert len(checkpoints) == 2

    def test_create_and_close_event_stream(self, handler: APIHandler):
        stream_id = handler.create_event_stream()

        assert len(stream_id) == 8
        assert handler.close_event_stream(stream_id)

    def test_get_stats(self, handler: APIHandler):
        stats = handler.get_stats()

        assert "active_requests" in stats
        assert "pending_checkpoints" in stats
        assert "active_streams" in stats
        assert "manager_stats" in stats


class TestCreateAPIHandler:
    """Tests for create_api_handler."""

    def test_creates_handler(self, shadow_git: ShadowGit, event_bus: EventBus):
        from moss.agents import create_manager

        manager = create_manager(shadow_git, event_bus)
        handler = create_api_handler(manager, event_bus)

        assert handler is not None
        assert handler.manager is manager
        assert handler.event_bus is event_bus
