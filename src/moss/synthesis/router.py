"""Strategy router for synthesis.

The router selects the best decomposition strategy for a given problem,
similar to how DWIM selects tools.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from moss.dwim import TFIDFIndex

if TYPE_CHECKING:
    from moss.memory import EpisodicStore

    from .strategy import DecompositionStrategy
    from .types import Context, Specification


@dataclass
class StrategyMatch:
    """Result of matching a specification to a strategy."""

    strategy: DecompositionStrategy
    confidence: float
    signals: dict[str, float]


class StrategyRouter:
    """Selects best decomposition strategy (like DWIM for tools).

    Uses multiple signals to select the best strategy:
    1. TF-IDF similarity between spec and strategy descriptions
    2. Strategy's self-assessment (can_handle, estimate_success)
    3. Historical success rate from memory (if available)

    Weights:
    - TF-IDF: 35% (semantic similarity)
    - Self-assessment: 35% (strategy confidence)
    - History: 30% (learned patterns)
    """

    def __init__(
        self,
        strategies: list[DecompositionStrategy],
        memory: EpisodicStore | None = None,
    ):
        self.strategies = strategies
        self.memory = memory
        self._index = TFIDFIndex()
        self._strategy_indices: dict[str, int] = {}
        self._build_index()

    def _build_index(self) -> None:
        """Build TF-IDF index over strategy descriptions."""
        for strategy in self.strategies:
            doc = strategy.document()
            idx = self._index.add_document(doc)
            self._strategy_indices[strategy.name] = idx

    def add_strategy(self, strategy: DecompositionStrategy) -> None:
        """Add a new strategy to the router."""
        self.strategies.append(strategy)
        doc = strategy.document()
        idx = self._index.add_document(doc)
        self._strategy_indices[strategy.name] = idx

    async def select_strategy(
        self,
        spec: Specification,
        context: Context,
    ) -> DecompositionStrategy:
        """Select the best strategy for the given specification.

        Args:
            spec: Problem specification
            context: Available resources

        Returns:
            Best matching strategy

        Raises:
            NoStrategyError: If no strategy can handle the problem
        """
        from .types import NoStrategyError

        matches = await self.rank_strategies(spec, context)

        if not matches:
            raise NoStrategyError(f"No strategy can handle: {spec.summary()}")

        return matches[0].strategy

    async def rank_strategies(
        self,
        spec: Specification,
        context: Context,
    ) -> list[StrategyMatch]:
        """Rank all strategies for the given specification.

        Returns list of StrategyMatch sorted by confidence (highest first).
        """
        # Build query from specification
        query_parts = [spec.description]
        if spec.type_signature:
            query_parts.append(spec.type_signature)
        query_parts.extend(spec.constraints)
        query = " ".join(query_parts)

        # Get TF-IDF scores
        tfidf_results = self._index.query(query, top_k=len(self.strategies))
        tfidf_scores: dict[int, float] = {idx: score for idx, score in tfidf_results}

        matches: list[StrategyMatch] = []

        for strategy in self.strategies:
            signals: dict[str, float] = {}

            # Signal 1: Can the strategy handle this problem?
            if not strategy.can_handle(spec, context):
                continue

            # Signal 2: TF-IDF similarity (35%)
            strategy_idx = self._strategy_indices.get(strategy.name, -1)
            tfidf_score = tfidf_scores.get(strategy_idx, 0.0)
            signals["tfidf"] = tfidf_score

            # Signal 3: Strategy's self-assessment (35%)
            estimate = strategy.estimate_success(spec, context)
            signals["estimate"] = estimate

            # Signal 4: Historical success rate (30%)
            history_score = await self._get_history_score(spec, strategy)
            signals["history"] = history_score

            # Combined score (weighted)
            confidence = tfidf_score * 0.35 + estimate * 0.35 + history_score * 0.30

            matches.append(
                StrategyMatch(
                    strategy=strategy,
                    confidence=confidence,
                    signals=signals,
                )
            )

        # Sort by confidence descending
        matches.sort(key=lambda m: m.confidence, reverse=True)
        return matches

    async def _get_history_score(
        self,
        spec: Specification,
        strategy: DecompositionStrategy,
    ) -> float:
        """Get historical success rate for this strategy on similar problems."""
        if self.memory is None:
            return 0.5  # Neutral if no memory

        try:
            # Query for similar past attempts
            history = await self.memory.query(
                query=spec.description,
                limit=20,
                filters={"strategy": strategy.name},
            )

            if not history:
                return 0.5  # Neutral if no history

            # Calculate success rate
            successes = sum(1 for h in history if h.get("outcome") == "success")
            return successes / len(history)

        except Exception:
            # Silently fail on memory errors
            return 0.5

    async def record_outcome(
        self,
        spec: Specification,
        strategy: DecompositionStrategy,
        success: bool,
        iterations: int = 0,
    ) -> None:
        """Record the outcome of a synthesis attempt for future learning."""
        if self.memory is None:
            return

        try:
            await self.memory.record(
                {
                    "type": "synthesis_outcome",
                    "spec": spec.summary(),
                    "spec_description": spec.description,
                    "strategy": strategy.name,
                    "outcome": "success" if success else "failure",
                    "iterations": iterations,
                }
            )
        except Exception:
            # Silently fail on memory errors
            pass
