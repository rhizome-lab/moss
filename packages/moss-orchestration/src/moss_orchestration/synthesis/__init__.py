"""Synthesis framework for recursive problem decomposition.

This module provides a domain-agnostic synthesis engine that integrates
with moss primitives (validation, shadow git, memory, events).

Core components:
- Specification: What to synthesize (description, types, examples, tests)
- Context: Available resources (primitives, library, solved problems)
- DecompositionStrategy: How to break problems into subproblems
- Composer: How to combine subproblem solutions
- StrategyRouter: Selects best strategy (like DWIM for tools)
- SynthesisFramework: Orchestrates the synthesis process

Example usage:
    from moss_orchestration.synthesis import (
        SynthesisFramework,
        Specification,
        Context,
        create_synthesis_framework,
    )

    # Create framework
    framework = create_synthesis_framework()

    # Define specification
    spec = Specification(
        description="Sort a list of users by registration date",
        type_signature="List[User] -> List[User]",
    )

    # Define context
    context = Context(
        primitives=["sorted", "key", "lambda"],
        library={"User": User},
    )

    # Synthesize
    result = await framework.synthesize(spec, context)
    if result.success:
        print(result.solution)
"""

from .cache import (
    ExecutionResultCache,
    SolutionCache,
    StrategyCache,
    SynthesisCache,
    clear_all_caches,
    get_cache_stats,
    get_solution_cache,
    get_strategy_cache,
    get_test_cache,
)
from .composer import CodeComposer, Composer, FunctionComposer, SequentialComposer
from .config import (
    GeneratorConfig,
    LearningConfig,
    StrategyConfig,
    SynthesisConfigLoader,
    SynthesisConfigWrapper,
    ValidatorConfig,
    get_default_config,
    list_available_presets,
    load_synthesis_config,
)
from .framework import (
    SynthesisConfig,
    SynthesisEventType,
    SynthesisFramework,
    SynthesisState,
    create_synthesis_framework,
)
from .learning import (
    StrategyLearner,
    StrategyOutcome,
    extract_features,
    feature_similarity,
    get_learner,
    reset_learner,
)
from .presets import (
    PresetName,
    SynthesisPreset,
    get_preset,
    get_preset_descriptions,
    list_presets,
    register_preset,
)
from .protocols import (
    Abstraction,
    CodeGenerator,
    GenerationCost,
    GenerationHints,
    GenerationResult,
    GeneratorMetadata,
    GeneratorType,
    LibraryMetadata,
    LibraryPlugin,
    SynthesisValidator,
    ValidationResult,
    ValidatorMetadata,
    ValidatorType,
)
from .registry import (
    GeneratorRegistry,
    LibraryRegistry,
    SynthesisRegistry,
    ValidatorRegistry,
    get_synthesis_registry,
    reset_synthesis_registry,
)
from .router import StrategyMatch, StrategyRouter
from .strategy import AtomicStrategy, DecompositionStrategy, StrategyMetadata
from .strategy_registry import (
    StrategyPlugin,
    StrategyRegistry,
    get_strategy_registry,
    reset_strategy_registry,
)
from .types import (
    CompositionError,
    Context,
    DecompositionError,
    NoStrategyError,
    Specification,
    Subproblem,
    SynthesisError,
    SynthesisResult,
    ValidationError,
)

__all__ = [
    "Abstraction",
    "AtomicStrategy",
    "CodeComposer",
    "CodeGenerator",
    "Composer",
    "CompositionError",
    "Context",
    "DecompositionError",
    "DecompositionStrategy",
    "ExecutionResultCache",
    "FunctionComposer",
    "GenerationCost",
    "GenerationHints",
    "GenerationResult",
    "GeneratorConfig",
    "GeneratorMetadata",
    "GeneratorRegistry",
    "GeneratorType",
    "LearningConfig",
    "LibraryMetadata",
    "LibraryPlugin",
    "LibraryRegistry",
    "NoStrategyError",
    "PresetName",
    "SequentialComposer",
    "SolutionCache",
    "Specification",
    "StrategyCache",
    "StrategyConfig",
    "StrategyLearner",
    "StrategyMatch",
    "StrategyMetadata",
    "StrategyOutcome",
    "StrategyPlugin",
    "StrategyRegistry",
    "StrategyRouter",
    "Subproblem",
    "SynthesisCache",
    "SynthesisConfig",
    "SynthesisConfigLoader",
    "SynthesisConfigWrapper",
    "SynthesisError",
    "SynthesisEventType",
    "SynthesisFramework",
    "SynthesisPreset",
    "SynthesisRegistry",
    "SynthesisResult",
    "SynthesisState",
    "SynthesisValidator",
    "ValidationError",
    "ValidationResult",
    "ValidatorConfig",
    "ValidatorMetadata",
    "ValidatorRegistry",
    "ValidatorType",
    "clear_all_caches",
    "create_synthesis_framework",
    "extract_features",
    "feature_similarity",
    "get_cache_stats",
    "get_default_config",
    "get_learner",
    "get_preset",
    "get_preset_descriptions",
    "get_solution_cache",
    "get_strategy_cache",
    "get_strategy_registry",
    "get_synthesis_registry",
    "get_test_cache",
    "list_available_presets",
    "list_presets",
    "load_synthesis_config",
    "register_preset",
    "reset_learner",
    "reset_strategy_registry",
    "reset_synthesis_registry",
]
