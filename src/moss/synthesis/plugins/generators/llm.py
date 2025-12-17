"""LLM-based code generator.

Generates code using Large Language Models via LiteLLM for unified access
to multiple providers (Anthropic, OpenAI, Cohere, etc.).

Features:
- Unified provider interface via LiteLLM
- Mock provider for testing (no API calls needed)
- Streaming support
- Cost estimation and budgeting
- Configurable prompts and parameters

Usage:
    from moss.synthesis.plugins.generators.llm import (
        LLMGenerator,
        MockLLMProvider,
        LiteLLMProvider,
    )

    # For testing (no API calls)
    generator = LLMGenerator(provider=MockLLMProvider())

    # For production (uses ANTHROPIC_API_KEY or OPENAI_API_KEY env vars)
    generator = LLMGenerator(provider=LiteLLMProvider(model="claude-3-haiku-20240307"))
"""

from __future__ import annotations

import asyncio
from abc import ABC, abstractmethod
from collections.abc import AsyncIterator
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


# =============================================================================
# Cost Estimation
# =============================================================================


@dataclass
class TokenUsage:
    """Token usage from an LLM call."""

    prompt_tokens: int = 0
    completion_tokens: int = 0

    @property
    def total_tokens(self) -> int:
        """Total tokens used."""
        return self.prompt_tokens + self.completion_tokens


@dataclass
class LLMCostEstimate:
    """Estimated cost for an LLM call."""

    estimated_prompt_tokens: int = 0
    estimated_completion_tokens: int = 0
    estimated_cost_usd: float = 0.0

    @classmethod
    def from_model(
        cls,
        model: str,
        prompt_tokens: int,
        completion_tokens: int,
    ) -> LLMCostEstimate:
        """Estimate cost based on model and token counts.

        Uses approximate pricing (as of 2024).
        """
        # Approximate pricing per 1M tokens (input/output)
        pricing = {
            # Anthropic
            "claude-3-opus": (15.0, 75.0),
            "claude-3-sonnet": (3.0, 15.0),
            "claude-3-haiku": (0.25, 1.25),
            "claude-3-5-sonnet": (3.0, 15.0),
            # OpenAI
            "gpt-4-turbo": (10.0, 30.0),
            "gpt-4o": (5.0, 15.0),
            "gpt-4o-mini": (0.15, 0.60),
            "gpt-3.5-turbo": (0.50, 1.50),
        }

        # Find matching model
        input_price, output_price = (1.0, 3.0)  # Default fallback
        for model_prefix, prices in pricing.items():
            if model_prefix in model.lower():
                input_price, output_price = prices
                break

        # Calculate cost (per million tokens)
        cost = (prompt_tokens * input_price + completion_tokens * output_price) / 1_000_000

        return cls(
            estimated_prompt_tokens=prompt_tokens,
            estimated_completion_tokens=completion_tokens,
            estimated_cost_usd=cost,
        )


@dataclass
class LLMResponse:
    """Response from an LLM provider."""

    content: str
    model: str
    usage: TokenUsage = field(default_factory=TokenUsage)
    finish_reason: str | None = None
    raw_response: Any = None


# =============================================================================
# Provider Protocol
# =============================================================================


class LLMProvider(ABC):
    """Abstract base class for LLM providers.

    Implementations:
    - MockLLMProvider: For testing without API calls
    - LiteLLMProvider: Production provider using LiteLLM
    """

    @property
    @abstractmethod
    def model(self) -> str:
        """The model identifier."""
        ...

    @property
    @abstractmethod
    def supports_streaming(self) -> bool:
        """Whether this provider supports streaming."""
        ...

    @abstractmethod
    async def generate(
        self,
        messages: list[dict[str, str]],
        *,
        max_tokens: int = 2048,
        temperature: float = 0.0,
        stop: list[str] | None = None,
    ) -> LLMResponse:
        """Generate a completion.

        Args:
            messages: Chat messages in OpenAI format
            max_tokens: Maximum tokens to generate
            temperature: Sampling temperature
            stop: Stop sequences

        Returns:
            LLMResponse with generated content
        """
        ...

    async def stream(
        self,
        messages: list[dict[str, str]],
        *,
        max_tokens: int = 2048,
        temperature: float = 0.0,
        stop: list[str] | None = None,
    ) -> AsyncIterator[str]:
        """Stream a completion.

        Default implementation calls generate and yields full response.
        Override for true streaming support.
        """
        response = await self.generate(
            messages,
            max_tokens=max_tokens,
            temperature=temperature,
            stop=stop,
        )
        yield response.content

    def estimate_tokens(self, text: str) -> int:
        """Estimate token count for text.

        Uses a simple heuristic (4 chars per token average).
        """
        return len(text) // 4 + 1


