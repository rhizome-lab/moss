"""Tests for LLM-based code generation."""

import asyncio
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from moss.synthesis.plugins.generators.llm import (
    LiteLLMProvider,
    LLMCostEstimate,
    LLMGenerator,
    LLMGeneratorConfig,
    MockLLMProvider,
    TokenUsage,
    create_llm_generator,
    create_mock_generator,
)
from moss.synthesis.plugins.protocols import CodeGenerator, GenerationCost
from moss.synthesis.types import Context, Specification

# =============================================================================
# Test Fixtures
# =============================================================================


@pytest.fixture
def simple_spec() -> Specification:
    """Simple specification for testing."""
    return Specification(
        description="Add two numbers together",
        type_signature="(int, int) -> int",
    )


@pytest.fixture
def complex_spec() -> Specification:
    """Complex specification with constraints and examples."""
    return Specification(
        description="Sort a list of users by registration date",
        type_signature="List[User] -> List[User]",
        constraints=["Preserve stable ordering for equal dates", "Handle empty lists"],
        examples=[
            (["user1", "user2"], ["user2", "user1"]),
        ],
    )


@pytest.fixture
def context() -> Context:
    """Simple context for testing."""
    return Context(
        primitives=["sorted", "lambda", "key"],
        library={},
    )


@pytest.fixture
def mock_provider() -> MockLLMProvider:
    """Basic mock provider."""
    return MockLLMProvider()


@pytest.fixture
def mock_generator(mock_provider: MockLLMProvider) -> LLMGenerator:
    """LLM generator with mock provider."""
    return LLMGenerator(provider=mock_provider)


# =============================================================================
# TokenUsage Tests
# =============================================================================


class TestTokenUsage:
    """Tests for TokenUsage dataclass."""

    def test_total_tokens(self) -> None:
        usage = TokenUsage(prompt_tokens=100, completion_tokens=50)
        assert usage.total_tokens == 150

    def test_default_values(self) -> None:
        usage = TokenUsage()
        assert usage.prompt_tokens == 0
        assert usage.completion_tokens == 0
        assert usage.total_tokens == 0


# =============================================================================
# LLMCostEstimate Tests
# =============================================================================


class TestLLMCostEstimate:
    """Tests for cost estimation."""

    def test_claude_haiku_cost(self) -> None:
        estimate = LLMCostEstimate.from_model(
            "claude-3-haiku-20240307",
            prompt_tokens=1000,
            completion_tokens=500,
        )
        assert estimate.estimated_prompt_tokens == 1000
        assert estimate.estimated_completion_tokens == 500
        # Haiku: $0.25/1M input, $1.25/1M output
        expected = (1000 * 0.25 + 500 * 1.25) / 1_000_000
        assert abs(estimate.estimated_cost_usd - expected) < 0.0001

    def test_gpt4_cost(self) -> None:
        estimate = LLMCostEstimate.from_model(
            "gpt-4-turbo",
            prompt_tokens=1000,
            completion_tokens=500,
        )
        # GPT-4-turbo: $10/1M input, $30/1M output
        expected = (1000 * 10.0 + 500 * 30.0) / 1_000_000
        assert abs(estimate.estimated_cost_usd - expected) < 0.0001

    def test_unknown_model_fallback(self) -> None:
        estimate = LLMCostEstimate.from_model(
            "unknown-model-xyz",
            prompt_tokens=1000,
            completion_tokens=500,
        )
        # Should use fallback pricing (1.0, 3.0)
        expected = (1000 * 1.0 + 500 * 3.0) / 1_000_000
        assert abs(estimate.estimated_cost_usd - expected) < 0.0001


# =============================================================================
# MockLLMProvider Tests
# =============================================================================


