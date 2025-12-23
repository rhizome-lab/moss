"""Rule decorator for defining custom analysis rules.

The @rule decorator provides a clean API for defining rules:

    from moss_orchestration.rules import rule, RuleContext, Violation

    @rule(
        backend="ast-grep",
        severity="warning",
        context="not:test",  # Skip test files
    )
    def no_bare_except(ctx: RuleContext) -> list[Violation]:
        '''Detect bare except clauses.'''
        violations = []
        for match in ctx.backend("ast-grep").matches:
            violations.append(ctx.violation(
                "Avoid bare except clauses",
                match.location,
            ))
        return violations

Rules can require multiple backends:

    @rule(backend=["ast-grep", "pyright"])
    def typed_dict_mutation(ctx: RuleContext) -> list[Violation]:
        '''Detect mutation of TypedDict instances.'''
        ast_matches = ctx.backend("ast-grep").matches
        types = ctx.backend("pyright").metadata.get("types", {})
        # Combine AST matches with type information
        ...
"""

from __future__ import annotations

from functools import wraps
from typing import TYPE_CHECKING

from .base import CodeContext, RuleSpec, Severity

if TYPE_CHECKING:
    from collections.abc import Callable

    from .base import RuleContext, Violation

# Global registry of rules
_RULE_REGISTRY: dict[str, RuleSpec] = {}


def rule(
    backend: str | list[str] = "python",
    *,
    name: str | None = None,
    severity: str | Severity = Severity.WARNING,
    category: str = "custom",
    context: str | list[str] | None = None,
    file_pattern: str | list[str] = "**/*.py",
    tags: list[str] | None = None,
    enabled: bool = True,
) -> Callable[[Callable[[RuleContext], list[Violation]]], RuleSpec]:
    """Decorator to define a rule.

    Args:
        backend: Backend(s) required by this rule.
            - "regex": Pattern matching (default)
            - "ast-grep": Structural patterns
            - "pyright": Type-aware analysis
            - "deps": Cross-file dependencies
            - "python": Arbitrary Python checks
            - Can be a list for multi-backend rules

        name: Rule name (defaults to function name)

        severity: Violation severity
            - "info": Informational
            - "warning": Should fix
            - "error": Must fix

        category: Rule category for grouping

        context: Code context filter
            - "library": Main source
            - "test": Test files
            - "example": Examples
            - "cli": CLI code
            - "not:test": Exclude test files
            - Can be a list

        file_pattern: Glob pattern(s) for applicable files

        tags: Optional tags for filtering rules

        enabled: Whether rule is enabled by default

    Returns:
        Decorated function registered as a RuleSpec
    """
    # Normalize backends to list
    backends = [backend] if isinstance(backend, str) else list(backend)

    # Normalize severity
    if isinstance(severity, str):
        severity = Severity(severity)

    # Normalize file patterns
    patterns = [file_pattern] if isinstance(file_pattern, str) else list(file_pattern)

    # Parse context specification
    contexts, exclude_contexts = _parse_context(context)

    def decorator(func: Callable[[RuleContext], list[Violation]]) -> RuleSpec:
        rule_name = name or func.__name__
        description = func.__doc__ or f"Rule: {rule_name}"

        spec = RuleSpec(
            name=rule_name,
            description=description.strip(),
            func=func,
            backends=backends,
            severity=severity,  # type: ignore
            category=category,
            contexts=contexts,
            exclude_contexts=exclude_contexts,
            file_patterns=patterns,
            enabled=enabled,
            tags=tags or [],
        )

        # Register globally
        _RULE_REGISTRY[rule_name] = spec

        # Also attach spec to function for introspection
        @wraps(func)
        def wrapper(ctx: RuleContext) -> list[Violation]:
            return func(ctx)

        wrapper._rule_spec = spec  # type: ignore
        return spec

    return decorator


