"""Sketch-based code generator.

This generator synthesizes code by filling holes in user-provided templates.
Users write code with `??` placeholders (holes), and the synthesizer finds
values that satisfy the specification.

High-level approach:
1. Parse the sketch to find holes
2. Generate candidate values for each hole
3. Test combinations against examples
4. Return the first (or best) valid completion

Key concepts:
- Hole: A placeholder (`??` or `??type`) to be filled
- Sketch: Template code with holes
- Completion: Concrete values for all holes

Inspired by:
- Sketch (Solar-Lezama): Programmer-guided synthesis
- Rosette (Torlak): Solver-aided programming
- Synquid: Type-driven hole filling

Example:
    Sketch: "def f(x): return x ?? 2"  # ?? can be +, -, *, //
    Examples: f(3) == 6, f(0) == 0
    → Completed: "def f(x): return x * 2"

Limitations:
- Requires well-placed holes (user must know structure)
- Limited hole types (operators, constants, simple expressions)
- Exponential in number of holes (mitigated by pruning)
"""

from __future__ import annotations

import ast
import logging
import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from moss.synthesis.plugins.protocols import (
    GenerationCost,
    GenerationHints,
    GenerationResult,
    GeneratorMetadata,
    GeneratorType,
)

if TYPE_CHECKING:
    from moss.synthesis.types import Context, Specification

logger = logging.getLogger(__name__)


# =============================================================================
# Hole Types and Candidates
# =============================================================================


@dataclass(frozen=True)
class Hole:
    """A hole in a sketch to be filled."""

    position: int  # Character position in source
    hole_type: str | None  # Optional type hint (e.g., "int", "op")
    original: str  # Original hole text (e.g., "??", "??int")


# Candidate values for different hole types
HOLE_CANDIDATES: dict[str | None, list[str]] = {
    # Default: operators and small constants
    None: ["+", "-", "*", "//", "%", "==", "!=", "<", ">", "<=", ">=", "and", "or"],
    # Operators
    "op": ["+", "-", "*", "//", "%"],
    "cmp": ["==", "!=", "<", ">", "<=", ">="],
    "bool": ["and", "or", "not"],
    # Constants
    "int": ["0", "1", "-1", "2", "10", "100"],
    "str": ['""', '" "', '"_"', '","', '"."'],
    "bool_val": ["True", "False"],
    # Simple expressions
    "expr": ["x", "y", "x + 1", "x - 1", "len(x)", "x[0]", "x[-1]"],
    # Method calls
    "method": [".strip()", ".lower()", ".upper()", ".split()", ".join()"],
}


def get_candidates(hole: Hole, context: Context | None = None) -> list[str]:
    """Get candidate values for a hole."""
    # Check if type-hinted
    if hole.hole_type and hole.hole_type in HOLE_CANDIDATES:
        return HOLE_CANDIDATES[hole.hole_type]

    # Use defaults
    return HOLE_CANDIDATES[None]


# =============================================================================
# Sketch Parser
# =============================================================================


class SketchParser:
    """Parse sketches to find holes."""

    # Pattern for holes: ?? or ??type
    HOLE_PATTERN = re.compile(r"\?\?(\w*)")

    def parse(self, sketch: str) -> list[Hole]:
        """Find all holes in a sketch.

        Args:
            sketch: Source code with ?? holes

        Returns:
            List of Hole objects
        """
        holes: list[Hole] = []
        for match in self.HOLE_PATTERN.finditer(sketch):
            hole_type = match.group(1) if match.group(1) else None
            holes.append(
                Hole(
                    position=match.start(),
                    hole_type=hole_type,
                    original=match.group(0),
                )
            )
        return holes

    def fill(self, sketch: str, holes: list[Hole], values: list[str]) -> str:
        """Fill holes with values.

        Args:
            sketch: Original sketch
            holes: List of holes (must be in position order)
            values: Values to fill (same length as holes)

        Returns:
            Completed code
        """
        assert len(holes) == len(values)

        # Fill from end to start to preserve positions
        result = sketch
        for hole, value in reversed(list(zip(holes, values, strict=False))):
            result = result[: hole.position] + value + result[hole.position + len(hole.original) :]
        return result


