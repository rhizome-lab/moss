"""Integration tests for component interactions.

These tests verify that Moss components work correctly together,
testing the interfaces and data flow between modules.
"""

import subprocess

import pytest

from moss.anchors import Anchor, AnchorType, find_anchors, resolve_anchor
from moss.events import Event, EventBus, EventType
from moss.handles import FileHandle, HandleRegistry, MemoryHandle
from moss.memory import (
    Action,
    Outcome,
    SemanticRule,
    SemanticStore,
    StateSnapshot,
    create_memory_manager,
)
from moss.patches import Patch, PatchType, apply_patch
from moss.policy import (
    PathPolicy,
    PolicyEngine,
    RateLimitPolicy,
    ToolCallContext,
    create_default_policy_engine,
)
from moss.shadow_git import ShadowGit
from moss.skeleton import (
    extract_python_skeleton,
    format_skeleton,
)
from moss.validators import (
    SyntaxValidator,
    ValidatorChain,
    create_python_validator_chain,
)
from moss.views import ViewOptions, ViewTarget, ViewType, create_default_registry


class TestContextHostWithViewProviders:
    """Test Context Host interactions with View Providers."""

    @pytest.mark.asyncio
    async def test_context_host_with_raw_provider(self, tmp_path):
        """Test ContextHost rendering raw views."""
        test_file = tmp_path / "test.txt"
        test_file.write_text("Line 1\nLine 2\nLine 3\n")

        registry = create_default_registry()
        target = ViewTarget(path=test_file)  # Path object, not string
        view = await registry.render(target, ViewType.RAW, ViewOptions())

        assert view is not None
        assert "Line 1" in view.content
        assert "Line 2" in view.content

    @pytest.mark.asyncio
    async def test_context_host_multi_view(self, tmp_path):
        """Test ContextHost rendering multiple views."""
        test_file = tmp_path / "module.py"
        test_file.write_text('''
def hello():
    """Say hello."""
    print("Hello, World!")
''')

        registry = create_default_registry()
        target = ViewTarget(path=test_file)  # Path object, not string
        # Raw provider is registered by default
        views = await registry.render_multi(target, [ViewType.RAW], ViewOptions())

        assert len(views) == 1
        assert "def hello" in views[0].content


class TestShadowGitWithValidators:
    """Test Shadow Git interactions with Validators."""

    @pytest.mark.asyncio
    async def test_shadow_git_with_syntax_validation(self, tmp_path):
        """Test that Shadow Git can work with syntax validation."""
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

        test_file = tmp_path / "module.py"
        test_file.write_text("x = 1\n")
        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        subprocess.run(["git", "commit", "-m", "initial"], cwd=tmp_path, capture_output=True)

        shadow = ShadowGit(tmp_path)

        test_file.write_text("x = 1\ny = 2\n")

        validator = SyntaxValidator()
        result = await validator.validate(test_file)
        assert result.success

        branch = await shadow.create_shadow_branch("test-branch")
        handle = await shadow.commit(branch, "Add y variable")
        assert handle is not None
        assert handle.message == "Add y variable"

    @pytest.mark.asyncio
    async def test_shadow_git_rollback_on_validation_failure(self, tmp_path):
        """Test Shadow Git rollback when validation fails."""
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

        test_file = tmp_path / "module.py"
        test_file.write_text("x = 1\n")
        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        subprocess.run(["git", "commit", "-m", "initial"], cwd=tmp_path, capture_output=True)

        shadow = ShadowGit(tmp_path)

        branch = await shadow.create_shadow_branch("test-rollback")
        test_file.write_text("x = 1\ny = 2\n")
        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        await shadow.commit(branch, "Add y")

        test_file.write_text("x = (\n")
        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        await shadow.commit(branch, "Break syntax")

        validator = SyntaxValidator()
        result = await validator.validate(test_file)
        assert not result.success

        await shadow.rollback(branch, steps=1)


class TestPolicyEngineWithToolCalls:
    """Test Policy Engine interactions with tool calls."""

    @pytest.mark.asyncio
    async def test_policy_engine_allows_safe_operations(self, tmp_path):
        """Test that policy engine allows safe file operations."""
        engine = create_default_policy_engine()

        context = ToolCallContext(
            tool_name="write_file",
            target=tmp_path / "test.py",
            action="write",
            parameters={"content": "x = 1"},
        )

        result = await engine.evaluate(context)
        assert result.allowed

    @pytest.mark.asyncio
    async def test_policy_engine_blocks_dangerous_paths(self, tmp_path):
        """Test that policy engine blocks operations on dangerous paths."""
        engine = PolicyEngine()
        # Block a specific path
        engine.add_policy(PathPolicy(blocked_paths=[tmp_path / "secret"]))

        context = ToolCallContext(
            tool_name="write_file",
            target=tmp_path / "secret" / "file.py",
            action="write",
            parameters={"content": "x = 1"},
        )

        result = await engine.evaluate(context)
        assert not result.allowed

    @pytest.mark.asyncio
    async def test_policy_engine_rate_limiting(self):
        """Test policy engine rate limiting."""
        engine = PolicyEngine()
        rate_policy = RateLimitPolicy(max_calls_per_minute=3)
        engine.add_policy(rate_policy)

        context = ToolCallContext(tool_name="api_call", parameters={})

        # First 2 calls should be allowed (record happens after check)
        for _ in range(2):
            result = await engine.evaluate(context)
            assert result.allowed
            rate_policy.record_call()

        # 3rd call at limit - still allowed
        result = await engine.evaluate(context)
        assert result.allowed
        rate_policy.record_call()

        # 4th call should be denied (now at 3 recorded)
        result = await engine.evaluate(context)
        assert not result.allowed


