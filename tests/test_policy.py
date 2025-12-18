"""Tests for Policy Engine."""

from pathlib import Path

import pytest

from moss.policy import (
    PathPolicy,
    Policy,
    PolicyDecision,
    PolicyEngine,
    PolicyResult,
    QuarantinePolicy,
    RateLimitPolicy,
    ToolCallContext,
    TrustPolicy,
    VelocityPolicy,
    create_default_policy_engine,
)
from moss.trust import Decision, TrustLevel, TrustManager, TrustRule


class TestPolicyResult:
    """Tests for PolicyResult."""

    def test_allow_is_allowed(self):
        result = PolicyResult(decision=PolicyDecision.ALLOW, policy_name="test")
        assert result.allowed

    def test_warn_is_allowed(self):
        result = PolicyResult(decision=PolicyDecision.WARN, policy_name="test")
        assert result.allowed

    def test_deny_is_not_allowed(self):
        result = PolicyResult(decision=PolicyDecision.DENY, policy_name="test")
        assert not result.allowed

    def test_quarantine_is_not_allowed(self):
        result = PolicyResult(decision=PolicyDecision.QUARANTINE, policy_name="test")
        assert not result.allowed


class TestVelocityPolicy:
    """Tests for VelocityPolicy."""

    @pytest.fixture
    def policy(self):
        return VelocityPolicy(stall_threshold=3, oscillation_threshold=2)

    async def test_allows_when_no_history(self, policy: VelocityPolicy):
        ctx = ToolCallContext(tool_name="edit")
        result = await policy.evaluate(ctx)
        assert result.allowed

    async def test_allows_progress(self, policy: VelocityPolicy):
        policy.record_error_count(5)
        policy.record_error_count(3)  # Progress!
        policy.record_error_count(1)  # More progress!

        ctx = ToolCallContext(tool_name="edit")
        result = await policy.evaluate(ctx)
        assert result.allowed

    async def test_denies_after_stall(self, policy: VelocityPolicy):
        policy.record_error_count(5)
        policy.record_error_count(5)  # Stall 1
        policy.record_error_count(5)  # Stall 2
        policy.record_error_count(5)  # Stall 3 - threshold

        ctx = ToolCallContext(tool_name="edit")
        result = await policy.evaluate(ctx)
        assert not result.allowed
        assert result.decision == PolicyDecision.DENY
        assert "Stalled" in (result.reason or "")

    async def test_denies_after_oscillation(self, policy: VelocityPolicy):
        policy.record_error_count(5)
        policy.record_error_count(3)  # Down
        policy.record_error_count(5)  # Up
        policy.record_error_count(3)  # Down - oscillation 1
        policy.record_error_count(5)  # Up
        policy.record_error_count(3)  # Down - oscillation 2

        ctx = ToolCallContext(tool_name="edit")
        result = await policy.evaluate(ctx)
        assert not result.allowed
        assert "Oscillating" in (result.reason or "")

    async def test_reset_clears_state(self, policy: VelocityPolicy):
        # Trigger stall
        for _ in range(4):
            policy.record_error_count(5)

        ctx = ToolCallContext(tool_name="edit")
        result = await policy.evaluate(ctx)
        assert not result.allowed

        # Reset
        policy.reset()

        result = await policy.evaluate(ctx)
        assert result.allowed


class TestQuarantinePolicy:
    """Tests for QuarantinePolicy."""

    @pytest.fixture
    def policy(self):
        return QuarantinePolicy(repair_tools={"repair", "fix_syntax"})

    async def test_allows_non_quarantined_files(self, policy: QuarantinePolicy, tmp_path: Path):
        target = tmp_path / "clean.py"
        ctx = ToolCallContext(tool_name="edit", target=target)

        result = await policy.evaluate(ctx)
        assert result.allowed

    async def test_quarantines_broken_files(self, policy: QuarantinePolicy, tmp_path: Path):
        target = tmp_path / "broken.py"
        policy.quarantine(target, "Syntax error at line 5")

        ctx = ToolCallContext(tool_name="edit", target=target)
        result = await policy.evaluate(ctx)

        assert not result.allowed
        assert result.decision == PolicyDecision.QUARANTINE
        assert "quarantined" in (result.reason or "").lower()

    async def test_allows_repair_tools(self, policy: QuarantinePolicy, tmp_path: Path):
        target = tmp_path / "broken.py"
        policy.quarantine(target, "Syntax error")

        ctx = ToolCallContext(tool_name="repair", target=target)
        result = await policy.evaluate(ctx)

        assert result.allowed
        assert result.decision == PolicyDecision.WARN

    async def test_release_from_quarantine(self, policy: QuarantinePolicy, tmp_path: Path):
        target = tmp_path / "fixed.py"
        policy.quarantine(target, "Was broken")

        assert policy.is_quarantined(target)
        assert policy.release(target)
        assert not policy.is_quarantined(target)

    async def test_quarantined_files_list(self, policy: QuarantinePolicy, tmp_path: Path):
        f1 = tmp_path / "a.py"
        f2 = tmp_path / "b.py"

        policy.quarantine(f1, "Error 1")
        policy.quarantine(f2, "Error 2")

        files = policy.quarantined_files
        assert len(files) == 2

    async def test_allows_no_target(self, policy: QuarantinePolicy):
        ctx = ToolCallContext(tool_name="shell")  # No target
        result = await policy.evaluate(ctx)
        assert result.allowed


