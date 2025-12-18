"""LLM provider protocol definitions.

Defines the core Protocol that all LLM providers must implement.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Protocol, runtime_checkable


class Role(Enum):
    """Message role in a conversation."""

    SYSTEM = "system"
    USER = "user"
    ASSISTANT = "assistant"


@dataclass
class Message:
    """A message in a conversation."""

    role: Role
    content: str

    @classmethod
    def system(cls, content: str) -> Message:
        """Create a system message."""
        return cls(role=Role.SYSTEM, content=content)

    @classmethod
    def user(cls, content: str) -> Message:
        """Create a user message."""
        return cls(role=Role.USER, content=content)

    @classmethod
    def assistant(cls, content: str) -> Message:
        """Create an assistant message."""
        return cls(role=Role.ASSISTANT, content=content)


@dataclass
class LLMResponse:
    """Response from an LLM provider."""

    content: str
    model: str | None = None
    usage: dict[str, int] = field(default_factory=dict)
    raw: object = None  # Provider-specific raw response

    @property
    def input_tokens(self) -> int:
        """Input token count."""
        return self.usage.get("input_tokens", 0)

    @property
    def output_tokens(self) -> int:
        """Output token count."""
        return self.usage.get("output_tokens", 0)


@runtime_checkable
class LLMProvider(Protocol):
    """Protocol for LLM providers.

    All providers must implement this interface. The simplest implementation
    just needs the complete() method.
    """

    def complete(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a prompt.

        Args:
            prompt: The user prompt to complete
            system: Optional system prompt
            **kwargs: Provider-specific options (temperature, max_tokens, etc.)

        Returns:
            LLMResponse with the completion
        """
        ...

    def chat(
        self,
        messages: list[Message],
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a multi-turn conversation.

        Args:
            messages: List of conversation messages
            **kwargs: Provider-specific options

        Returns:
            LLMResponse with the completion
        """
        ...

    @classmethod
    def is_available(cls) -> bool:
        """Check if this provider is available (dependencies installed).

        Returns:
            True if provider can be used
        """
        ...