class TestMockLLMProvider:
    """Tests for MockLLMProvider."""

    @pytest.mark.asyncio
    async def test_default_placeholder_generation(self) -> None:
        provider = MockLLMProvider()
        response = await provider.generate(
            [{"role": "user", "content": "Generate function 'add_numbers'"}]
        )
        assert response.content
        assert "add_numbers" in response.content
        assert "NotImplementedError" in response.content

    @pytest.mark.asyncio
    async def test_custom_responses(self) -> None:
        provider = MockLLMProvider(responses=["def foo(): return 42"])
        response = await provider.generate([{"role": "user", "content": "test"}])
        assert response.content == "def foo(): return 42"

    @pytest.mark.asyncio
    async def test_response_cycling(self) -> None:
        provider = MockLLMProvider(responses=["first", "second"])

        r1 = await provider.generate([{"role": "user", "content": "test"}])
        assert r1.content == "first"

        r2 = await provider.generate([{"role": "user", "content": "test"}])
        assert r2.content == "second"

        # Cycles back
        r3 = await provider.generate([{"role": "user", "content": "test"}])
        assert r3.content == "first"

    @pytest.mark.asyncio
    async def test_call_history_tracking(self) -> None:
        provider = MockLLMProvider()
        messages = [{"role": "user", "content": "test prompt"}]

        await provider.generate(messages, max_tokens=100, temperature=0.5)

        assert len(provider.call_history) == 1
        assert provider.call_history[0]["messages"] == messages
        assert provider.call_history[0]["max_tokens"] == 100
        assert provider.call_history[0]["temperature"] == 0.5

    @pytest.mark.asyncio
    async def test_token_usage(self) -> None:
        provider = MockLLMProvider(responses=["short response"])
        response = await provider.generate([{"role": "user", "content": "a longer prompt"}])

        assert response.usage.prompt_tokens > 0
        assert response.usage.completion_tokens > 0

    @pytest.mark.asyncio
    async def test_delay_simulation(self) -> None:
        provider = MockLLMProvider(delay_ms=50)

        start = asyncio.get_event_loop().time()
        await provider.generate([{"role": "user", "content": "test"}])
        elapsed = asyncio.get_event_loop().time() - start

        assert elapsed >= 0.04  # Allow some tolerance

    @pytest.mark.asyncio
    async def test_streaming(self) -> None:
        provider = MockLLMProvider(responses=["hello world"])
        chunks = []

        async for chunk in provider.stream([{"role": "user", "content": "test"}]):
            chunks.append(chunk)

        assert "".join(chunks) == "hello world"

    def test_reset(self) -> None:
        provider = MockLLMProvider(responses=["a", "b"])
        provider.call_history.append({"test": True})
        provider._response_index = 1

        provider.reset()

        assert len(provider.call_history) == 0
        assert provider._response_index == 0

    def test_set_responses(self) -> None:
        provider = MockLLMProvider(responses=["old"])
        provider.set_responses(["new1", "new2"])

        assert provider._responses == ["new1", "new2"]
        assert provider._response_index == 0

    def test_properties(self) -> None:
        provider = MockLLMProvider(model="test-model")
        assert provider.model == "test-model"
        assert provider.supports_streaming is True

    def test_estimate_tokens(self) -> None:
        provider = MockLLMProvider()
        # 4 chars per token average
        assert provider.estimate_tokens("hello") == 2  # 5 chars / 4 + 1
        assert provider.estimate_tokens("hello world test") == 5  # 16 chars / 4 + 1


# =============================================================================
# LiteLLMProvider Tests
# =============================================================================