# =============================================================================
# Mock Provider (for testing)
# =============================================================================


class MockLLMProvider(LLMProvider):
    """Mock LLM provider for testing.

    Returns predefined responses or generates deterministic placeholders.
    No API calls are made.

    Usage:
        # Simple mock with default responses
        provider = MockLLMProvider()

        # Mock with custom responses
        provider = MockLLMProvider(responses=[
            "def add(a, b): return a + b",
            "def multiply(a, b): return a * b",
        ])

        # Mock that tracks calls
        provider = MockLLMProvider()
        await provider.generate([...])
        assert len(provider.call_history) == 1
    """

    def __init__(
        self,
        model: str = "mock-model",
        responses: list[str] | None = None,
        delay_ms: int = 0,
    ) -> None:
        """Initialize mock provider.

        Args:
            model: Model name to report
            responses: Predefined responses to return (cycled if exhausted)
            delay_ms: Artificial delay to simulate latency
        """
        self._model = model
        self._responses = responses or []
        self._response_index = 0
        self._delay_ms = delay_ms
        self.call_history: list[dict[str, Any]] = []

    @property
    def model(self) -> str:
        return self._model

    @property
    def supports_streaming(self) -> bool:
        return True

    async def generate(
        self,
        messages: list[dict[str, str]],
        *,
        max_tokens: int = 2048,
        temperature: float = 0.0,
        stop: list[str] | None = None,
    ) -> LLMResponse:
        """Generate a mock response."""
        # Record the call
        self.call_history.append(
            {
                "messages": messages,
                "max_tokens": max_tokens,
                "temperature": temperature,
                "stop": stop,
            }
        )

        # Simulate latency
        if self._delay_ms > 0:
            await asyncio.sleep(self._delay_ms / 1000)

        # Get response
        if self._responses:
            content = self._responses[self._response_index % len(self._responses)]
            self._response_index += 1
        else:
            # Generate deterministic placeholder based on prompt
            content = self._generate_placeholder(messages)

        # Calculate mock usage
        prompt_tokens = sum(self.estimate_tokens(m.get("content", "")) for m in messages)
        completion_tokens = self.estimate_tokens(content)

        return LLMResponse(
            content=content,
            model=self._model,
            usage=TokenUsage(
                prompt_tokens=prompt_tokens,
                completion_tokens=completion_tokens,
            ),
            finish_reason="stop",
        )

    async def stream(
        self,
        messages: list[dict[str, str]],
        *,
        max_tokens: int = 2048,
        temperature: float = 0.0,
        stop: list[str] | None = None,
    ) -> AsyncIterator[str]:
        """Stream a mock response (simulates chunked output)."""
        response = await self.generate(
            messages,
            max_tokens=max_tokens,
            temperature=temperature,
            stop=stop,
        )

        # Simulate streaming by yielding chunks
        content = response.content
        chunk_size = 20
        for i in range(0, len(content), chunk_size):
            if self._delay_ms > 0:
                await asyncio.sleep(self._delay_ms / 1000 / 10)
            yield content[i : i + chunk_size]

    def _generate_placeholder(self, messages: list[dict[str, str]]) -> str:
        """Generate a deterministic placeholder from messages."""
        # Extract the user message (typically the last one)
        user_content = ""
        for msg in reversed(messages):
            if msg.get("role") == "user":
                user_content = msg.get("content", "")
                break

        # Extract function name hint from content
        import re

        match = re.search(r"function.*?[`'\"](\w+)[`'\"]", user_content, re.IGNORECASE)
        func_name = match.group(1) if match else "generated_function"

        # Generate a simple placeholder
        return f'''def {func_name}(*args, **kwargs):
    """Auto-generated by MockLLMProvider.

    Original request:
    {user_content[:200]}{"..." if len(user_content) > 200 else ""}
    """
    # TODO: Implement this function
    raise NotImplementedError("{func_name} not yet implemented")
'''

    def reset(self) -> None:
        """Reset call history and response index."""
        self.call_history.clear()
        self._response_index = 0

    def set_responses(self, responses: list[str]) -> None:
        """Set new responses."""
        self._responses = responses
        self._response_index = 0


