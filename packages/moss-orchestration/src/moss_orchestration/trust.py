"""Trust levels for fine-grained, composable permissions.

Provides intelligent permission management that balances safety with
productivity - smarter than basic "approve Y/N" prompts.

Configuration via `.moss/trust.yaml`:

    default: high

    levels:
      my-dev-level:
        inherit: high
        allow:
          - "bash:ruff *"
          - "bash:pytest *"
          - "write:src/**"
        deny:
          - "write:*.env"
          - "bash:rm -rf *"
        confirm:
          - "write:config/*"

Usage:
    from moss_orchestration.trust import TrustManager

    manager = TrustManager.load(project_root)
    decision = manager.check("bash", "ruff check src/")

    if decision.allowed:
        execute()
    elif decision.needs_confirm:
        if user_approves():
            execute()
"""

from __future__ import annotations

import fnmatch
import logging
import re
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


# =============================================================================
# Enums and Data Types
# =============================================================================


class Decision(Enum):
    """Result of a trust check."""

    ALLOW = "allow"  # Auto-approved, no confirmation needed
    DENY = "deny"  # Blocked, not permitted
    CONFIRM = "confirm"  # Needs user confirmation


@dataclass
class TrustDecision:
    """A trust decision with context."""

    decision: Decision
    operation: str
    target: str
    reason: str = ""
    matched_rule: str | None = None

    @property
    def allowed(self) -> bool:
        return self.decision == Decision.ALLOW

    @property
    def denied(self) -> bool:
        return self.decision == Decision.DENY

    @property
    def needs_confirm(self) -> bool:
        return self.decision == Decision.CONFIRM


@dataclass
class TrustRule:
    """A single trust rule."""

    pattern: str  # e.g., "bash:ruff *", "write:src/**"
    decision: Decision

    def matches(self, operation: str, target: str) -> bool:
        """Check if this rule matches the operation and target."""
        # Parse pattern: "operation:target_pattern"
        if ":" in self.pattern:
            op_pattern, target_pattern = self.pattern.split(":", 1)
        else:
            op_pattern = self.pattern
            target_pattern = "*"

        # Check operation match
        if not fnmatch.fnmatch(operation.lower(), op_pattern.lower()):
            return False

        # Check target match (supports glob patterns)
        if target_pattern == "*":
            return True

        # Convert glob to regex for more flexible matching
        regex_pattern = fnmatch.translate(target_pattern)
        return bool(re.match(regex_pattern, target, re.IGNORECASE))


@dataclass
class TrustLevel:
    """A named trust level with rules."""

    name: str
    allow_rules: list[TrustRule] = field(default_factory=list)
    deny_rules: list[TrustRule] = field(default_factory=list)
    confirm_rules: list[TrustRule] = field(default_factory=list)
    inherit: str | None = None  # Parent level to inherit from
    default_decision: Decision = Decision.CONFIRM

    def check(self, operation: str, target: str) -> TrustDecision:
        """Check if an operation is allowed at this level.

        Order of precedence:
        1. Explicit deny rules (highest priority)
        2. Explicit allow rules
        3. Explicit confirm rules
        4. Default decision
        """
        # Check deny rules first (highest priority)
        for rule in self.deny_rules:
            if rule.matches(operation, target):
                return TrustDecision(
                    decision=Decision.DENY,
                    operation=operation,
                    target=target,
                    reason="Matched deny rule",
                    matched_rule=rule.pattern,
                )

        # Check allow rules
        for rule in self.allow_rules:
            if rule.matches(operation, target):
                return TrustDecision(
                    decision=Decision.ALLOW,
                    operation=operation,
                    target=target,
                    reason="Matched allow rule",
                    matched_rule=rule.pattern,
                )

        # Check confirm rules
        for rule in self.confirm_rules:
            if rule.matches(operation, target):
                return TrustDecision(
                    decision=Decision.CONFIRM,
                    operation=operation,
                    target=target,
                    reason="Matched confirm rule",
                    matched_rule=rule.pattern,
                )

        # Default decision
        return TrustDecision(
            decision=self.default_decision,
            operation=operation,
            target=target,
            reason="No matching rule, using default",
        )


# =============================================================================
# Built-in Presets
# =============================================================================


