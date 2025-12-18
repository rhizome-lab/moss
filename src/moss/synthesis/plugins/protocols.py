"""Plugin protocols for the synthesis framework.

This module defines the protocols for pluggable synthesis components,
inspired by prior art: Synquid (type-driven), miniKanren (relational),
DreamCoder (library learning), and lambda^2 (bidirectional).

Key protocols:
- CodeGenerator: Generate code from specifications
- SynthesisValidator: Validate generated code
- LibraryPlugin: Manage reusable abstractions (DreamCoder-style)
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import TYPE_CHECKING, Any, Protocol, runtime_checkable

if TYPE_CHECKING:
    from moss.synthesis.types import Context, Specification


# =============================================================================
# Metadata Types
# =============================================================================


class GeneratorType(Enum):
    """Types of code generators."""

    PLACEHOLDER = "placeholder"
    TEMPLATE = "template"
    LLM = "llm"
    SMT = "smt"  # Z3-based (Synquid-style)
    RELATIONAL = "relational"  # miniKanren-style
    ENUMERATION = "enumeration"  # Bottom-up AST enumeration
    PBE = "pbe"  # Programming by Example (FlashFill/PROSE)


class ValidatorType(Enum):
    """Types of synthesis validators."""

    TEST = "test"  # pytest/jest
    TYPE = "type"  # mypy/pyright
    PROPERTY = "property"  # Hypothesis-based


@dataclass(frozen=True)
class GeneratorMetadata:
    """Metadata describing a code generator's capabilities.

    Attributes:
        name: Unique identifier (e.g., "template-crud")
        generator_type: Type of generator
        languages: Languages supported (empty = all)
        priority: Selection priority (higher = preferred)
        version: Plugin version
        description: Human-readable description
        supports_async: Whether generator supports async generation
        max_complexity: Maximum specification complexity this generator handles
    """

    name: str
    generator_type: GeneratorType
    languages: frozenset[str] = field(default_factory=frozenset)
    priority: int = 0
    version: str = "0.1.0"
    description: str = ""
    supports_async: bool = True
    max_complexity: int | None = None


@dataclass(frozen=True)
class ValidatorMetadata:
    """Metadata describing a synthesis validator's capabilities.

    Attributes:
        name: Unique identifier (e.g., "pytest-validator")
        validator_type: Type of validator
        languages: Languages supported (empty = all)
        priority: Selection priority (higher = preferred)
        version: Plugin version
        description: Human-readable description
        can_generate_counterexample: Whether validator can generate counterexamples
    """

    name: str
    validator_type: ValidatorType
    languages: frozenset[str] = field(default_factory=frozenset)
    priority: int = 0
    version: str = "0.1.0"
    description: str = ""
    can_generate_counterexample: bool = False


@dataclass(frozen=True)
class LibraryMetadata:
    """Metadata describing a library plugin's capabilities.

    Attributes:
        name: Unique identifier (e.g., "learned-abstractions")
        priority: Selection priority
        version: Plugin version
        description: Human-readable description
        supports_learning: Whether plugin can learn new abstractions
        persistence_type: How abstractions are persisted
    """

    name: str
    priority: int = 0
    version: str = "0.1.0"
    description: str = ""
    supports_learning: bool = False
    persistence_type: str = "memory"  # memory, file, database


# =============================================================================
# Result Types
# =============================================================================


@dataclass
class GenerationResult:
    """Result of a code generation attempt.

    Attributes:
        success: Whether generation succeeded
        code: Generated code (if successful)
        error: Error message (if failed)
        confidence: Generator's confidence in the solution (0-1)
        alternatives: Alternative solutions (if any)
        metadata: Additional information about generation
    """

    success: bool
    code: str | None = None
    error: str | None = None
    confidence: float = 0.0
    alternatives: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class ValidationResult:
    """Result of a validation attempt.

    Attributes:
        success: Whether validation passed
        passed_checks: Number of checks that passed
        total_checks: Total number of checks
        issues: List of validation issues
        counterexample: Failing input/output pair (if available)
        metadata: Additional validation information
    """

    success: bool
    passed_checks: int = 0
    total_checks: int = 0
    issues: list[str] = field(default_factory=list)
    counterexample: tuple[Any, Any] | None = None
    metadata: dict[str, Any] = field(default_factory=dict)

    @property
    def pass_rate(self) -> float:
        """Fraction of checks that passed."""
        if self.total_checks == 0:
            return 1.0 if self.success else 0.0
        return self.passed_checks / self.total_checks


@dataclass
class GenerationCost:
    """Estimated cost of code generation.

    Useful for selecting between generators and budgeting resources.
    """

    time_estimate_ms: int = 0
    token_estimate: int = 0  # For LLM-based generators
    complexity_score: int = 0


@dataclass(frozen=True)
class Abstraction:
    """A reusable code abstraction (DreamCoder-style).

    Abstractions are learned patterns that can be reused across
    multiple synthesis problems.

    Attributes:
        name: Identifier for the abstraction
        code: The abstraction's implementation
        type_signature: Type of the abstraction (if available)
        description: What the abstraction does
        usage_count: How often it's been used
        compression_gain: Bits saved by using this abstraction
    """

    name: str
    code: str
    type_signature: str | None = None
    description: str = ""
    usage_count: int = 0
    compression_gain: float = 0.0


@dataclass
class GenerationHints:
    """Hints to guide code generation.

    Attributes:
        preferred_style: Code style preferences
        abstractions: Available abstractions to use
        examples: Additional input/output examples
        constraints: Additional constraints
        timeout_ms: Generation timeout
    """

    preferred_style: str | None = None
    abstractions: list[Abstraction] = field(default_factory=list)
    examples: list[tuple[Any, Any]] = field(default_factory=list)
    constraints: list[str] = field(default_factory=list)
    timeout_ms: int | None = None


# =============================================================================
# Plugin Protocols
# =============================================================================


@runtime_checkable
class CodeGenerator(Protocol):
    """Protocol for code generation plugins.

    Code generators produce code from specifications. Built-in implementations:
    - PlaceholderGenerator: Returns TODO placeholders (current behavior)
    - TemplateGenerator: User-configurable templates
    - LLMGenerator: Claude/GPT integration (future)
    - SMTGenerator: Z3-based synthesis (future, Synquid-style)
    """

    @property
    def metadata(self) -> GeneratorMetadata:
        """Generator metadata describing capabilities."""
        ...

    def can_generate(self, spec: Specification, context: Context) -> bool:
        """Check if this generator can handle the given specification.

        Args:
            spec: The specification to generate code for
            context: Available resources

        Returns:
            True if this generator can handle the specification
        """
        ...

    async def generate(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None = None,
    ) -> GenerationResult:
        """Generate code for the given specification.

        Args:
            spec: What to generate
            context: Available resources (primitives, library, solved)
            hints: Optional hints to guide generation

        Returns:
            GenerationResult with code or error
        """
        ...

    def estimate_cost(self, spec: Specification, context: Context) -> GenerationCost:
        """Estimate the cost of generating code.

        Args:
            spec: The specification
            context: Available resources

        Returns:
            Estimated generation cost
        """
        ...


@runtime_checkable
class SynthesisValidator(Protocol):
    """Protocol for synthesis validation plugins.

    Validators check generated code against specifications. Built-in implementations:
    - PytestValidator: Run pytest/jest to validate code
    - TypeValidator: mypy/pyright type checking
    - PropertyValidator: Hypothesis-based property testing
    """

    @property
    def metadata(self) -> ValidatorMetadata:
        """Validator metadata describing capabilities."""
        ...

    def can_validate(self, spec: Specification, code: str) -> bool:
        """Check if this validator can validate the given code.

        Args:
            spec: The specification the code should satisfy
            code: The generated code

        Returns:
            True if this validator can check the code
        """
        ...

    async def validate(
        self,
        spec: Specification,
        code: str,
        context: Context,
    ) -> ValidationResult:
        """Validate generated code against specification.

        Args:
            spec: What the code should do
            code: The generated code
            context: Available resources

        Returns:
            ValidationResult indicating success or failure
        """
        ...

    async def generate_counterexample(
        self,
        spec: Specification,
        code: str,
        context: Context,
    ) -> tuple[Any, Any] | None:
        """Generate a counterexample showing where the code fails.

        Args:
            spec: The specification
            code: The failing code
            context: Available resources

        Returns:
            (input, expected_output) pair where code fails, or None
        """
        ...


@runtime_checkable
class LibraryPlugin(Protocol):
    """Protocol for library/abstraction plugins (DreamCoder-style).

    Library plugins manage reusable code abstractions that can be
    learned from successful synthesis runs.
    """

    @property
    def metadata(self) -> LibraryMetadata:
        """Library metadata describing capabilities."""
        ...

    def get_abstractions(self) -> list[Abstraction]:
        """Get all available abstractions.

        Returns:
            List of abstractions in the library
        """
        ...

    def search_abstractions(
        self,
        spec: Specification,
        context: Context,
    ) -> list[tuple[Abstraction, float]]:
        """Search for relevant abstractions for a specification.

        Args:
            spec: The specification to find abstractions for
            context: Available resources

        Returns:
            List of (abstraction, relevance_score) pairs, sorted by relevance
        """
        ...

    async def learn_abstraction(
        self,
        solutions: list[str],
        spec: Specification,
    ) -> Abstraction | None:
        """Learn a new abstraction from successful solutions.

        DreamCoder-style abstraction learning: identify common patterns
        across solutions and extract reusable abstractions.

        Args:
            solutions: List of successful solutions
            spec: The specification they solve

        Returns:
            New abstraction if one was learned, None otherwise
        """
        ...

    def record_usage(self, abstraction: Abstraction) -> None:
        """Record that an abstraction was used.

        Args:
            abstraction: The abstraction that was used
        """
        ...


# =============================================================================
# Export
# =============================================================================

__all__ = [
    "Abstraction",
    "CodeGenerator",
    "GenerationCost",
    "GenerationHints",
    "GenerationResult",
    "GeneratorMetadata",
    "GeneratorType",
    "LibraryMetadata",
    "LibraryPlugin",
    "SynthesisValidator",
    "ValidationResult",
    "ValidatorMetadata",
    "ValidatorType",
]
