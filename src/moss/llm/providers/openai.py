"""OpenAI SDK provider.

Direct integration with the OpenAI API via their official SDK.
Requires: pip install openai
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from moss.llm.protocol import LLMResponse, Message

if TYPE_CHECKING:
    import openai as openai_sdk


@dataclass
class OpenAIProvider:
    """LLM provider using the OpenAI SDK.

    Example:
        provider = OpenAIProvider(model="gpt-4o")
        response = provider.complete("Hello!")
    """

    model: str = "gpt-4o"
    api_key: str | None = None
    max_tokens: int = 4096
    temperature: float = 0.0

    _client: openai_sdk.OpenAI | None = None

    @property
    def client(self) -> openai_sdk.OpenAI:
        """Get or create the OpenAI client."""
        if self._client is None:
            import openai

            api_key = self.api_key or os.environ.get("OPENAI_API_KEY")
            self._client = openai.OpenAI(api_key=api_key)
        return self._client

    def complete(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a prompt using the OpenAI API.

        Args:
            prompt: The user prompt
            system: Optional system prompt
            **kwargs: Additional options (temperature, max_tokens, etc.)

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
        """Make the API call."""
        call_kwargs: dict[str, Any] = {
            "model": kwargs.get("model", self.model),
            "max_tokens": kwargs.get("max_tokens", self.max_tokens),
            "temperature": kwargs.get("temperature", self.temperature),
            "messages": messages,
        }

        response = self.client.chat.completions.create(**call_kwargs)

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
            model=response.model,
            usage=usage,
            raw=response,
        )

    @classmethod
    def is_available(cls) -> bool:
        """Check if OpenAI SDK is installed."""
        try:
            import openai  # noqa: F401

            return True
        except ImportError:
            return False