# =============================================================================
# LiteLLM Provider (production)
# =============================================================================


class LiteLLMProvider(LLMProvider):
    """LLM provider using LiteLLM for unified access to multiple backends.

    LiteLLM supports: Anthropic, OpenAI, Cohere, Azure, Bedrock, etc.
    See: https://docs.litellm.ai/docs/providers

    Usage:
        # Uses ANTHROPIC_API_KEY env var
        provider = LiteLLMProvider(model="claude-3-haiku-20240307")

        # Uses OPENAI_API_KEY env var
        provider = LiteLLMProvider(model="gpt-4o-mini")

        # Explicit API key
        provider = LiteLLMProvider(
            model="claude-3-haiku-20240307",
            api_key="sk-...",
        )
    """

    def __init__(
        self,
        model: str = "claude-3-haiku-20240307",
        api_key: str | None = None,
        api_base: str | None = None,
        timeout: float = 60.0,
    ) -> None:
        """Initialize LiteLLM provider.

        Args:
            model: Model identifier (LiteLLM format)
            api_key: Optional API key (defaults to env vars)
            api_base: Optional API base URL
            timeout: Request timeout in seconds
        """
        self._model = model
        self._api_key = api_key
        self._api_base = api_base
        self._timeout = timeout
        self._litellm: Any = None

    def _ensure_litellm(self) -> Any:
        """Lazy import of litellm."""
        if self._litellm is None:
            try:
                import litellm

                self._litellm = litellm
            except ImportError as e:
                raise ImportError(
                    "LiteLLM is required for LiteLLMProvider. Install with: pip install litellm"
                ) from e
        return self._litellm

    @property
    def model(self) -> str:
        return self._model

    @property
    def supports_streaming(self) -> bool:
        return True

    async def generate(
        self,
        messages: list[dict[str, str]],
        *,
        max_tokens: int = 2048,
        temperature: float = 0.0,
        stop: list[str] | None = None,
    ) -> LLMResponse:
        """Generate using LiteLLM."""
        litellm = self._ensure_litellm()

        kwargs: dict[str, Any] = {
            "model": self._model,
            "messages": messages,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "timeout": self._timeout,
        }

        if self._api_key:
            kwargs["api_key"] = self._api_key
        if self._api_base:
            kwargs["api_base"] = self._api_base
        if stop:
            kwargs["stop"] = stop

        response = await litellm.acompletion(**kwargs)

        # Extract content and usage
        content = response.choices[0].message.content or ""
        usage = TokenUsage(
            prompt_tokens=response.usage.prompt_tokens if response.usage else 0,
            completion_tokens=response.usage.completion_tokens if response.usage else 0,
        )

        return LLMResponse(
            content=content,
            model=response.model or self._model,
            usage=usage,
            finish_reason=response.choices[0].finish_reason,
            raw_response=response,
        )

    async def stream(
        self,
        messages: list[dict[str, str]],
        *,
        max_tokens: int = 2048,
        temperature: float = 0.0,
        stop: list[str] | None = None,
    ) -> AsyncIterator[str]:
        """Stream using LiteLLM."""
        litellm = self._ensure_litellm()

        kwargs: dict[str, Any] = {
            "model": self._model,
            "messages": messages,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "timeout": self._timeout,
            "stream": True,
        }

        if self._api_key:
            kwargs["api_key"] = self._api_key
        if self._api_base:
            kwargs["api_base"] = self._api_base
        if stop:
            kwargs["stop"] = stop

        response = await litellm.acompletion(**kwargs)

        async for chunk in response:
            if chunk.choices and chunk.choices[0].delta.content:
                yield chunk.choices[0].delta.content


