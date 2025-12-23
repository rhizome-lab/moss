"""Multi-backend rules system for custom code analysis.

This module provides a flexible framework for defining and running
custom code analysis rules. Rules can use multiple backends:

- **regex**: Simple pattern matching
- **ast-grep**: Structural AST patterns
- **python**: Arbitrary Python checks
- **pyright**: Type-aware analysis (planned)
- **deps**: Cross-file analysis (planned)

Quick Start:

    from moss_orchestration.rules import rule, RuleContext, Violation, RuleEngine

    # Define a rule
    @rule(backend="regex", severity="warning")
    def no_print(ctx: RuleContext) -> list[Violation]:
        '''Detect print statements.'''
        violations = []
        for match in ctx.backend("regex").matches:
            violations.append(ctx.violation(
                "Use logging instead of print",
                match.location,
            ))
        return violations

    # Run rules
    engine = RuleEngine()
    engine.add_rule(no_print)
    result = engine.check_directory(Path("."))

Context-Aware Rules:

    @rule(backend="python", context="not:test")
    def check_docstrings(ctx: RuleContext) -> list[Violation]:
        '''Public functions should have docstrings.'''
        violations = []
        for func in ctx.backend("python").metadata["functions"]:
            if not func["docstring"] and not func["name"].startswith("_"):
                violations.append(ctx.violation(
                    f"Missing docstring: {func['name']}",
                    ctx.location(func["line"]),
                ))
        return violations

Multi-Backend Rules:

    @rule(backend=["ast-grep", "python"])
    def complex_rule(ctx: RuleContext) -> list[Violation]:
        '''Combine multiple analysis techniques.'''
        ast_matches = ctx.backend("ast-grep").matches
        type_info = ctx.backend("python").metadata
        # Use both for richer analysis
        ...

Pattern Rules (Shorthand):

    from moss_orchestration.rules import pattern_rule, ast_pattern_rule

    # Simple regex pattern
    pattern_rule(
        "no-debug",
        r"import pdb",
        "Remove debug imports",
        severity="warning",
    )

    # AST-aware pattern
    ast_pattern_rule(
        "no-star-import",
        "from $MOD import *",
        "Avoid star imports",
    )
"""

from .base import (
    BackendResult,
    CodeContext,
    Location,
    Match,
    RuleContext,
    RuleResult,
    RuleSpec,
    Severity,
    Violation,
)
from .config import load_rules_from_config, load_rules_from_toml
from .context import (
    ContextDetector,
    ContextDetectorConfig,
    ContextHint,
    configure_detector,
    detect_context,
)
from .decorator import (
    ast_pattern_rule,
    clear_registry,
    get_registered_rules,
    get_rule,
    pattern_rule,
    register_rule,
    rule,
)
from .engine import (
    EngineConfig,
    RuleEngine,
    create_engine_with_builtins,
    get_builtin_rules,
)

__all__ = [
    "BackendResult",
    "CodeContext",
    "ContextDetector",
    "ContextDetectorConfig",
    "ContextHint",
    "EngineConfig",
    "Location",
    "Match",
    "RuleContext",
    "RuleEngine",
    "RuleResult",
    "RuleSpec",
    "Severity",
    "Violation",
    "ast_pattern_rule",
    "clear_registry",
    "configure_detector",
    "create_engine_with_builtins",
    "detect_context",
    "get_builtin_rules",
    "get_registered_rules",
    "get_rule",
    "load_rules_from_config",
    "load_rules_from_toml",
    "pattern_rule",
    "register_rule",
    "rule",
]
