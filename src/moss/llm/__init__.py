"""LLM provider abstraction layer.

Provides a unified interface for calling LLMs from multiple providers.
Supports both direct SDK integrations and CLI-based providers.

Providers can be registered via entry points or programmatically.

Entry point group: moss.llm.providers

Example:
    from moss.llm import get_provider, complete

    # Get default provider (from env or config)
    provider = get_provider()
    response = provider.complete("Summarize this text: ...")

    # Or use convenience function
    response = complete("Summarize this text: ...")

    # Specify provider explicitly
    provider = get_provider("litellm", model="claude-sonnet-4-20250514")

Available providers (when dependencies installed):
- cli: Shell out to llm/claude/gemini CLI tools (zero dependencies)
- litellm: LiteLLM multi-provider gateway (100+ providers including Anthropic, OpenAI)
- llm: Simon Willison's llm library
- bifrost: Bifrost high-performance gateway
- llamacpp: llama.cpp local inference
- kobold: KoboldAI/KoboldCpp
- exllama: ExLlamaV2

Example plugin registration in pyproject.toml:
    [project.entry-points."moss.llm.providers"]
    my_provider = "my_package.providers:MyProvider"
"""

from __future__ import annotations

import os
from importlib.metadata import entry_points

from moss.llm.protocol import LLMProvider, LLMResponse, Message, Role

# Registry of available providers
_PROVIDERS: dict[str, type[LLMProvider]] = {}


def register_provider(name: str, provider_class: type[LLMProvider]) -> None:
    """Register an LLM provider.

    Args:
        name: Provider name (e.g., "anthropic", "openai")
        provider_class: Provider class implementing LLMProvider protocol
    """
    _PROVIDERS[name] = provider_class


def list_providers() -> list[str]:
    """List all registered providers.

    Returns:
        List of provider names
    """
    return list(_PROVIDERS.keys())


def get_provider(
    name: str | None = None,
    *,
    model: str | None = None,
    **kwargs: object,
) -> LLMProvider:
    """Get an LLM provider instance.

    Args:
        name: Provider name. If None, uses MOSS_LLM_PROVIDER env var or "cli"
        model: Model to use. If None, uses MOSS_LLM_MODEL env var or provider default
        **kwargs: Additional provider-specific configuration

    Returns:
        Configured LLMProvider instance

    Raises:
        ValueError: If provider not found or not available
    """
    if name is None:
        name = os.environ.get("MOSS_LLM_PROVIDER", "cli")

    if model is None:
        model = os.environ.get("MOSS_LLM_MODEL")

    if name not in _PROVIDERS:
        available = ", ".join(_PROVIDERS.keys()) or "none"
        raise ValueError(f"Provider '{name}' not found. Available: {available}")

    provider_class = _PROVIDERS[name]

    # Check if provider is available
    if hasattr(provider_class, "is_available") and not provider_class.is_available():
        raise ValueError(f"Provider '{name}' is not available (missing dependencies)")

    if model:
        kwargs["model"] = model

    return provider_class(**kwargs)


def complete(
    prompt: str,
    *,
    system: str | None = None,
    provider: str | None = None,
    model: str | None = None,
    **kwargs: object,
) -> str:
    """Convenience function to complete a prompt.

    Args:
        prompt: The user prompt
        system: Optional system prompt
        provider: Provider name (uses default if None)
        model: Model name (uses provider default if None)
        **kwargs: Additional provider-specific options

    Returns:
        The completion text
    """
    llm = get_provider(provider, model=model)
    response = llm.complete(prompt, system=system, **kwargs)
    return response.content


def _discover_entry_points() -> None:
    """Discover and register providers from entry points."""
    try:
        eps = entry_points(group="moss.llm.providers")
        for ep in eps:
            try:
                provider_class = ep.load()
                # Only register if not already registered (builtins take precedence)
                if ep.name not in _PROVIDERS:
                    register_provider(ep.name, provider_class)
            except (ImportError, AttributeError, TypeError):
                pass  # Skip failed imports
    except (TypeError, StopIteration):
        pass


def _register_builtin_providers() -> None:
    """Register built-in providers based on installed dependencies."""
    # CLI provider (always available - zero deps)
    from moss.llm.providers.cli import CLIProvider

    register_provider("cli", CLIProvider)

    # LiteLLM
    try:
        from moss.llm.providers.litellm import LiteLLMProvider

        register_provider("litellm", LiteLLMProvider)
    except ImportError:
        pass

    # Simon Willison's llm
    try:
        from moss.llm.providers.llm_lib import LLMLibProvider

        register_provider("llm", LLMLibProvider)
    except ImportError:
        pass

    # Bifrost
    try:
        from moss.llm.providers.bifrost import BifrostProvider

        register_provider("bifrost", BifrostProvider)
    except ImportError:
        pass

    # llama.cpp
    try:
        from moss.llm.providers.llamacpp import LlamaCppProvider

        register_provider("llamacpp", LlamaCppProvider)
    except ImportError:
        pass

    # KoboldAI/KoboldCpp
    try:
        from moss.llm.providers.kobold import KoboldProvider

        register_provider("kobold", KoboldProvider)
    except ImportError:
        pass

    # ExLlamaV2
    try:
        from moss.llm.providers.exllama import ExLlamaProvider

        register_provider("exllama", ExLlamaProvider)
    except ImportError:
        pass


# Auto-register on import
_register_builtin_providers()
_discover_entry_points()

__all__ = [
    "LLMProvider",
    "LLMResponse",
    "Message",
    "Role",
    "complete",
    "get_provider",
    "list_providers",
    "register_provider",
]