# =============================================================================
# LLM Generator
# =============================================================================


# Default system prompt for code generation
DEFAULT_SYSTEM_PROMPT = """You are an expert programmer. \
Generate clean, correct, well-documented code.

Guidelines:
- Write production-quality code with proper error handling
- Follow language idioms and best practices
- Include docstrings/comments for complex logic
- Use descriptive names for variables and functions
- Keep functions focused and single-purpose

Respond ONLY with the code. No explanations or markdown formatting."""


@dataclass
class LLMGeneratorConfig:
    """Configuration for LLM generator."""

    system_prompt: str = DEFAULT_SYSTEM_PROMPT
    max_tokens: int = 2048
    temperature: float = 0.0
    stop_sequences: list[str] = field(default_factory=list)
    max_retries: int = 2
    budget_usd: float | None = None  # Maximum spend per generation


class LLMGenerator:
    """Code generator using Large Language Models.

    Uses LiteLLM for unified access to multiple providers with a clean
    abstraction layer for testing (MockLLMProvider).

    Usage:
        # For testing
        provider = MockLLMProvider(responses=["def foo(): return 42"])
        generator = LLMGenerator(provider=provider)

        # For production
        provider = LiteLLMProvider(model="claude-3-haiku-20240307")
        generator = LLMGenerator(provider=provider)

        # Generate code
        result = await generator.generate(spec, context)
    """

    def __init__(
        self,
        provider: LLMProvider | None = None,
        config: LLMGeneratorConfig | None = None,
    ) -> None:
        """Initialize LLM generator.

        Args:
            provider: LLM provider (defaults to MockLLMProvider for safety)
            config: Generator configuration
        """
        self._provider = provider or MockLLMProvider()
        self._config = config or LLMGeneratorConfig()
        self._total_cost_usd = 0.0

        self._metadata = GeneratorMetadata(
            name="llm",
            generator_type=GeneratorType.LLM,
            priority=20,  # Higher priority than templates
            description=f"LLM-based code generation using {self._provider.model}",
            supports_async=True,
        )

    @property
    def metadata(self) -> GeneratorMetadata:
        """Return generator metadata."""
        return self._metadata

    @property
    def provider(self) -> LLMProvider:
        """Get the LLM provider."""
        return self._provider

    @property
    def total_cost_usd(self) -> float:
        """Total cost accumulated by this generator."""
        return self._total_cost_usd

    def can_generate(self, spec: Specification, context: Context) -> bool:
        """LLM can generate for any specification."""
        # Check budget if configured
        if self._config.budget_usd is not None:
            estimated = self.estimate_cost(spec, context)
            remaining = self._config.budget_usd - self._total_cost_usd
            if estimated.token_estimate > 0:
                # Rough cost estimate
                cost_estimate = LLMCostEstimate.from_model(
                    self._provider.model,
                    estimated.token_estimate,
                    estimated.token_estimate * 2,
                )
                if cost_estimate.estimated_cost_usd > remaining:
                    return False

        return True

    def _build_prompt(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None,
    ) -> list[dict[str, str]]:
        """Build the prompt messages for code generation."""
        messages = [{"role": "system", "content": self._config.system_prompt}]

        # Build user prompt
        parts = [f"Generate code for the following specification:\n\n{spec.description}"]

        if spec.type_signature:
            parts.append(f"\nType signature: {spec.type_signature}")

        if spec.constraints:
            parts.append("\nConstraints:")
            for c in spec.constraints:
                parts.append(f"- {c}")

        if spec.examples:
            parts.append("\nExamples:")
            for inp, out in spec.examples[:5]:  # Limit examples
                parts.append(f"  {inp!r} -> {out!r}")

        # Add hints
        if hints:
            if hints.preferred_style:
                parts.append(f"\nStyle preference: {hints.preferred_style}")
            if hints.constraints:
                parts.append("\nAdditional constraints:")
                for c in hints.constraints:
                    parts.append(f"- {c}")
            if hints.abstractions:
                parts.append("\nAvailable abstractions you can use:")
                for abs in hints.abstractions[:5]:
                    parts.append(f"- {abs.name}: {abs.description}")

        # Add context primitives
        if context.primitives:
            parts.append(f"\nAvailable primitives: {', '.join(context.primitives[:20])}")

        messages.append({"role": "user", "content": "\n".join(parts)})

        return messages

    def _extract_code(self, content: str) -> str:
        """Extract code from LLM response.

        Handles markdown code blocks and raw code.
        """
        import re

        # Try to extract from markdown code block
        code_block = re.search(r"```(?:python)?\s*\n(.*?)\n```", content, re.DOTALL)
        if code_block:
            return code_block.group(1).strip()

        # Try to find code that looks like a function/class definition
        lines = content.split("\n")
        code_lines = []
        in_code = False

        for line in lines:
            # Start of code
            if re.match(r"^(def |class |async def |@)", line):
                in_code = True
            if in_code:
                code_lines.append(line)

        if code_lines:
            return "\n".join(code_lines)

        # Return as-is if no extraction worked
        return content.strip()

    async def generate(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None = None,
    ) -> GenerationResult:
        """Generate code using the LLM.

        Args:
            spec: What to generate
            context: Available resources
            hints: Optional hints

        Returns:
            GenerationResult with generated code
        """
        # Build prompt
        messages = self._build_prompt(spec, context, hints)

        # Configure stop sequences
        stop = self._config.stop_sequences.copy()
        if hints and hints.timeout_ms:
            timeout = hints.timeout_ms / 1000
        else:
            timeout = 30.0

        # Try generation with retries
        last_error: str | None = None
        for attempt in range(self._config.max_retries + 1):
            try:
                response = await asyncio.wait_for(
                    self._provider.generate(
                        messages,
                        max_tokens=self._config.max_tokens,
                        temperature=self._config.temperature,
                        stop=stop if stop else None,
                    ),
                    timeout=timeout,
                )

                # Track cost
                cost = LLMCostEstimate.from_model(
                    response.model,
                    response.usage.prompt_tokens,
                    response.usage.completion_tokens,
                )
                self._total_cost_usd += cost.estimated_cost_usd

                # Extract code
                code = self._extract_code(response.content)

                return GenerationResult(
                    success=True,
                    code=code,
                    confidence=0.7,  # LLM output confidence
                    metadata={
                        "source": "llm",
                        "model": response.model,
                        "usage": {
                            "prompt_tokens": response.usage.prompt_tokens,
                            "completion_tokens": response.usage.completion_tokens,
                        },
                        "cost_usd": cost.estimated_cost_usd,
                        "attempt": attempt + 1,
                    },
                )

            except TimeoutError:
                last_error = f"Generation timed out after {timeout}s"
            except Exception as e:
                last_error = str(e)

        return GenerationResult(
            success=False,
            error=last_error or "Unknown error",
            metadata={"attempts": self._config.max_retries + 1},
        )

    async def generate_stream(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None = None,
    ) -> AsyncIterator[str]:
        """Stream code generation.

        Yields chunks of generated code as they become available.
        """
        if not self._provider.supports_streaming:
            # Fall back to non-streaming
            result = await self.generate(spec, context, hints)
            if result.success and result.code:
                yield result.code
            return

        messages = self._build_prompt(spec, context, hints)

        async for chunk in self._provider.stream(
            messages,
            max_tokens=self._config.max_tokens,
            temperature=self._config.temperature,
            stop=self._config.stop_sequences if self._config.stop_sequences else None,
        ):
            yield chunk

    def estimate_cost(self, spec: Specification, context: Context) -> GenerationCost:
        """Estimate the cost of generating code for this specification."""
        # Estimate prompt tokens
        prompt_parts = [
            self._config.system_prompt,
            spec.description,
            spec.type_signature or "",
            " ".join(spec.constraints),
            str(spec.examples),
            " ".join(context.primitives) if context.primitives else "",
        ]
        prompt_text = " ".join(prompt_parts)
        prompt_tokens = self._provider.estimate_tokens(prompt_text)

        # Estimate completion tokens (rough heuristic: 2x prompt for code)
        completion_tokens = prompt_tokens * 2

        # Estimate time (rough: 50 tokens/sec)
        time_ms = (prompt_tokens + completion_tokens) * 20

        return GenerationCost(
            time_estimate_ms=time_ms,
            token_estimate=prompt_tokens + completion_tokens,
            complexity_score=5,  # LLM is more complex than templates
        )

    def reset_cost_tracking(self) -> None:
        """Reset the accumulated cost tracker."""
        self._total_cost_usd = 0.0


