"""Built-in code generator plugins.

Generators:
- PlaceholderGenerator: Returns TODO placeholders (current behavior)
- TemplateGenerator: User-configurable code templates
- LLMGenerator: LLM-based code generation via LiteLLM
- EnumerativeGenerator: Bottom-up AST enumeration for simple synthesis
"""

from .enumeration import EnumerationConfig, EnumerativeGenerator
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
    "EnumerationConfig",
    "EnumerativeGenerator",
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