class TestLiteLLMProvider:
    """Tests for LiteLLMProvider (mocked LiteLLM calls)."""

    def test_init(self) -> None:
        provider = LiteLLMProvider(model="claude-3-haiku-20240307")
        assert provider.model == "claude-3-haiku-20240307"
        assert provider.supports_streaming is True

    def test_missing_litellm_import(self) -> None:
        provider = LiteLLMProvider()

        with patch.dict("sys.modules", {"litellm": None}):
            provider._litellm = None  # Reset cached import
            with pytest.raises(ImportError, match="LiteLLM is required"):
                provider._ensure_litellm()

    @pytest.mark.asyncio
    async def test_generate_with_mocked_litellm(self) -> None:
        provider = LiteLLMProvider(model="test-model")

        # Mock the litellm module
        mock_response = MagicMock()
        mock_response.choices = [MagicMock()]
        mock_response.choices[0].message.content = "def test(): pass"
        mock_response.choices[0].finish_reason = "stop"
        mock_response.model = "test-model"
        mock_response.usage = MagicMock()
        mock_response.usage.prompt_tokens = 10
        mock_response.usage.completion_tokens = 5

        mock_litellm = MagicMock()
        mock_litellm.acompletion = AsyncMock(return_value=mock_response)
        provider._litellm = mock_litellm

        response = await provider.generate([{"role": "user", "content": "test"}])

        assert response.content == "def test(): pass"
        assert response.usage.prompt_tokens == 10
        assert response.usage.completion_tokens == 5
        assert response.finish_reason == "stop"

    @pytest.mark.asyncio
    async def test_stream_with_mocked_litellm(self) -> None:
        provider = LiteLLMProvider(model="test-model")

        # Mock streaming response
        async def mock_stream():
            for text in ["hello", " ", "world"]:
                chunk = MagicMock()
                chunk.choices = [MagicMock()]
                chunk.choices[0].delta.content = text
                yield chunk

        mock_litellm = MagicMock()
        mock_litellm.acompletion = AsyncMock(return_value=mock_stream())
        provider._litellm = mock_litellm

        chunks = []
        async for chunk in provider.stream([{"role": "user", "content": "test"}]):
            chunks.append(chunk)

        assert "".join(chunks) == "hello world"


# =============================================================================
# LLMGenerator Tests
# =============================================================================