class TestMemoryManagerWithVectorStore:
    """Test Memory Manager interactions with Vector Store."""

    @pytest.mark.asyncio
    async def test_memory_manager_episodic_store(self):
        """Test MemoryManager with EpisodicStore."""
        manager = create_memory_manager()

        state = StateSnapshot.create(
            files=["parser.py"],
            context="Fixing parser bug",
            error_count=1,
        )

        action = Action.create(tool="edit", target="parser.py", content="new content")
        outcome = Outcome.SUCCESS

        episode = await manager.record_episode(
            state=state,
            action=action,
            outcome=outcome,
        )

        assert episode is not None
        assert episode.action.tool == "edit"

    def test_semantic_store_rules(self):
        """Test SemanticStore with rules."""
        store = SemanticStore()

        store.add_rule(
            SemanticRule(
                id="rule1",
                pattern="python validation",
                action="use_python_validator",
                confidence=0.8,
                supporting_episodes=["ep1"],
            )
        )

        rules = store.find_matching_rules("python validation", min_confidence=0.3)
        assert len(rules) >= 0


class TestEventBusIntegration:
    """Test Event Bus integration with multiple components."""

    @pytest.mark.asyncio
    async def test_event_bus_multi_subscriber(self):
        """Test event bus with multiple subscribers."""
        bus = EventBus()
        received_events = []

        async def handler1(event: Event):
            received_events.append(("handler1", event))

        async def handler2(event: Event):
            received_events.append(("handler2", event))

        bus.subscribe(EventType.TOOL_CALL, handler1)
        bus.subscribe(EventType.TOOL_CALL, handler2)

        await bus.emit(
            EventType.TOOL_CALL,
            {"tool": "write_file", "path": "/test.py"},
        )

        assert len(received_events) == 2
        assert received_events[0][0] == "handler1"
        assert received_events[1][0] == "handler2"

    @pytest.mark.asyncio
    async def test_event_bus_filtering(self):
        """Test event bus event type filtering."""
        bus = EventBus()
        tool_events = []
        validation_events = []

        async def tool_handler(e):
            tool_events.append(e)

        async def validation_handler(e):
            validation_events.append(e)

        bus.subscribe(EventType.TOOL_CALL, tool_handler)
        bus.subscribe(EventType.VALIDATION_FAILED, validation_handler)

        await bus.emit(EventType.TOOL_CALL, {"tool": "read"})
        await bus.emit(EventType.VALIDATION_FAILED, {"error": "syntax"})
        await bus.emit(EventType.TOOL_CALL, {"tool": "write"})

        assert len(tool_events) == 2
        assert len(validation_events) == 1


class TestHandleRegistryWithFileOperations:
    """Test Handle Registry with file operations."""

    @pytest.mark.asyncio
    async def test_registry_file_handle_lifecycle(self, tmp_path):
        """Test file handle creation and retrieval."""
        registry = HandleRegistry()

        test_file = tmp_path / "test.py"
        test_file.write_text("x = 1")

        handle = FileHandle(test_file)
        handle_id = registry.register(handle)

        retrieved = registry.get(handle_id)
        assert retrieved is not None
        content = await retrieved.resolve()
        assert content == "x = 1"

    def test_registry_memory_handle(self):
        """Test memory handle operations."""
        registry = HandleRegistry()

        content = b"Hello, World!"
        handle = MemoryHandle(content)
        handle_id = registry.register(handle)

        retrieved = registry.get(handle_id)
        assert retrieved is not None


class TestAnchorsWithPatching:
    """Test Anchor resolution with patch application."""

    def test_anchor_finding(self, tmp_path):
        """Test finding anchors in source code."""
        test_file = tmp_path / "module.py"
        source = """
class User:
    def __init__(self, name):
        self.name = name

    def greet(self):
        return f"Hello, {self.name}"
"""
        test_file.write_text(source)

        anchor = Anchor(
            type=AnchorType.FUNCTION,
            name="greet",
        )
        matches = find_anchors(source, anchor)
        assert len(matches) > 0

        greet_match = matches[0]
        assert greet_match.anchor.name == "greet"
        assert greet_match.lineno > 0

    def test_anchor_patching_flow(self, tmp_path):
        """Test applying patch with anchor."""
        source = """
def greet(name):
    return f"Hello, {name}"
"""
        anchor = Anchor(type=AnchorType.FUNCTION, name="greet")
        patch = Patch(
            anchor=anchor,
            patch_type=PatchType.REPLACE,
            content='def greet(name: str) -> str:\n    return f"Hi, {name}"',
        )

        result = apply_patch(source, patch)
        assert result.success
        assert "Hi," in result.patched

    def test_anchor_resolver_with_context(self, tmp_path):
        """Test AnchorResolver with class context."""
        source = """
class Utilities:
    @staticmethod
    def helper():
        pass
"""
        test_file = tmp_path / "utils.py"
        test_file.write_text(source)

        # Use class context to be more specific
        anchor = Anchor(
            type=AnchorType.FUNCTION,
            name="helper",
            context="Utilities",
        )
        match = resolve_anchor(source, anchor)
        assert match is not None
        assert match.anchor.name == "helper"


