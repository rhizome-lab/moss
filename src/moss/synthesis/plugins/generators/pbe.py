"""Programming by Example (PBE) code generator.

This generator synthesizes code from input/output examples using a
domain-specific language (DSL) approach, inspired by FlashFill and PROSE.

High-level approach:
1. Analyze examples to find patterns (constants, positions, substrings)
2. Build a version space of consistent programs
3. Pick the most general program that explains all examples

Key concepts:
- Version space: Set of all programs consistent with examples
- Witness functions: Guide search based on example structure
- DSL: Domain-specific language for string transformations

Inspired by:
- FlashFill (Gulwani): String transformation by example
- PROSE (Microsoft): General framework for PBE
- BlinkFill: Extension for semi-structured data

Limitations:
- Primarily for string transformations
- Requires good examples that cover edge cases
- May produce overfitting programs without diverse examples

Example:
    Input: ("John", "Doe")  Output: "Doe, J."
    Input: ("Jane", "Smith") Output: "Smith, J."
    → Synthesized: f"{last}, {first[0]}."
"""

from __future__ import annotations

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
# DSL for String Transformations
# =============================================================================


@dataclass(frozen=True)
class DslExpr:
    """Base class for DSL expressions."""

    def evaluate(self, inputs: dict[str, Any]) -> Any:
        """Evaluate this expression with given inputs."""
        raise NotImplementedError

    def to_python(self, params: list[str]) -> str:
        """Convert to Python code."""
        raise NotImplementedError


@dataclass(frozen=True)
class ConstStr(DslExpr):
    """Constant string expression."""

    value: str

    def evaluate(self, inputs: dict[str, Any]) -> str:
        return self.value

    def to_python(self, params: list[str]) -> str:
        return repr(self.value)


@dataclass(frozen=True)
class InputRef(DslExpr):
    """Reference to an input parameter."""

    index: int

    def evaluate(self, inputs: dict[str, Any]) -> Any:
        key = f"arg{self.index}"
        return inputs.get(key, "")

    def to_python(self, params: list[str]) -> str:
        if self.index < len(params):
            return params[self.index]
        return f"arg{self.index}"


@dataclass(frozen=True)
class SubStr(DslExpr):
    """Substring extraction."""

    source: DslExpr
    start: int | None  # None means from beginning
    end: int | None  # None means to end

    def evaluate(self, inputs: dict[str, Any]) -> str:
        s = str(self.source.evaluate(inputs))
        return s[self.start : self.end]

    def to_python(self, params: list[str]) -> str:
        src = self.source.to_python(params)
        if self.start is None and self.end is None:
            return src
        elif self.start is None:
            return f"{src}[:{self.end}]"
        elif self.end is None:
            return f"{src}[{self.start}:]"
        else:
            return f"{src}[{self.start}:{self.end}]"


@dataclass(frozen=True)
class Concat(DslExpr):
    """Concatenation of expressions."""

    parts: tuple[DslExpr, ...]

    def evaluate(self, inputs: dict[str, Any]) -> str:
        return "".join(str(p.evaluate(inputs)) for p in self.parts)

    def to_python(self, params: list[str]) -> str:
        if len(self.parts) == 1:
            return self.parts[0].to_python(params)
        # Use f-string if simple, otherwise + concatenation
        parts_code = [p.to_python(params) for p in self.parts]
        # Check if can use simple f-string
        if all(isinstance(p, (ConstStr, InputRef, SubStr)) for p in self.parts):
            fstring_parts = []
            for p, code in zip(self.parts, parts_code, strict=False):
                if isinstance(p, ConstStr):
                    fstring_parts.append(p.value)
                else:
                    fstring_parts.append("{" + code + "}")
            return 'f"' + "".join(fstring_parts) + '"'
        return " + ".join(parts_code)


