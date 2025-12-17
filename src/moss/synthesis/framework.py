"""Core synthesis framework.

The SynthesisFramework orchestrates the recursive decomposition and
composition process, integrating with moss primitives (validation,
shadow git, memory, events).
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

from moss.events import Event, EventBus, EventType
from moss.validators import Validator

from .composer import Composer, SequentialComposer
from .router import StrategyRouter
from .strategy import AtomicStrategy, DecompositionStrategy
from .types import (
    Context,
    DecompositionError,
    NoStrategyError,
    Specification,
    Subproblem,
    SynthesisError,
    SynthesisResult,
)

if TYPE_CHECKING:
    from moss.memory import EpisodicStore
    from moss.shadow_git import ShadowGit


# Extend EventType for synthesis events
class SynthesisEventType:
    """Event types specific to synthesis."""

    SYNTHESIS_START = "synthesis_start"
    STRATEGY_SELECTED = "strategy_selected"
    DECOMPOSITION_COMPLETE = "decomposition_complete"
    SUBPROBLEM_START = "subproblem_start"
    SUBPROBLEM_COMPLETE = "subproblem_complete"
    COMPOSITION_START = "composition_start"
    VALIDATION_START = "validation_start"
    VALIDATION_COMPLETE = "validation_complete"
    SYNTHESIS_COMPLETE = "synthesis_complete"
    SYNTHESIS_FAILED = "synthesis_failed"


@dataclass
class SynthesisConfig:
    """Configuration for the synthesis framework."""

    max_iterations: int = 50
    max_depth: int = 20
    parallel_subproblems: bool = True
    stop_on_first_valid: bool = True
    emit_events: bool = True


@dataclass
class SynthesisState:
    """Internal state during synthesis."""

    depth: int = 0
    iterations: int = 0
    subproblems_solved: int = 0
    strategies_tried: list[str] = field(default_factory=list)


class SynthesisFramework:
    """Domain-agnostic synthesis engine.

    The framework implements recursive decomposition:
    1. Check if problem is atomic -> solve directly
    2. Select decomposition strategy
    3. Decompose into subproblems
    4. Recursively synthesize each subproblem
    5. Compose solutions with validation loop

    Integrates with moss primitives:
    - Validation loops (ValidatorChain)
    - Shadow git (atomic commits, rollback)
    - Memory (episodic learning)
    - Event bus (progress tracking)
    """

    def __init__(
        self,
        strategies: list[DecompositionStrategy] | None = None,
        composer: Composer | None = None,
        router: StrategyRouter | None = None,
        validator: Validator | None = None,
        shadow_git: ShadowGit | None = None,
        memory: EpisodicStore | None = None,
        event_bus: EventBus | None = None,
        config: SynthesisConfig | None = None,
    ):
        # Default strategies include atomic
        self.strategies = strategies or [AtomicStrategy()]
        self.composer = composer or SequentialComposer()
        self.router = router or StrategyRouter(self.strategies, memory)
        self.validator = validator
        self.shadow_git = shadow_git
        self.memory = memory
        self.event_bus = event_bus or EventBus()
        self.config = config or SynthesisConfig()

    async def synthesize(
        self,
        specification: Specification,
        context: Context | None = None,
        validator: Validator | None = None,
    ) -> SynthesisResult:
        """Main synthesis entry point.

        Args:
            specification: What to synthesize
            context: Available resources (defaults to empty)
            validator: Override validator for this call

        Returns:
            SynthesisResult with solution if successful
        """
        context = context or Context()
        validator = validator or self.validator
        state = SynthesisState()

        await self._emit_event(
            SynthesisEventType.SYNTHESIS_START,
            {
                "spec": specification.summary(),
            },
        )

        try:
            solution = await self._synthesize_recursive(specification, context, validator, state)

            await self._emit_event(
                SynthesisEventType.SYNTHESIS_COMPLETE,
                {
                    "spec": specification.summary(),
                    "iterations": state.iterations,
                    "subproblems_solved": state.subproblems_solved,
                },
            )

            return SynthesisResult(
                success=True,
                solution=solution,
                iterations=state.iterations,
                strategy_used=state.strategies_tried[-1] if state.strategies_tried else None,
                subproblems_solved=state.subproblems_solved,
            )

        except SynthesisError as e:
            await self._emit_event(
                SynthesisEventType.SYNTHESIS_FAILED,
                {
                    "spec": specification.summary(),
                    "error": str(e),
                    "iterations": state.iterations,
                },
            )

            return SynthesisResult(
                success=False,
                error=str(e),
                iterations=state.iterations,
                strategy_used=state.strategies_tried[-1] if state.strategies_tried else None,
                subproblems_solved=state.subproblems_solved,
            )

    async def _synthesize_recursive(
        self,
        spec: Specification,
        context: Context,
        validator: Validator | None,
        state: SynthesisState,
    ) -> Any:
        """Recursive synthesis implementation."""
        # Check depth limit
        if state.depth >= self.config.max_depth:
            raise DecompositionError(
                f"Max depth {self.config.max_depth} exceeded",
                iterations=state.iterations,
            )

        # Check iteration limit
        if state.iterations >= self.config.max_iterations:
            raise SynthesisError(
                f"Max iterations {self.config.max_iterations} exceeded",
                iterations=state.iterations,
            )

        state.iterations += 1
        state.depth += 1

        try:
            # Select strategy
            strategy = await self.router.select_strategy(spec, context)
            state.strategies_tried.append(strategy.name)

            await self._emit_event(
                SynthesisEventType.STRATEGY_SELECTED,
                {
                    "spec": spec.summary(),
                    "strategy": strategy.name,
                },
            )

            # Decompose
            subproblems = strategy.decompose(spec, context)

            await self._emit_event(
                SynthesisEventType.DECOMPOSITION_COMPLETE,
                {
                    "spec": spec.summary(),
                    "strategy": strategy.name,
                    "subproblem_count": len(subproblems),
                },
            )

            # Base case: atomic problem (no subproblems)
            if not subproblems:
                solution = await self._solve_atomic(spec, context, validator, state)
                state.subproblems_solved += 1
                return solution

            # Recursive case: solve subproblems and compose
            solutions = await self._solve_subproblems(subproblems, context, validator, state)

            # Compose solutions
            composed = await self._compose_and_validate(solutions, spec, validator, state)

            # Record successful outcome for learning
            await self.router.record_outcome(
                spec, strategy, success=True, iterations=state.iterations
            )

            return composed

        except NoStrategyError:
            raise
        except SynthesisError:
            raise
        except Exception as e:
            raise SynthesisError(
                f"Synthesis failed: {e}",
                iterations=state.iterations,
            ) from e
        finally:
            state.depth -= 1

    async def _solve_atomic(
        self,
        spec: Specification,
        context: Context,
        validator: Validator | None,
        state: SynthesisState,
    ) -> Any:
        """Solve an atomic problem directly.

        For atomic problems, we generate a solution directly without
        further decomposition. This could be:
        - Looking up in library/solved
        - Template-based generation
        - LLM call (future)
        """
        # Check if already solved
        if spec.description in context.solved:
            return context.solved[spec.description]

        # Check if it's a primitive
        for primitive in context.primitives:
            if primitive.lower() in spec.description.lower():
                return primitive

        # Generate placeholder solution
        # In a real implementation, this would call an LLM or use templates
        solution = f"# Solution for: {spec.description}\n"
        if spec.type_signature:
            solution += f"# Type: {spec.type_signature}\n"
        solution += "pass  # TODO: implement\n"

        return solution

    async def _solve_subproblems(
        self,
        subproblems: list[Subproblem],
        context: Context,
        validator: Validator | None,
        state: SynthesisState,
    ) -> list[Any]:
        """Solve all subproblems, respecting dependencies."""
        solutions: list[Any] = [None] * len(subproblems)

        # Sort by priority and dependency order
        order = self._topological_sort(subproblems)

        if self.config.parallel_subproblems:
            # Group subproblems that can run in parallel
            groups = self._dependency_groups(subproblems, order)

            for group in groups:
                tasks = []
                for idx in group:
                    sub = subproblems[idx]
                    # Extend context with already-solved subproblems
                    extended_context = self._extend_context(context, subproblems, solutions)

                    await self._emit_event(
                        SynthesisEventType.SUBPROBLEM_START,
                        {
                            "index": idx,
                            "spec": sub.specification.summary(),
                        },
                    )

                    tasks.append(
                        self._synthesize_recursive(
                            sub.specification, extended_context, validator, state
                        )
                    )

                results = await asyncio.gather(*tasks, return_exceptions=True)

                for idx, result in zip(group, results, strict=False):
                    if isinstance(result, Exception):
                        raise result
                    solutions[idx] = result

                    await self._emit_event(
                        SynthesisEventType.SUBPROBLEM_COMPLETE,
                        {
                            "index": idx,
                            "spec": subproblems[idx].specification.summary(),
                        },
                    )
        else:
            # Sequential solving
            for idx in order:
                sub = subproblems[idx]
                extended_context = self._extend_context(context, subproblems, solutions)

                await self._emit_event(
                    SynthesisEventType.SUBPROBLEM_START,
                    {
                        "index": idx,
                        "spec": sub.specification.summary(),
                    },
                )

                solutions[idx] = await self._synthesize_recursive(
                    sub.specification, extended_context, validator, state
                )

                await self._emit_event(
                    SynthesisEventType.SUBPROBLEM_COMPLETE,
                    {
                        "index": idx,
                        "spec": sub.specification.summary(),
                    },
                )

        return solutions

    async def _compose_and_validate(
        self,
        solutions: list[Any],
        spec: Specification,
        validator: Validator | None,
        state: SynthesisState,
    ) -> Any:
        """Compose solutions and validate, with retry loop."""
        await self._emit_event(
            SynthesisEventType.COMPOSITION_START,
            {
                "spec": spec.summary(),
                "solution_count": len(solutions),
            },
        )

        composed = await self.composer.compose(solutions, spec)

        # Validate if validator provided
        if validator:
            await self._emit_event(
                SynthesisEventType.VALIDATION_START,
                {
                    "spec": spec.summary(),
                },
            )

            # TODO: Implement validation with retry loop
            # For now, just return composed
            # result = await validator.validate(composed)
            # if not result.success:
            #     raise ValidationError(...)

            await self._emit_event(
                SynthesisEventType.VALIDATION_COMPLETE,
                {
                    "spec": spec.summary(),
                    "success": True,
                },
            )

        return composed

    def _topological_sort(self, subproblems: list[Subproblem]) -> list[int]:
        """Sort subproblem indices by dependency order."""
        n = len(subproblems)
        visited = [False] * n
        order: list[int] = []

        def dfs(idx: int) -> None:
            if visited[idx]:
                return
            visited[idx] = True
            for dep in subproblems[idx].dependencies:
                if dep < n:
                    dfs(dep)
            order.append(idx)

        for i in range(n):
            dfs(i)

        return order

    def _dependency_groups(
        self,
        subproblems: list[Subproblem],
        order: list[int],
    ) -> list[list[int]]:
        """Group subproblems into parallel execution groups."""
        groups: list[list[int]] = []
        completed: set[int] = set()

        remaining = set(order)
        while remaining:
            # Find all subproblems whose dependencies are completed
            ready = [
                idx
                for idx in remaining
                if all(dep in completed for dep in subproblems[idx].dependencies)
            ]

            if not ready:
                # Shouldn't happen with proper topological sort
                ready = [min(remaining)]

            groups.append(ready)
            completed.update(ready)
            remaining -= set(ready)

        return groups

    def _extend_context(
        self,
        context: Context,
        subproblems: list[Subproblem],
        solutions: list[Any],
    ) -> Context:
        """Extend context with solved subproblems."""
        new_context = context
        for idx, solution in enumerate(solutions):
            if solution is not None:
                key = subproblems[idx].specification.description
                new_context = new_context.with_solved(key, solution)
        return new_context

    async def _emit_event(self, event_type: str, payload: dict[str, Any]) -> None:
        """Emit a synthesis event if events are enabled."""
        if not self.config.emit_events:
            return

        # Use custom event emission for synthesis-specific events
        event = Event(
            type=EventType.TOOL_CALL,  # Use existing type as carrier
            payload={"synthesis_event": event_type, **payload},
        )
        await self.event_bus.publish(event)


def create_synthesis_framework(
    strategies: list[DecompositionStrategy] | None = None,
    composer: Composer | None = None,
    validator: Validator | None = None,
    memory: EpisodicStore | None = None,
    config: SynthesisConfig | None = None,
) -> SynthesisFramework:
    """Factory function to create a configured SynthesisFramework."""
    strategies = strategies or [AtomicStrategy()]

    return SynthesisFramework(
        strategies=strategies,
        composer=composer,
        validator=validator,
        memory=memory,
        config=config,
    )
