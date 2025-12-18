"""Tests for synthesis plugin architecture."""

from __future__ import annotations

import pytest

from moss.synthesis.plugins import (
    Abstraction,
    CodeGenerator,
    GenerationHints,
    GeneratorType,
    LibraryPlugin,
    SynthesisRegistry,
    SynthesisValidator,
    ValidatorType,
    get_synthesis_registry,
    reset_synthesis_registry,
)
from moss.synthesis.plugins.generators import PlaceholderGenerator, TemplateGenerator
from moss.synthesis.plugins.libraries import MemoryLibrary
from moss.synthesis.plugins.strategies import StrategyRegistry
from moss.synthesis.plugins.validators import TestValidator, TypeValidator
from moss.synthesis.strategies import TypeDrivenDecomposition
from moss.synthesis.types import Context, Specification


@pytest.fixture
def spec() -> Specification:
    """Sample specification for tests."""
    return Specification(
        description="Create a function that adds two numbers",
        type_signature="(int, int) -> int",
        examples=(((1, 2), 3), ((0, 0), 0)),
    )


@pytest.fixture
def context() -> Context:
    """Sample context for tests."""
    return Context(primitives=("add", "subtract", "multiply"))


class TestPlaceholderGenerator:
    """Tests for PlaceholderGenerator."""

    def test_metadata(self):
        """Test generator metadata."""
        gen = PlaceholderGenerator()
        assert gen.metadata.name == "placeholder"
        assert gen.metadata.generator_type == GeneratorType.PLACEHOLDER
        assert gen.metadata.priority == -100

    def test_can_generate_always_true(self, spec, context):
        """Placeholder can always generate."""
        gen = PlaceholderGenerator()
        assert gen.can_generate(spec, context) is True

    @pytest.mark.asyncio
    async def test_generate_placeholder(self, spec):
        """Test placeholder code generation."""
        gen = PlaceholderGenerator()
        # Use empty context to ensure no primitive matches
        result = await gen.generate(spec, Context())

        assert result.success is True
        assert result.code is not None
        assert "TODO: implement" in result.code
        assert result.confidence == 0.0

    @pytest.mark.asyncio
    async def test_generate_from_solved(self, spec, context):
        """Test returning solved code from context."""
        gen = PlaceholderGenerator()
        solved_context = context.with_solved(spec.description, "def add(a, b): return a + b")

        result = await gen.generate(spec, solved_context)

        assert result.success is True
        assert "return a + b" in result.code

    def test_protocol_compliance(self):
        """Verify generator implements CodeGenerator protocol."""
        gen = PlaceholderGenerator()
        assert isinstance(gen, CodeGenerator)

    @pytest.mark.asyncio
    async def test_generate_from_primitive(self):
        """Test returning primitive match from context."""
        gen = PlaceholderGenerator()
        spec = Specification(description="use the print function")
        context = Context(primitives=("print", "len", "str"))

        result = await gen.generate(spec, context)

        assert result.success is True
        assert result.code == "print"
        assert result.confidence == 0.5
        assert result.metadata.get("source") == "primitive"

    @pytest.mark.asyncio
    async def test_generate_with_constraints(self):
        """Test placeholder includes constraints in output."""
        gen = PlaceholderGenerator()
        spec = Specification(
            description="test function",
            constraints=("must be pure", "no side effects"),
        )

        result = await gen.generate(spec, Context())

        assert result.success is True
        assert "Constraints:" in result.code
        assert "must be pure" in result.code
        assert "no side effects" in result.code

    def test_estimate_cost(self, spec, context):
        """Test cost estimation."""
        gen = PlaceholderGenerator()

        cost = gen.estimate_cost(spec, context)

        assert cost.time_estimate_ms == 1
        assert cost.token_estimate == 0
        assert cost.complexity_score == 0


