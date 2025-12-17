"""Tests for Silent Loop."""

import asyncio
from pathlib import Path

import pytest

from moss.anchors import Anchor, AnchorType
from moss.events import EventBus
from moss.loop import (
    LoopConfig,
    LoopStatus,
    SilentLoop,
    VelocityMetrics,
)
from moss.patches import Patch, PatchType
from moss.shadow_git import ShadowGit
from moss.validators import SyntaxValidator, ValidatorChain


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


class TestVelocityMetrics:
    """Tests for VelocityMetrics."""

    def test_initial_state(self):
        metrics = VelocityMetrics()
        assert metrics.iterations == 0
        assert metrics.errors_fixed == 0
        assert not metrics.is_stalled
        assert not metrics.is_oscillating

    def test_record_improvement(self):
        metrics = VelocityMetrics()
        metrics.total_errors = 5

        metrics.record_iteration(3)  # Fixed 2 errors

        assert metrics.errors_fixed == 2
        assert metrics.total_errors == 3
        assert metrics.stall_count == 0

    def test_record_regression(self):
        metrics = VelocityMetrics()
        metrics.total_errors = 3

        metrics.record_iteration(5)  # Introduced 2 errors

        assert metrics.errors_introduced == 2
        assert metrics.total_errors == 5

    def test_stall_detection(self):
        metrics = VelocityMetrics()
        metrics.total_errors = 5

        metrics.record_iteration(5)  # No change
        assert not metrics.is_stalled

        metrics.record_iteration(5)  # No change
        assert not metrics.is_stalled

        metrics.record_iteration(5)  # No change - stalled
        assert metrics.is_stalled

    def test_oscillation_detection(self):
        metrics = VelocityMetrics()
        metrics.total_errors = 5

        metrics.record_iteration(3)  # Down
        metrics.record_iteration(5)  # Up
        metrics.record_iteration(3)  # Down
        metrics.record_iteration(5)  # Up - oscillating

        assert metrics.oscillation_count >= 1

    def test_progress_ratio(self):
        metrics = VelocityMetrics()
        metrics.errors_fixed = 8
        metrics.errors_introduced = 2

        assert metrics.progress_ratio == 0.8


class TestSilentLoop:
    """Tests for SilentLoop."""

    @pytest.fixture
    def shadow_git(self, git_repo: Path):
        return ShadowGit(git_repo)

    @pytest.fixture
    def validators(self):
        return ValidatorChain([SyntaxValidator()])

    @pytest.fixture
    def loop(self, shadow_git: ShadowGit, validators: ValidatorChain):
        return SilentLoop(shadow_git, validators)

    async def test_success_on_valid_patch(
        self, loop: SilentLoop, shadow_git: ShadowGit, git_repo: Path
    ):
        # Create a file with valid Python
        test_file = git_repo / "test.py"
        test_file.write_text("x = 1")

        branch = await shadow_git.create_shadow_branch("test")

        # Apply a valid patch
        patch = Patch(
            anchor=Anchor(type=AnchorType.VARIABLE, name="x"),
            patch_type=PatchType.REPLACE,
            content="x = 2",
        )

        result = await loop.run(branch, test_file, [patch])

        assert result.success
        assert result.status == LoopStatus.SUCCESS
        assert len(result.iterations) >= 1

    async def test_patch_rejected_keeps_valid(
        self, loop: SilentLoop, shadow_git: ShadowGit, git_repo: Path
    ):
        test_file = git_repo / "test.py"
        test_file.write_text("x = 1")

        branch = await shadow_git.create_shadow_branch("test")

        # Apply an invalid patch - should be rejected by apply_patch
        patch = Patch(
            anchor=Anchor(type=AnchorType.VARIABLE, name="x"),
            patch_type=PatchType.REPLACE,
            content="x = ",  # Invalid syntax
        )

        result = await loop.run(branch, test_file, [patch])

        # Patch is rejected, file stays valid, so validation passes
        assert result.success
        assert result.iterations[0].patch_applied is False

    async def test_event_emission(
        self, shadow_git: ShadowGit, validators: ValidatorChain, git_repo: Path
    ):
        bus = EventBus()
        events_received: list = []

        async def handler(event):
            events_received.append(event)

        bus.subscribe_all(handler)

        loop = SilentLoop(shadow_git, validators, event_bus=bus)

        test_file = git_repo / "test.py"
        test_file.write_text("x = 1")

        branch = await shadow_git.create_shadow_branch("test")
        patch = Patch(
            anchor=Anchor(type=AnchorType.VARIABLE, name="x"),
            patch_type=PatchType.REPLACE,
            content="x = 2",
        )

        await loop.run(branch, test_file, [patch])

        # Should have emitted some events
        assert len(events_received) >= 1

    async def test_max_iterations(
        self, shadow_git: ShadowGit, validators: ValidatorChain, git_repo: Path
    ):
        config = LoopConfig(max_iterations=2)
        loop = SilentLoop(shadow_git, validators, config=config)

        test_file = git_repo / "test.py"
        test_file.write_text("x = ")  # Invalid

        branch = await shadow_git.create_shadow_branch("test")

        result = await loop.run(branch, test_file, [])

        assert result.status == LoopStatus.FAILED
        assert len(result.iterations) <= 2

    async def test_run_single(self, loop: SilentLoop, shadow_git: ShadowGit, git_repo: Path):
        test_file = git_repo / "test.py"
        test_file.write_text("x = 1")

        branch = await shadow_git.create_shadow_branch("test")

        patch = Patch(
            anchor=Anchor(type=AnchorType.VARIABLE, name="x"),
            patch_type=PatchType.REPLACE,
            content="x = 2",
        )

        result = await loop.run_single(branch, test_file, patch)

        assert result.success


class TestLoopConfig:
    """Tests for LoopConfig."""

    def test_default_config(self):
        config = LoopConfig()
        assert config.max_iterations == 10
        assert config.stall_threshold == 3
        assert config.auto_commit is True

    def test_custom_config(self):
        config = LoopConfig(max_iterations=5, auto_commit=False)
        assert config.max_iterations == 5
        assert config.auto_commit is False
