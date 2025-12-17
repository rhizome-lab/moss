"""Tests for the synthesis framework."""

from __future__ import annotations

import pytest

from moss.synthesis import (
    AtomicStrategy,
    CodeComposer,
    CompositionError,
    Context,
    DecompositionError,
    DecompositionStrategy,
    FunctionComposer,
    NoStrategyError,
    SequentialComposer,
    Specification,
    StrategyMetadata,
    StrategyRouter,
    Subproblem,
    SynthesisConfig,
    SynthesisError,
    SynthesisFramework,
    SynthesisResult,
    ValidationError,
    create_synthesis_framework,
)

# =============================================================================
# Test Types
# =============================================================================


class TestSpecification:
    """Tests for Specification dataclass."""

    def test_create_basic(self):
        spec = Specification(description="Add two numbers")
        assert spec.description == "Add two numbers"
        assert spec.type_signature is None
        assert spec.examples == ()
        assert spec.tests == ()
        assert spec.constraints == ()

    def test_create_with_type(self):
        spec = Specification(
            description="Add two numbers",
            type_signature="(int, int) -> int",
        )
        assert spec.type_signature == "(int, int) -> int"

    def test_create_with_examples(self):
        spec = Specification(
            description="Add two numbers",
            examples=((2, 3, 5), (0, 0, 0)),
        )
        assert len(spec.examples) == 2

    def test_summary(self):
        spec = Specification(
            description="Add two numbers",
            type_signature="(int, int) -> int",
        )
        summary = spec.summary()
        assert "Add two numbers" in summary
        assert "(int, int) -> int" in summary

    def test_summary_truncates_long_description(self):
        spec = Specification(description="A" * 100)
        summary = spec.summary()
        assert "..." in summary
        assert len(summary) < 100

    def test_with_examples(self):
        spec = Specification(description="test")
        new_spec = spec.with_examples([((1, 2), 3)])
        assert len(new_spec.examples) == 1
        assert spec.examples == ()  # Original unchanged

    def test_with_constraints(self):
        spec = Specification(description="test")
        new_spec = spec.with_constraints(["x > 0"])
        assert len(new_spec.constraints) == 1
        assert spec.constraints == ()  # Original unchanged


class TestContext:
    """Tests for Context dataclass."""

    def test_create_empty(self):
        ctx = Context()
        assert ctx.primitives == ()
        assert ctx.library == {}
        assert ctx.solved == {}

    def test_create_with_primitives(self):
        ctx = Context(primitives=("add", "sub", "mul"))
        assert "add" in ctx.primitives

    def test_with_solved(self):
        ctx = Context()
        new_ctx = ctx.with_solved("add", lambda x, y: x + y)
        assert "add" in new_ctx.solved
        assert ctx.solved == {}  # Original unchanged

    def test_extend(self):
        ctx = Context(primitives=("add",))
        new_ctx = ctx.extend(primitives=["sub"])
        assert "add" in new_ctx.primitives
        assert "sub" in new_ctx.primitives


class TestSubproblem:
    """Tests for Subproblem dataclass."""

    def test_create_basic(self):
        spec = Specification(description="test")
        sub = Subproblem(specification=spec)
        assert sub.specification == spec
        assert sub.dependencies == ()
        assert sub.priority == 0

    def test_create_with_dependencies(self):
        spec = Specification(description="test")
        sub = Subproblem(specification=spec, dependencies=(0, 1))
        assert sub.dependencies == (0, 1)

    def test_summary(self):
        spec = Specification(description="test")
        sub = Subproblem(specification=spec, dependencies=(0,))
        summary = sub.summary()
        assert "test" in summary
        assert "deps" in summary


class TestSynthesisResult:
    """Tests for SynthesisResult dataclass."""

    def test_success_result(self):
        result = SynthesisResult(
            success=True,
            solution="def add(x, y): return x + y",
            iterations=5,
            strategy_used="atomic",
        )
        assert result.success
        assert result.solution is not None
        assert result.error is None

    def test_failure_result(self):
        result = SynthesisResult(
            success=False,
            error="No strategy found",
            iterations=10,
        )
        assert not result.success
        assert result.error is not None

    def test_str_success(self):
        result = SynthesisResult(success=True, iterations=5, strategy_used="atomic")
        s = str(result)
        assert "success" in s
        assert "5" in s
        assert "atomic" in s

    def test_str_failure(self):
        result = SynthesisResult(success=False, error="failed")
        s = str(result)
        assert "failed" in s


class TestExceptions:
    """Tests for synthesis exceptions."""

    def test_synthesis_error(self):
        err = SynthesisError("test error", iterations=5)
        assert str(err) == "test error"
        assert err.iterations == 5

    def test_no_strategy_error(self):
        err = NoStrategyError("no strategy")
        assert isinstance(err, SynthesisError)

    def test_decomposition_error(self):
        err = DecompositionError("decomposition failed")
        assert isinstance(err, SynthesisError)

    def test_composition_error(self):
        err = CompositionError("composition failed")
        assert isinstance(err, SynthesisError)

    def test_validation_error(self):
        err = ValidationError("validation failed")
        assert isinstance(err, SynthesisError)


