"""Core types for the synthesis framework."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True)
class Specification:
    """What to synthesize.

    A specification describes the problem to solve, including:
    - Natural language description
    - Type information (if available)
    - Input/output examples
    - Test cases
    - Constraints
    """

    description: str
    type_signature: str | None = None
    examples: tuple[tuple[Any, Any], ...] = field(default_factory=tuple)
    tests: tuple[Any, ...] = field(default_factory=tuple)
    constraints: tuple[str, ...] = field(default_factory=tuple)
    metadata: dict[str, Any] = field(default_factory=dict)

    def summary(self) -> str:
        """Return a short summary for logging."""
        desc = self.description[:50] + "..." if len(self.description) > 50 else self.description
        return f"{desc} [{self.type_signature or 'untyped'}]"

    def with_examples(self, examples: list[tuple[Any, Any]]) -> Specification:
        """Return a new Specification with additional examples."""
        return Specification(
            description=self.description,
            type_signature=self.type_signature,
            examples=self.examples + tuple(examples),
            tests=self.tests,
            constraints=self.constraints,
            metadata=self.metadata,
        )

    def with_constraints(self, constraints: list[str]) -> Specification:
        """Return a new Specification with additional constraints."""
        return Specification(
            description=self.description,
            type_signature=self.type_signature,
            examples=self.examples,
            tests=self.tests,
            constraints=self.constraints + tuple(constraints),
            metadata=self.metadata,
        )


@dataclass(frozen=True)
class Context:
    """Available resources for synthesis.

    Context includes:
    - Primitives: built-in operations available
    - Library: available functions/classes/modules
    - Solved: previously solved subproblems (memoization)
    """

    primitives: tuple[str, ...] = field(default_factory=tuple)
    library: dict[str, Any] = field(default_factory=dict)
    solved: dict[str, Any] = field(default_factory=dict)

    def with_solved(self, key: str, solution: Any) -> Context:
        """Return a new Context with an additional solved subproblem."""
        new_solved = dict(self.solved)
        new_solved[key] = solution
        return Context(
            primitives=self.primitives,
            library=self.library,
            solved=new_solved,
        )

    def extend(self, primitives: list[str] | None = None) -> Context:
        """Return a new Context with extended primitives."""
        new_primitives = self.primitives
        if primitives:
            new_primitives = self.primitives + tuple(primitives)
        return Context(
            primitives=new_primitives,
            library=self.library,
            solved=self.solved,
        )


@dataclass(frozen=True)
class Subproblem:
    """A decomposed subproblem.

    When a problem is decomposed, each part becomes a Subproblem with:
    - Its own specification
    - Dependencies on other subproblems (by index)
    - Additional constraints
    - Priority for ordering
    """

    specification: Specification
    dependencies: tuple[int, ...] = field(default_factory=tuple)
    constraints: tuple[str, ...] = field(default_factory=tuple)
    priority: int = 0

    def summary(self) -> str:
        """Return a short summary for logging."""
        deps = f" [deps: {self.dependencies}]" if self.dependencies else ""
        return f"{self.specification.summary()}{deps}"


@dataclass
class SynthesisResult:
    """Result of a synthesis attempt.

    Contains the solution (if successful) along with metadata about
    the synthesis process.
    """

    success: bool
    solution: Any = None
    iterations: int = 0
    strategy_used: str | None = None
    subproblems_solved: int = 0
    error: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)

    def __str__(self) -> str:
        if self.success:
            return f"SynthesisResult(success, {self.iterations} iters, {self.strategy_used})"
        return f"SynthesisResult(failed: {self.error})"


class SynthesisError(Exception):
    """Base exception for synthesis failures."""

    def __init__(self, message: str, iterations: int = 0, partial_solution: Any = None):
        super().__init__(message)
        self.iterations = iterations
        self.partial_solution = partial_solution


class NoStrategyError(SynthesisError):
    """No strategy can handle the given specification."""

    pass


class DecompositionError(SynthesisError):
    """Strategy failed to decompose the problem."""

    pass


class CompositionError(SynthesisError):
    """Failed to compose subproblem solutions."""

    pass


class ValidationError(SynthesisError):
    """Solution failed validation."""

    pass