def _create_full_trust() -> TrustLevel:
    """Full trust - no confirmations, agent runs freely."""
    return TrustLevel(
        name="full",
        allow_rules=[TrustRule("*:*", Decision.ALLOW)],
        default_decision=Decision.ALLOW,
    )


def _create_high_trust() -> TrustLevel:
    """High trust - auto-approve reads/searches, confirm writes/commands."""
    return TrustLevel(
        name="high",
        allow_rules=[
            TrustRule("read:*", Decision.ALLOW),
            TrustRule("search:*", Decision.ALLOW),
            TrustRule("grep:*", Decision.ALLOW),
            TrustRule("glob:*", Decision.ALLOW),
            TrustRule("lint:*", Decision.ALLOW),
            TrustRule("test:*", Decision.ALLOW),
            TrustRule("bash:git status", Decision.ALLOW),
            TrustRule("bash:git diff*", Decision.ALLOW),
            TrustRule("bash:git log*", Decision.ALLOW),
            TrustRule("bash:ruff *", Decision.ALLOW),
            TrustRule("bash:pytest *", Decision.ALLOW),
            TrustRule("bash:uv *", Decision.ALLOW),
        ],
        deny_rules=[
            TrustRule("write:*.env", Decision.DENY),
            TrustRule("write:*credentials*", Decision.DENY),
            TrustRule("write:*secret*", Decision.DENY),
            TrustRule("bash:rm -rf *", Decision.DENY),
            TrustRule("bash:sudo *", Decision.DENY),
        ],
        confirm_rules=[
            TrustRule("write:*", Decision.CONFIRM),
            TrustRule("delete:*", Decision.CONFIRM),
            TrustRule("bash:git commit*", Decision.CONFIRM),
            TrustRule("bash:git push*", Decision.CONFIRM),
            TrustRule("bash:*", Decision.CONFIRM),
        ],
        default_decision=Decision.CONFIRM,
    )


def _create_medium_trust() -> TrustLevel:
    """Medium trust - auto-approve reads, confirm writes and commands."""
    return TrustLevel(
        name="medium",
        allow_rules=[
            TrustRule("read:*", Decision.ALLOW),
            TrustRule("search:*", Decision.ALLOW),
            TrustRule("grep:*", Decision.ALLOW),
            TrustRule("glob:*", Decision.ALLOW),
        ],
        deny_rules=[
            TrustRule("write:*.env", Decision.DENY),
            TrustRule("write:*credentials*", Decision.DENY),
            TrustRule("bash:rm -rf *", Decision.DENY),
            TrustRule("bash:sudo *", Decision.DENY),
        ],
        confirm_rules=[
            TrustRule("write:*", Decision.CONFIRM),
            TrustRule("delete:*", Decision.CONFIRM),
            TrustRule("bash:*", Decision.CONFIRM),
            TrustRule("lint:*", Decision.CONFIRM),
            TrustRule("test:*", Decision.CONFIRM),
        ],
        default_decision=Decision.CONFIRM,
    )


def _create_low_trust() -> TrustLevel:
    """Low trust - confirm everything except reads."""
    return TrustLevel(
        name="low",
        allow_rules=[
            TrustRule("read:*", Decision.ALLOW),
        ],
        deny_rules=[
            TrustRule("bash:rm *", Decision.DENY),
            TrustRule("bash:sudo *", Decision.DENY),
            TrustRule("delete:*", Decision.DENY),
        ],
        default_decision=Decision.CONFIRM,
    )


BUILTIN_LEVELS: dict[str, TrustLevel] = {
    "full": _create_full_trust(),
    "high": _create_high_trust(),
    "medium": _create_medium_trust(),
    "low": _create_low_trust(),
}


# =============================================================================
# Trust Manager
# =============================================================================