class TestTemplateGenerator:
    """Tests for TemplateGenerator."""

    def test_metadata(self):
        """Test generator metadata."""
        gen = TemplateGenerator()
        assert gen.metadata.name == "template"
        assert gen.metadata.generator_type == GeneratorType.TEMPLATE
        assert gen.metadata.priority == 10

    @pytest.mark.asyncio
    async def test_generate_crud_create(self):
        """Test CRUD create template matching."""
        gen = TemplateGenerator()
        spec = Specification(description="Create a new user")

        result = await gen.generate(spec, Context())

        assert result.success is True
        assert result.code is not None
        assert "create" in result.code.lower() or "def" in result.code

    @pytest.mark.asyncio
    async def test_generate_with_hint(self):
        """Test template selection via hints."""
        gen = TemplateGenerator()
        spec = Specification(description="Some function")
        hints = GenerationHints(preferred_style="crud/create")

        result = await gen.generate(spec, Context(), hints)

        assert result.success is True
        assert "create" in result.code.lower()

    def test_get_available_templates(self):
        """Test listing available templates."""
        gen = TemplateGenerator()
        templates = gen.get_available_templates()

        assert len(templates) > 0
        assert "crud/create" in templates
        assert "function/pure" in templates

    def test_add_custom_template(self):
        """Test adding custom templates."""
        gen = TemplateGenerator()
        gen.add_template("custom/test", "def test_${name}(): pass")

        templates = gen.get_available_templates()
        assert "custom/test" in templates

    def test_protocol_compliance(self):
        """Verify generator implements CodeGenerator protocol."""
        gen = TemplateGenerator()
        assert isinstance(gen, CodeGenerator)


class TestTestValidator:
    """Tests for TestValidator (TestExecutorValidator)."""

    def test_metadata(self):
        """Test validator metadata."""
        val = TestValidator()
        assert val.metadata.name == "pytest"
        assert val.metadata.validator_type == ValidatorType.TEST
        assert val.metadata.can_generate_counterexample is True

    def test_can_validate_with_tests(self, spec):
        """Test can_validate returns True with tests."""
        val = TestValidator()
        # Spec has examples which can be converted to tests
        assert val.can_validate(spec, "def add(a, b): return a + b") is True

    def test_can_validate_without_tests(self):
        """Test can_validate returns False without tests."""
        val = TestValidator()
        spec = Specification(description="No tests")
        assert val.can_validate(spec, "pass") is False

    def test_protocol_compliance(self):
        """Verify validator implements SynthesisValidator protocol."""
        val = TestValidator()
        assert isinstance(val, SynthesisValidator)


class TestTypeValidator:
    """Tests for TypeValidator."""

    def test_metadata(self):
        """Test validator metadata."""
        val = TypeValidator()
        assert val.metadata.name == "mypy"
        assert val.metadata.validator_type == ValidatorType.TYPE

    def test_can_validate_with_types(self, spec):
        """Test can_validate returns True with typed code."""
        val = TypeValidator()
        code = "def add(a: int, b: int) -> int: return a + b"
        assert val.can_validate(spec, code) is True

    def test_can_validate_without_types(self):
        """Test can_validate returns False without types."""
        val = TypeValidator()
        spec = Specification(description="No type signature")
        assert val.can_validate(spec, "x = 1") is False

    def test_protocol_compliance(self):
        """Verify validator implements SynthesisValidator protocol."""
        val = TypeValidator()
        assert isinstance(val, SynthesisValidator)