@dataclass(frozen=True)
class Upper(DslExpr):
    """Convert to uppercase."""

    source: DslExpr

    def evaluate(self, inputs: dict[str, Any]) -> str:
        return str(self.source.evaluate(inputs)).upper()

    def to_python(self, params: list[str]) -> str:
        return f"{self.source.to_python(params)}.upper()"


@dataclass(frozen=True)
class Lower(DslExpr):
    """Convert to lowercase."""

    source: DslExpr

    def evaluate(self, inputs: dict[str, Any]) -> str:
        return str(self.source.evaluate(inputs)).lower()

    def to_python(self, params: list[str]) -> str:
        return f"{self.source.to_python(params)}.lower()"


@dataclass(frozen=True)
class Replace(DslExpr):
    """Replace occurrences in string."""

    source: DslExpr
    old: str
    new: str

    def evaluate(self, inputs: dict[str, Any]) -> str:
        return str(self.source.evaluate(inputs)).replace(self.old, self.new)

    def to_python(self, params: list[str]) -> str:
        return f"{self.source.to_python(params)}.replace({self.old!r}, {self.new!r})"


# =============================================================================
# PBE Synthesizer
# =============================================================================


@dataclass
class Candidate:
    """A candidate program with its score."""

    expr: DslExpr
    score: float = 0.0  # Higher is better
    examples_matched: int = 0


