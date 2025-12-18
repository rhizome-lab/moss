"""LiteLLM provider.

Multi-provider gateway supporting 100+ LLM providers through a unified interface.
Requires: pip install litellm
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from moss.llm.protocol import LLMResponse, Message


@dataclass
class LiteLLMProvider:
    """LLM provider using LiteLLM.

    LiteLLM provides a unified interface to many LLM providers including:
    - OpenAI, Azure OpenAI
    - Anthropic
    - Google (Vertex AI, Gemini)
    - AWS Bedrock
    - Cohere, Replicate, Hugging Face
    - Local models (Ollama, vLLM)
    - And many more...

    Example:
        # Use OpenAI
        provider = LiteLLMProvider(model="gpt-4o")

        # Use Anthropic
        provider = LiteLLMProvider(model="claude-3-opus-20240229")

        # Use local Ollama
        provider = LiteLLMProvider(model="ollama/llama2")
    """

    model: str = "gpt-4o"
    max_tokens: int = 4096
    temperature: float = 0.0

    def complete(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a prompt using LiteLLM.

        Args:
            prompt: The user prompt
            system: Optional system prompt
            **kwargs: Additional options passed to LiteLLM

        Returns:
            LLMResponse with the completion
        """
        messages: list[dict[str, str]] = []
        if system:
            messages.append({"role": "system", "content": system})
        messages.append({"role": "user", "content": prompt})

        return self._call_api(messages, **kwargs)

    def chat(
        self,
        messages: list[Message],
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a multi-turn conversation."""
        api_messages: list[dict[str, str]] = []

        for msg in messages:
            api_messages.append(
                {
                    "role": msg.role.value,
                    "content": msg.content,
                }
            )

        return self._call_api(api_messages, **kwargs)

    def _call_api(
        self,
        messages: list[dict[str, str]],
        **kwargs: object,
    ) -> LLMResponse:
        """Make the API call via LiteLLM."""
        import litellm

        call_kwargs: dict[str, Any] = {
            "model": kwargs.get("model", self.model),
            "max_tokens": kwargs.get("max_tokens", self.max_tokens),
            "temperature": kwargs.get("temperature", self.temperature),
            "messages": messages,
        }

        response = litellm.completion(**call_kwargs)

        # Extract content
        content = response.choices[0].message.content or ""

        usage = {}
        if response.usage:
            usage = {
                "input_tokens": response.usage.prompt_tokens,
                "output_tokens": response.usage.completion_tokens,
            }

        return LLMResponse(
            content=content,
            model=getattr(response, "model", self.model),
            usage=usage,
            raw=response,
        )

    @classmethod
    def is_available(cls) -> bool:
        """Check if LiteLLM is installed."""
        try:
            import litellm  # noqa: F401

            return True
        except ImportError:
            return False