def _parse_context(
    context: str | list[str] | None,
) -> tuple[list[CodeContext] | None, list[CodeContext] | None]:
    """Parse context specification into include/exclude lists.

    Args:
        context: Context spec like "test", "not:test", or ["library", "cli"]

    Returns:
        Tuple of (include_contexts, exclude_contexts)
    """
    if context is None:
        return None, None

    specs = [context] if isinstance(context, str) else list(context)

    include: list[CodeContext] = []
    exclude: list[CodeContext] = []

    for spec in specs:
        if spec.startswith("not:"):
            ctx_name = spec[4:]
            try:
                exclude.append(CodeContext(ctx_name))
            except ValueError:
                pass  # Ignore invalid contexts
        else:
            try:
                include.append(CodeContext(spec))
            except ValueError:
                pass

    return (include or None), (exclude or None)


def get_registered_rules() -> dict[str, RuleSpec]:
    """Get all registered rules."""
    return dict(_RULE_REGISTRY)


def get_rule(name: str) -> RuleSpec | None:
    """Get a rule by name."""
    return _RULE_REGISTRY.get(name)


def clear_registry() -> None:
    """Clear the rule registry (for testing)."""
    _RULE_REGISTRY.clear()


def register_rule(spec: RuleSpec) -> None:
    """Manually register a rule spec."""
    _RULE_REGISTRY[spec.name] = spec


# =============================================================================
# Convenience functions for common patterns
# =============================================================================


def pattern_rule(
    name: str,
    pattern: str,
    message: str,
    *,
    backend: str = "regex",
    severity: str | Severity = Severity.WARNING,
    category: str = "custom",
    context: str | list[str] | None = None,
    file_pattern: str = "**/*.py",
    fix: str | None = None,
) -> RuleSpec:
    """Create a simple pattern-based rule without a function.

    This is a shorthand for rules that just match a pattern and report:

        pattern_rule(
            "no-print",
            r"\\bprint\\s*\\(",
            "Use logging instead of print",
            severity="info",
        )

    Args:
        name: Rule name
        pattern: Pattern to match (backend-specific)
        message: Message for violations
        backend: Backend to use
        severity: Violation severity
        category: Rule category
        context: Code context filter
        file_pattern: File glob pattern
        fix: Optional fix suggestion

    Returns:
        RuleSpec for the pattern rule
    """
    from .base import Violation

    # Normalize severity
    if isinstance(severity, str):
        severity = Severity(severity)

    # Parse context
    contexts, exclude_contexts = _parse_context(context)

    def check(ctx: RuleContext) -> list[Violation]:
        violations: list[Violation] = []
        backend_result = ctx.backend(backend)
        for match in backend_result.matches:
            violations.append(
                Violation(
                    rule_name=name,
                    message=message,
                    location=match.location,
                    severity=severity,  # type: ignore
                    category=category,
                    fix=fix,
                )
            )
        return violations

    spec = RuleSpec(
        name=name,
        description=message,
        func=check,
        backends=[backend],
        severity=severity,  # type: ignore
        category=category,
        contexts=contexts,
        exclude_contexts=exclude_contexts,
        file_patterns=[file_pattern] if isinstance(file_pattern, str) else file_pattern,
        enabled=True,
        tags=[],
    )

    # Store pattern in spec for backend to use
    spec._pattern = pattern  # type: ignore

    _RULE_REGISTRY[name] = spec
    return spec


def ast_pattern_rule(
    name: str,
    pattern: str,
    message: str,
    *,
    severity: str | Severity = Severity.WARNING,
    category: str = "custom",
    context: str | list[str] | None = None,
    file_pattern: str = "**/*.py",
    fix: str | None = None,
) -> RuleSpec:
    """Shorthand for ast-grep pattern rules.

    Example:
        ast_pattern_rule(
            "no-bare-except",
            "except: $BODY",
            "Avoid bare except clauses",
        )
    """
    return pattern_rule(
        name=name,
        pattern=pattern,
        message=message,
        backend="ast-grep",
        severity=severity,
        category=category,
        context=context,
        file_pattern=file_pattern,
        fix=fix,
    )
