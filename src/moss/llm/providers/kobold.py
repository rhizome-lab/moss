"""KoboldAI/KoboldCpp provider.

Interface with KoboldAI or KoboldCpp API for local model inference.
Supports both the classic Kobold API and the newer OpenAI-compatible endpoint.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from moss.llm.protocol import LLMResponse, Message, Role


@dataclass
class KoboldProvider:
    """LLM provider for KoboldAI/KoboldCpp.

    Supports:
    - KoboldAI (original Python version)
    - KoboldCpp (C++ port, faster)
    - Both classic /api/v1/generate and OpenAI-compatible /v1/chat/completions

    Example:
        # Classic Kobold API
        provider = KoboldProvider(api_url="http://localhost:5001")

        # OpenAI-compatible mode
        provider = KoboldProvider(
            api_url="http://localhost:5001",
            use_openai_api=True
        )
    """

    api_url: str = "http://localhost:5001"
    use_openai_api: bool = False
    max_tokens: int = 2048
    temperature: float = 0.0
    top_p: float = 1.0
    rep_pen: float = 1.0  # Repetition penalty

    def complete(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a prompt.

        Args:
            prompt: The user prompt
            system: Optional system prompt
            **kwargs: Additional options

        Returns:
            LLMResponse with the completion
        """
        if self.use_openai_api:
            return self._complete_openai(prompt, system=system, **kwargs)
        else:
            return self._complete_classic(prompt, system=system, **kwargs)

    def chat(
        self,
        messages: list[Message],
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a multi-turn conversation."""
        system = None
        parts = []

        for msg in messages:
            if msg.role == Role.SYSTEM:
                system = msg.content
            elif msg.role == Role.USER:
                parts.append(f"User: {msg.content}")
            else:
                parts.append(f"Assistant: {msg.content}")

        prompt = "\n".join(parts)
        if not prompt.endswith("Assistant:"):
            prompt += "\nAssistant:"

        return self.complete(prompt, system=system, **kwargs)

    def _complete_classic(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete using classic Kobold API."""
        import httpx

        url = f"{self.api_url}/api/v1/generate"

        # Build full prompt
        full_prompt = prompt
        if system:
            full_prompt = f"{system}\n\n{prompt}"

        payload: dict[str, Any] = {
            "prompt": full_prompt,
            "max_length": kwargs.get("max_tokens", self.max_tokens),
            "temperature": kwargs.get("temperature", self.temperature),
            "top_p": kwargs.get("top_p", self.top_p),
            "rep_pen": kwargs.get("rep_pen", self.rep_pen),
        }

        response = httpx.post(url, json=payload, timeout=300)
        response.raise_for_status()
        data = response.json()

        # Kobold returns results in 'results' array
        content = ""
        if data.get("results"):
            content = data["results"][0].get("text", "")

        return LLMResponse(content=content.strip(), model="kobold", raw=data)

    def _complete_openai(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete using OpenAI-compatible API."""
        import httpx

        url = f"{self.api_url}/v1/chat/completions"

        messages = []
        if system:
            messages.append({"role": "system", "content": system})
        messages.append({"role": "user", "content": prompt})

        payload: dict[str, Any] = {
            "messages": messages,
            "max_tokens": kwargs.get("max_tokens", self.max_tokens),
            "temperature": kwargs.get("temperature", self.temperature),
        }

        response = httpx.post(url, json=payload, timeout=300)
        response.raise_for_status()
        data = response.json()

        content = data["choices"][0]["message"]["content"]
        usage = {}
        if "usage" in data:
            usage = {
                "input_tokens": data["usage"].get("prompt_tokens", 0),
                "output_tokens": data["usage"].get("completion_tokens", 0),
            }

        return LLMResponse(content=content, model="kobold", usage=usage, raw=data)

    @classmethod
    def is_available(cls) -> bool:
        """Check if httpx is installed."""
        try:
            import httpx  # noqa: F401

            return True
        except ImportError:
            return False
