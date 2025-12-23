"""Strategy plugin protocol and registry.

Provides auto-discovery of decomposition strategies via entry points.

Usage:
    from moss_orchestration.synthesis.strategy_registry import (
        StrategyPlugin,
        StrategyRegistry,
        get_strategy_registry,
    )

    # Get all registered strategies
    registry = get_strategy_registry()
    strategies = registry.get_all()
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Protocol, runtime_checkable

if TYPE_CHECKING:
    from moss_orchestration.synthesis.strategy import StrategyMetadata
    from moss_orchestration.synthesis.types import Context, Specification, Subproblem


# =============================================================================
# Protocol
# =============================================================================


@runtime_checkable
class StrategyPlugin(Protocol):
    """Protocol for decomposition strategy plugins.

    Strategies can be discovered via entry points in the
    'moss.synthesis.strategies' group.
    """

    @property
    def metadata(self) -> StrategyMetadata:
        """Return strategy metadata for routing."""
        ...

    @property
    def name(self) -> str:
        """Strategy name."""
        ...

    def can_handle(self, spec: Specification, context: Context) -> bool:
        """Check if this strategy can handle the specification."""
        ...

    def decompose(
        self,
        spec: Specification,
        context: Context,
    ) -> list[Subproblem]:
        """Decompose the problem into subproblems."""
        ...

    def estimate_success(self, spec: Specification, context: Context) -> float:
        """Estimate probability of success."""
        ...

    def document(self) -> str:
        """Generate documentation string for TF-IDF indexing."""
        ...


# =============================================================================
# Registry
# =============================================================================


@dataclass
class StrategyRegistry:
    """Registry for strategy plugins."""

    _strategies: dict[str, StrategyPlugin] = field(default_factory=dict)
    _enabled: set[str] = field(default_factory=set)
    _disabled: set[str] = field(default_factory=set)

    def register(self, strategy: StrategyPlugin) -> None:
        """Register a strategy plugin."""
        self._strategies[strategy.name] = strategy
        # Enable by default unless explicitly disabled
        if strategy.name not in self._disabled:
            self._enabled.add(strategy.name)

    def unregister(self, name: str) -> None:
        """Unregister a strategy plugin."""
        self._strategies.pop(name, None)
        self._enabled.discard(name)
        self._disabled.discard(name)

    def enable(self, name: str) -> None:
        """Enable a strategy."""
        self._enabled.add(name)
        self._disabled.discard(name)

    def disable(self, name: str) -> None:
        """Disable a strategy."""
        self._disabled.add(name)
        self._enabled.discard(name)

    def get(self, name: str) -> StrategyPlugin | None:
        """Get a strategy by name."""
        return self._strategies.get(name)

    def get_all(self, enabled_only: bool = True) -> list[StrategyPlugin]:
        """Get all strategies.

        Args:
            enabled_only: If True, only return enabled strategies.

        Returns:
            List of strategy plugins.
        """
        if enabled_only:
            return [s for n, s in self._strategies.items() if n in self._enabled]
        return list(self._strategies.values())

    def get_enabled(self) -> list[str]:
        """Get list of enabled strategy names."""
        return list(self._enabled)

    def get_disabled(self) -> list[str]:
        """Get list of disabled strategy names."""
        return list(self._disabled)

    def is_enabled(self, name: str) -> bool:
        """Check if a strategy is enabled."""
        return name in self._enabled

    def discover_plugins(self) -> None:
        """Discover strategy plugins from entry points."""
        from importlib.metadata import entry_points

        eps = entry_points(group="moss.synthesis.strategies")

        for ep in eps:
            try:
                strategy_class = ep.load()
                strategy = strategy_class()
                self.register(strategy)
            except (ImportError, AttributeError, TypeError):
                # Silently skip failed plugins
                pass

    def register_builtins(self) -> None:
        """Register built-in strategies."""
        from moss_orchestration.synthesis.strategies import (
            PatternBasedDecomposition,
            TestDrivenDecomposition,
            TypeDrivenDecomposition,
        )

        self.register(TypeDrivenDecomposition())
        self.register(TestDrivenDecomposition())
        self.register(PatternBasedDecomposition())


# =============================================================================
# Global Registry
# =============================================================================


_registry: StrategyRegistry | None = None


def get_strategy_registry() -> StrategyRegistry:
    """Get the global strategy registry.

    Initializes and populates the registry on first call.
    """
    global _registry
    if _registry is None:
        _registry = StrategyRegistry()
        _registry.discover_plugins()
        _registry.register_builtins()
    return _registry


def reset_strategy_registry() -> None:
    """Reset the global strategy registry."""
    global _registry
    _registry = None


__all__ = [
    "StrategyPlugin",
    "StrategyRegistry",
    "get_strategy_registry",
    "reset_strategy_registry",
]
