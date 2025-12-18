"""Policy Engine: Intercept tool calls and enforce safety rules."""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import UTC, datetime, timedelta
from enum import Enum, auto
from pathlib import Path
from typing import Any

from moss.events import EventBus, EventType
from moss.trust import Decision as TrustDecisionType
from moss.trust import TrustDecision, TrustManager


class PolicyDecision(Enum):
    """Decision returned by a policy."""

    ALLOW = auto()  # Proceed with the action
    DENY = auto()  # Block the action
    WARN = auto()  # Allow but emit warning
    QUARANTINE = auto()  # Lock the target, require special handling


@dataclass
class PolicyResult:
    """Result of policy evaluation."""

    decision: PolicyDecision
    policy_name: str
    reason: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)

    @property
    def allowed(self) -> bool:
        return self.decision in (PolicyDecision.ALLOW, PolicyDecision.WARN)


@dataclass
class ToolCallContext:
    """Context for a tool call being evaluated."""

    tool_name: str
    target: Path | None = None
    action: str | None = None  # e.g., "write", "delete", "execute"
    parameters: dict[str, Any] = field(default_factory=dict)
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))


class Policy(ABC):
    """Abstract base for policies."""

    @property
    @abstractmethod
    def name(self) -> str:
        """Policy name for logging."""
        ...

    @property
    def priority(self) -> int:
        """Higher priority policies are evaluated first. Default 0."""
        return 0

    @abstractmethod
    async def evaluate(self, context: ToolCallContext) -> PolicyResult:
        """Evaluate the policy for a tool call.

        Returns:
            PolicyResult with decision and optional reason
        """
        ...


class VelocityPolicy(Policy):
    """Track progress and detect stalls/oscillation.

    Enforces that agents make consistent forward progress.
    If error counts stall or oscillate, blocks further actions.
    """

    def __init__(
        self,
        *,
        stall_threshold: int = 3,
        oscillation_threshold: int = 2,
        window_seconds: int = 300,
    ):
        self.stall_threshold = stall_threshold
        self.oscillation_threshold = oscillation_threshold
        self.window = timedelta(seconds=window_seconds)

        self._error_history: list[tuple[datetime, int]] = []
        self._stall_count = 0
        self._oscillation_count = 0
        self._is_blocked = False
        self._block_reason: str | None = None

    @property
    def name(self) -> str:
        return "velocity"

    @property
    def priority(self) -> int:
        return 10  # High priority - check early

    def record_error_count(self, error_count: int) -> None:
        """Record an error count observation."""
        now = datetime.now(UTC)

        # Prune old entries
        cutoff = now - self.window
        self._error_history = [(t, c) for t, c in self._error_history if t > cutoff]

        # Add new entry
        self._error_history.append((now, error_count))

        # Analyze for stall
        if len(self._error_history) >= 2:
            prev_count = self._error_history[-2][1]
            if error_count == prev_count:
                self._stall_count += 1
            else:
                self._stall_count = 0

        # Analyze for oscillation
        if len(self._error_history) >= 4:
            counts = [c for _, c in self._error_history[-4:]]
            diffs = [counts[i + 1] - counts[i] for i in range(len(counts) - 1)]
            if all(d != 0 for d in diffs):
                signs = [d > 0 for d in diffs]
                if signs == [True, False, True] or signs == [False, True, False]:
                    self._oscillation_count += 1

        # Check thresholds
        if self._stall_count >= self.stall_threshold:
            self._is_blocked = True
            self._block_reason = f"Stalled: no progress for {self._stall_count} iterations"
        elif self._oscillation_count >= self.oscillation_threshold:
            self._is_blocked = True
            self._block_reason = f"Oscillating: {self._oscillation_count} cycles detected"

    def reset(self) -> None:
        """Reset velocity tracking."""
        self._error_history.clear()
        self._stall_count = 0
        self._oscillation_count = 0
        self._is_blocked = False
        self._block_reason = None

    async def evaluate(self, context: ToolCallContext) -> PolicyResult:
        if self._is_blocked:
            return PolicyResult(
                decision=PolicyDecision.DENY,
                policy_name=self.name,
                reason=self._block_reason,
                metadata={
                    "stall_count": self._stall_count,
                    "oscillation_count": self._oscillation_count,
                },
            )
        return PolicyResult(decision=PolicyDecision.ALLOW, policy_name=self.name)


