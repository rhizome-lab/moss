"""Built-in code generator plugins.

Generators:
- PlaceholderGenerator: Returns TODO placeholders (current behavior)
- TemplateGenerator: User-configurable code templates
- LLMGenerator: LLM-based code generation via LiteLLM
- EnumerativeGenerator: Bottom-up AST enumeration for simple synthesis
- ComponentGenerator: Type-directed library composition (SyPet/InSynth style)
- SMTGenerator: Z3-based type-driven synthesis (Synquid style)
- PBEGenerator: Programming by Example (FlashFill/PROSE style)
- SketchGenerator: Fill holes in user templates (Sketch/Rosette style)
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
from .pbe import PBEGenerator
from .placeholder import PlaceholderGenerator
from .sketch import SketchGenerator
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
    "PBEGenerator",
    "PlaceholderGenerator",
    "SMTGenerator",
    "SketchGenerator",
    "TemplateGenerator",
    "TokenUsage",
    "create_llm_generator",
    "create_mock_generator",
]