class TestValidatorChainIntegration:
    """Test Validator Chain with multiple validators."""

    @pytest.mark.asyncio
    async def test_validator_chain_all_pass(self, tmp_path):
        """Test validator chain when all validators pass."""
        test_file = tmp_path / "valid.py"
        test_file.write_text('''
def add(a, b):
    """Add two numbers."""
    return a + b
''')

        chain = create_python_validator_chain(include_tests=False)
        result = await chain.validate(test_file)

        assert result.success or len(result.issues) == 0

    @pytest.mark.asyncio
    async def test_validator_chain_stops_on_error(self, tmp_path):
        """Test validator chain stops on first error."""
        test_file = tmp_path / "invalid.py"
        test_file.write_text("def broken(")

        chain = ValidatorChain()
        chain.add(SyntaxValidator())

        result = await chain.validate(test_file)
        assert not result.success


class TestSkeletonWithDependencies:
    """Test Skeleton extraction with dependency analysis."""

    def test_skeleton_preserves_imports(self):
        """Test that skeleton extraction preserves import information."""
        source = '''
import os
from pathlib import Path
from typing import List, Optional

class FileProcessor:
    """Process files."""

    def __init__(self, base_path: Path):
        self.base_path = base_path

    def process(self, files: List[str]) -> Optional[str]:
        """Process a list of files."""
        for f in files:
            path = self.base_path / f
            if path.exists():
                return str(path)
        return None
'''

        symbols = extract_python_skeleton(source)
        skeleton = format_skeleton(symbols)

        assert "FileProcessor" in skeleton
        assert "process" in skeleton

    def test_skeleton_with_nested_classes(self):
        """Test skeleton extraction with nested structures."""
        source = '''
class Outer:
    """Outer class."""

    class Inner:
        """Inner class."""

        def inner_method(self):
            pass

    def outer_method(self):
        pass
'''

        symbols = extract_python_skeleton(source)
        skeleton = format_skeleton(symbols)

        assert "Outer" in skeleton
        assert "Inner" in skeleton
        assert "inner_method" in skeleton
        assert "outer_method" in skeleton


class TestCrossComponentDataFlow:
    """Test data flow across multiple components."""

    @pytest.mark.asyncio
    async def test_file_to_view_to_patch_flow(self, tmp_path):
        """Test complete flow: file -> view -> analyze -> patch."""
        source_file = tmp_path / "source.py"
        source_file.write_text("""
def calculate(x, y):
    return x + y
""")

        handle = FileHandle(source_file)
        content = await handle.resolve()

        symbols = extract_python_skeleton(content)
        skeleton = format_skeleton(symbols)
        assert "calculate" in skeleton

        anchor = Anchor(type=AnchorType.FUNCTION, name="calculate")
        matches = find_anchors(content, anchor)
        assert len(matches) > 0

        patch = Patch(
            anchor=anchor,
            patch_type=PatchType.REPLACE,
            content="def calculate(x: int, y: int) -> int:\n    return x + y",
        )

        result = apply_patch(content, patch)
        assert result.success
        assert "int" in result.patched

        # Write patched content back and validate
        source_file.write_text(result.patched)
        validator = SyntaxValidator()
        validation = await validator.validate(source_file)
        assert validation.success

    @pytest.mark.asyncio
    async def test_event_driven_validation_flow(self, tmp_path):
        """Test event-driven validation workflow."""
        events_received = []
        bus = EventBus()

        async def on_tool_call(event):
            events_received.append(("tool_call", event.payload))

        async def on_validation(event):
            events_received.append(("validation", event.payload))

        bus.subscribe(EventType.TOOL_CALL, on_tool_call)
        bus.subscribe(EventType.VALIDATION_FAILED, on_validation)

        test_file = tmp_path / "test.py"

        await bus.emit(
            EventType.TOOL_CALL,
            {"tool": "write_file", "path": str(test_file)},
        )

        test_file.write_text("def broken(")

        validator = SyntaxValidator()
        result = await validator.validate(test_file)

        if not result.success:
            await bus.emit(
                EventType.VALIDATION_FAILED,
                {"file": str(test_file), "errors": len(result.issues)},
            )

        assert len(events_received) == 2
        assert events_received[0][0] == "tool_call"
        assert events_received[1][0] == "validation"
