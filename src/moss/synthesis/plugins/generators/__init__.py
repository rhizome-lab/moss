"""Built-in code generator plugins.

Generators:
- PlaceholderGenerator: Returns TODO placeholders (current behavior)
- TemplateGenerator: User-configurable code templates
- LLMGenerator: LLM-based code generation via LiteLLM
"""

from .llm import (
    LiteLLMProvider,
    LLMGenerator,
    LLMGeneratorConfig,
    LLMProvider,
    LLMResponse,
    MockLLMProvider,
    TokenUsage,
    create_llm_generator,
    create_mock_generator,
)
from .placeholder import PlaceholderGenerator
from .template import TemplateGenerator

__all__ = [
    "LLMGenerator",
    "LLMGeneratorConfig",
    "LLMProvider",
    "LLMResponse",
    "LiteLLMProvider",
    "MockLLMProvider",
    "PlaceholderGenerator",
    "TemplateGenerator",
    "TokenUsage",
    "create_llm_generator",
    "create_mock_generator",
]
