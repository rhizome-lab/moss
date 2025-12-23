"""Core synthesis framework.

The SynthesisFramework orchestrates the recursive decomposition and
composition process, integrating with moss primitives (validation,
shadow git, memory, events).

Supports pluggable components:
- Code generators (placeholder, template, LLM)
- Synthesis validators (test, type check)
- Library plugins (abstraction management)
"""

from __future__ import annotations

import asyncio
import logging
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

from moss_orchestration.events import Event, EventBus, EventType
from moss_orchestration.validators import Validator

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
    ValidationError,
)

if TYPE_CHECKING:
    from moss_context.memory import EpisodicStore
    from moss_orchestration.shadow_git import ShadowGit

    from .plugins import CodeGenerator, LibraryPlugin, SynthesisValidator

logger = logging.getLogger(__name__)


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

    # Validation retry loop settings
    max_validation_retries: int = 3
    validation_timeout_ms: int = 30000

    # Generator selection
    prefer_templates: bool = True  # Use templates before placeholder


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

    Plugin support:
    - Code generators (placeholder, template, LLM)
    - Synthesis validators (test, type check)
    - Library plugins (abstraction management)
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
        # Plugin support
        generator: CodeGenerator | None = None,
        synthesis_validators: list[SynthesisValidator] | None = None,
        library: LibraryPlugin | None = None,
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

        # Plugin components (lazy initialization)
        self._generator = generator
        self._synthesis_validators = synthesis_validators
        self._library = library
        self._plugins_initialized = False

    def _ensure_plugins(self) -> None:
        """Ensure plugins are initialized from registry if not provided."""
        if self._plugins_initialized:
            return

        from .plugins import get_synthesis_registry

        registry = get_synthesis_registry()

        # Use registry defaults if not provided
        if self._generator is None:
            generators = registry.generators.get_all()
            if generators:
                # Use highest priority generator
                self._generator = generators[0]

        if self._synthesis_validators is None:
            self._synthesis_validators = registry.validators.get_all()

        if self._library is None:
            libraries = registry.libraries.get_all()
            if libraries:
                self._library = libraries[0]

        self._plugins_initialized = True

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
            # Select strategy - if no strategy matches, treat as atomic
            try:
                strategy = await self.router.select_strategy(spec, context)
                state.strategies_tried.append(strategy.name)
            except NoStrategyError:
                # No decomposition strategy - solve atomically
                logger.debug("No strategy for %s, solving atomically", spec.summary())
                solution = await self._solve_atomic(spec, context, validator, state)
                state.subproblems_solved += 1
                return solution

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
        """Solve an atomic problem directly using code generator plugins.

        For atomic problems, we generate a solution directly without
        further decomposition. Uses the plugin system to select and
        run the appropriate code generator.

        Generation order:
        1. Check if already solved in context
        2. Check library for matching abstractions
        3. Use code generator plugin (template, LLM, etc.)
        4. Validate and retry if needed
        """
        self._ensure_plugins()

        # Check if already solved
        if spec.description in context.solved:
            return context.solved[spec.description]

        # Check if it's a primitive
        for primitive in context.primitives:
            if primitive.lower() in spec.description.lower():
                return primitive

        # Check library for matching abstractions
        if self._library is not None:
            abstractions = self._library.search_abstractions(spec, context)
            if abstractions:
                best_abstraction, score = abstractions[0]
                if score > 0.7:  # High confidence match
                    self._library.record_usage(best_abstraction)
                    logger.debug(
                        "Using abstraction '%s' (score=%.2f) for: %s",
                        best_abstraction.name,
                        score,
                        spec.summary(),
                    )
                    return best_abstraction.code

        # Build generation hints from library
        from .plugins import GenerationHints

        hints = GenerationHints()
        if self._library is not None:
            relevant_abstractions = self._library.search_abstractions(spec, context)
            hints = GenerationHints(
                abstractions=[a for a, _ in relevant_abstractions[:5]],
                examples=list(spec.examples),
                constraints=list(spec.constraints),
            )

        # Generate code using plugin
        if self._generator is not None:
            result = await self._generator.generate(spec, context, hints)

            if result.success and result.code:
                logger.debug(
                    "Generated code using %s (confidence=%.2f)",
                    self._generator.metadata.name,
                    result.confidence,
                )
                return result.code
            else:
                logger.warning(
                    "Generator %s failed: %s",
                    self._generator.metadata.name,
                    result.error,
                )

        # Fallback: generate placeholder
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
        """Compose solutions and validate, with retry loop.

        Implements the validation retry loop:
        1. Compose solutions
        2. Run synthesis validators (if available)
        3. On failure, attempt to fix and retry
        4. Fall back to legacy validator if synthesis validators unavailable
        """
        self._ensure_plugins()

        await self._emit_event(
            SynthesisEventType.COMPOSITION_START,
            {
                "spec": spec.summary(),
                "solution_count": len(solutions),
            },
        )

        composed = await self.composer.compose(solutions, spec)

        # Run validation with retry loop
        if self._synthesis_validators or validator:
            composed = await self._validate_with_retry(composed, spec, validator, state)

        return composed

    async def _validate_with_retry(
        self,
        code: str,
        spec: Specification,
        legacy_validator: Validator | None,
        state: SynthesisState,
    ) -> str:
        """Validate code with retry loop.

        If validation fails, attempts to regenerate and retry up to
        max_validation_retries times.

        Args:
            code: The composed code to validate
            spec: The specification
            legacy_validator: Optional legacy moss validator
            state: Current synthesis state

        Returns:
            Validated code (possibly modified via retries)

        Raises:
            ValidationError: If validation fails after all retries
        """
        from .plugins import GenerationHints

        current_code = code
        retries = 0
        issues: list[str] = []

        while retries <= self.config.max_validation_retries:
            await self._emit_event(
                SynthesisEventType.VALIDATION_START,
                {
                    "spec": spec.summary(),
                    "retry": retries,
                },
            )

            # Run synthesis validators (plugin-based)
            all_passed = True
            validation_issues: list[str] = []

            if self._synthesis_validators:
                for sv in self._synthesis_validators:
                    if sv.can_validate(spec, current_code):
                        try:
                            result = await sv.validate(spec, current_code, Context())

                            if not result.success:
                                all_passed = False
                                validation_issues.extend(result.issues)
                                logger.debug(
                                    "Validator %s failed: %s",
                                    sv.metadata.name,
                                    result.issues,
                                )

                                # Try to get counterexample for better error reporting
                                if sv.metadata.can_generate_counterexample:
                                    counterexample = await sv.generate_counterexample(
                                        spec, current_code, Context()
                                    )
                                    if counterexample:
                                        ce_in, ce_out = counterexample
                                        validation_issues.append(
                                            f"Counterexample: {ce_in!r} -> expected {ce_out!r}"
                                        )
                            else:
                                logger.debug(
                                    "Validator %s passed (%d/%d checks)",
                                    sv.metadata.name,
                                    result.passed_checks,
                                    result.total_checks,
                                )

                        except (OSError, ValueError, RuntimeError) as e:
                            logger.warning("Validator %s error: %s", sv.metadata.name, e)
                            # Don't fail on validator errors, continue with others

            # Run legacy validator if provided and no synthesis validators
            if legacy_validator and not self._synthesis_validators:
                try:
                    # Legacy validators work with files, so write code to temp file
                    import tempfile

                    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
                        f.write(current_code)
                        temp_path = Path(f.name)

                    try:
                        legacy_result = await legacy_validator.validate(temp_path)
                        if not legacy_result.success:
                            all_passed = False
                            # Extract error messages from issues
                            for issue in legacy_result.errors:
                                validation_issues.append(str(issue))
                            if not legacy_result.errors:
                                validation_issues.append("Validation failed")
                    finally:
                        temp_path.unlink(missing_ok=True)
                except (OSError, ValueError) as e:
                    logger.warning("Legacy validator error: %s", e)

            # Check results
            if all_passed:
                await self._emit_event(
                    SynthesisEventType.VALIDATION_COMPLETE,
                    {
                        "spec": spec.summary(),
                        "success": True,
                        "retries": retries,
                    },
                )
                return current_code

            # Validation failed - attempt retry
            issues = validation_issues
            retries += 1

            if retries > self.config.max_validation_retries:
                break

            logger.info(
                "Validation failed, retry %d/%d: %s",
                retries,
                self.config.max_validation_retries,
                issues[:3],
            )

            # Attempt to regenerate with issues as hints
            if self._generator is not None:
                hints = GenerationHints(
                    constraints=[f"Fix: {issue}" for issue in issues[:5]],
                    examples=list(spec.examples),
                )

                gen_result = await self._generator.generate(spec, Context(), hints)
                if gen_result.success and gen_result.code:
                    current_code = gen_result.code
                    logger.debug("Regenerated code for retry")
                else:
                    # Generator failed, can't improve
                    break
            else:
                # No generator to retry with
                break

        # All retries exhausted
        await self._emit_event(
            SynthesisEventType.VALIDATION_COMPLETE,
            {
                "spec": spec.summary(),
                "success": False,
                "retries": retries,
                "issues": issues[:5],
            },
        )

        raise ValidationError(
            f"Validation failed after {retries} retries: {'; '.join(issues[:3])}",
            iterations=state.iterations,
            partial_solution=current_code,
        )

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
    # Plugin support
    generator: CodeGenerator | None = None,
    synthesis_validators: list[SynthesisValidator] | None = None,
    library: LibraryPlugin | None = None,
) -> SynthesisFramework:
    """Factory function to create a configured SynthesisFramework.

    Args:
        strategies: Decomposition strategies to use
        composer: Composer for combining solutions
        validator: Legacy moss validator
        memory: Episodic memory store
        config: Framework configuration
        generator: Code generator plugin (or use registry default)
        synthesis_validators: Synthesis validator plugins (or use registry default)
        library: Library plugin for abstractions (or use registry default)

    Returns:
        Configured SynthesisFramework instance
    """
    strategies = strategies or [AtomicStrategy()]

    return SynthesisFramework(
        strategies=strategies,
        composer=composer,
        validator=validator,
        memory=memory,
        config=config,
        generator=generator,
        synthesis_validators=synthesis_validators,
        library=library,
    )