class PBESynthesizer:
    """Synthesize programs from input/output examples."""

    def __init__(self, max_candidates: int = 100, max_concat_parts: int = 5):
        self.max_candidates = max_candidates
        self.max_concat_parts = max_concat_parts

    def synthesize(
        self,
        examples: list[tuple[Any, Any]],
        num_inputs: int,
    ) -> list[Candidate]:
        """Synthesize programs that explain all examples.

        Args:
            examples: List of (inputs, expected_output) pairs
            num_inputs: Number of input arguments

        Returns:
            List of candidate programs, sorted by score (best first)
        """
        if not examples:
            return []

        candidates: list[Candidate] = []

        # Generate atomic expressions (constants, input refs)
        atoms = self._generate_atoms(examples, num_inputs)

        # Try each atom alone
        for atom in atoms:
            cand = self._evaluate_candidate(atom, examples)
            if cand.examples_matched > 0:
                candidates.append(cand)

        # Try simple transformations
        for atom in atoms:
            # Upper/lower
            for expr in [Upper(atom), Lower(atom)]:
                cand = self._evaluate_candidate(expr, examples)
                if cand.examples_matched > 0:
                    candidates.append(cand)

            # Substrings
            for start, end in [(0, 1), (1, None), (0, -1), (-1, None)]:
                expr = SubStr(atom, start, end)
                cand = self._evaluate_candidate(expr, examples)
                if cand.examples_matched > 0:
                    candidates.append(cand)

        # Try concatenations of atoms and constants
        candidates.extend(self._search_concats(atoms, examples))

        # Filter to only those matching all examples
        perfect = [c for c in candidates if c.examples_matched == len(examples)]
        if perfect:
            # Sort by simplicity (lower score = simpler)
            perfect.sort(key=lambda c: -c.score)
            return perfect[: self.max_candidates]

        # Return best partial matches
        candidates.sort(key=lambda c: (-c.examples_matched, -c.score))
        return candidates[: self.max_candidates]

    def _generate_atoms(self, examples: list[tuple[Any, Any]], num_inputs: int) -> list[DslExpr]:
        """Generate atomic expressions from examples."""
        atoms: list[DslExpr] = []

        # Input references
        for i in range(num_inputs):
            atoms.append(InputRef(i))

        # Constants found in outputs
        for _, output in examples:
            if isinstance(output, str):
                # Find constant substrings that appear in all outputs
                for match in re.finditer(r"[^\w]+", output):
                    const = match.group()
                    if len(const) <= 5:  # Don't add long constants
                        const_expr = ConstStr(const)
                        if const_expr not in atoms:
                            atoms.append(const_expr)

        # Common punctuation
        for p in [", ", ". ", " ", "-", "_", ":", "/"]:
            if ConstStr(p) not in atoms:
                atoms.append(ConstStr(p))

        return atoms

    def _search_concats(
        self, atoms: list[DslExpr], examples: list[tuple[Any, Any]]
    ) -> list[Candidate]:
        """Search for concatenation expressions."""
        candidates: list[Candidate] = []

        # Try pairs and triples
        for length in range(2, min(self.max_concat_parts + 1, len(atoms) + 1)):
            for combo in self._generate_concat_combos(atoms, length):
                expr = Concat(tuple(combo))
                cand = self._evaluate_candidate(expr, examples)
                if cand.examples_matched > 0:
                    candidates.append(cand)

                    if len(candidates) > self.max_candidates:
                        return candidates

        return candidates

    def _generate_concat_combos(self, atoms: list[DslExpr], length: int) -> list[list[DslExpr]]:
        """Generate combinations for concatenation."""
        if length <= 2:
            return list(list(c) for c in __import__("itertools").product(atoms, repeat=length))

        # For longer lengths, be more selective
        combos: list[list[DslExpr]] = []
        inputs = [a for a in atoms if isinstance(a, InputRef)]
        consts = [a for a in atoms if isinstance(a, ConstStr)]

        # Patterns like: input, const, input
        for inp1 in inputs:
            for const in consts:
                for inp2 in inputs:
                    combos.append([inp1, const, inp2])

        # Patterns like: substr, const, substr
        for inp1 in inputs:
            for const in consts:
                for inp2 in inputs:
                    combos.append([SubStr(inp1, 0, 1), const, inp2])
                    combos.append([inp1, const, SubStr(inp2, 0, 1)])

        return combos[: self.max_candidates]

    def _evaluate_candidate(self, expr: DslExpr, examples: list[tuple[Any, Any]]) -> Candidate:
        """Evaluate a candidate against examples."""
        matched = 0
        for inputs, expected in examples:
            try:
                # Build input dict
                if isinstance(inputs, tuple):
                    input_dict = {f"arg{i}": v for i, v in enumerate(inputs)}
                else:
                    input_dict = {"arg0": inputs}

                result = expr.evaluate(input_dict)
                if str(result) == str(expected):
                    matched += 1
            except Exception:
                pass

        # Score: prefer simpler expressions
        score = matched / len(examples) if examples else 0
        # Penalty for complexity
        complexity = self._expr_complexity(expr)
        score -= complexity * 0.01

        return Candidate(expr=expr, score=score, examples_matched=matched)

    def _expr_complexity(self, expr: DslExpr) -> int:
        """Compute complexity of an expression."""
        if isinstance(expr, (ConstStr, InputRef)):
            return 1
        elif isinstance(expr, (SubStr, Upper, Lower)):
            return 1 + self._expr_complexity(expr.source)
        elif isinstance(expr, Replace):
            return 2 + self._expr_complexity(expr.source)
        elif isinstance(expr, Concat):
            return sum(self._expr_complexity(p) for p in expr.parts)
        return 1


# =============================================================================
# PBE Generator
# =============================================================================