class QuarantinePolicy(Policy):
    """Lock files with parse errors until repaired.

    Files in quarantine can only be modified by "repair" tools.
    """

    def __init__(self, repair_tools: set[str] | None = None):
        self._quarantined: dict[Path, str] = {}  # path -> reason
        self.repair_tools = repair_tools or {"repair", "fix_syntax", "raw_edit"}

    @property
    def name(self) -> str:
        return "quarantine"

    @property
    def priority(self) -> int:
        return 20  # Very high priority

    def quarantine(self, path: Path, reason: str) -> None:
        """Add a file to quarantine."""
        self._quarantined[path.resolve()] = reason

    def release(self, path: Path) -> bool:
        """Release a file from quarantine. Returns True if was quarantined."""
        return self._quarantined.pop(path.resolve(), None) is not None

    def is_quarantined(self, path: Path) -> bool:
        """Check if a file is quarantined."""
        return path.resolve() in self._quarantined

    def get_quarantine_reason(self, path: Path) -> str | None:
        """Get the quarantine reason for a file."""
        return self._quarantined.get(path.resolve())

    @property
    def quarantined_files(self) -> list[Path]:
        """Get all quarantined files."""
        return list(self._quarantined.keys())

    async def evaluate(self, context: ToolCallContext) -> PolicyResult:
        if context.target is None:
            return PolicyResult(decision=PolicyDecision.ALLOW, policy_name=self.name)

        resolved = context.target.resolve()
        if resolved not in self._quarantined:
            return PolicyResult(decision=PolicyDecision.ALLOW, policy_name=self.name)

        # File is quarantined - check if tool is allowed
        if context.tool_name in self.repair_tools:
            return PolicyResult(
                decision=PolicyDecision.WARN,
                policy_name=self.name,
                reason=f"File is quarantined: {self._quarantined[resolved]}. "
                f"Repair tool '{context.tool_name}' allowed.",
            )

        return PolicyResult(
            decision=PolicyDecision.QUARANTINE,
            policy_name=self.name,
            reason=f"File is quarantined: {self._quarantined[resolved]}. "
            f"Only repair tools {self.repair_tools} may modify it.",
            metadata={"quarantine_reason": self._quarantined[resolved]},
        )


class RateLimitPolicy(Policy):
    """Limit the rate of tool calls.

    Prevents runaway agents from consuming excessive resources.
    """

    def __init__(
        self,
        *,
        max_calls_per_minute: int = 60,
        max_calls_per_target: int = 10,
    ):
        self.max_calls_per_minute = max_calls_per_minute
        self.max_calls_per_target = max_calls_per_target

        self._call_times: list[datetime] = []
        self._target_calls: dict[Path, int] = {}

    @property
    def name(self) -> str:
        return "rate_limit"

    def _prune_old_calls(self) -> None:
        """Remove call records older than 1 minute."""
        cutoff = datetime.now(UTC) - timedelta(minutes=1)
        self._call_times = [t for t in self._call_times if t > cutoff]

    def record_call(self, target: Path | None = None) -> None:
        """Record a tool call."""
        now = datetime.now(UTC)
        self._call_times.append(now)
        if target:
            resolved = target.resolve()
            self._target_calls[resolved] = self._target_calls.get(resolved, 0) + 1

    def reset_target(self, target: Path) -> None:
        """Reset call count for a specific target."""
        self._target_calls.pop(target.resolve(), None)

    async def evaluate(self, context: ToolCallContext) -> PolicyResult:
        self._prune_old_calls()

        # Check global rate
        if len(self._call_times) >= self.max_calls_per_minute:
            return PolicyResult(
                decision=PolicyDecision.DENY,
                policy_name=self.name,
                reason=f"Rate limit exceeded: "
                f"{len(self._call_times)}/{self.max_calls_per_minute} calls/min",
            )

        # Check per-target rate
        if context.target:
            resolved = context.target.resolve()
            count = self._target_calls.get(resolved, 0)
            if count >= self.max_calls_per_target:
                return PolicyResult(
                    decision=PolicyDecision.WARN,
                    policy_name=self.name,
                    reason=f"Target {context.target.name} modified {count} times. "
                    f"Consider a different approach.",
                )

        return PolicyResult(decision=PolicyDecision.ALLOW, policy_name=self.name)


