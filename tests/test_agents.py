"""Tests for Multi-Agent system."""

import asyncio
from pathlib import Path

import pytest

from moss.agents import (
    Constraint,
    Manager,
    MergeStrategy,
    SimpleWorker,
    Ticket,
    TicketPriority,
    TicketResult,
    TicketStatus,
    Worker,
    WorkerStatus,
    create_manager,
)
from moss.handles import HandleRef
from moss.shadow_git import ShadowBranch, ShadowGit


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
    proc = await asyncio.create_subprocess_exec(
        "git", "config", "user.name", "Test User", cwd=repo
    )
    await proc.wait()

    (repo / "README.md").write_text("# Test")
    proc = await asyncio.create_subprocess_exec("git", "add", "-A", cwd=repo)
    await proc.wait()
    proc = await asyncio.create_subprocess_exec(
        "git", "commit", "-m", "Initial", cwd=repo
    )
    await proc.wait()

    return repo


@pytest.fixture
def shadow_git(git_repo: Path):
    return ShadowGit(git_repo)


class TestConstraint:
    """Tests for Constraint."""

    def test_create_constraint(self):
        constraint = Constraint(
            name="no-breaking-changes",
            description="Do not modify public API",
        )

        assert constraint.name == "no-breaking-changes"
        assert "public API" in constraint.description

    def test_constraint_to_prompt(self):
        constraint = Constraint(
            name="lint",
            description="Code must pass ruff check",
        )

        prompt = constraint.to_prompt()

        assert "lint" in prompt
        assert "ruff check" in prompt


class TestTicket:
    """Tests for Ticket."""

    def test_create_ticket(self):
        from uuid import uuid4

        ticket = Ticket.create(
            task="Refactor authentication module",
            handles=[HandleRef(handle_id=uuid4(), handle_type="file", location="src/auth.py")],
            constraints=[Constraint("test", "Must pass tests")],
            priority=TicketPriority.HIGH,
        )

        assert len(ticket.id) == 8
        assert ticket.task == "Refactor authentication module"
        assert len(ticket.handles) == 1
        assert len(ticket.constraints) == 1
        assert ticket.priority == TicketPriority.HIGH
        assert ticket.status == TicketStatus.PENDING

    def test_ticket_with_metadata(self):
        ticket = Ticket.create(
            task="Fix bug",
            deadline="2024-01-01",
            assignee="worker-1",
        )

        assert ticket.metadata["deadline"] == "2024-01-01"
        assert ticket.metadata["assignee"] == "worker-1"

    def test_ticket_to_prompt(self):
        from uuid import uuid4

        ticket = Ticket.create(
            task="Add logging",
            handles=[HandleRef(handle_id=uuid4(), handle_type="file", location="src/main.py")],
            constraints=[Constraint("format", "Use structured logging")],
            context="This is for debugging production issues",
        )

        prompt = ticket.to_prompt()

        assert "Add logging" in prompt
        assert "src/main.py" in prompt
        assert "structured logging" in prompt
        assert "debugging production" in prompt


class TestTicketResult:
    """Tests for TicketResult."""

    def test_success_result(self):
        result = TicketResult(
            ticket_id="abc123",
            success=True,
            summary="Successfully refactored auth module",
            duration_ms=1500,
        )

        assert result.success
        assert result.ticket_id == "abc123"
        assert result.duration_ms == 1500

    def test_failure_result(self):
        result = TicketResult(
            ticket_id="abc123",
            success=False,
            summary="Failed to complete task",
            error="Syntax error in generated code",
        )

        assert not result.success
        assert result.error is not None


