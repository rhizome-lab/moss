"""Anthropic SDK provider.

Direct integration with the Anthropic API via their official SDK.
Requires: pip install anthropic
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from moss.llm.protocol import LLMResponse, Message, Role

if TYPE_CHECKING:
    import anthropic as anthropic_sdk


@dataclass
class AnthropicProvider:
    """LLM provider using the Anthropic SDK.

    Example:
        provider = AnthropicProvider(model="claude-sonnet-4-20250514")
        response = provider.complete("Hello!")
    """

    model: str = "claude-sonnet-4-20250514"
    api_key: str | None = None
    max_tokens: int = 4096
    temperature: float = 0.0

    _client: anthropic_sdk.Anthropic | None = None

    @property
    def client(self) -> anthropic_sdk.Anthropic:
        """Get or create the Anthropic client."""
        if self._client is None:
            import anthropic

            api_key = self.api_key or os.environ.get("ANTHROPIC_API_KEY")
            self._client = anthropic.Anthropic(api_key=api_key)
        return self._client

    def complete(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a prompt using the Anthropic API.

        Args:
            prompt: The user prompt
            system: Optional system prompt
            **kwargs: Additional options (temperature, max_tokens, etc.)

        Returns:
            LLMResponse with the completion
        """
        messages = [{"role": "user", "content": prompt}]
        return self._call_api(messages, system=system, **kwargs)

    def chat(
        self,
        messages: list[Message],
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a multi-turn conversation."""
        system = None
        api_messages = []

        for msg in messages:
            if msg.role == Role.SYSTEM:
                system = msg.content
            else:
                api_messages.append(
                    {
                        "role": msg.role.value,
                        "content": msg.content,
                    }
                )

        return self._call_api(api_messages, system=system, **kwargs)

    def _call_api(
        self,
        messages: list[dict[str, Any]],
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Make the API call."""
        call_kwargs: dict[str, Any] = {
            "model": kwargs.get("model", self.model),
            "max_tokens": kwargs.get("max_tokens", self.max_tokens),
            "temperature": kwargs.get("temperature", self.temperature),
            "messages": messages,
        }

        if system:
            call_kwargs["system"] = system

        response = self.client.messages.create(**call_kwargs)

        # Extract text content
        content = ""
        for block in response.content:
            if hasattr(block, "text"):
                content += block.text

        return LLMResponse(
            content=content,
            model=response.model,
            usage={
                "input_tokens": response.usage.input_tokens,
                "output_tokens": response.usage.output_tokens,
            },
            raw=response,
        )

    @classmethod
    def is_available(cls) -> bool:
        """Check if Anthropic SDK is installed."""
        try:
            import anthropic  # noqa: F401

            return True
        except ImportError:
            return False
