"""llama.cpp provider.

Interface with llama.cpp server (OpenAI-compatible) or llama-cpp-python library.
Supports local model inference with GGUF models.

Grammar Support:
    GBNF grammars can be used to constrain output format. Pass the `grammar`
    parameter to complete() or chat() with a GBNF grammar string.

    from moss.llm.gbnf import GRAMMARS
    response = provider.complete("Generate JSON", grammar=GRAMMARS["json"])
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from typing import Any

from moss.llm.protocol import LLMResponse, Message, Role


@dataclass
class LlamaCppProvider:
    """LLM provider for llama.cpp.

    Supports two modes:
    1. Server mode: Connect to llama.cpp server (--server flag)
    2. Library mode: Use llama-cpp-python directly

    Server mode (default):
        # Start server: ./server -m model.gguf --port 8080
        provider = LlamaCppProvider(api_url="http://localhost:8080")

    Library mode:
        provider = LlamaCppProvider(
            model_path="/path/to/model.gguf",
            use_server=False
        )

    Grammar-constrained inference:
        from moss.llm.gbnf import GRAMMARS
        provider = LlamaCppProvider()
        response = provider.complete("Output JSON:", grammar=GRAMMARS["json"])

    Example:
        provider = LlamaCppProvider()
        response = provider.complete("Hello!")
    """

    api_url: str = "http://localhost:8080"
    model_path: str | None = None
    use_server: bool = True
    n_ctx: int = 4096
    max_tokens: int = 2048
    temperature: float = 0.0
    n_gpu_layers: int = -1  # -1 = all layers on GPU

    _llm: Any = field(default=None, repr=False)
    _grammar_cache: dict[str, Any] = field(default_factory=dict, repr=False)

    @property
    def llm(self) -> Any:
        """Get or create the llama-cpp-python instance (library mode only)."""
        if self._llm is None and not self.use_server:
            from llama_cpp import Llama

            model_path = self.model_path or os.environ.get("LLAMA_MODEL_PATH")
            if not model_path:
                raise ValueError("model_path required for library mode")

            self._llm = Llama(
                model_path=model_path,
                n_ctx=self.n_ctx,
                n_gpu_layers=self.n_gpu_layers,
            )
        return self._llm

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
        # Extract system and build messages
        system = None
        user_messages = []
        for msg in messages:
            if msg.role == Role.SYSTEM:
                system = msg.content
            else:
                user_messages.append({"role": msg.role.value, "content": msg.content})

        if self.use_server:
            return self._chat_server(user_messages, system=system, **kwargs)
        else:
            # Library mode: concatenate to single prompt
            parts = []
            if system:
                parts.append(f"System: {system}")
            for msg in user_messages:
                prefix = "User" if msg["role"] == "user" else "Assistant"
                parts.append(f"{prefix}: {msg['content']}")
            parts.append("Assistant:")
            prompt = "\n\n".join(parts)
            return self._complete_library(prompt, system=system)

    def _complete_server(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete via llama.cpp server (OpenAI-compatible API)."""
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

        # Add grammar constraint if provided
        grammar = kwargs.get("grammar")
        if grammar:
            payload["grammar"] = grammar

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

        return LLMResponse(content=content, model="llama.cpp", usage=usage, raw=data)

    def _chat_server(
        self,
        messages: list[dict[str, str]],
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Chat via llama.cpp server."""
        import httpx

        url = f"{self.api_url}/v1/chat/completions"

        api_messages = []
        if system:
            api_messages.append({"role": "system", "content": system})
        api_messages.extend(messages)

        payload: dict[str, Any] = {
            "messages": api_messages,
            "max_tokens": kwargs.get("max_tokens", self.max_tokens),
            "temperature": kwargs.get("temperature", self.temperature),
        }

        # Add grammar constraint if provided
        grammar = kwargs.get("grammar")
        if grammar:
            payload["grammar"] = grammar

        response = httpx.post(url, json=payload, timeout=300)
        response.raise_for_status()
        data = response.json()

        content = data["choices"][0]["message"]["content"]
        return LLMResponse(content=content, model="llama.cpp", raw=data)

    def _get_llama_grammar(self, grammar_str: str) -> Any:
        """Get or create cached LlamaGrammar object."""
        if grammar_str in self._grammar_cache:
            return self._grammar_cache[grammar_str]

        from llama_cpp import LlamaGrammar

        grammar = LlamaGrammar.from_string(grammar_str)
        self._grammar_cache[grammar_str] = grammar
        return grammar

    def _complete_library(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete via llama-cpp-python library."""
        llm = self.llm

        # Build full prompt
        full_prompt = prompt
        if system:
            full_prompt = f"{system}\n\n{prompt}"

        # Prepare kwargs for llama call
        call_kwargs: dict[str, Any] = {
            "max_tokens": kwargs.get("max_tokens", self.max_tokens),
            "temperature": kwargs.get("temperature", self.temperature),
            "echo": False,
        }

        # Add grammar constraint if provided
        grammar_str = kwargs.get("grammar")
        if grammar_str:
            call_kwargs["grammar"] = self._get_llama_grammar(str(grammar_str))

        output = llm(full_prompt, **call_kwargs)

        content = output["choices"][0]["text"]
        usage = {
            "input_tokens": output.get("usage", {}).get("prompt_tokens", 0),
            "output_tokens": output.get("usage", {}).get("completion_tokens", 0),
        }

        return LLMResponse(content=content, model="llama.cpp", usage=usage, raw=output)

    @classmethod
    def is_available(cls) -> bool:
        """Check if llama.cpp support is available."""
        # Server mode just needs httpx
        try:
            import httpx  # noqa: F401

            return True
        except ImportError:
            pass

        # Or llama-cpp-python
        try:
            import llama_cpp  # noqa: F401

            return True
        except ImportError:
            pass

        return False
