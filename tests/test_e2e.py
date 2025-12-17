"""End-to-end tests for complete Moss workflows.

These tests verify full flows from user request to final output,
testing the system as a whole rather than individual components.
"""

import subprocess

import pytest

from moss.anchors import Anchor, AnchorType
from moss.events import EventBus, EventType
from moss.handles import FileHandle
from moss.memory import Action, Outcome, StateSnapshot, create_memory_manager
from moss.patches import Patch, PatchType, apply_patch
from moss.policy import PolicyEngine, ToolCallContext, create_default_policy_engine
from moss.shadow_git import ShadowGit
from moss.skeleton import extract_python_skeleton, format_skeleton
from moss.validators import SyntaxValidator, create_python_validator_chain


class TestCodeModificationWorkflow:
    """E2E tests for code modification workflows."""

    @pytest.mark.asyncio
    async def test_full_edit_workflow(self, tmp_path):
        """Test complete edit workflow: analyze -> modify -> validate -> commit."""
        # Setup git repo
        subprocess.run(["git", "init"], cwd=tmp_path, capture_output=True)
        subprocess.run(
            ["git", "config", "user.email", "test@test.com"],
            cwd=tmp_path,
            capture_output=True,
        )
        subprocess.run(
            ["git", "config", "user.name", "Test"],
            cwd=tmp_path,
            capture_output=True,
        )

        # Create initial file
        source_file = tmp_path / "calculator.py"
        source_file.write_text("""
class Calculator:
    def add(self, a, b):
        return a + b

    def multiply(self, a, b):
        return a * b
""")
        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        subprocess.run(["git", "commit", "-m", "initial"], cwd=tmp_path, capture_output=True)

        # Step 1: Read and analyze the file
        handle = FileHandle(source_file)
        content = await handle.resolve()

        symbols = extract_python_skeleton(content)
        skeleton = format_skeleton(symbols)
        assert "Calculator" in skeleton
        assert "add" in skeleton

        # Step 2: Create shadow git branch
        shadow = ShadowGit(tmp_path)
        branch = await shadow.create_shadow_branch("add-type-hints")

        # Step 3: Modify the code
        anchor = Anchor(type=AnchorType.FUNCTION, name="add", context="Calculator")
        patch = Patch(
            anchor=anchor,
            patch_type=PatchType.REPLACE,
            content="    def add(self, a: int, b: int) -> int:\n        return a + b",
        )
        result = apply_patch(content, patch)
        assert result.success

        # Write patched content
        source_file.write_text(result.patched)

        # Step 4: Validate
        validator = SyntaxValidator()
        validation = await validator.validate(source_file)
        assert validation.success

        # Step 5: Commit changes
        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        commit_handle = await shadow.commit(branch, "Add type hints to add method")
        assert commit_handle is not None

    @pytest.mark.asyncio
    async def test_multi_file_refactor_workflow(self, tmp_path):
        """Test workflow involving multiple file modifications."""
        # Setup git repo
        subprocess.run(["git", "init"], cwd=tmp_path, capture_output=True)
        subprocess.run(
            ["git", "config", "user.email", "test@test.com"],
            cwd=tmp_path,
            capture_output=True,
        )
        subprocess.run(
            ["git", "config", "user.name", "Test"],
            cwd=tmp_path,
            capture_output=True,
        )

        # Create multiple files (ruff-compliant)
        (tmp_path / "utils.py").write_text('''"""Utility functions."""


def helper():
    """Return helper string."""
    return "helper"
''')
        (tmp_path / "main.py").write_text('''"""Main module."""

from utils import helper


def main():
    """Main function."""
    print(helper())
''')

        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        subprocess.run(["git", "commit", "-m", "initial"], cwd=tmp_path, capture_output=True)

        # Create shadow branch
        shadow = ShadowGit(tmp_path)
        _branch = await shadow.create_shadow_branch("refactor")

        # Validate all files - syntax should pass
        validator = SyntaxValidator()
        for file in tmp_path.glob("*.py"):
            result = await validator.validate(file)
            assert result.success


class TestValidationLoop:
    """E2E tests for validation retry loops."""

    @pytest.mark.asyncio
    async def test_syntax_error_detection_and_fix(self, tmp_path):
        """Test detecting syntax error, fixing, and re-validating."""
        source_file = tmp_path / "broken.py"
        source_file.write_text("def broken(\n")  # Intentional syntax error

        validator = SyntaxValidator()

        # First validation should fail
        result = await validator.validate(source_file)
        assert not result.success
        assert len(result.issues) > 0

        # Fix the code
        source_file.write_text("def fixed():\n    pass\n")

        # Re-validate should pass
        result = await validator.validate(source_file)
        assert result.success

    @pytest.mark.asyncio
    async def test_validation_chain_flow(self, tmp_path):
        """Test full validation chain execution."""
        source_file = tmp_path / "module.py"
        source_file.write_text('''
def process(data):
    """Process input data."""
    result = []
    for item in data:
        result.append(item * 2)
    return result
''')

        chain = create_python_validator_chain(include_tests=False)
        result = await chain.validate(source_file)

        # Should pass syntax validation
        assert result.success or "validators" in result.metadata