class TestRateLimitPolicy:
    """Tests for RateLimitPolicy."""

    @pytest.fixture
    def policy(self):
        return RateLimitPolicy(max_calls_per_minute=5, max_calls_per_target=2)

    async def test_allows_within_limit(self, policy: RateLimitPolicy):
        ctx = ToolCallContext(tool_name="edit")
        result = await policy.evaluate(ctx)
        assert result.allowed

    async def test_denies_after_global_limit(self, policy: RateLimitPolicy):
        # Record max calls
        for _ in range(5):
            policy.record_call()

        ctx = ToolCallContext(tool_name="edit")
        result = await policy.evaluate(ctx)

        assert not result.allowed
        assert result.decision == PolicyDecision.DENY
        assert "Rate limit" in (result.reason or "")

    async def test_warns_after_target_limit(self, policy: RateLimitPolicy, tmp_path: Path):
        target = tmp_path / "busy.py"

        # Record max calls to this target
        for _ in range(2):
            policy.record_call(target)

        ctx = ToolCallContext(tool_name="edit", target=target)
        result = await policy.evaluate(ctx)

        assert result.allowed  # Warning, not denial
        assert result.decision == PolicyDecision.WARN
        assert "modified" in (result.reason or "").lower()


class TestPathPolicy:
    """Tests for PathPolicy."""

    @pytest.fixture
    def policy(self):
        return PathPolicy(blocked_patterns=[".git", ".env", "secrets"])

    async def test_allows_normal_paths(self, policy: PathPolicy, tmp_path: Path):
        target = tmp_path / "src" / "main.py"
        ctx = ToolCallContext(tool_name="edit", target=target)

        result = await policy.evaluate(ctx)
        assert result.allowed

    async def test_blocks_git_directory(self, policy: PathPolicy, tmp_path: Path):
        target = tmp_path / ".git" / "config"
        ctx = ToolCallContext(tool_name="edit", target=target)

        result = await policy.evaluate(ctx)
        assert not result.allowed
        assert ".git" in (result.reason or "")

    async def test_blocks_env_files(self, policy: PathPolicy, tmp_path: Path):
        target = tmp_path / ".env"
        ctx = ToolCallContext(tool_name="edit", target=target)

        result = await policy.evaluate(ctx)
        assert not result.allowed

    async def test_blocks_secrets(self, policy: PathPolicy, tmp_path: Path):
        target = tmp_path / "secrets" / "api_key.txt"
        ctx = ToolCallContext(tool_name="edit", target=target)

        result = await policy.evaluate(ctx)
        assert not result.allowed

    async def test_allows_no_target(self, policy: PathPolicy):
        ctx = ToolCallContext(tool_name="shell")
        result = await policy.evaluate(ctx)
        assert result.allowed


class TestPolicyEngine:
    """Tests for PolicyEngine."""

    @pytest.fixture
    def engine(self):
        return PolicyEngine(
            policies=[
                QuarantinePolicy(),
                VelocityPolicy(),
                PathPolicy(),
            ]
        )

    async def test_allows_when_all_pass(self, engine: PolicyEngine, tmp_path: Path):
        target = tmp_path / "clean.py"
        result = await engine.check("edit", target=target)

        assert result.allowed
        assert len(result.results) == 3  # All policies evaluated

    async def test_stops_on_first_deny(self, engine: PolicyEngine, tmp_path: Path):
        target = tmp_path / ".git" / "config"  # Blocked by PathPolicy
        result = await engine.check("edit", target=target)

        assert not result.allowed
        assert result.blocking_result is not None
        assert result.blocking_result.policy_name == "path"

    async def test_priority_order(self):
        class LowPriority(Policy):
            @property
            def name(self) -> str:
                return "low"

            @property
            def priority(self) -> int:
                return 1

            async def evaluate(self, context: ToolCallContext) -> PolicyResult:
                return PolicyResult(decision=PolicyDecision.DENY, policy_name="low")

        class HighPriority(Policy):
            @property
            def name(self) -> str:
                return "high"

            @property
            def priority(self) -> int:
                return 100

            async def evaluate(self, context: ToolCallContext) -> PolicyResult:
                return PolicyResult(decision=PolicyDecision.DENY, policy_name="high")

        # Add in wrong order
        engine = PolicyEngine(policies=[LowPriority(), HighPriority()])

        result = await engine.check("test")

        # High priority should block first
        assert result.blocking_result is not None
        assert result.blocking_result.policy_name == "high"

    async def test_collects_warnings(self, tmp_path: Path):
        # Create policies that warn
        quarantine = QuarantinePolicy(repair_tools={"repair"})
        target = tmp_path / "broken.py"
        quarantine.quarantine(target, "Broken")

        engine = PolicyEngine(policies=[quarantine])
        result = await engine.check("repair", target=target)

        assert result.allowed
        assert len(result.warnings) == 1

    def test_add_policy(self, engine: PolicyEngine):
        initial_count = len(engine.policies)
        engine.add_policy(RateLimitPolicy())
        assert len(engine.policies) == initial_count + 1

    def test_remove_policy(self, engine: PolicyEngine):
        initial_count = len(engine.policies)
        removed = engine.remove_policy("path")
        assert removed
        assert len(engine.policies) == initial_count - 1

    def test_get_policy(self, engine: PolicyEngine):
        policy = engine.get_policy("velocity")
        assert policy is not None
        assert policy.name == "velocity"

        missing = engine.get_policy("nonexistent")
        assert missing is None


