"""Component-based code generator using type-directed library composition.

This generator synthesizes code by composing available library functions
to reach the goal type from input types, inspired by SyPet and InSynth.

High-level approach:
1. Build a type graph from available functions (context.library)
2. Use graph search to find paths from input types to output type
3. Generate code by composing functions along the path

Key concepts:
- Type graph: Nodes = types, Edges = functions/methods
- Reachability: Can we reach goal type from input types?
- Petri net model: Places = types, Transitions = functions, Tokens = variables

Inspired by:
- SyPet (Feng et al.): Component-based synthesis via Petri nets
- InSynth (Gvero et al.): Type-directed code completion
- Prospector (Mandelin et al.): API jungloid mining

Limitations:
- Requires type signatures in context.library
- Limited to single-path compositions (no branching)
- Doesn't handle polymorphism well
"""

from __future__ import annotations

import logging
import re
from collections import deque
from dataclasses import dataclass, field
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
# Type Graph
# =============================================================================


@dataclass
class TypeNode:
    """A type in the type graph."""

    name: str
    is_primitive: bool = False

    def __hash__(self) -> int:
        return hash(self.name)

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, TypeNode):
            return False
        return self.name == other.name


@dataclass
class FunctionEdge:
    """A function that transforms types."""

    name: str
    input_types: list[str]  # Parameter types
    output_type: str  # Return type
    is_method: bool = False  # x.func() vs func(x)
    is_constructor: bool = False  # Creates new instance


@dataclass
class TypeGraph:
    """Graph of types connected by functions.

    Nodes are types, edges are functions that transform between types.
    Used to find paths from input types to goal type.
    """

    nodes: dict[str, TypeNode] = field(default_factory=dict)
    edges: list[FunctionEdge] = field(default_factory=list)
    # Map from type to functions that produce it
    producers: dict[str, list[FunctionEdge]] = field(default_factory=dict)
    # Map from type to functions that consume it
    consumers: dict[str, list[FunctionEdge]] = field(default_factory=dict)

    def add_type(self, type_name: str, is_primitive: bool = False) -> TypeNode:
        """Add a type node to the graph."""
        if type_name not in self.nodes:
            self.nodes[type_name] = TypeNode(type_name, is_primitive)
        return self.nodes[type_name]

    def add_function(self, func: FunctionEdge) -> None:
        """Add a function edge to the graph."""
        self.edges.append(func)

        # Index by output type (producers)
        if func.output_type not in self.producers:
            self.producers[func.output_type] = []
        self.producers[func.output_type].append(func)

        # Index by input types (consumers)
        for input_type in func.input_types:
            if input_type not in self.consumers:
                self.consumers[input_type] = []
            self.consumers[input_type].append(func)

    def find_path(
        self,
        input_types: list[str],
        goal_type: str,
        max_depth: int = 5,
    ) -> list[FunctionEdge] | None:
        """Find a path from input types to goal type using BFS.

        Returns the sequence of functions to apply, or None if no path exists.
        """
        # Normalize types
        input_types = [self._normalize_type(t) for t in input_types]
        goal_type = self._normalize_type(goal_type)

        # BFS state: (available_types, path)
        initial_types = frozenset(input_types)
        queue: deque[tuple[frozenset[str], list[FunctionEdge]]] = deque()
        queue.append((initial_types, []))
        visited: set[frozenset[str]] = {initial_types}

        while queue:
            available, path = queue.popleft()

            # Check if we've reached the goal
            if goal_type in available:
                return path

            # Stop if too deep
            if len(path) >= max_depth:
                continue

            # Try each function that consumes available types
            for func in self.edges:
                # Check if all inputs are available
                if all(self._type_compatible(t, available) for t in func.input_types):
                    # Apply function: add output type
                    new_available = frozenset(available | {func.output_type})

                    if new_available not in visited:
                        visited.add(new_available)
                        queue.append((new_available, [*path, func]))

        return None

    def _normalize_type(self, type_name: str) -> str:
        """Normalize type name for comparison."""
        # Remove generic parameters for now
        type_name = re.sub(r"\[.*\]", "", type_name)
        # Handle common aliases
        type_aliases = {
            "string": "str",
            "integer": "int",
            "boolean": "bool",
            "float64": "float",
            "list": "list",
            "dict": "dict",
        }
        return type_aliases.get(type_name.lower(), type_name)

    def _type_compatible(self, required: str, available: frozenset[str]) -> bool:
        """Check if required type is available (with some flexibility)."""
        required = self._normalize_type(required)
        for avail in available:
            avail = self._normalize_type(avail)
            if required == avail:
                return True
            # Handle Any compatibility
            if required == "Any" or avail == "Any":
                return True
            # Handle object/base class
            if required == "object":
                return True
        return False


