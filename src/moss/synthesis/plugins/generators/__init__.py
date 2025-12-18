"""Built-in code generator plugins.

Generators:
- PlaceholderGenerator: Returns TODO placeholders (current behavior)
- TemplateGenerator: User-configurable code templates
- LLMGenerator: LLM-based code generation via LiteLLM
- EnumerativeGenerator: Bottom-up AST enumeration for simple synthesis
- ComponentGenerator: Type-directed library composition (SyPet/InSynth style)
- SMTGenerator: Z3-based type-driven synthesis (Synquid style)
"""

from .component import ComponentGenerator
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
from .smt import SMTGenerator
from .template import TemplateGenerator

__all__ = [
    "ComponentGenerator",
    "EnumerationConfig",
    "EnumerativeGenerator",
    "LLMGenerator",
    "LLMGeneratorConfig",
    "LLMProvider",
    "LLMResponse",
    "LiteLLMProvider",
    "MockLLMProvider",
    "PlaceholderGenerator",
    "SMTGenerator",
    "TemplateGenerator",
    "TokenUsage",
    "create_llm_generator",
    "create_mock_generator",
]