class TestMemoryLibrary:
    """Tests for MemoryLibrary."""

    def test_add_and_get_abstractions(self):
        """Test adding and retrieving abstractions."""
        lib = MemoryLibrary()

        abstraction = Abstraction(
            name="add",
            code="def add(a, b): return a + b",
            type_signature="(int, int) -> int",
            description="Add two numbers",
        )

        lib.add_abstraction(abstraction)

        abstractions = lib.get_abstractions()
        assert len(abstractions) == 1
        assert abstractions[0].name == "add"

    def test_search_abstractions(self, spec, context):
        """Test searching for relevant abstractions."""
        lib = MemoryLibrary()

        lib.add_abstraction(
            Abstraction(
                name="add_numbers",
                code="def add(a, b): return a + b",
                description="Add two numbers together",
            )
        )
        lib.add_abstraction(
            Abstraction(
                name="multiply",
                code="def mul(a, b): return a * b",
                description="Multiply two numbers",
            )
        )

        results = lib.search_abstractions(spec, context)

        assert len(results) > 0
        # "add" should rank higher for "adds two numbers"
        names = [a.name for a, _ in results]
        assert "add_numbers" in names

    def test_record_usage(self):
        """Test recording abstraction usage."""
        lib = MemoryLibrary()

        abstraction = Abstraction(name="test", code="pass", usage_count=0)
        lib.add_abstraction(abstraction)

        lib.record_usage(abstraction)

        updated = lib.get_abstractions()[0]
        assert updated.usage_count == 1

    def test_protocol_compliance(self):
        """Verify library implements LibraryPlugin protocol."""
        lib = MemoryLibrary()
        assert isinstance(lib, LibraryPlugin)

    def test_remove_abstraction(self):
        """Test removing an abstraction."""
        lib = MemoryLibrary()
        abstraction = Abstraction(name="to_remove", code="pass")
        lib.add_abstraction(abstraction)

        assert lib.remove_abstraction("to_remove") is True
        assert lib.remove_abstraction("nonexistent") is False
        assert len(lib.get_abstractions()) == 0

    def test_clear(self):
        """Test clearing all abstractions."""
        lib = MemoryLibrary()
        lib.add_abstraction(Abstraction(name="a", code="pass"))
        lib.add_abstraction(Abstraction(name="b", code="pass"))

        lib.clear()

        assert len(lib.get_abstractions()) == 0
        assert len(lib) == 0

    def test_len(self):
        """Test __len__ method."""
        lib = MemoryLibrary()
        assert len(lib) == 0

        lib.add_abstraction(Abstraction(name="a", code="pass"))
        assert len(lib) == 1

        lib.add_abstraction(Abstraction(name="b", code="pass"))
        assert len(lib) == 2

    @pytest.mark.asyncio
    async def test_learn_abstraction_returns_none(self, spec):
        """Test that MemoryLibrary doesn't learn (returns None)."""
        lib = MemoryLibrary()
        result = await lib.learn_abstraction(["code1", "code2"], spec)
        assert result is None

    def test_search_with_type_signature(self, context):
        """Test searching with type signature matching."""
        lib = MemoryLibrary()

        lib.add_abstraction(
            Abstraction(
                name="int_func",
                code="def f(x): return x + 1",
                type_signature="(int) -> int",
                description="Integer function",
            )
        )
        lib.add_abstraction(
            Abstraction(
                name="str_func",
                code="def f(x): return str(x)",
                type_signature="(int) -> str",
                description="String function",
            )
        )

        spec_int = Specification(
            description="test integer function",
            type_signature="(int) -> int",
        )
        results = lib.search_abstractions(spec_int, context)

        # int_func should match better due to return type
        assert len(results) > 0


class TestSynthesisRegistry:
    """Tests for SynthesisRegistry."""

    @pytest.fixture(autouse=True)
    def reset_registry(self):
        """Reset global registry before each test."""
        reset_synthesis_registry()
        yield
        reset_synthesis_registry()

    def test_register_generator(self):
        """Test registering a generator."""
        registry = SynthesisRegistry()
        gen = PlaceholderGenerator()

        registry.generators.register(gen)

        assert registry.generators.get("placeholder") is gen

    def test_register_validator(self):
        """Test registering a validator."""
        registry = SynthesisRegistry()
        val = TestValidator()

        registry.validators.register(val)

        assert registry.validators.get("pytest") is val

    def test_register_library(self):
        """Test registering a library."""
        registry = SynthesisRegistry()
        lib = MemoryLibrary()

        registry.libraries.register(lib)

        assert registry.libraries.get("memory") is lib

    def test_find_best_generator(self, spec, context):
        """Test finding best generator for spec."""
        registry = SynthesisRegistry()
        registry.generators.register(PlaceholderGenerator())
        registry.generators.register(TemplateGenerator())

        best = registry.generators.find_best(spec, context)

        # Template has higher priority
        assert best is not None
        assert best.metadata.name == "template"

    def test_find_all_applicable_validators(self, spec):
        """Test finding all applicable validators."""
        registry = SynthesisRegistry()
        registry.validators.register(TestValidator())
        registry.validators.register(TypeValidator())

        code = "def add(a: int, b: int) -> int: return a + b"
        applicable = registry.validators.find_all_applicable(spec, code)

        # Both should be applicable for typed code with examples
        assert len(applicable) >= 1

    def test_register_builtins(self):
        """Test registering built-in plugins."""
        registry = SynthesisRegistry()
        registry.register_builtins()

        # Should have at least placeholder generator
        assert len(registry.generators.get_all()) >= 1

    def test_global_registry(self):
        """Test global registry singleton."""
        registry1 = get_synthesis_registry()
        registry2 = get_synthesis_registry()

        assert registry1 is registry2

    def test_duplicate_registration_raises(self):
        """Test that duplicate registration raises error."""
        registry = SynthesisRegistry()
        gen = PlaceholderGenerator()

        registry.generators.register(gen)

        with pytest.raises(ValueError, match="already registered"):
            registry.generators.register(gen)