class PathPolicy(Policy):
    """Restrict access to certain paths.

    Blocks access to sensitive directories or files.
    """

    def __init__(
        self,
        *,
        blocked_patterns: list[str] | None = None,
        blocked_paths: list[Path] | None = None,
    ):
        self.blocked_patterns = blocked_patterns or [
            ".git",
            ".env",
            "__pycache__",
            "node_modules",
            ".ssh",
            ".aws",
            "credentials",
            "secrets",
        ]
        self.blocked_paths = [p.resolve() for p in (blocked_paths or [])]

    @property
    def name(self) -> str:
        return "path"

    def _matches_pattern(self, path: Path) -> str | None:
        """Check if path matches any blocked pattern."""
        path_str = str(path)
        for pattern in self.blocked_patterns:
            if pattern in path_str:
                return pattern
        return None

    async def evaluate(self, context: ToolCallContext) -> PolicyResult:
        if context.target is None:
            return PolicyResult(decision=PolicyDecision.ALLOW, policy_name=self.name)

        resolved = context.target.resolve()

        # Check blocked paths
        for blocked in self.blocked_paths:
            if resolved == blocked or blocked in resolved.parents:
                return PolicyResult(
                    decision=PolicyDecision.DENY,
                    policy_name=self.name,
                    reason=f"Path {context.target} is explicitly blocked",
                )

        # Check patterns
        pattern = self._matches_pattern(resolved)
        if pattern:
            return PolicyResult(
                decision=PolicyDecision.DENY,
                policy_name=self.name,
                reason=f"Path contains blocked pattern: '{pattern}'",
            )

        return PolicyResult(decision=PolicyDecision.ALLOW, policy_name=self.name)


class TrustPolicy(Policy):
    """Enforce trust levels from TrustManager.

    Integrates the configurable trust system with the policy engine.
    Trust rules from .moss/trust.yaml are evaluated for each tool call.
    """

    def __init__(
        self,
        trust_manager: TrustManager | None = None,
        root: Path | None = None,
    ):
        """Initialize with a TrustManager or load from config.

        Args:
            trust_manager: Pre-configured TrustManager
            root: Project root for loading config (if trust_manager not provided)
        """
        if trust_manager:
            self._manager = trust_manager
        elif root:
            self._manager = TrustManager.load(root)
        else:
            self._manager = TrustManager()  # Default config

    @property
    def name(self) -> str:
        return "trust"

    @property
    def priority(self) -> int:
        return 5  # Below quarantine (20), velocity (10), but above rate limit (0)

    def _map_action_to_operation(self, context: ToolCallContext) -> str:
        """Map tool call context to trust operation type."""
        # Use explicit action if provided
        if context.action:
            return context.action

        # Infer from tool name
        tool_name = context.tool_name.lower()

        read_tools = {"read", "grep", "glob", "search", "find", "cat", "head", "tail"}
        write_tools = {"write", "edit", "patch", "apply", "create", "update"}
        delete_tools = {"delete", "remove", "rm", "unlink"}
        bash_tools = {"bash", "shell", "exec", "run", "command"}

        if any(t in tool_name for t in read_tools):
            return "read"
        if any(t in tool_name for t in write_tools):
            return "write"
        if any(t in tool_name for t in delete_tools):
            return "delete"
        if any(t in tool_name for t in bash_tools):
            return "bash"

        # Default to the tool name itself
        return tool_name

    def _get_target(self, context: ToolCallContext) -> str:
        """Get target string from context."""
        if context.target:
            return str(context.target)

        # Check parameters for common target fields
        for key in ("path", "file", "command", "target", "cmd"):
            if key in context.parameters:
                return str(context.parameters[key])

        return "*"

    def _map_decision(self, trust_decision: TrustDecision) -> PolicyDecision:
        """Map TrustDecision to PolicyDecision."""
        if trust_decision.decision == TrustDecisionType.ALLOW:
            return PolicyDecision.ALLOW
        if trust_decision.decision == TrustDecisionType.DENY:
            return PolicyDecision.DENY
        # CONFIRM maps to WARN - allows but should notify
        return PolicyDecision.WARN

    async def evaluate(self, context: ToolCallContext) -> PolicyResult:
        """Evaluate trust rules for the tool call."""
        operation = self._map_action_to_operation(context)
        target = self._get_target(context)

        trust_decision = self._manager.check(operation, target)
        policy_decision = self._map_decision(trust_decision)

        return PolicyResult(
            decision=policy_decision,
            policy_name=self.name,
            reason=trust_decision.reason,
            metadata={
                "trust_decision": trust_decision.decision.value,
                "matched_rule": trust_decision.matched_rule,
                "operation": operation,
                "target": target,
            },
        )