# =============================================================================
# Component Generator
# =============================================================================


class ComponentGenerator:
    """Generate code by composing library functions.

    Uses type-directed search to find function compositions that
    transform inputs to the desired output type.
    """

    def __init__(
        self,
        max_depth: int = 5,
        max_solutions: int = 3,
    ) -> None:
        """Initialize the generator.

        Args:
            max_depth: Maximum composition depth (number of function calls)
            max_solutions: Maximum alternative solutions to generate

        """
        self.max_depth = max_depth
        self.max_solutions = max_solutions

    @property
    def metadata(self) -> GeneratorMetadata:
        """Return generator metadata."""
        return GeneratorMetadata(
            name="component",
            generator_type=GeneratorType.RELATIONAL,
            priority=12,  # Between template (10) and LLM (20)
            version="0.1.0",
            description="Compose library functions to reach goal type (SyPet/InSynth style)",
            supports_async=True,
            max_complexity=4,
        )

    def can_generate(self, spec: Specification, context: Context) -> bool:
        """Check if this generator can handle the specification.

        Requires:
        - Type signature with input and output types
        - Non-empty library in context
        """
        # Need type signature
        if not spec.type_signature:
            return False

        # Need library functions to compose
        if not context.library:
            return False

        # Parse type signature to check validity
        parsed = self._parse_type_signature(spec.type_signature)
        if not parsed:
            return False

        input_types, output_type = parsed
        return not (not input_types or not output_type)

    async def generate(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None = None,
    ) -> GenerationResult:
        """Generate code by composing library functions.

        Args:
            spec: What to generate
            context: Available resources (library, primitives)
            hints: Optional generation hints

        Returns:
            GenerationResult with composed code

        """
        # Check already solved
        if spec.description in context.solved:
            return GenerationResult(
                success=True,
                code=context.solved[spec.description],
                confidence=0.95,
                metadata={"source": "cached"},
            )

        # Parse type signature
        parsed = self._parse_type_signature(spec.type_signature or "")
        if not parsed:
            return GenerationResult(
                success=False,
                error="Could not parse type signature",
                metadata={"source": "component"},
            )

        input_types, output_type = parsed
        logger.debug(
            "ComponentGenerator: %s -> %s",
            input_types,
            output_type,
        )

        # Build type graph from library
        graph = self._build_type_graph(context)
        logger.debug("Type graph: %d nodes, %d edges", len(graph.nodes), len(graph.edges))

        # Find path from inputs to output
        path = graph.find_path(input_types, output_type, max_depth=self.max_depth)

        if path is None:
            return GenerationResult(
                success=False,
                error=f"No composition found from {input_types} to {output_type}",
                confidence=0.0,
                metadata={"source": "component", "graph_nodes": len(graph.nodes)},
            )

        # Generate code from path
        code = self._generate_code_from_path(spec, input_types, output_type, path)

        # Calculate confidence based on path length
        # Shorter paths = higher confidence
        confidence = max(0.5, 1.0 - (len(path) * 0.1))

        return GenerationResult(
            success=True,
            code=code,
            confidence=confidence,
            metadata={
                "source": "component",
                "path_length": len(path),
                "functions_used": [f.name for f in path],
            },
        )

    def estimate_cost(self, spec: Specification, context: Context) -> GenerationCost:
        """Estimate the cost of generation."""
        # Cost depends on library size and type complexity
        library_size = len(context.library)
        complexity = 1
        if spec.type_signature:
            # More complex types = higher cost
            complexity = spec.type_signature.count("->") + 1

        return GenerationCost(
            time_estimate_ms=50 + library_size * 10,
            token_estimate=0,  # No LLM tokens
            complexity_score=complexity,
        )

    def _parse_type_signature(
        self,
        sig: str,
    ) -> tuple[list[str], str] | None:
        """Parse a type signature like '(int, str) -> list' or 'int -> str'.

        Returns (input_types, output_type) or None if unparseable.
        """
        if not sig or "->" not in sig:
            return None

        parts = sig.split("->")
        if len(parts) != 2:
            return None

        input_part = parts[0].strip()
        output_type = parts[1].strip()

        # Parse input types
        if input_part.startswith("(") and input_part.endswith(")"):
            # Multiple inputs: (int, str)
            inner = input_part[1:-1]
            input_types = [t.strip() for t in inner.split(",") if t.strip()]
        else:
            # Single input: int
            input_types = [input_part] if input_part else []

        return input_types, output_type

    def _build_type_graph(self, context: Context) -> TypeGraph:
        """Build type graph from context library."""
        graph = TypeGraph()

        # Add primitive types
        for prim in ("int", "str", "float", "bool", "list", "dict", "None"):
            graph.add_type(prim, is_primitive=True)

        # Add functions from library
        for name, info in context.library.items():
            if isinstance(info, dict):
                # Structured library entry with type info
                type_sig = info.get("type", info.get("signature", ""))
                if type_sig:
                    parsed = self._parse_type_signature(type_sig)
                    if parsed:
                        input_types, output_type = parsed
                        graph.add_function(
                            FunctionEdge(
                                name=name,
                                input_types=input_types,
                                output_type=output_type,
                            )
                        )
                        # Add types to graph
                        graph.add_type(output_type)
                        for it in input_types:
                            graph.add_type(it)

            elif callable(info):
                # Actual callable - try to get type hints
                try:
                    import inspect

                    hints = getattr(info, "__annotations__", {})
                    if "return" in hints:
                        return_type = self._type_to_str(hints["return"])
                        sig = inspect.signature(info)
                        input_types = []
                        for param in sig.parameters.values():
                            if param.name in hints:
                                input_types.append(self._type_to_str(hints[param.name]))
                            else:
                                input_types.append("Any")

                        graph.add_function(
                            FunctionEdge(
                                name=name,
                                input_types=input_types,
                                output_type=return_type,
                            )
                        )
                        graph.add_type(return_type)
                        for it in input_types:
                            graph.add_type(it)
                except Exception:
                    pass  # Skip functions we can't analyze

        # Add primitives as producers of themselves (identity)
        for prim in context.primitives:
            # Primitives like "len", "sum" etc.
            if prim == "len":
                graph.add_function(FunctionEdge("len", ["list"], "int"))
            elif prim == "sum":
                graph.add_function(FunctionEdge("sum", ["list"], "int"))
            elif prim == "str":
                graph.add_function(FunctionEdge("str", ["Any"], "str"))
            elif prim == "int":
                graph.add_function(FunctionEdge("int", ["Any"], "int"))
            elif prim == "list":
                graph.add_function(FunctionEdge("list", ["Any"], "list"))
            elif prim == "sorted":
                graph.add_function(FunctionEdge("sorted", ["list"], "list"))
            elif prim == "reversed":
                graph.add_function(FunctionEdge("reversed", ["list"], "list"))
            elif prim == "map":
                graph.add_function(FunctionEdge("list", ["map"], "list"))
            elif prim == "filter":
                graph.add_function(FunctionEdge("list", ["filter"], "list"))

        return graph

    def _type_to_str(self, type_hint: Any) -> str:
        """Convert a type hint to string representation."""
        if type_hint is None:
            return "None"
        if isinstance(type_hint, str):
            return type_hint
        if hasattr(type_hint, "__name__"):
            return type_hint.__name__
        return str(type_hint)

    def _generate_code_from_path(
        self,
        spec: Specification,
        input_types: list[str],
        output_type: str,
        path: list[FunctionEdge],
    ) -> str:
        """Generate Python code from a function composition path."""
        lines = []

        # Extract function name from description
        func_name = self._extract_function_name(spec.description)

        # Generate parameter names
        param_names = [f"arg{i}" for i in range(len(input_types))]

        # Function signature
        params = ", ".join(
            f"{name}: {typ}" for name, typ in zip(param_names, input_types, strict=False)
        )
        lines.append(f"def {func_name}({params}) -> {output_type}:")

        # Docstring
        lines.append(f'    """{spec.description}"""')

        # Generate body by composing functions
        if not path:
            # Direct return if no transformation needed
            lines.append(f"    return {param_names[0]}")
        else:
            # Apply functions in sequence
            current_var = param_names[0] if param_names else "input"
            for i, func in enumerate(path):
                result_var = f"result{i}" if i < len(path) - 1 else "result"

                # Generate function call
                if func.is_method:
                    call = f"{current_var}.{func.name}()"
                elif len(func.input_types) > 1:
                    # Multi-argument function
                    args = ", ".join(param_names[: len(func.input_types)])
                    call = f"{func.name}({args})"
                else:
                    call = f"{func.name}({current_var})"

                lines.append(f"    {result_var} = {call}")
                current_var = result_var

            lines.append("    return result")

        return "\n".join(lines)

    def _extract_function_name(self, description: str) -> str:
        """Extract a function name from the description."""
        # Look for common patterns
        patterns = [
            r"function\s+(\w+)",
            r"def\s+(\w+)",
            r"(\w+)\s+function",
        ]
        for pattern in patterns:
            match = re.search(pattern, description, re.IGNORECASE)
            if match:
                return match.group(1)

        # Default: convert description to snake_case
        words = re.findall(r"\w+", description.lower())[:3]
        return "_".join(words) if words else "synthesized"