class TestStrategyRegistry:
    """Tests for StrategyRegistry."""

    def test_register_and_get(self):
        """Test registering and retrieving a strategy."""
        registry = StrategyRegistry()
        strategy = TypeDrivenDecomposition()

        registry.register(strategy)

        assert registry.get("type_driven") is strategy
        assert registry.is_enabled("type_driven") is True

    def test_unregister(self):
        """Test unregistering a strategy."""
        registry = StrategyRegistry()
        strategy = TypeDrivenDecomposition()
        registry.register(strategy)

        registry.unregister("type_driven")

        assert registry.get("type_driven") is None
        assert registry.is_enabled("type_driven") is False

    def test_enable_disable(self):
        """Test enabling and disabling strategies."""
        registry = StrategyRegistry()
        strategy = TypeDrivenDecomposition()
        registry.register(strategy)

        registry.disable("type_driven")
        assert registry.is_enabled("type_driven") is False
        assert "type_driven" in registry.get_disabled()

        registry.enable("type_driven")
        assert registry.is_enabled("type_driven") is True
        assert "type_driven" in registry.get_enabled()

    def test_get_all(self):
        """Test getting all strategies."""
        registry = StrategyRegistry()
        strategy = TypeDrivenDecomposition()
        registry.register(strategy)

        # Enabled only (default)
        all_enabled = registry.get_all(enabled_only=True)
        assert len(all_enabled) == 1

        # Disable and check
        registry.disable("type_driven")
        all_enabled = registry.get_all(enabled_only=True)
        assert len(all_enabled) == 0

        # All regardless of status
        all_strategies = registry.get_all(enabled_only=False)
        assert len(all_strategies) == 1

    def test_register_builtins(self):
        """Test registering built-in strategies."""
        registry = StrategyRegistry()
        registry.register_builtins()

        # Should have type_driven, test_driven, pattern_based
        all_strategies = registry.get_all()
        names = [s.name for s in all_strategies]
        assert "type_driven" in names
        assert "test_driven" in names
        assert "pattern_based" in names