# =============================================================================
# Factory Functions
# =============================================================================


def create_llm_generator(
    model: str | None = None,
    mock: bool = False,
    **kwargs: Any,
) -> LLMGenerator:
    """Create an LLM generator with appropriate provider.

    Args:
        model: Model identifier (e.g., "claude-3-haiku-20240307")
        mock: If True, use MockLLMProvider for testing
        **kwargs: Additional arguments for provider/config

    Returns:
        Configured LLMGenerator

    Examples:
        # For testing
        generator = create_llm_generator(mock=True)

        # For production with Claude
        generator = create_llm_generator(model="claude-3-haiku-20240307")

        # With custom responses for testing
        generator = create_llm_generator(
            mock=True,
            responses=["def foo(): return 42"],
        )
    """
    if mock:
        provider = MockLLMProvider(
            model=model or "mock-model",
            responses=kwargs.pop("responses", None),
            delay_ms=kwargs.pop("delay_ms", 0),
        )
    else:
        provider = LiteLLMProvider(
            model=model or "claude-3-haiku-20240307",
            api_key=kwargs.pop("api_key", None),
            api_base=kwargs.pop("api_base", None),
            timeout=kwargs.pop("timeout", 60.0),
        )

    config = LLMGeneratorConfig(
        system_prompt=kwargs.pop("system_prompt", DEFAULT_SYSTEM_PROMPT),
        max_tokens=kwargs.pop("max_tokens", 2048),
        temperature=kwargs.pop("temperature", 0.0),
        stop_sequences=kwargs.pop("stop_sequences", []),
        max_retries=kwargs.pop("max_retries", 2),
        budget_usd=kwargs.pop("budget_usd", None),
    )

    return LLMGenerator(provider=provider, config=config)


def create_mock_generator(
    responses: list[str] | None = None,
    delay_ms: int = 0,
) -> LLMGenerator:
    """Create a mock LLM generator for testing.

    Args:
        responses: Predefined responses to return
        delay_ms: Artificial delay to simulate latency

    Returns:
        LLMGenerator with MockLLMProvider
    """
    return create_llm_generator(
        mock=True,
        responses=responses,
        delay_ms=delay_ms,
    )


# Protocol compliance check
_mock_gen = LLMGenerator(provider=MockLLMProvider())
assert isinstance(_mock_gen, CodeGenerator)


__all__ = [
    "DEFAULT_SYSTEM_PROMPT",
    "LLMCostEstimate",
    "LLMGenerator",
    "LLMGeneratorConfig",
    "LLMProvider",
    "LLMResponse",
    "LiteLLMProvider",
    "MockLLMProvider",
    "TokenUsage",
    "create_llm_generator",
    "create_mock_generator",
]
