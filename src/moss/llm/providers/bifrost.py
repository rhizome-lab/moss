"""Bifrost provider.

High-performance LLM gateway with adaptive load balancing.
Requires: pip install bifrost-python (or similar client)

Note: Bifrost is primarily a gateway/proxy, so this provider
interfaces with a running Bifrost server.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Any

from moss.llm.protocol import LLMResponse, Message


@dataclass
class BifrostProvider:
    """LLM provider using Bifrost gateway.

    Bifrost provides:
    - 50x faster than LiteLLM
    - Adaptive load balancing
    - Cluster mode
    - Guardrails
    - 1000+ model support
    - <100 µs overhead at 5k RPS

    Requires a running Bifrost server. Configure via:
    - BIFROST_API_URL environment variable
    - Or pass api_url parameter

    Example:
        provider = BifrostProvider(
            api_url="http://localhost:8080",
            model="gpt-4o"
        )
        response = provider.complete("Hello!")
    """

    model: str = "gpt-4o"
    api_url: str | None = None
    api_key: str | None = None
    max_tokens: int = 4096
    temperature: float = 0.0

    @property
    def base_url(self) -> str:
        """Get the Bifrost API URL."""
        return self.api_url or os.environ.get("BIFROST_API_URL", "http://localhost:8080")

    def complete(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a prompt via Bifrost.

        Args:
            prompt: The user prompt
            system: Optional system prompt
            **kwargs: Additional options

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
        """Make the API call via Bifrost."""
        import httpx

        url = f"{self.base_url}/v1/chat/completions"

        headers: dict[str, str] = {"Content-Type": "application/json"}
        api_key = self.api_key or os.environ.get("BIFROST_API_KEY")
        if api_key:
            headers["Authorization"] = f"Bearer {api_key}"

        payload: dict[str, Any] = {
            "model": kwargs.get("model", self.model),
            "max_tokens": kwargs.get("max_tokens", self.max_tokens),
            "temperature": kwargs.get("temperature", self.temperature),
            "messages": messages,
        }

        response = httpx.post(url, json=payload, headers=headers, timeout=300)
        response.raise_for_status()

        data = response.json()

        # OpenAI-compatible response format
        content = data["choices"][0]["message"]["content"]

        usage = {}
        if "usage" in data:
            usage = {
                "input_tokens": data["usage"].get("prompt_tokens", 0),
                "output_tokens": data["usage"].get("completion_tokens", 0),
            }

        return LLMResponse(
            content=content,
            model=data.get("model", self.model),
            usage=usage,
            raw=data,
        )

    @classmethod
    def is_available(cls) -> bool:
        """Check if httpx is installed (used for API calls)."""
        try:
            import httpx  # noqa: F401

            return True
        except ImportError:
            return False