@dataclass
class PolicyEngineResult:
    """Result of evaluating all policies."""

    allowed: bool
    results: list[PolicyResult]
    blocking_result: PolicyResult | None = None

    @property
    def warnings(self) -> list[PolicyResult]:
        return [r for r in self.results if r.decision == PolicyDecision.WARN]


class PolicyEngine:
    """Evaluate policies before tool calls.

    Policies are evaluated in priority order (highest first).
    First DENY or QUARANTINE stops evaluation.
    """

    def __init__(
        self,
        policies: list[Policy] | None = None,
        event_bus: EventBus | None = None,
    ):
        self._policies = list(policies or [])
        self._policies.sort(key=lambda p: -p.priority)  # Highest priority first
        self.event_bus = event_bus

    def add_policy(self, policy: Policy) -> None:
        """Add a policy and re-sort by priority."""
        self._policies.append(policy)
        self._policies.sort(key=lambda p: -p.priority)

    def remove_policy(self, name: str) -> bool:
        """Remove a policy by name. Returns True if found."""
        for i, p in enumerate(self._policies):
            if p.name == name:
                self._policies.pop(i)
                return True
        return False

    def get_policy(self, name: str) -> Policy | None:
        """Get a policy by name."""
        for p in self._policies:
            if p.name == name:
                return p
        return None

    @property
    def policies(self) -> list[Policy]:
        """Get all policies (sorted by priority)."""
        return list(self._policies)

    async def _emit(self, event_type: EventType, payload: dict[str, Any]) -> None:
        """Emit an event if event bus is configured."""
        if self.event_bus:
            await self.event_bus.emit(event_type, payload)

    async def evaluate(self, context: ToolCallContext) -> PolicyEngineResult:
        """Evaluate all policies for a tool call.

        Returns:
            PolicyEngineResult with overall decision and individual results
        """
        results: list[PolicyResult] = []

        for policy in self._policies:
            result = await policy.evaluate(context)
            results.append(result)

            if result.decision in (PolicyDecision.DENY, PolicyDecision.QUARANTINE):
                await self._emit(
                    EventType.TOOL_CALL,
                    {
                        "action": "policy_blocked",
                        "tool": context.tool_name,
                        "policy": policy.name,
                        "reason": result.reason,
                    },
                )
                return PolicyEngineResult(
                    allowed=False,
                    results=results,
                    blocking_result=result,
                )

        # All policies passed
        return PolicyEngineResult(allowed=True, results=results)

    async def check(
        self,
        tool_name: str,
        target: Path | None = None,
        action: str | None = None,
        **parameters: Any,
    ) -> PolicyEngineResult:
        """Convenience method to check a tool call."""
        context = ToolCallContext(
            tool_name=tool_name,
            target=target,
            action=action,
            parameters=parameters,
        )
        return await self.evaluate(context)


def create_default_policy_engine(
    event_bus: EventBus | None = None,
    root: Path | None = None,
    include_trust: bool = True,
) -> PolicyEngine:
    """Create a policy engine with sensible defaults.

    Args:
        event_bus: Event bus for emitting policy events
        root: Project root for loading trust config
        include_trust: Whether to include TrustPolicy (default: True)

    Returns:
        Configured PolicyEngine
    """
    policies: list[Policy] = [
        QuarantinePolicy(),
        VelocityPolicy(),
        RateLimitPolicy(),
        PathPolicy(),
    ]

    if include_trust:
        policies.append(TrustPolicy(root=root))

    return PolicyEngine(policies=policies, event_bus=event_bus)