class TestComponentGenerator:
    """Tests for ComponentGenerator (SyPet/InSynth style)."""

    @pytest.fixture
    def component_gen(self):
        """Create ComponentGenerator instance."""
        from moss.synthesis.plugins.generators import ComponentGenerator

        return ComponentGenerator(max_depth=3, max_solutions=3)

    @pytest.fixture
    def library_context(self) -> Context:
        """Context with library functions for composition."""
        return Context(
            primitives=("len", "sum", "sorted", "str", "int"),
            library={
                "double": {"type": "(int) -> int", "description": "Double a number"},
                "stringify": {"type": "(int) -> str", "description": "Convert int to str"},
                "parse_int": {"type": "(str) -> int", "description": "Parse string to int"},
            },
        )

    def test_metadata(self, component_gen):
        """Test generator metadata."""
        assert component_gen.metadata.name == "component"
        assert component_gen.metadata.generator_type == GeneratorType.RELATIONAL
        assert component_gen.metadata.priority == 12  # Between template and LLM

    def test_can_generate_requires_type_signature(self, component_gen, library_context):
        """Test can_generate requires type signature."""
        # No type signature
        spec = Specification(description="test")
        assert component_gen.can_generate(spec, library_context) is False

        # With type signature
        spec = Specification(description="test", type_signature="(int) -> str")
        assert component_gen.can_generate(spec, library_context) is True

    def test_can_generate_requires_library(self, component_gen):
        """Test can_generate requires library."""
        spec = Specification(description="test", type_signature="(int) -> str")
        empty_context = Context()
        assert component_gen.can_generate(spec, empty_context) is False

    @pytest.mark.asyncio
    async def test_generate_identity(self, component_gen, library_context):
        """Test generating identity function (direct return)."""
        spec = Specification(
            description="return the input integer",
            type_signature="(int) -> int",
        )

        result = await component_gen.generate(spec, library_context)

        # Should find a path (even if trivial)
        assert result.success is True
        assert result.code is not None
        assert "def" in result.code
        assert "return" in result.code

    @pytest.mark.asyncio
    async def test_generate_from_cached(self, component_gen, library_context):
        """Test returning cached solution from context."""
        spec = Specification(description="cached", type_signature="(int) -> int")
        cached_context = library_context.with_solved("cached", "def cached(x): return x")

        result = await component_gen.generate(spec, cached_context)

        assert result.success is True
        assert result.metadata.get("source") == "cached"

    def test_estimate_cost(self, component_gen, library_context):
        """Test cost estimation scales with library size."""
        spec = Specification(description="test", type_signature="(int) -> int")

        cost = component_gen.estimate_cost(spec, library_context)

        # Cost should depend on library size
        assert cost.time_estimate_ms > 50
        assert cost.token_estimate == 0  # No LLM tokens
        assert cost.complexity_score >= 1

    def test_protocol_compliance(self, component_gen):
        """Verify generator implements CodeGenerator protocol."""
        assert isinstance(component_gen, CodeGenerator)

    def test_parse_type_signature(self, component_gen):
        """Test type signature parsing."""
        # Single input
        result = component_gen._parse_type_signature("int -> str")
        assert result == (["int"], "str")

        # Multiple inputs
        result = component_gen._parse_type_signature("(int, str) -> bool")
        assert result == (["int", "str"], "bool")

        # Invalid
        result = component_gen._parse_type_signature("no arrow")
        assert result is None