# =============================================================================
# Test Strategy
# =============================================================================


class TestAtomicStrategy:
    """Tests for AtomicStrategy."""

    def test_metadata(self):
        strategy = AtomicStrategy()
        assert strategy.name == "atomic"
        assert "atomic" in strategy.description.lower()

    def test_can_handle_primitive(self):
        strategy = AtomicStrategy()
        spec = Specification(description="add two numbers")
        ctx = Context(primitives=("add",))
        assert strategy.can_handle(spec, ctx)

    def test_can_handle_solved(self):
        strategy = AtomicStrategy()
        spec = Specification(description="add two numbers")
        ctx = Context(solved={"add two numbers": lambda x, y: x + y})
        assert strategy.can_handle(spec, ctx)

    def test_can_handle_simple(self):
        strategy = AtomicStrategy()
        spec = Specification(description="simple task")
        ctx = Context()
        assert strategy.can_handle(spec, ctx)

    def test_decompose_returns_empty(self):
        strategy = AtomicStrategy()
        spec = Specification(description="simple")
        ctx = Context()
        result = strategy.decompose(spec, ctx)
        assert result == []

    def test_estimate_success_solved(self):
        strategy = AtomicStrategy()
        spec = Specification(description="test")
        ctx = Context(solved={"test": "solution"})
        assert strategy.estimate_success(spec, ctx) == 1.0

    def test_estimate_success_primitive(self):
        strategy = AtomicStrategy()
        spec = Specification(description="use add")
        ctx = Context(primitives=("add",))
        assert strategy.estimate_success(spec, ctx) == 0.9

    def test_document(self):
        strategy = AtomicStrategy()
        doc = strategy.document()
        assert "atomic" in doc


# =============================================================================
# Test Composer
# =============================================================================


class TestSequentialComposer:
    """Tests for SequentialComposer."""

    def test_name(self):
        composer = SequentialComposer()
        assert composer.name == "sequential"

    @pytest.mark.asyncio
    async def test_compose_strings(self):
        composer = SequentialComposer()
        spec = Specification(description="test")
        result = await composer.compose(["part1", "part2"], spec)
        assert "part1" in result
        assert "part2" in result

    @pytest.mark.asyncio
    async def test_compose_lists(self):
        composer = SequentialComposer()
        spec = Specification(description="test")
        result = await composer.compose([[1, 2], [3, 4]], spec)
        assert result == [1, 2, 3, 4]

    @pytest.mark.asyncio
    async def test_compose_empty(self):
        composer = SequentialComposer()
        spec = Specification(description="test")
        result = await composer.compose([], spec)
        assert result is None


class TestFunctionComposer:
    """Tests for FunctionComposer."""

    def test_name(self):
        composer = FunctionComposer()
        assert composer.name == "function"

    @pytest.mark.asyncio
    async def test_compose_single(self):
        composer = FunctionComposer()
        spec = Specification(description="test")

        def fn(x: int) -> int:
            return x + 1

        result = await composer.compose([fn], spec)
        assert result(5) == 6

    @pytest.mark.asyncio
    async def test_compose_chain(self):
        composer = FunctionComposer()
        spec = Specification(description="test")

        def fn1(x: int) -> int:
            return x + 1

        def fn2(x: int) -> int:
            return x * 2

        result = await composer.compose([fn1, fn2], spec)
        assert result(5) == 12  # (5 + 1) * 2


class TestCodeComposer:
    """Tests for CodeComposer."""

    def test_name(self):
        composer = CodeComposer()
        assert composer.name == "code"

    @pytest.mark.asyncio
    async def test_compose_code(self):
        composer = CodeComposer()
        spec = Specification(description="Math functions")
        code1 = "import math\n\ndef add(x, y):\n    return x + y"
        code2 = "import math\n\ndef sub(x, y):\n    return x - y"
        result = await composer.compose([code1, code2], spec)
        assert "import math" in result
        assert "def add" in result
        assert "def sub" in result
        # Imports should be deduplicated
        assert result.count("import math") == 1

    @pytest.mark.asyncio
    async def test_compose_adds_docstring(self):
        composer = CodeComposer()
        spec = Specification(description="Test module")
        result = await composer.compose(["x = 1"], spec)
        assert '"""Test module"""' in result


# =============================================================================
# Test Router
# =============================================================================


class MockStrategy(DecompositionStrategy):
    """Mock strategy for testing."""

    def __init__(self, name: str, keywords: tuple[str, ...] = ()):
        self._name = name
        self._keywords = keywords
        self._can_handle = True
        self._estimate = 0.5

    @property
    def metadata(self) -> StrategyMetadata:
        return StrategyMetadata(
            name=self._name,
            description=f"Mock strategy {self._name}",
            keywords=self._keywords,
        )

    def can_handle(self, spec: Specification, context: Context) -> bool:
        return self._can_handle

    def decompose(self, spec: Specification, context: Context) -> list[Subproblem]:
        return []

    def estimate_success(self, spec: Specification, context: Context) -> float:
        return self._estimate