# =============================================================================
# Sketch Synthesizer
# =============================================================================


class SketchSynthesizer:
    """Synthesize completions for sketches."""

    def __init__(self, max_candidates: int = 1000, max_holes: int = 5):
        self.max_candidates = max_candidates
        self.max_holes = max_holes
        self._parser = SketchParser()

    def synthesize(
        self,
        sketch: str,
        examples: list[tuple[Any, Any]],
        context: Context | None = None,
    ) -> list[tuple[str, float]]:
        """Synthesize completions for a sketch.

        Args:
            sketch: Code template with ?? holes
            examples: Input/output examples to validate against
            context: Optional context for additional candidates

        Returns:
            List of (completed_code, score) tuples, sorted by score
        """
        # Parse holes
        holes = self._parser.parse(sketch)

        if not holes:
            # No holes - validate the sketch as-is
            if self._validate(sketch, examples):
                return [(sketch, 1.0)]
            return []

        if len(holes) > self.max_holes:
            logger.warning(
                "Sketch has %d holes (max %d), limiting search",
                len(holes),
                self.max_holes,
            )
            holes = holes[: self.max_holes]

        # Get candidates for each hole
        candidates_per_hole = [get_candidates(h, context) for h in holes]

        # Enumerate combinations
        completions: list[tuple[str, float]] = []
        count = 0

        for values in self._enumerate_combinations(candidates_per_hole):
            if count >= self.max_candidates:
                break

            completed = self._parser.fill(sketch, holes, list(values))
            count += 1

            # Validate
            if self._validate(completed, examples):
                # Score: prefer simpler completions
                score = self._score_completion(values)
                completions.append((completed, score))

        # Sort by score (higher is better)
        completions.sort(key=lambda x: -x[1])
        return completions

    def _enumerate_combinations(
        self, candidates_per_hole: list[list[str]]
    ) -> list[tuple[str, ...]]:
        """Generate combinations of hole values."""
        import itertools

        return list(itertools.product(*candidates_per_hole))

    def _validate(self, code: str, examples: list[tuple[Any, Any]]) -> bool:
        """Validate completed code against examples."""
        if not examples:
            # No examples - just check syntax
            return self._is_valid_syntax(code)

        # Try to extract and execute the function
        try:
            # Check syntax first
            ast.parse(code)

            # Find function name
            func_name = self._extract_func_name(code)
            if not func_name:
                return False

            # Execute
            namespace: dict[str, Any] = {}
            exec(code, namespace)

            func = namespace.get(func_name)
            if not callable(func):
                return False

            # Test examples
            for inputs, expected in examples:
                if isinstance(inputs, tuple):
                    result = func(*inputs)
                else:
                    result = func(inputs)
                if result != expected:
                    return False

            return True

        except Exception:
            return False

    def _is_valid_syntax(self, code: str) -> bool:
        """Check if code has valid Python syntax."""
        try:
            ast.parse(code)
            return True
        except SyntaxError:
            return False

    def _extract_func_name(self, code: str) -> str | None:
        """Extract the first function name from code."""
        try:
            tree = ast.parse(code)
            for node in ast.walk(tree):
                if isinstance(node, ast.FunctionDef):
                    return node.name
        except Exception:
            pass
        return None

    def _score_completion(self, values: tuple[str, ...]) -> float:
        """Score a completion (higher = better).

        Prefers:
        - Shorter values
        - Simpler operators
        - Smaller constants
        """
        score = 1.0

        for v in values:
            # Penalty for length
            score -= len(v) * 0.01

            # Prefer simple operators
            if v in ["+", "-", "*", "=="]:
                score += 0.1
            elif v in ["//", "%", "!=", "<", ">"]:
                score += 0.05

            # Prefer small constants
            try:
                n = int(v)
                if abs(n) <= 1:
                    score += 0.1
                elif abs(n) <= 10:
                    score += 0.05
            except ValueError:
                pass

        return score


# =============================================================================
# Sketch Generator
# =============================================================================


