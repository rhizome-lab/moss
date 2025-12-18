"""Type-driven decomposition strategy.

Decomposes problems based on type signatures, using type structure
to determine what subproblems need to be solved.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from moss.synthesis.strategy import DecompositionStrategy, StrategyMetadata
from moss.synthesis.types import Context, Specification, Subproblem


@dataclass
class TypeInfo:
    """Parsed type information."""

    input_types: list[str]
    output_type: str
    is_generic: bool = False
    type_params: list[str] | None = None


def parse_type_signature(sig: str) -> TypeInfo | None:
    """Parse a type signature string into TypeInfo.

    Handles formats like:
    - "(int, int) -> int"
    - "List[int] -> List[int]"
    - "str -> int"
    """
    if not sig or "->" not in sig:
        return None

    # Split on ->
    parts = sig.split("->")
    if len(parts) != 2:
        return None

    input_part = parts[0].strip()
    output_type = parts[1].strip()

    # Parse input types
    input_types: list[str] = []
    if input_part.startswith("(") and input_part.endswith(")"):
        # Multiple inputs: (int, str) -> ...
        inner = input_part[1:-1]
        input_types = [t.strip() for t in inner.split(",") if t.strip()]
    else:
        # Single input: str -> ...
        input_types = [input_part]

    # Check for generics
    is_generic = bool(re.search(r"\[.+\]", sig))
    type_params = re.findall(r"\[([^\]]+)\]", sig)

    return TypeInfo(
        input_types=input_types,
        output_type=output_type,
        is_generic=is_generic,
        type_params=type_params,
    )


def is_composite_type(type_str: str) -> bool:
    """Check if a type is composite (has multiple components)."""
    composite_patterns = [
        r"^tuple\[",
        r"^Tuple\[",
        r"^dict\[",
        r"^Dict\[",
        r"^dataclass",
        r"^namedtuple",
    ]
    return any(re.match(p, type_str, re.IGNORECASE) for p in composite_patterns)


def is_collection_type(type_str: str) -> bool:
    """Check if a type is a collection."""
    collection_patterns = [
        r"^list\[",
        r"^List\[",
        r"^set\[",
        r"^Set\[",
        r"^Sequence\[",
        r"^Iterable\[",
    ]
    return any(re.match(p, type_str, re.IGNORECASE) for p in collection_patterns)


def extract_inner_type(type_str: str) -> str | None:
    """Extract inner type from a generic type like List[T]."""
    match = re.match(r"^\w+\[(.+)\]$", type_str)
    return match.group(1) if match else None


class TypeDrivenDecomposition(DecompositionStrategy):
    """Decompose based on type signatures and type constraints.

    This strategy works best when:
    - Rich type information is available
    - Type signatures clearly indicate structure
    - Types can be decomposed into simpler components

    Decomposition approaches:
    1. Composite output -> decompose into component builders
    2. Collection transform -> map/filter/reduce pattern
    3. Type conversion -> chain of intermediate types
    """

    @property
    def metadata(self) -> StrategyMetadata:
        return StrategyMetadata(
            name="type_driven",
            description="Decompose based on type signatures and type constraints",
            keywords=(
                "type",
                "typed",
                "signature",
                "types",
                "generic",
                "transform",
                "convert",
            ),
        )

    def can_handle(self, spec: Specification, context: Context) -> bool:
        """Check if we have type information to work with."""
        return spec.type_signature is not None and len(spec.type_signature) > 0

    def decompose(
        self,
        spec: Specification,
        context: Context,
    ) -> list[Subproblem]:
        """Decompose based on type analysis."""
        if not spec.type_signature:
            return []

        type_info = parse_type_signature(spec.type_signature)
        if not type_info:
            return []

        subproblems: list[Subproblem] = []

        # Strategy 1: Composite output type -> build components
        if is_composite_type(type_info.output_type):
            subproblems = self._decompose_composite(spec, type_info, context)

        # Strategy 2: Collection transform -> map/filter pattern
        elif is_collection_type(type_info.output_type) and any(
            is_collection_type(t) for t in type_info.input_types
        ):
            subproblems = self._decompose_collection_transform(spec, type_info, context)

        # Strategy 3: Type conversion chain
        elif self._can_decompose_via_intermediate(type_info, context):
            subproblems = self._decompose_via_intermediate(spec, type_info, context)

        return subproblems

    def estimate_success(self, spec: Specification, context: Context) -> float:
        """Estimate based on type richness."""
        if not spec.type_signature:
            return 0.0

        score = 0.5  # Base score for having types

        type_info = parse_type_signature(spec.type_signature)
        if not type_info:
            return 0.3

        # Bonus for generic types (more structure)
        if type_info.is_generic:
            score += 0.15

        # Bonus for known collection types
        if is_collection_type(type_info.output_type):
            score += 0.1

        # Bonus for composite types (clear decomposition path)
        if is_composite_type(type_info.output_type):
            score += 0.15

        # Penalty for too many input types (complexity)
        if len(type_info.input_types) > 3:
            score -= 0.1

        return min(1.0, max(0.0, score))

    def _decompose_composite(
        self,
        spec: Specification,
        type_info: TypeInfo,
        context: Context,
    ) -> list[Subproblem]:
        """Decompose when output is a composite type."""
        subproblems: list[Subproblem] = []

        # Extract component types from tuple/dict
        if type_info.type_params:
            for i, component in enumerate(type_info.type_params[0].split(",")):
                component = component.strip()
                sub_spec = Specification(
                    description=f"Build component {i}: {component}",
                    type_signature=f"({', '.join(type_info.input_types)}) -> {component}",
                    constraints=spec.constraints,
                )
                subproblems.append(
                    Subproblem(
                        specification=sub_spec,
                        priority=i,
                    )
                )

        return subproblems

    def _decompose_collection_transform(
        self,
        spec: Specification,
        type_info: TypeInfo,
        context: Context,
    ) -> list[Subproblem]:
        """Decompose collection transformations into map/filter/reduce."""
        subproblems: list[Subproblem] = []

        # Find collection input and output types
        input_inner = None
        output_inner = None

        for inp in type_info.input_types:
            if is_collection_type(inp):
                input_inner = extract_inner_type(inp)
                break

        output_inner = extract_inner_type(type_info.output_type)

        if input_inner and output_inner:
            # Create element transformation subproblem
            sub_spec = Specification(
                description=f"Transform element from {input_inner} to {output_inner}",
                type_signature=f"{input_inner} -> {output_inner}",
                constraints=spec.constraints,
            )
            subproblems.append(
                Subproblem(
                    specification=sub_spec,
                    priority=0,
                )
            )

            # Create mapping subproblem
            input_t = type_info.input_types[0]
            output_t = type_info.output_type
            map_sig = f"({input_t}, ({input_inner} -> {output_inner})) -> {output_t}"
            map_spec = Specification(
                description="Apply transformation to collection",
                type_signature=map_sig,
                constraints=(),
            )
            subproblems.append(
                Subproblem(
                    specification=map_spec,
                    dependencies=(0,),
                    priority=1,
                )
            )

        return subproblems

    def _can_decompose_via_intermediate(
        self,
        type_info: TypeInfo,
        context: Context,
    ) -> bool:
        """Check if we can decompose via intermediate types."""
        # Check if library has functions that could form a chain
        input_type = type_info.input_types[0] if type_info.input_types else ""
        output_type = type_info.output_type

        # Don't decompose if already using placeholder intermediate types
        placeholder_types = {"intermediate", "any", "object", "unknown"}
        if input_type.lower() in placeholder_types or output_type.lower() in placeholder_types:
            return False

        # Don't decompose simple primitive conversions
        primitive_types = {"int", "str", "bool", "float", "bytes", "none"}
        if input_type.lower() in primitive_types and output_type.lower() in primitive_types:
            return False

        # Simple heuristic: different types suggest intermediate steps
        return input_type != output_type and not is_collection_type(output_type)

    def _decompose_via_intermediate(
        self,
        spec: Specification,
        type_info: TypeInfo,
        context: Context,
    ) -> list[Subproblem]:
        """Decompose via intermediate type conversions."""
        subproblems: list[Subproblem] = []

        input_type = type_info.input_types[0] if type_info.input_types else "Any"
        output_type = type_info.output_type

        # Create two-step conversion (input -> intermediate -> output)
        # Use a generic intermediate type
        intermediate = "intermediate"

        # Step 1: Input to intermediate
        step1_spec = Specification(
            description=f"Convert {input_type} to {intermediate} form",
            type_signature=f"{input_type} -> {intermediate}",
            constraints=spec.constraints,
        )
        subproblems.append(Subproblem(specification=step1_spec, priority=0))

        # Step 2: Intermediate to output
        step2_spec = Specification(
            description=f"Convert {intermediate} to {output_type}",
            type_signature=f"{intermediate} -> {output_type}",
            constraints=spec.constraints,
        )
        subproblems.append(
            Subproblem(
                specification=step2_spec,
                dependencies=(0,),
                priority=1,
            )
        )

        return subproblems
