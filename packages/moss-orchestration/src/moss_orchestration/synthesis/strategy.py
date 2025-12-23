"""Decomposition strategies for synthesis.

Strategies determine how to break down a complex problem into
smaller, more manageable subproblems.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .types import Context, Specification, Subproblem


@dataclass
class StrategyMetadata:
    """Metadata about a decomposition strategy."""

    name: str
    description: str
    keywords: tuple[str, ...] = field(default_factory=tuple)


class DecompositionStrategy(ABC):
    """Base class for all decomposition strategies.

    A decomposition strategy determines:
    1. Whether it can handle a given problem (can_handle)
    2. How to break the problem into subproblems (decompose)
    3. Estimated probability of success (estimate_success)

    Strategies are selected by the StrategyRouter based on:
    - TF-IDF similarity to strategy description
    - Strategy's self-assessment (can_handle, estimate_success)
    - Historical success rate from memory
    """

    @property
    @abstractmethod
    def metadata(self) -> StrategyMetadata:
        """Return strategy metadata for routing."""
        ...

    @property
    def name(self) -> str:
        """Shorthand for metadata.name."""
        return self.metadata.name

    @property
    def description(self) -> str:
        """Shorthand for metadata.description."""
        return self.metadata.description

    @property
    def keywords(self) -> tuple[str, ...]:
        """Shorthand for metadata.keywords."""
        return self.metadata.keywords

    @abstractmethod
    def can_handle(self, spec: Specification, context: Context) -> bool:
        """Check if this strategy can handle the given specification.

        Returns True if the strategy has a reasonable chance of success.
        This is a quick check, not a full analysis.
        """
        ...

    @abstractmethod
    def decompose(
        self,
        spec: Specification,
        context: Context,
    ) -> list[Subproblem]:
        """Decompose the problem into subproblems.

        Returns:
            List of subproblems. Empty list means the problem is atomic.

        Raises:
            DecompositionError: If decomposition fails.
        """
        ...

    @abstractmethod
    def estimate_success(self, spec: Specification, context: Context) -> float:
        """Estimate the probability of success for this strategy.

        Returns:
            Float between 0.0 and 1.0 indicating confidence.
        """
        ...

    def document(self) -> str:
        """Generate documentation string for TF-IDF indexing."""
        keywords_str = " ".join(self.keywords)
        return f"{self.name} {self.description} {keywords_str}"


class AtomicStrategy(DecompositionStrategy):
    """Strategy for problems that are already atomic.

    Returns an empty list of subproblems, signaling that the problem
    should be solved directly.
    """

    @property
    def metadata(self) -> StrategyMetadata:
        return StrategyMetadata(
            name="atomic",
            description="Handle atomic problems that need no decomposition",
            keywords=("simple", "atomic", "direct", "trivial", "base", "primitive"),
        )

    def can_handle(self, spec: Specification, context: Context) -> bool:
        """Check if problem is atomic (can be solved directly)."""
        # Check if description matches a primitive
        desc_lower = spec.description.lower()
        for primitive in context.primitives:
            if primitive.lower() in desc_lower:
                return True

        # Check if already solved
        if spec.description in context.solved:
            return True

        # Check if very simple (heuristic)
        if len(spec.description) < 50 and not spec.constraints:
            return True

        return False

    def decompose(
        self,
        spec: Specification,
        context: Context,
    ) -> list[Subproblem]:
        """Atomic problems return empty list (no decomposition needed)."""
        return []

    def estimate_success(self, spec: Specification, context: Context) -> float:
        """High confidence for atomic problems."""
        if spec.description in context.solved:
            return 1.0
        if any(p.lower() in spec.description.lower() for p in context.primitives):
            return 0.9
        return 0.7