class PBEGenerator:
    """Generate code using Programming by Example.

    Uses a DSL-based approach to synthesize string transformations
    from input/output examples.
    """

    def __init__(self, max_candidates: int = 50, max_concat_parts: int = 4) -> None:
        """Initialize the generator.

        Args:
            max_candidates: Maximum candidate programs to evaluate
            max_concat_parts: Maximum parts in concatenation
        """
        self.max_candidates = max_candidates
        self.max_concat_parts = max_concat_parts
        self._synthesizer = PBESynthesizer(max_candidates, max_concat_parts)

    @property
    def metadata(self) -> GeneratorMetadata:
        """Return generator metadata."""
        return GeneratorMetadata(
            name="pbe",
            generator_type=GeneratorType.PBE,
            priority=14,  # Between enumeration (5) and SMT (15)
            version="0.1.0",
            description="Programming by Example synthesis (FlashFill/PROSE style)",
            supports_async=True,
            max_complexity=3,  # Best for simple transformations
        )

    def can_generate(self, spec: Specification, context: Context) -> bool:
        """Check if this generator can handle the specification.

        Requires:
        - Input/output examples
        - Output type compatible with strings
        """
        # Need examples
        if not spec.examples:
            return False

        # Check if examples look like string transformations
        for _inputs, output in spec.examples:
            if isinstance(output, str):
                return True

        return False

    async def generate(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None = None,
    ) -> GenerationResult:
        """Generate code from examples.

        Args:
            spec: Specification with examples
            context: Available resources
            hints: Optional generation hints

        Returns:
            GenerationResult with synthesized code
        """
        # Check already solved
        if spec.description in context.solved:
            return GenerationResult(
                success=True,
                code=context.solved[spec.description],
                confidence=0.95,
                metadata={"source": "cached"},
            )

        # Need examples
        if not spec.examples:
            return GenerationResult(
                success=False,
                error="PBE requires input/output examples",
                metadata={"source": "pbe"},
            )

        # Determine number of inputs
        first_inputs = spec.examples[0][0]
        if isinstance(first_inputs, tuple):
            num_inputs = len(first_inputs)
        else:
            num_inputs = 1

        # Synthesize
        candidates = self._synthesizer.synthesize(
            list(spec.examples),
            num_inputs,
        )

        if not candidates:
            return GenerationResult(
                success=False,
                error="No program found that explains the examples",
                confidence=0.0,
                metadata={"source": "pbe"},
            )

        # Get best candidate
        best = candidates[0]

        # Check if it matches all examples
        all_examples = len(spec.examples)
        if best.examples_matched < all_examples:
            return GenerationResult(
                success=False,
                error=f"Best candidate matches {best.examples_matched}/{all_examples} examples",
                confidence=best.score,
                metadata={"source": "pbe", "partial_match": True},
            )

        # Generate code
        code = self._candidate_to_code(spec, num_inputs, best)

        # Generate alternatives
        alternatives = [
            self._candidate_to_code(spec, num_inputs, c)
            for c in candidates[1:3]
            if c.examples_matched == all_examples
        ]

        return GenerationResult(
            success=True,
            code=code,
            confidence=min(0.9, best.score + 0.1),
            alternatives=alternatives,
            metadata={
                "source": "pbe",
                "candidates_evaluated": len(candidates),
                "examples_matched": best.examples_matched,
            },
        )

    def estimate_cost(self, spec: Specification, context: Context) -> GenerationCost:
        """Estimate the cost of generation."""
        num_examples = len(spec.examples) if spec.examples else 0
        return GenerationCost(
            time_estimate_ms=50 + num_examples * 20,
            token_estimate=0,  # No LLM tokens
            complexity_score=num_examples,
        )

    def _candidate_to_code(
        self,
        spec: Specification,
        num_inputs: int,
        candidate: Candidate,
    ) -> str:
        """Convert a candidate to Python code."""
        # Extract function name
        func_name = self._extract_function_name(spec.description)

        # Generate parameters
        param_names = [f"arg{i}" for i in range(num_inputs)]
        params = ", ".join(param_names)

        # Get expression code
        expr_code = candidate.expr.to_python(param_names)

        lines = [
            f"def {func_name}({params}) -> str:",
            f'    """{spec.description}"""',
            f"    return {expr_code}",
        ]

        return "\n".join(lines)

    def _extract_function_name(self, description: str) -> str:
        """Extract a function name from description."""
        patterns = [
            r"function\s+(\w+)",
            r"def\s+(\w+)",
            r"(\w+)\s+function",
        ]
        for pattern in patterns:
            match = re.search(pattern, description, re.IGNORECASE)
            if match:
                return match.group(1)

        words = re.findall(r"\w+", description.lower())[:3]
        return "_".join(words) if words else "transform"
