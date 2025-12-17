"""Solution composition for synthesis.

Composers combine solutions to subproblems into a complete solution
for the original problem.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .types import Specification


class Composer(ABC):
    """Base class for solution composition.

    A composer takes solutions to subproblems and combines them into
    a solution for the parent problem.
    """

    @property
    @abstractmethod
    def name(self) -> str:
        """Composer name for logging."""
        ...

    @abstractmethod
    async def compose(
        self,
        solutions: list[Any],
        spec: Specification,
    ) -> Any:
        """Combine subproblem solutions into a final solution.

        Args:
            solutions: Solutions to subproblems (in dependency order)
            spec: Original specification

        Returns:
            Combined solution

        Raises:
            CompositionError: If composition fails
        """
        ...


class SequentialComposer(Composer):
    """Compose solutions by concatenating them in order.

    This is the simplest composer - it assumes solutions can be
    combined by joining them together.
    """

    @property
    def name(self) -> str:
        return "sequential"

    async def compose(
        self,
        solutions: list[Any],
        spec: Specification,
    ) -> Any:
        """Concatenate solutions.

        If solutions are strings, join with newlines.
        If solutions are lists, flatten.
        Otherwise, return as tuple.
        """
        if not solutions:
            return None

        # All strings -> join with newlines
        if all(isinstance(s, str) for s in solutions):
            return "\n\n".join(solutions)

        # All lists -> flatten
        if all(isinstance(s, list) for s in solutions):
            result: list[Any] = []
            for s in solutions:
                result.extend(s)
            return result

        # Mixed or other -> return as tuple
        return tuple(solutions)


class FunctionComposer(Composer):
    """Compose solutions as function compositions.

    Chains solutions together where output of one becomes input to next.
    """

    @property
    def name(self) -> str:
        return "function"

    async def compose(
        self,
        solutions: list[Any],
        spec: Specification,
    ) -> Any:
        """Compose as chained function calls.

        Returns a lambda that applies each solution in sequence.
        """
        if not solutions:
            return lambda x: x

        if len(solutions) == 1:
            return solutions[0]

        # Create composition: f3(f2(f1(x)))
        def composed(*args: Any, **kwargs: Any) -> Any:
            result = solutions[0](*args, **kwargs)
            for fn in solutions[1:]:
                result = fn(result)
            return result

        return composed


class CodeComposer(Composer):
    """Compose code solutions into a single module.

    Combines code snippets with proper imports and structure.
    """

    @property
    def name(self) -> str:
        return "code"

    async def compose(
        self,
        solutions: list[Any],
        spec: Specification,
    ) -> Any:
        """Compose code snippets into a module.

        Handles:
        - Import deduplication
        - Function ordering (dependencies first)
        - Docstring generation
        """
        if not solutions:
            return ""

        # Extract imports and code blocks
        imports: set[str] = set()
        code_blocks: list[str] = []

        for solution in solutions:
            if not isinstance(solution, str):
                solution = str(solution)

            lines = solution.split("\n")
            solution_imports: list[str] = []
            solution_code: list[str] = []

            for line in lines:
                stripped = line.strip()
                if stripped.startswith("import ") or stripped.startswith("from "):
                    solution_imports.append(stripped)
                else:
                    solution_code.append(line)

            imports.update(solution_imports)
            code = "\n".join(solution_code).strip()
            if code:
                code_blocks.append(code)

        # Build final module
        parts: list[str] = []

        # Module docstring
        if spec.description:
            parts.append(f'"""{spec.description}"""')
            parts.append("")

        # Sorted imports
        if imports:
            sorted_imports = sorted(imports)
            parts.extend(sorted_imports)
            parts.append("")

        # Code blocks
        parts.extend(code_blocks)

        return "\n".join(parts)