class TestLLMGenerator:
    """Tests for LLMGenerator."""

    def test_protocol_compliance(self) -> None:
        generator = LLMGenerator(provider=MockLLMProvider())
        assert isinstance(generator, CodeGenerator)

    def test_metadata(self) -> None:
        provider = MockLLMProvider(model="my-model")
        generator = LLMGenerator(provider=provider)

        assert generator.metadata.name == "llm"
        assert "my-model" in generator.metadata.description

    def test_can_generate_always_true(
        self,
        mock_generator: LLMGenerator,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        assert mock_generator.can_generate(simple_spec, context) is True

    def test_can_generate_respects_budget(
        self,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        config = LLMGeneratorConfig(budget_usd=1.0)  # $1 budget
        generator = LLMGenerator(
            provider=MockLLMProvider(),
            config=config,
        )

        # First call should work (budget not exceeded)
        assert generator.can_generate(simple_spec, context) is True

        # Simulate exceeding budget
        generator._total_cost_usd = 1.5  # Exceeded $1 budget

        # Should now reject (budget exceeded)
        assert generator.can_generate(simple_spec, context) is False

    @pytest.mark.asyncio
    async def test_generate_simple(
        self,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        provider = MockLLMProvider(responses=["def add(a, b): return a + b"])
        generator = LLMGenerator(provider=provider)

        result = await generator.generate(simple_spec, context)

        assert result.success is True
        assert result.code == "def add(a, b): return a + b"
        assert result.metadata["source"] == "llm"

    @pytest.mark.asyncio
    async def test_generate_extracts_markdown_code(
        self,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        provider = MockLLMProvider(
            responses=["Here's the code:\n\n```python\ndef add(a, b):\n    return a + b\n```"]
        )
        generator = LLMGenerator(provider=provider)

        result = await generator.generate(simple_spec, context)

        assert result.success is True
        assert "def add(a, b):" in result.code
        assert "```" not in result.code

    @pytest.mark.asyncio
    async def test_generate_with_complex_spec(
        self,
        complex_spec: Specification,
        context: Context,
    ) -> None:
        provider = MockLLMProvider()
        generator = LLMGenerator(provider=provider)

        result = await generator.generate(complex_spec, context)

        assert result.success is True
        # Check that prompt included constraints
        call = provider.call_history[0]
        user_msg = call["messages"][1]["content"]
        assert "stable ordering" in user_msg.lower()

    @pytest.mark.asyncio
    async def test_generate_uses_hints(
        self,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        from moss.synthesis.plugins.protocols import Abstraction, GenerationHints

        hints = GenerationHints(
            preferred_style="functional",
            constraints=["Use list comprehensions"],
            abstractions=[Abstraction(name="map", code="", description="Map function")],
        )

        provider = MockLLMProvider()
        generator = LLMGenerator(provider=provider)

        await generator.generate(simple_spec, context, hints)

        call = provider.call_history[0]
        user_msg = call["messages"][1]["content"]
        assert "functional" in user_msg
        assert "list comprehensions" in user_msg
        assert "map" in user_msg.lower()

    @pytest.mark.asyncio
    async def test_generate_tracks_cost(
        self,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        provider = MockLLMProvider(responses=["def f(): pass"])
        generator = LLMGenerator(provider=provider)

        assert generator.total_cost_usd == 0.0

        result = await generator.generate(simple_spec, context)

        assert generator.total_cost_usd > 0
        assert result.metadata.get("cost_usd", 0) > 0

    @pytest.mark.asyncio
    async def test_generate_retries_on_error(
        self,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        # Create provider that fails first, then succeeds
        provider = MockLLMProvider()

        call_count = 0
        original_generate = provider.generate

        async def flaky_generate(*args, **kwargs):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                raise RuntimeError("Temporary error")
            return await original_generate(*args, **kwargs)

        provider.generate = flaky_generate

        config = LLMGeneratorConfig(max_retries=2)
        generator = LLMGenerator(provider=provider, config=config)

        result = await generator.generate(simple_spec, context)

        assert result.success is True
        assert call_count == 2

    @pytest.mark.asyncio
    async def test_generate_fails_after_retries(
        self,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        provider = MockLLMProvider()

        async def always_fail(*args, **kwargs):
            raise RuntimeError("Persistent error")

        provider.generate = always_fail

        config = LLMGeneratorConfig(max_retries=1)
        generator = LLMGenerator(provider=provider, config=config)

        result = await generator.generate(simple_spec, context)

        assert result.success is False
        assert "Persistent error" in result.error

    @pytest.mark.asyncio
    async def test_generate_stream(
        self,
        simple_spec: Specification,
        context: Context,
    ) -> None:
        provider = MockLLMProvider(responses=["def add(a, b): return a + b"])
        generator = LLMGenerator(provider=provider)

        chunks = []
        async for chunk in generator.generate_stream(simple_spec, context):
            chunks.append(chunk)

        assert "".join(chunks) == "def add(a, b): return a + b"

    def test_estimate_cost(
        self,
        simple_spec: Specification,
        context: Context,
        mock_generator: LLMGenerator,
    ) -> None:
        cost = mock_generator.estimate_cost(simple_spec, context)

        assert isinstance(cost, GenerationCost)
        assert cost.time_estimate_ms > 0
        assert cost.token_estimate > 0
        assert cost.complexity_score > 0

    def test_reset_cost_tracking(self, mock_generator: LLMGenerator) -> None:
        mock_generator._total_cost_usd = 1.0

        mock_generator.reset_cost_tracking()

        assert mock_generator.total_cost_usd == 0.0

    def test_custom_system_prompt(self) -> None:
        config = LLMGeneratorConfig(system_prompt="Custom prompt here")
        generator = LLMGenerator(
            provider=MockLLMProvider(),
            config=config,
        )

        assert generator._config.system_prompt == "Custom prompt here"


# =============================================================================
# Factory Function Tests
# =============================================================================


class TestFactoryFunctions:
    """Tests for factory functions."""

    def test_create_llm_generator_mock(self) -> None:
        generator = create_llm_generator(mock=True)

        assert isinstance(generator.provider, MockLLMProvider)

    def test_create_llm_generator_with_model(self) -> None:
        generator = create_llm_generator(mock=True, model="test-model")

        assert generator.provider.model == "test-model"

    def test_create_llm_generator_with_responses(self) -> None:
        generator = create_llm_generator(
            mock=True,
            responses=["response1", "response2"],
        )

        assert isinstance(generator.provider, MockLLMProvider)
        assert generator.provider._responses == ["response1", "response2"]

    def test_create_llm_generator_production(self) -> None:
        generator = create_llm_generator(
            mock=False,
            model="claude-3-haiku-20240307",
        )

        assert isinstance(generator.provider, LiteLLMProvider)
        assert generator.provider.model == "claude-3-haiku-20240307"

    def test_create_mock_generator(self) -> None:
        generator = create_mock_generator(
            responses=["def foo(): pass"],
            delay_ms=10,
        )

        assert isinstance(generator.provider, MockLLMProvider)
        assert generator.provider._responses == ["def foo(): pass"]
        assert generator.provider._delay_ms == 10


# =============================================================================
# Code Extraction Tests
# =============================================================================


class TestCodeExtraction:
    """Tests for code extraction from LLM responses."""

    @pytest.mark.asyncio
    async def test_extract_from_plain_code(self) -> None:
        provider = MockLLMProvider(responses=["def hello():\n    print('world')"])
        generator = LLMGenerator(provider=provider)
        spec = Specification(description="test")
        context = Context()

        result = await generator.generate(spec, context)

        assert "def hello():" in result.code

    @pytest.mark.asyncio
    async def test_extract_from_markdown_python(self) -> None:
        provider = MockLLMProvider(responses=["```python\ndef hello():\n    pass\n```"])
        generator = LLMGenerator(provider=provider)
        spec = Specification(description="test")
        context = Context()

        result = await generator.generate(spec, context)

        assert result.code == "def hello():\n    pass"

    @pytest.mark.asyncio
    async def test_extract_from_markdown_no_language(self) -> None:
        provider = MockLLMProvider(responses=["```\ndef hello():\n    pass\n```"])
        generator = LLMGenerator(provider=provider)
        spec = Specification(description="test")
        context = Context()

        result = await generator.generate(spec, context)

        assert result.code == "def hello():\n    pass"

    @pytest.mark.asyncio
    async def test_extract_function_from_prose(self) -> None:
        response = (
            "Here's a function that adds numbers:\n"
            "def add(a, b):\n    return a + b\n\n"
            "This works great!"
        )
        provider = MockLLMProvider(responses=[response])
        generator = LLMGenerator(provider=provider)
        spec = Specification(description="test")
        context = Context()

        result = await generator.generate(spec, context)

        assert "def add(a, b):" in result.code
        assert "return a + b" in result.code

    @pytest.mark.asyncio
    async def test_extract_class(self) -> None:
        provider = MockLLMProvider(responses=["class Foo:\n    def bar(self):\n        pass"])
        generator = LLMGenerator(provider=provider)
        spec = Specification(description="test")
        context = Context()

        result = await generator.generate(spec, context)

        assert "class Foo:" in result.code

    @pytest.mark.asyncio
    async def test_extract_async_function(self) -> None:
        provider = MockLLMProvider(responses=["async def fetch():\n    await something()"])
        generator = LLMGenerator(provider=provider)
        spec = Specification(description="test")
        context = Context()

        result = await generator.generate(spec, context)

        assert "async def fetch():" in result.code

    @pytest.mark.asyncio
    async def test_extract_decorated_function(self) -> None:
        provider = MockLLMProvider(responses=["@decorator\ndef func():\n    pass"])
        generator = LLMGenerator(provider=provider)
        spec = Specification(description="test")
        context = Context()

        result = await generator.generate(spec, context)

        assert "@decorator" in result.code


# =============================================================================
# Integration Tests
# =============================================================================


class TestLLMGeneratorIntegration:
    """Integration tests for LLM generator."""

    @pytest.mark.asyncio
    async def test_full_workflow(self) -> None:
        """Test complete generation workflow."""
        # Setup
        response = (
            "```python\ndef calculate_sum(numbers: list[int]) -> int:\n    return sum(numbers)\n```"
        )
        provider = MockLLMProvider(responses=[response])
        generator = LLMGenerator(provider=provider)

        spec = Specification(
            description="Calculate the sum of a list of numbers",
            type_signature="list[int] -> int",
            examples=[([1, 2, 3], 6)],
        )
        context = Context(primitives=["sum", "reduce"])

        # Check can generate
        assert generator.can_generate(spec, context)

        # Estimate cost
        cost = generator.estimate_cost(spec, context)
        assert cost.token_estimate > 0

        # Generate
        result = await generator.generate(spec, context)

        # Verify result
        assert result.success
        assert "calculate_sum" in result.code
        assert "return sum(numbers)" in result.code

        # Verify cost tracking
        assert generator.total_cost_usd > 0

        # Verify call history
        assert len(provider.call_history) == 1
        call = provider.call_history[0]
        assert "sum" in call["messages"][1]["content"].lower()