class SketchGenerator:
    """Generate code by filling holes in user sketches.

    Users provide code templates with `??` placeholders, and the generator
    finds values that satisfy the specification.
    """

    def __init__(self, max_candidates: int = 500, max_holes: int = 4) -> None:
        """Initialize the generator.

        Args:
            max_candidates: Maximum combinations to try
            max_holes: Maximum holes to fill
        """
        self.max_candidates = max_candidates
        self.max_holes = max_holes
        self._synthesizer = SketchSynthesizer(max_candidates, max_holes)

    @property
    def metadata(self) -> GeneratorMetadata:
        """Return generator metadata."""
        return GeneratorMetadata(
            name="sketch",
            generator_type=GeneratorType.ENUMERATION,
            priority=13,  # Between template (10) and PBE (14)
            version="0.1.0",
            description="Fill holes in user-provided code sketches (Sketch/Rosette style)",
            supports_async=True,
            max_complexity=3,
        )

    def can_generate(self, spec: Specification, context: Context) -> bool:
        """Check if this generator can handle the specification.

        Requires:
        - A sketch (code template with ?? holes) in hints or description
        - Preferably examples for validation
        """
        # Check for sketch in description (look for ??)
        if spec.description and "??" in spec.description:
            return True

        # Check constraints for sketch indicator
        if spec.constraints:
            for constraint in spec.constraints:
                if "??" in constraint or "sketch" in constraint.lower():
                    return True

        return False

    async def generate(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None = None,
    ) -> GenerationResult:
        """Generate code by filling sketch holes.

        Args:
            spec: Specification with sketch template
            context: Available resources
            hints: Optional hints (may contain sketch)

        Returns:
            GenerationResult with completed code
        """
        # Check already solved
        if spec.description in context.solved:
            return GenerationResult(
                success=True,
                code=context.solved[spec.description],
                confidence=0.95,
                metadata={"source": "cached"},
            )

        # Extract sketch from spec
        sketch = self._extract_sketch(spec, hints)
        if not sketch:
            return GenerationResult(
                success=False,
                error="No sketch found (include code with ?? holes)",
                metadata={"source": "sketch"},
            )

        # Get examples
        examples = list(spec.examples) if spec.examples else []

        # Synthesize
        completions = self._synthesizer.synthesize(sketch, examples, context)

        if not completions:
            return GenerationResult(
                success=False,
                error="No valid completion found for sketch",
                confidence=0.0,
                metadata={"source": "sketch", "sketch": sketch},
            )

        # Best completion
        best_code, best_score = completions[0]

        # Alternatives
        alternatives = [code for code, _ in completions[1:3]]

        return GenerationResult(
            success=True,
            code=best_code,
            confidence=min(0.9, best_score),
            alternatives=alternatives,
            metadata={
                "source": "sketch",
                "completions_found": len(completions),
                "original_sketch": sketch,
            },
        )

    def estimate_cost(self, spec: Specification, context: Context) -> GenerationCost:
        """Estimate the cost of generation."""
        # Check holes count
        sketch = self._extract_sketch(spec, None)
        holes = 0
        if sketch:
            holes = sketch.count("??")

        # Exponential in holes
        complexity = min(2**holes, self.max_candidates)

        return GenerationCost(
            time_estimate_ms=50 + complexity * 2,
            token_estimate=0,
            complexity_score=holes + 1,
        )

    def _extract_sketch(
        self,
        spec: Specification,
        hints: GenerationHints | None,
    ) -> str | None:
        """Extract sketch from specification."""
        # Check hints first
        if hints and hints.preferred_style and "??" in hints.preferred_style:
            return hints.preferred_style

        # Check description
        if spec.description and "??" in spec.description:
            # Try to extract code block
            code_match = re.search(r"```(?:python)?\s*(.*?)\s*```", spec.description, re.DOTALL)
            if code_match:
                return code_match.group(1)

            # If description looks like code, use it
            if "def " in spec.description or "return " in spec.description:
                return spec.description

        # Check constraints
        if spec.constraints:
            for constraint in spec.constraints:
                if "??" in constraint:
                    return constraint

        return None
