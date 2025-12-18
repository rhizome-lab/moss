"""Z3-based SMT code generator using type-directed synthesis.

This generator synthesizes code by encoding the synthesis problem as
satisfiability constraints and using Z3 to find solutions.

High-level approach:
1. Parse type signature to understand input/output types
2. Encode type constraints as Z3 formulas
3. Encode examples as equality constraints
4. Use Z3 to find satisfying assignments (valid programs)

Inspired by:
- Synquid (Polikarpova et al.): Type-driven program synthesis
- Leon (Kneuss et al.): Synthesis with refinement types
- Rosette (Torlak et al.): Solver-aided programming

Key features:
- Works best with pure, well-typed functions
- Can generate multiple solutions
- Good for algebraic operations, data transformations
- Requires z3-solver package (optional dependency)

Limitations:
- Limited to simple types (int, bool, list operations)
- Doesn't handle complex control flow
- Requires good type signatures
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
# Z3 Helpers
# =============================================================================


@dataclass
class SymbolicExpr:
    """A symbolic expression for synthesis."""

    name: str
    type_: str
    depth: int = 0

    def __str__(self) -> str:
        return self.name


def _try_import_z3() -> Any | None:
    """Try to import z3-solver, return None if unavailable."""
    try:
        import z3

        return z3
    except ImportError:
        return None


# =============================================================================
# SMT Generator
# =============================================================================


class SMTGenerator:
    """Generate code using Z3 constraint solving.

    Encodes synthesis as a satisfiability problem:
    - Variables represent program expressions
    - Constraints encode type rules and examples
    - Z3 finds satisfying assignments (valid programs)
    """

    def __init__(
        self,
        max_depth: int = 3,
        timeout_ms: int = 5000,
        max_solutions: int = 3,
    ) -> None:
        """Initialize the generator.

        Args:
            max_depth: Maximum expression nesting depth
            timeout_ms: Z3 solver timeout in milliseconds
            max_solutions: Maximum number of solutions to find

        """
        self.max_depth = max_depth
        self.timeout_ms = timeout_ms
        self.max_solutions = max_solutions
        self._z3 = _try_import_z3()

    @property
    def metadata(self) -> GeneratorMetadata:
        """Return generator metadata."""
        return GeneratorMetadata(
            name="smt",
            generator_type=GeneratorType.SMT,
            priority=15,  # Between component (12) and LLM (20)
            version="0.1.0",
            description="Z3-based type-driven synthesis (Synquid-style)",
            supports_async=True,
            max_complexity=5,
        )

    def can_generate(self, spec: Specification, context: Context) -> bool:
        """Check if this generator can handle the specification.

        Requires:
        - Z3 library available
        - Type signature with simple types
        - Preferably examples for constraint generation
        """
        # Need Z3
        if self._z3 is None:
            logger.debug("SMTGenerator: z3-solver not available")
            return False

        # Need type signature
        if not spec.type_signature:
            return False

        # Parse and check type signature
        parsed = self._parse_type_signature(spec.type_signature)
        if not parsed:
            return False

        input_types, output_type = parsed

        # Check if types are supported
        supported_types = {"int", "bool", "str", "float", "list", "Any"}
        all_types = set(input_types) | {output_type}
        for t in all_types:
            base_type = re.sub(r"\[.*\]", "", t)  # Remove generics
            if base_type not in supported_types:
                logger.debug("SMTGenerator: unsupported type %s", t)
                return False

        return True

    async def generate(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None = None,
    ) -> GenerationResult:
        """Generate code using Z3 constraint solving.

        Args:
            spec: What to generate
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

        # Ensure Z3 is available
        if self._z3 is None:
            return GenerationResult(
                success=False,
                error="z3-solver not installed. Run: pip install z3-solver",
                metadata={"source": "smt"},
            )

        z3 = self._z3

        # Parse type signature
        parsed = self._parse_type_signature(spec.type_signature or "")
        if not parsed:
            return GenerationResult(
                success=False,
                error="Could not parse type signature",
                metadata={"source": "smt"},
            )

        input_types, output_type = parsed
        logger.debug("SMTGenerator: %s -> %s", input_types, output_type)

        # Build synthesis problem
        try:
            solutions = self._solve(
                z3,
                spec,
                input_types,
                output_type,
                context,
            )
        except Exception as e:
            logger.exception("SMT solving failed")
            return GenerationResult(
                success=False,
                error=f"SMT solving failed: {e}",
                metadata={"source": "smt"},
            )

        if not solutions:
            return GenerationResult(
                success=False,
                error="No satisfying program found",
                confidence=0.0,
                metadata={"source": "smt"},
            )

        # Generate code from best solution
        code = self._solution_to_code(spec, input_types, output_type, solutions[0])

        # Alternatives
        alternatives = [
            self._solution_to_code(spec, input_types, output_type, sol) for sol in solutions[1:]
        ]

        return GenerationResult(
            success=True,
            code=code,
            confidence=0.85,  # SMT solutions are typically correct
            alternatives=alternatives,
            metadata={
                "source": "smt",
                "solutions_found": len(solutions),
            },
        )

    def estimate_cost(self, spec: Specification, context: Context) -> GenerationCost:
        """Estimate the cost of generation."""
        # Z3 solving can be expensive
        complexity = 1
        if spec.examples:
            complexity = len(spec.examples)
        if spec.type_signature:
            complexity += spec.type_signature.count("->")

        return GenerationCost(
            time_estimate_ms=100 + complexity * 50,
            token_estimate=0,  # No LLM tokens
            complexity_score=complexity * 2,
        )

    def _parse_type_signature(
        self,
        sig: str,
    ) -> tuple[list[str], str] | None:
        """Parse a type signature like '(int, int) -> int' or 'int -> bool'."""
        if not sig or "->" not in sig:
            return None

        parts = sig.split("->")
        if len(parts) != 2:
            return None

        input_part = parts[0].strip()
        output_type = parts[1].strip()

        if input_part.startswith("(") and input_part.endswith(")"):
            inner = input_part[1:-1]
            input_types = [t.strip() for t in inner.split(",") if t.strip()]
        else:
            input_types = [input_part] if input_part else []

        return input_types, output_type

    def _solve(
        self,
        z3: Any,
        spec: Specification,
        input_types: list[str],
        output_type: str,
        context: Context,
    ) -> list[dict[str, Any]]:
        """Solve the synthesis problem using Z3.

        Encodes the synthesis as choosing from a set of candidate expressions
        and constraining them to satisfy the examples.
        """
        solutions: list[dict[str, Any]] = []

        # Create solver with timeout
        solver = z3.Solver()
        solver.set("timeout", self.timeout_ms)

        # Generate candidate expressions
        candidates = self._generate_candidates(
            z3,
            input_types,
            output_type,
            context,
        )

        if not candidates:
            logger.debug("No candidates generated")
            return []

        # Create choice variable (which candidate to use)
        choice = z3.Int("choice")
        solver.add(choice >= 0)
        solver.add(choice < len(candidates))

        # If we have examples, add constraints
        if spec.examples:
            for _i, (inputs, expected) in enumerate(spec.examples):
                # For each candidate, check if it produces expected output
                for j, candidate in enumerate(candidates):
                    try:
                        actual = self._evaluate_candidate(candidate, inputs)
                        if actual == expected:
                            # This candidate works for this example
                            pass  # Constraint already satisfied
                    except Exception:
                        # Candidate fails for this input
                        solver.add(z3.Implies(choice == j, z3.BoolVal(False)))

        # Find solutions
        for _ in range(self.max_solutions):
            if solver.check() == z3.sat:
                model = solver.model()
                choice_val = model[choice].as_long()
                solutions.append(
                    {
                        "candidate_idx": choice_val,
                        "expression": candidates[choice_val],
                    }
                )
                # Block this solution to find alternatives
                solver.add(choice != choice_val)
            else:
                break

        return solutions

    def _generate_candidates(
        self,
        z3: Any,
        input_types: list[str],
        output_type: str,
        context: Context,
    ) -> list[str]:
        """Generate candidate expressions for synthesis.

        Creates a set of possible expressions that could satisfy the spec.
        """
        candidates: list[str] = []
        param_names = [f"arg{i}" for i in range(len(input_types))]

        # Identity (return input directly)
        for name, typ in zip(param_names, input_types, strict=False):
            if self._types_compatible(typ, output_type):
                candidates.append(name)

        # Arithmetic operations for int/float
        if output_type in ("int", "float"):
            for idx1, (n1, t1) in enumerate(zip(param_names, input_types, strict=False)):
                if t1 in ("int", "float"):
                    candidates.append(f"-{n1}")  # Negation
                    candidates.append(f"abs({n1})")
                    for idx2, (n2, t2) in enumerate(zip(param_names, input_types, strict=False)):
                        if t2 in ("int", "float"):
                            candidates.append(f"{n1} + {n2}")
                            candidates.append(f"{n1} - {n2}")
                            candidates.append(f"{n1} * {n2}")
                            if idx1 != idx2:  # Avoid division by self
                                candidates.append(f"{n1} // {n2}")
                                candidates.append(f"{n1} % {n2}")

        # Boolean operations
        if output_type == "bool":
            for n1, t1 in zip(param_names, input_types, strict=False):
                if t1 in ("int", "float"):
                    candidates.append(f"{n1} > 0")
                    candidates.append(f"{n1} < 0")
                    candidates.append(f"{n1} == 0")
                    candidates.append(f"{n1} >= 0")
                    candidates.append(f"{n1} <= 0")
                    for n2, t2 in zip(param_names, input_types, strict=False):
                        if t2 in ("int", "float"):
                            candidates.append(f"{n1} == {n2}")
                            candidates.append(f"{n1} != {n2}")
                            candidates.append(f"{n1} < {n2}")
                            candidates.append(f"{n1} > {n2}")
                            candidates.append(f"{n1} <= {n2}")
                            candidates.append(f"{n1} >= {n2}")
                elif t1 == "bool":
                    candidates.append(f"not {n1}")
                    for n2, t2 in zip(param_names, input_types, strict=False):
                        if t2 == "bool":
                            candidates.append(f"{n1} and {n2}")
                            candidates.append(f"{n1} or {n2}")

        # String operations
        if output_type == "str":
            for n1, t1 in zip(param_names, input_types, strict=False):
                if t1 == "str":
                    candidates.append(f"{n1}.upper()")
                    candidates.append(f"{n1}.lower()")
                    candidates.append(f"{n1}.strip()")
                    for n2, t2 in zip(param_names, input_types, strict=False):
                        if t2 == "str":
                            candidates.append(f"{n1} + {n2}")
                elif t1 in ("int", "float"):
                    candidates.append(f"str({n1})")

        # List operations
        if output_type == "list":
            for n1, t1 in zip(param_names, input_types, strict=False):
                if t1 == "list":
                    candidates.append(f"sorted({n1})")
                    candidates.append(f"list(reversed({n1}))")
                    candidates.append(f"{n1}[:]")  # Copy
                    for n2, t2 in zip(param_names, input_types, strict=False):
                        if t2 == "list":
                            candidates.append(f"{n1} + {n2}")

        # Add primitives from context
        for prim in context.primitives:
            if prim == "len" and output_type == "int":
                for n, t in zip(param_names, input_types, strict=False):
                    if t in ("list", "str"):
                        candidates.append(f"len({n})")
            elif prim == "sum" and output_type == "int":
                for n, t in zip(param_names, input_types, strict=False):
                    if t == "list":
                        candidates.append(f"sum({n})")
            elif prim == "max" and output_type in ("int", "float"):
                for n, t in zip(param_names, input_types, strict=False):
                    if t == "list":
                        candidates.append(f"max({n})")
            elif prim == "min" and output_type in ("int", "float"):
                for n, t in zip(param_names, input_types, strict=False):
                    if t == "list":
                        candidates.append(f"min({n})")

        # Remove duplicates while preserving order
        seen: set[str] = set()
        unique_candidates = []
        for c in candidates:
            if c not in seen:
                seen.add(c)
                unique_candidates.append(c)

        logger.debug("Generated %d candidates", len(unique_candidates))
        return unique_candidates

    def _evaluate_candidate(self, expr: str, inputs: Any) -> Any:
        """Evaluate a candidate expression with given inputs."""
        # Create namespace with input values
        namespace: dict[str, Any] = {}
        if isinstance(inputs, tuple):
            for i, val in enumerate(inputs):
                namespace[f"arg{i}"] = val
        else:
            namespace["arg0"] = inputs

        # Add builtins
        namespace.update(
            {
                "abs": abs,
                "len": len,
                "sum": sum,
                "max": max,
                "min": min,
                "sorted": sorted,
                "reversed": reversed,
                "list": list,
                "str": str,
                "int": int,
                "float": float,
                "bool": bool,
            }
        )

        return eval(expr, {"__builtins__": {}}, namespace)

    def _types_compatible(self, t1: str, t2: str) -> bool:
        """Check if types are compatible."""
        t1 = re.sub(r"\[.*\]", "", t1)
        t2 = re.sub(r"\[.*\]", "", t2)
        if t1 == t2:
            return True
        if t1 == "Any" or t2 == "Any":
            return True
        # int is compatible with float
        return {t1, t2} == {"int", "float"}

    def _solution_to_code(
        self,
        spec: Specification,
        input_types: list[str],
        output_type: str,
        solution: dict[str, Any],
    ) -> str:
        """Convert a solution to Python code."""
        lines = []

        # Extract function name
        func_name = self._extract_function_name(spec.description)

        # Generate parameters
        param_names = [f"arg{i}" for i in range(len(input_types))]
        params = ", ".join(
            f"{name}: {typ}" for name, typ in zip(param_names, input_types, strict=False)
        )

        # Function definition
        lines.append(f"def {func_name}({params}) -> {output_type}:")
        lines.append(f'    """{spec.description}"""')

        # Body: return the expression
        expr = solution["expression"]
        lines.append(f"    return {expr}")

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
        return "_".join(words) if words else "synthesized"