class TestWorker:
    """Tests for Worker."""

    @pytest.fixture
    def worker(self, shadow_git: ShadowGit):
        async def executor(ticket: Ticket, branch: ShadowBranch) -> TicketResult:
            return TicketResult(
                ticket_id=ticket.id,
                success=True,
                summary=f"Completed: {ticket.task}",
            )

        return SimpleWorker(shadow_git, executor)

    async def test_worker_lifecycle(
        self, worker: SimpleWorker, shadow_git: ShadowGit
    ):
        ticket = Ticket.create(task="Test task")

        result = await worker.run(ticket)

        assert result.success
        assert worker.status == WorkerStatus.COMPLETED
        assert ticket.status == TicketStatus.COMPLETED

    async def test_worker_creates_branch(
        self, worker: SimpleWorker, shadow_git: ShadowGit
    ):
        ticket = Ticket.create(task="Test task")

        await worker.spawn(ticket)

        assert worker._state.branch is not None
        assert f"worker-{worker.id}" in worker._state.branch.name

    async def test_worker_failure(self, shadow_git: ShadowGit):
        async def failing_executor(
            ticket: Ticket, branch: ShadowBranch
        ) -> TicketResult:
            raise RuntimeError("Simulated failure")

        worker = SimpleWorker(shadow_git, failing_executor)
        ticket = Ticket.create(task="Failing task")

        result = await worker.run(ticket)

        assert not result.success
        assert worker.status == WorkerStatus.FAILED
        assert ticket.status == TicketStatus.FAILED
        assert "Simulated failure" in (result.error or "")

    async def test_worker_terminate(self, worker: SimpleWorker):
        await worker.terminate()

        assert worker.status == WorkerStatus.TERMINATED


class TestManager:
    """Tests for Manager."""

    @pytest.fixture
    def manager(self, shadow_git: ShadowGit):
        return create_manager(shadow_git)

    def test_create_ticket(self, manager: Manager):
        ticket = manager.create_ticket(
            task="Implement feature X",
            priority=TicketPriority.HIGH,
        )

        assert ticket.id in manager._tickets
        assert ticket.priority == TicketPriority.HIGH

    def test_get_pending_tickets(self, manager: Manager):
        # Create tickets with different priorities
        manager.create_ticket("Low priority", priority=TicketPriority.LOW)
        manager.create_ticket("Critical", priority=TicketPriority.CRITICAL)
        manager.create_ticket("Normal", priority=TicketPriority.NORMAL)

        pending = manager.get_pending_tickets()

        assert len(pending) == 3
        # Should be sorted by priority (highest first)
        assert pending[0].priority == TicketPriority.CRITICAL
        assert pending[2].priority == TicketPriority.LOW

    async def test_delegate_to_worker(
        self, manager: Manager, shadow_git: ShadowGit
    ):
        async def executor(ticket: Ticket, branch: ShadowBranch) -> TicketResult:
            return TicketResult(
                ticket_id=ticket.id,
                success=True,
                summary="Done",
            )

        ticket = manager.create_ticket("Test task")
        worker = SimpleWorker(shadow_git, executor)

        result = await manager.delegate(ticket, worker)

        assert result.success
        assert result.ticket_id == ticket.id
        assert ticket.status == TicketStatus.COMPLETED

    async def test_delegate_parallel(
        self, manager: Manager, shadow_git: ShadowGit
    ):
        async def executor(ticket: Ticket, branch: ShadowBranch) -> TicketResult:
            await asyncio.sleep(0.01)  # Simulate work
            return TicketResult(
                ticket_id=ticket.id,
                success=True,
                summary=f"Done: {ticket.task}",
            )

        tickets = [
            manager.create_ticket(f"Task {i}") for i in range(3)
        ]

        def worker_factory():
            return SimpleWorker(shadow_git, executor)

        results = await manager.delegate_parallel(tickets, worker_factory)

        assert len(results) == 3
        assert all(r.success for r in results)

    def test_stats(self, manager: Manager):
        manager.create_ticket("Task 1")
        manager.create_ticket("Task 2")

        stats = manager.stats()

        assert stats["total_tickets"] == 2
        assert stats["tickets_by_status"]["PENDING"] == 2


class TestMergeStrategy:
    """Tests for merge strategies."""

    def test_merge_strategy_values(self):
        assert MergeStrategy.SQUASH.value == 1
        assert MergeStrategy.REBASE.value == 2
        assert MergeStrategy.MERGE.value == 3
        assert MergeStrategy.FAST_FORWARD.value == 4


class TestCreateManager:
    """Tests for create_manager."""

    def test_creates_manager(self, shadow_git: ShadowGit):
        manager = create_manager(shadow_git)

        assert manager is not None
        assert manager.shadow_git is shadow_git
        assert manager.merge_strategy == MergeStrategy.SQUASH