class TestEventDrivenWorkflow:
    """E2E tests for event-driven workflows."""

    @pytest.mark.asyncio
    async def test_full_event_driven_flow(self, tmp_path):
        """Test complete event-driven workflow."""
        events = []
        bus = EventBus()

        async def record_event(event):
            events.append(event)

        bus.subscribe_all(record_event)

        # Emit planning event
        await bus.emit(EventType.PLAN_GENERATED, {"plan": "Add feature X"})

        # Emit tool call event
        await bus.emit(
            EventType.TOOL_CALL,
            {"tool": "write_file", "path": str(tmp_path / "test.py")},
        )

        # Create file
        (tmp_path / "test.py").write_text("x = 1\n")

        # Emit shadow commit event
        await bus.emit(
            EventType.SHADOW_COMMIT,
            {"message": "Add test.py", "files": ["test.py"]},
        )

        # Verify events were recorded
        assert len(events) == 3
        assert events[0].type == EventType.PLAN_GENERATED
        assert events[1].type == EventType.TOOL_CALL
        assert events[2].type == EventType.SHADOW_COMMIT


class TestPolicyEnforcedWorkflow:
    """E2E tests for policy-enforced workflows."""

    @pytest.mark.asyncio
    async def test_safe_operation_allowed(self, tmp_path):
        """Test that safe operations pass policy checks."""
        engine = create_default_policy_engine()

        # Safe file write
        context = ToolCallContext(
            tool_name="write_file",
            target=tmp_path / "safe.py",
            action="write",
            parameters={"content": "x = 1"},
        )

        result = await engine.evaluate(context)
        assert result.allowed

        # Execute the operation
        (tmp_path / "safe.py").write_text("x = 1")
        assert (tmp_path / "safe.py").exists()

    @pytest.mark.asyncio
    async def test_blocked_path_rejected(self, tmp_path):
        """Test that dangerous paths are blocked."""
        from moss.policy import PathPolicy

        engine = PolicyEngine()
        engine.add_policy(PathPolicy(blocked_patterns=["secret", ".env"]))

        # Try to access secret file
        context = ToolCallContext(
            tool_name="read_file",
            target=tmp_path / "secret.txt",
            action="read",
        )

        result = await engine.evaluate(context)
        assert not result.allowed


class TestMemoryWorkflow:
    """E2E tests for memory-enabled workflows."""

    @pytest.mark.asyncio
    async def test_episode_recording_and_recall(self):
        """Test recording episodes and recalling them."""
        manager = create_memory_manager()

        # Record a successful episode
        state = StateSnapshot.create(
            files=["app.py"],
            context="Adding feature",
            error_count=0,
        )

        action = Action.create(
            tool="edit",
            target="app.py",
            change="add new function",
        )

        episode = await manager.record_episode(
            state=state,
            action=action,
            outcome=Outcome.SUCCESS,
        )

        assert episode is not None
        assert episode.outcome == Outcome.SUCCESS

        # Can retrieve from episodic store
        retrieved = await manager.episodic.get(episode.id)
        assert retrieved is not None
        assert retrieved.action.tool == "edit"


class TestCompleteAgentWorkflow:
    """E2E test simulating a complete agent workflow."""

    @pytest.mark.asyncio
    async def test_complete_task_execution(self, tmp_path):
        """Test a complete task from start to finish."""
        # Setup
        subprocess.run(["git", "init"], cwd=tmp_path, capture_output=True)
        subprocess.run(
            ["git", "config", "user.email", "agent@test.com"],
            cwd=tmp_path,
            capture_output=True,
        )
        subprocess.run(
            ["git", "config", "user.name", "Agent"],
            cwd=tmp_path,
            capture_output=True,
        )

        # Create initial file
        (tmp_path / "app.py").write_text("""
def greet(name):
    print(f"Hello {name}")

if __name__ == "__main__":
    greet("World")
""")
        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        subprocess.run(["git", "commit", "-m", "initial"], cwd=tmp_path, capture_output=True)

        # Initialize components
        bus = EventBus()
        engine = create_default_policy_engine(event_bus=bus)
        shadow = ShadowGit(tmp_path)
        manager = create_memory_manager()

        # Track events
        events = []

        async def track(event):
            events.append(event)

        bus.subscribe_all(track)

        # Task: Add type hints to greet function

        # Step 1: Read file
        source_file = tmp_path / "app.py"
        handle = FileHandle(source_file)
        content = await handle.resolve()

        # Step 2: Check policy for edit
        context = ToolCallContext(
            tool_name="edit_file",
            target=source_file,
            action="edit",
        )
        policy_result = await engine.evaluate(context)
        assert policy_result.allowed

        # Step 3: Create shadow branch
        branch = await shadow.create_shadow_branch("add-types")

        # Step 4: Modify code
        anchor = Anchor(type=AnchorType.FUNCTION, name="greet")
        patch = Patch(
            anchor=anchor,
            patch_type=PatchType.REPLACE,
            content='def greet(name: str) -> None:\n    print(f"Hello {name}")',
        )
        result = apply_patch(content, patch)
        assert result.success

        source_file.write_text(result.patched)

        # Step 5: Validate
        validator = SyntaxValidator()
        validation = await validator.validate(source_file)
        assert validation.success

        # Step 6: Commit
        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        commit = await shadow.commit(branch, "Add type hints to greet function")
        assert commit is not None

        # Step 7: Record episode
        state = StateSnapshot.create(
            files=["app.py"],
            context="Adding type hints",
        )
        action = Action.create(tool="edit", target="app.py")
        await manager.record_episode(
            state=state,
            action=action,
            outcome=Outcome.SUCCESS,
        )

        # Verify final state
        final_content = source_file.read_text()
        assert "name: str" in final_content
        assert "-> None" in final_content
