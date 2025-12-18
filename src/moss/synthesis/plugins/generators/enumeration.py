"""Enumerative code generator using bottom-up AST enumeration.

This generator synthesizes code by:
1. Parsing the specification to understand inputs, outputs, and constraints
2. Enumerating small AST fragments (expressions, statements)
3. Validating fragments against input/output examples
4. Composing valid fragments into complete solutions

Inspired by:
- Bottom-up synthesis (Flash Fill, Prose)
- Type-directed synthesis (Synquid, Myth)
- Program-by-example approaches

This is a non-LLM approach useful for:
- Simple transformations (list ops, string manipulation)
- Functions with clear examples
- Pattern-based synthesis

Limitations:
- Scales poorly with program size
- Requires good examples to prune search
- Limited to simple types and operations
"""

from __future__ import annotations

import ast
import itertools
import logging
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

from moss.synthesis.plugins.protocols import (
    CodeGenerator,
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
# Expression Grammar
# =============================================================================


@dataclass(frozen=True)
class ExprFragment:
    """A synthesized expression fragment.

    Represents a candidate expression in the search space.
    """

    code: str  # Python code string
    ast_node: ast.expr | None = None  # Parsed AST
    depth: int = 1  # Complexity measure
    type_hint: str | None = None  # Inferred type


# Grammar productions for Python expressions
# Each production takes arguments and produces an expression


def var_production(name: str) -> ExprFragment:
    """Produce a variable reference."""
    return ExprFragment(code=name, depth=1, type_hint=None)


def const_production(value: Any) -> ExprFragment:
    """Produce a constant."""
    return ExprFragment(code=repr(value), depth=1)


def binop_production(left: ExprFragment, op: str, right: ExprFragment) -> ExprFragment:
    """Produce a binary operation."""
    code = f"({left.code} {op} {right.code})"
    return ExprFragment(code=code, depth=left.depth + right.depth + 1)


def call_production(func: str, args: list[ExprFragment]) -> ExprFragment:
    """Produce a function call."""
    arg_str = ", ".join(a.code for a in args)
    code = f"{func}({arg_str})"
    depth = sum(a.depth for a in args) + 1
    return ExprFragment(code=code, depth=depth)


def subscript_production(base: ExprFragment, index: ExprFragment) -> ExprFragment:
    """Produce a subscript expression."""
    code = f"{base.code}[{index.code}]"
    return ExprFragment(code=code, depth=base.depth + index.depth + 1)


def attr_production(base: ExprFragment, attr: str) -> ExprFragment:
    """Produce an attribute access."""
    code = f"{base.code}.{attr}"
    return ExprFragment(code=code, depth=base.depth + 1)


def method_production(base: ExprFragment, method: str, args: list[ExprFragment]) -> ExprFragment:
    """Produce a method call."""
    arg_str = ", ".join(a.code for a in args)
    code = f"{base.code}.{method}({arg_str})"
    depth = base.depth + sum(a.depth for a in args) + 1
    return ExprFragment(code=code, depth=depth)


def list_production(elements: list[ExprFragment]) -> ExprFragment:
    """Produce a list literal."""
    elem_str = ", ".join(e.code for e in elements)
    code = f"[{elem_str}]"
    depth = sum(e.depth for e in elements) + 1
    return ExprFragment(code=code, depth=depth)


def listcomp_production(element: ExprFragment, var: str, iterable: ExprFragment) -> ExprFragment:
    """Produce a list comprehension."""
    code = f"[{element.code} for {var} in {iterable.code}]"
    depth = element.depth + iterable.depth + 2
    return ExprFragment(code=code, depth=depth)


def lambda_production(params: list[str], body: ExprFragment) -> ExprFragment:
    """Produce a lambda expression."""
    param_str = ", ".join(params)
    code = f"lambda {param_str}: {body.code}"
    return ExprFragment(code=code, depth=body.depth + 1)


def ternary_production(
    condition: ExprFragment, if_true: ExprFragment, if_false: ExprFragment
) -> ExprFragment:
    """Produce a ternary expression."""
    code = f"({if_true.code} if {condition.code} else {if_false.code})"
    depth = condition.depth + if_true.depth + if_false.depth + 1
    return ExprFragment(code=code, depth=depth)


# =============================================================================
# Synthesis Engine
# =============================================================================


@dataclass
class EnumerationConfig:
    """Configuration for enumeration-based synthesis."""

    max_depth: int = 5  # Maximum expression depth
    max_candidates: int = 1000  # Maximum candidates to enumerate
    timeout_ms: int = 5000  # Timeout in milliseconds
    prune_early: bool = True  # Prune candidates that fail examples


@dataclass
class SynthesisState:
    """State during enumeration-based synthesis."""

    # Input parameters from specification
    param_names: list[str] = field(default_factory=list)
    param_types: list[str] = field(default_factory=list)
    return_type: str = "Any"

    # Extracted examples for testing
    examples: list[tuple[tuple[Any, ...], Any]] = field(default_factory=list)

    # Working fragments at each depth level
    fragments_by_depth: dict[int, list[ExprFragment]] = field(default_factory=dict)

    # Successfully tested fragments
    valid_fragments: list[ExprFragment] = field(default_factory=list)

    # Statistics
    candidates_tested: int = 0
    candidates_pruned: int = 0


class EnumerativeGenerator:
    """Generator that enumerates program fragments to find solutions.

    Uses bottom-up enumeration:
    1. Start with atomic expressions (variables, constants)
    2. Build larger expressions by combining smaller ones
    3. Test each candidate against examples
    4. Return first candidate that passes all tests

    Best for:
    - Simple list/string transformations
    - Arithmetic computations
    - Pattern-based operations with clear examples
    """

    def __init__(self, config: EnumerationConfig | None = None) -> None:
        """Initialize the enumerative generator.

        Args:
            config: Configuration for enumeration
        """
        self.config = config or EnumerationConfig()
        self._metadata = GeneratorMetadata(
            name="enumeration",
            generator_type=GeneratorType.RELATIONAL,  # Bottom-up enumeration
            priority=5,  # Lower than templates, higher than placeholder
            description="Bottom-up AST enumeration for simple synthesis",
            max_complexity=3,  # Only handles simple specs
        )

        # Common operations by type
        self._list_methods = ["append", "extend", "pop", "sort", "reverse"]
        self._str_methods = ["lower", "upper", "strip", "split", "join", "replace"]
        self._common_funcs = ["len", "sum", "min", "max", "sorted", "reversed", "list"]
        self._binops = ["+", "-", "*", "/", "//", "%", "==", "!=", "<", ">", "<=", ">="]

    @property
    def metadata(self) -> GeneratorMetadata:
        """Return generator metadata."""
        return self._metadata

    def can_generate(self, spec: Specification, context: Context) -> bool:
        """Check if enumeration can handle this specification.

        Best for specs with:
        - Input/output examples
        - Simple types (int, str, list)
        - Short descriptions (simple problems)
        """
        # Need examples to prune search space
        if not spec.examples:
            return False

        # Only handle simple problems
        if len(spec.description) > 200:
            return False

        return True

    def _parse_type_signature(self, sig: str | None) -> tuple[list[str], str]:
        """Parse a type signature to extract param types and return type.

        Args:
            sig: Type signature like "(int, str) -> list[int]"

        Returns:
            (param_types, return_type)
        """
        if not sig:
            return [], "Any"

        import re

        # Match (params) -> return
        match = re.match(r"\(([^)]*)\)\s*->\s*(.+)", sig.strip())
        if not match:
            return [], "Any"

        params_str = match.group(1)
        return_type = match.group(2).strip()

        # Split params
        param_types = []
        if params_str.strip():
            for p in params_str.split(","):
                param_types.append(p.strip())

        return param_types, return_type

    def _extract_params(self, spec: Specification) -> tuple[list[str], list[str]]:
        """Extract parameter names and types from specification.

        Returns:
            (param_names, param_types)
        """
        param_types, _ = self._parse_type_signature(spec.type_signature)

        # Generate parameter names
        param_names = []
        for i, t in enumerate(param_types):
            # Try to infer name from type
            if "str" in t.lower():
                name = "s" if i == 0 else f"s{i}"
            elif "int" in t.lower():
                name = "n" if i == 0 else f"n{i}"
            elif "list" in t.lower():
                name = "items" if i == 0 else f"items{i}"
            else:
                name = f"arg{i}"
            param_names.append(name)

        # If no type signature, infer from examples
        if not param_names and spec.examples:
            first_input = spec.examples[0][0]
            if isinstance(first_input, tuple):
                for i in range(len(first_input)):
                    param_names.append(f"arg{i}")
            else:
                param_names = ["x"]
                param_types = [type(first_input).__name__]

        return param_names, param_types

    def _prepare_examples(self, spec: Specification) -> list[tuple[tuple[Any, ...], Any]]:
        """Convert specification examples to test format.

        Returns:
            List of (inputs_tuple, expected_output)
        """
        prepared = []
        for inp, out in spec.examples:
            # Ensure input is a tuple
            if not isinstance(inp, tuple):
                inp = (inp,)
            prepared.append((inp, out))
        return prepared

    def _test_candidate(
        self,
        code: str,
        param_names: list[str],
        examples: list[tuple[tuple[Any, ...], Any]],
    ) -> bool:
        """Test a candidate expression against examples.

        Args:
            code: Expression code to test
            param_names: Parameter names
            examples: (inputs, output) pairs

        Returns:
            True if all examples pass
        """
        if not examples:
            return True

        # Build test function
        params_str = ", ".join(param_names)
        func_code = f"def _test_func({params_str}):\n    return {code}"

        try:
            # Compile and execute
            local_ns: dict[str, Any] = {}
            exec(func_code, {"__builtins__": __builtins__}, local_ns)
            test_func = local_ns["_test_func"]

            # Test each example
            for inputs, expected in examples:
                try:
                    result = test_func(*inputs)
                    if result != expected:
                        return False
                except Exception:
                    return False

            return True
        except Exception:
            return False

    def _enumerate_atoms(self, state: SynthesisState) -> list[ExprFragment]:
        """Enumerate atomic expressions (depth 1).

        Returns variable references and small constants.
        """
        atoms = []

        # Variables (parameters)
        for name in state.param_names:
            atoms.append(var_production(name))

        # Common constants
        for c in [0, 1, -1, True, False, "", None, []]:
            atoms.append(const_production(c))

        return atoms

    def _enumerate_depth(self, state: SynthesisState, depth: int) -> list[ExprFragment]:
        """Enumerate expressions at a specific depth.

        Combines smaller fragments into larger expressions.
        """
        if depth == 1:
            return self._enumerate_atoms(state)

        fragments = []
        smaller = []

        # Collect fragments from all smaller depths
        for d in range(1, depth):
            smaller.extend(state.fragments_by_depth.get(d, []))

        if not smaller:
            return []

        # Limit combinations to avoid explosion
        max_per_category = 20
        smaller = smaller[:50]  # Limit total fragments considered

        # Binary operations
        for left, right in itertools.islice(itertools.product(smaller, smaller), max_per_category):
            for op in self._binops:
                if left.depth + right.depth + 1 == depth:
                    fragments.append(binop_production(left, op, right))

        # Function calls
        for func in self._common_funcs:
            for arg in smaller:
                if arg.depth + 1 == depth:
                    fragments.append(call_production(func, [arg]))

        # Method calls on parameters
        for param_name in state.param_names:
            param_frag = var_production(param_name)

            # String methods
            for method in self._str_methods[:3]:
                if param_frag.depth + 1 <= depth:
                    fragments.append(method_production(param_frag, method, []))

            # List methods that return values
            for method in ["copy", "count", "index"]:
                if param_frag.depth + 1 <= depth:
                    fragments.append(method_production(param_frag, method, []))

        # Subscript
        for base in smaller:
            for idx in [const_production(0), const_production(-1)]:
                if base.depth + idx.depth + 1 == depth:
                    fragments.append(subscript_production(base, idx))

        # List comprehensions (for lists)
        if depth >= 3:
            for iterable in smaller:
                elem_var = "x"
                elem_frag = var_production(elem_var)
                for op in ["+", "*"]:
                    for const in [const_production(1), const_production(2)]:
                        comp = listcomp_production(
                            binop_production(elem_frag, op, const),
                            elem_var,
                            iterable,
                        )
                        if comp.depth == depth:
                            fragments.append(comp)

        return fragments[: max_per_category * 5]  # Limit output

    async def generate(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None = None,
    ) -> GenerationResult:
        """Generate code using bottom-up enumeration.

        Args:
            spec: The specification with examples
            context: Available resources
            hints: Optional hints

        Returns:
            GenerationResult with synthesized code
        """
        if not spec.examples:
            return GenerationResult(
                success=False,
                error="Enumeration requires input/output examples",
            )

        # Initialize synthesis state
        param_names, param_types = self._extract_params(spec)
        _, return_type = self._parse_type_signature(spec.type_signature)
        examples = self._prepare_examples(spec)

        state = SynthesisState(
            param_names=param_names,
            param_types=param_types,
            return_type=return_type,
            examples=examples,
        )

        logger.debug(
            "Starting enumeration with %d params, %d examples",
            len(param_names),
            len(examples),
        )

        # Bottom-up enumeration
        for depth in range(1, self.config.max_depth + 1):
            fragments = self._enumerate_depth(state, depth)
            state.fragments_by_depth[depth] = fragments

            logger.debug("Depth %d: %d candidates", depth, len(fragments))

            # Test each fragment
            for frag in fragments:
                state.candidates_tested += 1

                if state.candidates_tested > self.config.max_candidates:
                    break

                # Test against examples
                if self._test_candidate(frag.code, param_names, examples):
                    # Found a solution!
                    func_code = self._build_function(frag.code, param_names, spec)
                    return GenerationResult(
                        success=True,
                        code=func_code,
                        confidence=0.8,  # High confidence when examples pass
                        metadata={
                            "source": "enumeration",
                            "depth": depth,
                            "candidates_tested": state.candidates_tested,
                            "expression": frag.code,
                        },
                    )
                else:
                    state.candidates_pruned += 1

            if state.candidates_tested > self.config.max_candidates:
                break

        # No solution found
        return GenerationResult(
            success=False,
            error=f"No solution found after {state.candidates_tested} candidates",
            metadata={
                "candidates_tested": state.candidates_tested,
                "candidates_pruned": state.candidates_pruned,
                "max_depth": self.config.max_depth,
            },
        )

    def _build_function(self, expr_code: str, param_names: list[str], spec: Specification) -> str:
        """Build a complete function from the synthesized expression.

        Args:
            expr_code: The synthesized expression
            param_names: Parameter names
            spec: The specification

        Returns:
            Complete Python function code
        """
        import re

        # Extract function name from description
        desc = spec.description.lower()
        for prefix in ["create a ", "make a ", "return ", "compute ", "calculate "]:
            if desc.startswith(prefix):
                desc = desc[len(prefix) :]
                break

        words = re.findall(r"\w+", desc)[:2]
        func_name = "_".join(words) or "synthesized"

        # Build function
        params_str = ", ".join(param_names)
        type_sig = ""
        if spec.type_signature:
            type_sig = f" -> {spec.type_signature.split('->')[-1].strip()}"

        func = f'''def {func_name}({params_str}){type_sig}:
    """Synthesized function.

    {spec.description}
    """
    return {expr_code}
'''
        return func

    def estimate_cost(self, spec: Specification, context: Context) -> GenerationCost:
        """Estimate cost of enumeration-based synthesis.

        Cost depends on number of examples and expected depth.
        """
        num_examples = len(spec.examples) if spec.examples else 0

        # More examples = faster pruning but more testing
        base_time = 100  # Base time in ms
        per_example = 10
        per_depth = 50

        time_estimate = (
            base_time + (num_examples * per_example) + (self.config.max_depth * per_depth)
        )

        return GenerationCost(
            time_estimate_ms=time_estimate,
            token_estimate=0,  # No API calls
            complexity_score=min(self.config.max_depth, 5),
        )


# Protocol compliance check
assert isinstance(EnumerativeGenerator(), CodeGenerator)