class TestStrategyRouter:
    """Tests for StrategyRouter."""

    def test_create_router(self):
        strategies = [MockStrategy("test")]
        router = StrategyRouter(strategies)
        assert len(router.strategies) == 1

    def test_add_strategy(self):
        router = StrategyRouter([])
        router.add_strategy(MockStrategy("test"))
        assert len(router.strategies) == 1

    @pytest.mark.asyncio
    async def test_select_strategy(self):
        strategies = [MockStrategy("type", ("type", "typed"))]
        router = StrategyRouter(strategies)
        spec = Specification(description="type-based task")
        ctx = Context()
        strategy = await router.select_strategy(spec, ctx)
        assert strategy.name == "type"

    @pytest.mark.asyncio
    async def test_select_no_strategy(self):
        strategy = MockStrategy("test")
        strategy._can_handle = False
        router = StrategyRouter([strategy])
        spec = Specification(description="task")
        ctx = Context()
        with pytest.raises(NoStrategyError):
            await router.select_strategy(spec, ctx)

    @pytest.mark.asyncio
    async def test_rank_strategies(self):
        s1 = MockStrategy("first", ("first", "priority"))
        s1._estimate = 0.9
        s2 = MockStrategy("second", ("second",))
        s2._estimate = 0.3
        router = StrategyRouter([s1, s2])
        spec = Specification(description="first priority task")
        ctx = Context()
        matches = await router.rank_strategies(spec, ctx)
        assert len(matches) == 2
        assert matches[0].strategy.name == "first"


# =============================================================================
# Test Framework
# =============================================================================


class TestSynthesisFramework:
    """Tests for SynthesisFramework."""

    def test_create_framework(self):
        framework = SynthesisFramework()
        assert framework.strategies is not None
        assert framework.composer is not None
        assert framework.router is not None

    def test_create_with_config(self):
        config = SynthesisConfig(max_iterations=100, max_depth=10)
        framework = SynthesisFramework(config=config)
        assert framework.config.max_iterations == 100
        assert framework.config.max_depth == 10

    @pytest.mark.asyncio
    async def test_synthesize_atomic(self):
        framework = create_synthesis_framework()
        spec = Specification(description="simple task")
        ctx = Context()
        result = await framework.synthesize(spec, ctx)
        assert result.success
        assert result.strategy_used == "atomic"

    @pytest.mark.asyncio
    async def test_synthesize_with_primitive(self):
        framework = create_synthesis_framework()
        spec = Specification(description="use add function")
        ctx = Context(primitives=("add",))
        result = await framework.synthesize(spec, ctx)
        assert result.success

    @pytest.mark.asyncio
    async def test_synthesize_with_solved(self):
        framework = create_synthesis_framework()
        spec = Specification(description="solved problem")
        ctx = Context(solved={"solved problem": "precomputed solution"})
        result = await framework.synthesize(spec, ctx)
        assert result.success
        assert result.solution == "precomputed solution"


class TestCreateSynthesisFramework:
    """Tests for create_synthesis_framework factory."""

    def test_creates_framework(self):
        framework = create_synthesis_framework()
        assert isinstance(framework, SynthesisFramework)

    def test_creates_with_strategies(self):
        strategies = [MockStrategy("custom")]
        framework = create_synthesis_framework(strategies=strategies)
        assert len(framework.strategies) == 1

    def test_creates_with_config(self):
        config = SynthesisConfig(max_iterations=200)
        framework = create_synthesis_framework(config=config)
        assert framework.config.max_iterations == 200


# =============================================================================
# Test Integration
# =============================================================================


class TestSynthesisIntegration:
    """Integration tests for the synthesis framework."""

    @pytest.mark.asyncio
    async def test_full_synthesis_flow(self):
        """Test a complete synthesis flow."""
        framework = create_synthesis_framework()

        spec = Specification(
            description="Add two numbers",
            type_signature="(int, int) -> int",
            examples=(((2, 3), 5), ((0, 0), 0)),
        )

        ctx = Context(
            primitives=("add", "+", "return"),
        )

        result = await framework.synthesize(spec, ctx)

        assert result.success
        assert result.iterations > 0
        assert result.strategy_used is not None

    @pytest.mark.asyncio
    async def test_synthesis_tracks_iterations(self):
        """Test that synthesis tracks iteration count."""
        framework = create_synthesis_framework()
        spec = Specification(description="test")
        result = await framework.synthesize(spec)
        assert result.iterations >= 1

    @pytest.mark.asyncio
    async def test_synthesis_respects_max_depth(self):
        """Test that synthesis respects max depth config."""
        config = SynthesisConfig(max_depth=1)
        framework = create_synthesis_framework(config=config)
        spec = Specification(description="test")
        # Should still work for atomic problems
        result = await framework.synthesize(spec)
        assert result.success