class TrustManager:
    """Manages trust levels and makes permission decisions."""

    def __init__(
        self,
        levels: dict[str, TrustLevel] | None = None,
        default_level: str = "high",
    ) -> None:
        """Initialize the trust manager.

        Args:
            levels: Custom trust levels (merged with built-ins)
            default_level: Default level to use
        """
        self.levels = {**BUILTIN_LEVELS}
        if levels:
            self.levels.update(levels)

        self.default_level = default_level
        self._resolved_cache: dict[str, TrustLevel] = {}

    def get_level(self, name: str) -> TrustLevel:
        """Get a trust level by name, resolving inheritance."""
        if name in self._resolved_cache:
            return self._resolved_cache[name]

        if name not in self.levels:
            logger.warning("Unknown trust level %s, using %s", name, self.default_level)
            name = self.default_level

        level = self.levels[name]

        # Resolve inheritance
        if level.inherit and level.inherit in self.levels:
            parent = self.get_level(level.inherit)
            resolved = TrustLevel(
                name=level.name,
                allow_rules=[*parent.allow_rules, *level.allow_rules],
                deny_rules=[*parent.deny_rules, *level.deny_rules],
                confirm_rules=[*parent.confirm_rules, *level.confirm_rules],
                default_decision=level.default_decision,
            )
            self._resolved_cache[name] = resolved
            return resolved

        self._resolved_cache[name] = level
        return level

    def check(
        self,
        operation: str,
        target: str,
        level_name: str | None = None,
    ) -> TrustDecision:
        """Check if an operation is allowed.

        Args:
            operation: Type of operation (read, write, bash, delete, etc.)
            target: Target of the operation (file path, command, etc.)
            level_name: Trust level to use (default: manager's default)

        Returns:
            TrustDecision with the result
        """
        level = self.get_level(level_name or self.default_level)
        return level.check(operation, target)

    @classmethod
    def load(cls, root: Path) -> TrustManager:
        """Load trust configuration from project.

        Looks for:
        1. .moss/trust.yaml
        2. .moss/trust.toml
        3. moss.toml [trust] section

        Args:
            root: Project root directory

        Returns:
            Configured TrustManager
        """
        root = Path(root).resolve()
        custom_levels: dict[str, TrustLevel] = {}
        default_level = "high"

        # Try .moss/trust.yaml
        trust_yaml = root / ".moss" / "trust.yaml"
        if trust_yaml.exists():
            try:
                import yaml

                data = yaml.safe_load(trust_yaml.read_text())
                custom_levels, default_level = cls._parse_config(data)
            except ImportError:
                logger.warning("PyYAML not installed, cannot load trust.yaml")
            except (OSError, yaml.YAMLError) as e:
                logger.warning("Failed to load trust.yaml: %s", e)

        # Try .moss/trust.toml
        trust_toml = root / ".moss" / "trust.toml"
        if trust_toml.exists() and not custom_levels:
            try:
                import tomllib

                data = tomllib.loads(trust_toml.read_text())
                custom_levels, default_level = cls._parse_config(data)
            except (OSError, tomllib.TOMLDecodeError) as e:
                logger.warning("Failed to load trust.toml: %s", e)

        # Try moss.toml [trust] section
        moss_toml = root / "moss.toml"
        if moss_toml.exists() and not custom_levels:
            try:
                import tomllib

                data = tomllib.loads(moss_toml.read_text())
                if "trust" in data:
                    custom_levels, default_level = cls._parse_config(data["trust"])
            except (OSError, tomllib.TOMLDecodeError) as e:
                logger.warning("Failed to load trust from moss.toml: %s", e)

        return cls(levels=custom_levels, default_level=default_level)

    @classmethod
    def _parse_config(cls, data: dict[str, Any]) -> tuple[dict[str, TrustLevel], str]:
        """Parse trust configuration from a dictionary."""
        levels: dict[str, TrustLevel] = {}
        default_level = data.get("default", "high")

        for name, level_data in data.get("levels", {}).items():
            levels[name] = TrustLevel(
                name=name,
                allow_rules=[TrustRule(p, Decision.ALLOW) for p in level_data.get("allow", [])],
                deny_rules=[TrustRule(p, Decision.DENY) for p in level_data.get("deny", [])],
                confirm_rules=[
                    TrustRule(p, Decision.CONFIRM) for p in level_data.get("confirm", [])
                ],
                inherit=level_data.get("inherit"),
                default_decision=Decision.CONFIRM,
            )

        return levels, default_level


# =============================================================================
# Convenience Functions
# =============================================================================


def check_trust(
    operation: str,
    target: str,
    root: Path | str | None = None,
    level: str | None = None,
) -> TrustDecision:
    """Convenience function to check trust.

    Args:
        operation: Type of operation
        target: Target of the operation
        root: Project root (for loading config)
        level: Trust level to use

    Returns:
        TrustDecision
    """
    if root:
        manager = TrustManager.load(Path(root))
    else:
        manager = TrustManager()

    return manager.check(operation, target, level)
