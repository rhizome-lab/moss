"""ExLlamaV2 provider.

Interface with ExLlamaV2 for high-performance local inference.
Supports both direct library use and the tabbyAPI server.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Any

from moss_orchestration.llm.protocol import LLMResponse, Message, Role


@dataclass
class ExLlamaProvider:
    """LLM provider for ExLlamaV2.

    ExLlamaV2 provides extremely fast inference for GPTQ/EXL2 quantized models.
    Supports two modes:

    1. Server mode: Connect to tabbyAPI or similar ExLlama-based server
    2. Library mode: Use exllamav2 directly (requires GPU)

    Server mode (default):
        provider = ExLlamaProvider(api_url="http://localhost:5000")

    Library mode:
        provider = ExLlamaProvider(
            model_path="/path/to/model",
            use_server=False
        )

    Example:
        provider = ExLlamaProvider()
        response = provider.complete("Hello!")
    """

    api_url: str = "http://localhost:5000"
    model_path: str | None = None
    use_server: bool = True
    max_tokens: int = 2048
    temperature: float = 0.0
    top_p: float = 1.0
    top_k: int = 50
    repetition_penalty: float = 1.0

    _generator: Any = None
    _model: Any = None
    _tokenizer: Any = None

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
        if self.use_server:
            return self._complete_server(prompt, system=system, **kwargs)
        else:
            return self._complete_library(prompt, system=system, **kwargs)

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
        if parts and not parts[-1].startswith("Assistant:"):
            prompt += "\nAssistant:"

        return self.complete(prompt, system=system, **kwargs)

    def _complete_server(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete via tabbyAPI or similar server (OpenAI-compatible)."""
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
            "top_p": kwargs.get("top_p", self.top_p),
            "top_k": kwargs.get("top_k", self.top_k),
            "repetition_penalty": kwargs.get("repetition_penalty", self.repetition_penalty),
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

        return LLMResponse(content=content, model="exllama", usage=usage, raw=data)

    def _complete_library(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete via exllamav2 library directly."""
        self._ensure_model_loaded()

        from exllamav2.generator import ExLlamaV2Sampler

        # Build full prompt
        full_prompt = prompt
        if system:
            full_prompt = f"{system}\n\n{prompt}"

        # Configure sampling
        settings = ExLlamaV2Sampler.Settings()
        settings.temperature = kwargs.get("temperature", self.temperature)
        settings.top_p = kwargs.get("top_p", self.top_p)
        settings.top_k = kwargs.get("top_k", self.top_k)
        settings.token_repetition_penalty = kwargs.get(
            "repetition_penalty", self.repetition_penalty
        )

        # Generate
        max_tokens = kwargs.get("max_tokens", self.max_tokens)
        output = self._generator.generate_simple(
            full_prompt,
            settings,
            max_tokens,
            add_bos=True,
        )

        # Remove prompt from output if echoed
        if output.startswith(full_prompt):
            output = output[len(full_prompt) :]

        return LLMResponse(content=output.strip(), model="exllama")

    def _ensure_model_loaded(self) -> None:
        """Load the model if not already loaded."""
        if self._generator is not None:
            return

        from exllamav2 import ExLlamaV2, ExLlamaV2Cache, ExLlamaV2Config, ExLlamaV2Tokenizer
        from exllamav2.generator import ExLlamaV2BaseGenerator

        model_path = self.model_path or os.environ.get("EXLLAMA_MODEL_PATH")
        if not model_path:
            raise ValueError("model_path required for library mode")

        # Load config
        config = ExLlamaV2Config()
        config.model_dir = model_path
        config.prepare()

        # Load model
        self._model = ExLlamaV2(config)
        self._model.load()

        # Load tokenizer
        self._tokenizer = ExLlamaV2Tokenizer(config)

        # Create cache and generator
        cache = ExLlamaV2Cache(self._model)
        self._generator = ExLlamaV2BaseGenerator(self._model, cache, self._tokenizer)

    @classmethod
    def is_available(cls) -> bool:
        """Check if ExLlamaV2 support is available."""
        # Server mode just needs httpx
        try:
            import httpx  # noqa: F401

            return True
        except ImportError:
            pass

        # Or exllamav2 library
        try:
            import exllamav2  # noqa: F401

            return True
        except ImportError:
            pass

        return False
