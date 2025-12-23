"""Simon Willison's llm library provider.

Python API for the llm library, which supports many providers through plugins.
Requires: pip install llm
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from moss_orchestration.llm.protocol import LLMResponse, Message, Role

if TYPE_CHECKING:
    import llm as llm_lib


@dataclass
class LLMLibProvider:
    """LLM provider using Simon Willison's llm library.

    The llm library provides a plugin-based architecture supporting:
    - OpenAI (built-in)
    - Anthropic (via llm-claude-3)
    - Google (via llm-gemini)
    - Local models (via llm-ollama, llm-llamafile)
    - Many community plugins

    Example:
        # Use default model
        provider = LLMLibProvider()

        # Specify model
        provider = LLMLibProvider(model="claude-3-opus")

        # With system prompt
        response = provider.complete("Hello!", system="Be concise")
    """

    model: str | None = None

    _model_instance: llm_lib.Model | None = None

    @property
    def model_instance(self) -> llm_lib.Model:
        """Get or create the llm model instance."""
        if self._model_instance is None:
            import llm

            if self.model:
                self._model_instance = llm.get_model(self.model)
            else:
                self._model_instance = llm.get_model()
        return self._model_instance

    def complete(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a prompt using the llm library.

        Args:
            prompt: The user prompt
            system: Optional system prompt
            **kwargs: Additional options passed to the model

        Returns:
            LLMResponse with the completion
        """
        model = self.model_instance

        response = model.prompt(prompt, system=system)
        content = response.text()

        # Extract usage if available
        usage = {}
        if hasattr(response, "input_tokens") and response.input_tokens:
            usage["input_tokens"] = response.input_tokens
        if hasattr(response, "output_tokens") and response.output_tokens:
            usage["output_tokens"] = response.output_tokens

        return LLMResponse(
            content=content,
            model=model.model_id,
            usage=usage,
            raw=response,
        )

    def chat(
        self,
        messages: list[Message],
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a multi-turn conversation.

        The llm library has conversation support, but for simplicity
        we concatenate messages here.
        """

        model = self.model_instance

        # Start a conversation
        conversation = model.conversation()

        # Extract system prompt
        system = None
        for msg in messages:
            if msg.role == Role.SYSTEM:
                system = msg.content
                break

        # Process messages
        response = None
        for msg in messages:
            if msg.role == Role.SYSTEM:
                continue
            elif msg.role == Role.USER:
                response = conversation.prompt(msg.content, system=system)
                system = None  # Only use system on first prompt
            # Assistant messages are automatically tracked by conversation

        if response is None:
            # No user messages, create empty response
            return LLMResponse(content="", model=model.model_id)

        content = response.text()

        usage = {}
        if hasattr(response, "input_tokens") and response.input_tokens:
            usage["input_tokens"] = response.input_tokens
        if hasattr(response, "output_tokens") and response.output_tokens:
            usage["output_tokens"] = response.output_tokens

        return LLMResponse(
            content=content,
            model=model.model_id,
            usage=usage,
            raw=response,
        )

    @classmethod
    def is_available(cls) -> bool:
        """Check if llm library is installed."""
        try:
            import llm  # noqa: F401

            return True
        except ImportError:
            return False