class TestSMTGenerator:
    """Tests for SMTGenerator (Z3-based Synquid style)."""

    @pytest.fixture
    def smt_gen(self):
        """Create SMTGenerator instance."""
        from moss.synthesis.plugins.generators import SMTGenerator

        return SMTGenerator(max_depth=3, timeout_ms=1000, max_solutions=3)

    @pytest.fixture
    def numeric_context(self) -> Context:
        """Context for numeric synthesis."""
        return Context(primitives=("len", "sum", "max", "min", "abs"))

    def test_metadata(self, smt_gen):
        """Test generator metadata."""
        assert smt_gen.metadata.name == "smt"
        assert smt_gen.metadata.generator_type == GeneratorType.SMT
        assert smt_gen.metadata.priority == 15  # Between component and LLM

    def test_can_generate_requires_z3(self, smt_gen, numeric_context):
        """Test can_generate depends on Z3 availability."""
        spec = Specification(
            description="add two numbers",
            type_signature="(int, int) -> int",
        )

        # Can generate depends on whether Z3 is installed
        result = smt_gen.can_generate(spec, numeric_context)
        # Result is True if Z3 is available, False otherwise
        if smt_gen._z3 is None:
            assert result is False
        else:
            assert result is True

    def test_can_generate_requires_type_signature(self, smt_gen, numeric_context):
        """Test can_generate requires type signature."""
        spec = Specification(description="test")
        assert smt_gen.can_generate(spec, numeric_context) is False

    def test_can_generate_requires_supported_types(self, smt_gen, numeric_context):
        """Test can_generate checks for supported types."""
        # Supported types
        spec = Specification(description="test", type_signature="(int, int) -> bool")
        if smt_gen._z3:
            assert smt_gen.can_generate(spec, numeric_context) is True

        # Unsupported type
        spec = Specification(description="test", type_signature="(CustomType) -> OtherType")
        assert smt_gen.can_generate(spec, numeric_context) is False

    @pytest.mark.asyncio
    async def test_generate_without_z3(self, smt_gen, numeric_context):
        """Test generate returns error when Z3 not available."""
        # Temporarily remove Z3
        original_z3 = smt_gen._z3
        smt_gen._z3 = None

        spec = Specification(
            description="test function",
            type_signature="(int, int) -> int",
        )

        result = await smt_gen.generate(spec, numeric_context)

        assert result.success is False
        assert "z3-solver" in result.error.lower()

        # Restore
        smt_gen._z3 = original_z3

    @pytest.mark.asyncio
    async def test_generate_from_cached(self, smt_gen, numeric_context):
        """Test returning cached solution from context."""
        spec = Specification(description="cached", type_signature="(int) -> int")
        cached_context = numeric_context.with_solved("cached", "def cached(x): return x")

        result = await smt_gen.generate(spec, cached_context)

        assert result.success is True
        assert result.metadata.get("source") == "cached"

    @pytest.mark.asyncio
    async def test_generate_arithmetic(self, smt_gen, numeric_context):
        """Test generating arithmetic operations (if Z3 available)."""
        if smt_gen._z3 is None:
            pytest.skip("Z3 not available")

        spec = Specification(
            description="add function",
            type_signature="(int, int) -> int",
            examples=(((1, 2), 3), ((0, 0), 0), ((-1, 1), 0)),
        )

        result = await smt_gen.generate(spec, numeric_context)

        assert result.success is True
        assert result.code is not None
        assert "def" in result.code

    @pytest.mark.asyncio
    async def test_generate_boolean(self, smt_gen, numeric_context):
        """Test generating boolean operations (if Z3 available)."""
        if smt_gen._z3 is None:
            pytest.skip("Z3 not available")

        spec = Specification(
            description="is_positive function",
            type_signature="(int) -> bool",
            examples=(((5,), True), ((-3,), False), ((0,), False)),
        )

        result = await smt_gen.generate(spec, numeric_context)

        # May or may not find a solution depending on generated candidates
        assert result.code is not None or result.error is not None

    def test_estimate_cost(self, smt_gen, numeric_context):
        """Test cost estimation."""
        spec = Specification(
            description="test",
            type_signature="(int, int) -> int",
            examples=(((1, 2), 3),),
        )

        cost = smt_gen.estimate_cost(spec, numeric_context)

        # Cost should depend on examples and complexity
        assert cost.time_estimate_ms > 100
        assert cost.token_estimate == 0  # No LLM tokens
        assert cost.complexity_score >= 2

    def test_protocol_compliance(self, smt_gen):
        """Verify generator implements CodeGenerator protocol."""
        assert isinstance(smt_gen, CodeGenerator)

    def test_parse_type_signature(self, smt_gen):
        """Test type signature parsing."""
        # Single input
        result = smt_gen._parse_type_signature("int -> bool")
        assert result == (["int"], "bool")

        # Multiple inputs
        result = smt_gen._parse_type_signature("(int, int) -> int")
        assert result == (["int", "int"], "int")

        # Invalid
        result = smt_gen._parse_type_signature("invalid")
        assert result is None

    def test_evaluate_candidate(self, smt_gen):
        """Test candidate expression evaluation."""
        # Single arg
        result = smt_gen._evaluate_candidate("arg0 + 1", 5)
        assert result == 6

        # Multiple args
        result = smt_gen._evaluate_candidate("arg0 + arg1", (3, 4))
        assert result == 7

        # String operation
        result = smt_gen._evaluate_candidate("arg0.upper()", "hello")
        assert result == "HELLO"

    def test_types_compatible(self, smt_gen):
        """Test type compatibility checking."""
        # Same type
        assert smt_gen._types_compatible("int", "int") is True

        # Any compatibility
        assert smt_gen._types_compatible("Any", "int") is True
        assert smt_gen._types_compatible("str", "Any") is True

        # int/float compatibility
        assert smt_gen._types_compatible("int", "float") is True

        # Different types
        assert smt_gen._types_compatible("str", "int") is False