class TestTrustPolicy:
    """Tests for TrustPolicy."""

    @pytest.fixture
    def manager(self) -> TrustManager:
        """Create a TrustManager with custom rules."""
        custom_level = TrustLevel(
            name="custom",
            allow_rules=[
                TrustRule("read:*", Decision.ALLOW),
                TrustRule("bash:ruff *", Decision.ALLOW),
            ],
            deny_rules=[
                TrustRule("write:*.env", Decision.DENY),
                TrustRule("bash:rm -rf *", Decision.DENY),
            ],
            confirm_rules=[
                TrustRule("write:*", Decision.CONFIRM),
            ],
        )
        return TrustManager(levels={"custom": custom_level}, default_level="custom")

    @pytest.fixture
    def policy(self, manager: TrustManager) -> TrustPolicy:
        return TrustPolicy(trust_manager=manager)

    async def test_allows_read_operations(self, policy: TrustPolicy, tmp_path: Path):
        ctx = ToolCallContext(tool_name="read_file", target=tmp_path / "test.py")
        result = await policy.evaluate(ctx)
        assert result.decision == PolicyDecision.ALLOW

    async def test_denies_env_writes(self, policy: TrustPolicy, tmp_path: Path):
        ctx = ToolCallContext(
            tool_name="write_file",
            target=tmp_path / ".env",
            action="write",
        )
        result = await policy.evaluate(ctx)
        assert result.decision == PolicyDecision.DENY

    async def test_warns_for_confirm_operations(self, policy: TrustPolicy, tmp_path: Path):
        ctx = ToolCallContext(
            tool_name="write_file",
            target=tmp_path / "normal.py",
            action="write",
        )
        result = await policy.evaluate(ctx)
        assert result.decision == PolicyDecision.WARN  # CONFIRM maps to WARN

    async def test_allows_trusted_bash_commands(self, policy: TrustPolicy):
        ctx = ToolCallContext(
            tool_name="bash",
            parameters={"command": "ruff check src/"},
        )
        result = await policy.evaluate(ctx)
        assert result.decision == PolicyDecision.ALLOW

    async def test_denies_dangerous_bash_commands(self, policy: TrustPolicy):
        ctx = ToolCallContext(
            tool_name="bash",
            parameters={"command": "rm -rf /"},
        )
        result = await policy.evaluate(ctx)
        assert result.decision == PolicyDecision.DENY

    async def test_infers_operation_from_tool_name(self, policy: TrustPolicy):
        # Tool name contains "grep" -> operation = "read"
        ctx = ToolCallContext(tool_name="grep_search")
        result = await policy.evaluate(ctx)
        assert result.metadata["operation"] == "read"

    async def test_uses_explicit_action(self, policy: TrustPolicy):
        ctx = ToolCallContext(tool_name="some_tool", action="write")
        result = await policy.evaluate(ctx)
        assert result.metadata["operation"] == "write"

    async def test_metadata_contains_decision_info(self, policy: TrustPolicy, tmp_path: Path):
        ctx = ToolCallContext(tool_name="read_file", target=tmp_path / "test.py")
        result = await policy.evaluate(ctx)

        assert "trust_decision" in result.metadata
        assert "operation" in result.metadata
        assert "target" in result.metadata

    def test_loads_from_root(self, tmp_path: Path):
        # TrustPolicy can load config from root (even if no config exists)
        policy = TrustPolicy(root=tmp_path)
        assert policy._manager is not None


class TestCreateDefaultPolicyEngine:
    """Tests for create_default_policy_engine."""

    def test_creates_engine_with_defaults(self):
        engine = create_default_policy_engine()
        names = [p.name for p in engine.policies]

        assert "quarantine" in names
        assert "velocity" in names
        assert "rate_limit" in names
        assert "path" in names
        assert "trust" in names  # Now included by default

    def test_creates_engine_without_trust(self):
        engine = create_default_policy_engine(include_trust=False)
        names = [p.name for p in engine.policies]

        assert "quarantine" in names
        assert "trust" not in names

    def test_creates_engine_with_root(self, tmp_path: Path):
        engine = create_default_policy_engine(root=tmp_path)
        trust_policy = engine.get_policy("trust")

        assert trust_policy is not None

    async def test_default_engine_works(self, tmp_path: Path):
        engine = create_default_policy_engine()
        target = tmp_path / "test.py"

        result = await engine.check("edit", target=target)
        assert result.allowed
