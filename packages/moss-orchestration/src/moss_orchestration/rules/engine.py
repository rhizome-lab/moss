"""Rule engine for executing rules against codebases.

The engine orchestrates:
1. File discovery and filtering
2. Context detection
3. Backend execution
4. Rule evaluation
5. Result aggregation

Usage:
    from moss_orchestration.rules import RuleEngine, get_registered_rules

    engine = RuleEngine()
    engine.add_rules(get_registered_rules().values())
    result = engine.check_directory(Path("."))

    for violation in result.violations:
        print(f"{violation.location}: {violation.message}")
"""

from __future__ import annotations

import fnmatch
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

from .backends import get_backend
from .base import (
    BackendResult,
    CodeContext,
    RuleContext,
    RuleResult,
    RuleSpec,
    Violation,
)
from .context import detect_context
from .decorator import pattern_rule

if TYPE_CHECKING:
    from collections.abc import Iterable


@dataclass
class EngineConfig:
    """Configuration for the rule engine."""

    # File discovery
    include_patterns: list[str] = field(default_factory=lambda: ["**/*.py"])
    exclude_patterns: list[str] = field(
        default_factory=lambda: [
            "**/__pycache__/*",
            "**/.venv/*",
            "**/venv/*",
            "**/node_modules/*",
            "**/.git/*",
            "**/*.pyc",
        ]
    )

    # Execution
    parallel: bool = True
    max_workers: int = 4

    # Error handling
    continue_on_error: bool = True
    collect_errors: bool = True


class RuleEngine:
    """Engine for executing rules against code.

    The engine manages:
    - Rule registration and filtering
    - Backend instantiation and caching
    - Parallel file processing
    - Context detection
    - Result aggregation
    """

    def __init__(self, config: EngineConfig | None = None) -> None:
        self.config = config or EngineConfig()
        self.rules: dict[str, RuleSpec] = {}
        self._backend_cache: dict[str, Any] = {}

    def add_rule(self, rule: RuleSpec) -> None:
        """Add a rule to the engine."""
        self.rules[rule.name] = rule

    def add_rules(self, rules: Iterable[RuleSpec]) -> None:
        """Add multiple rules."""
        for rule in rules:
            self.add_rule(rule)

    def remove_rule(self, name: str) -> bool:
        """Remove a rule by name."""
        if name in self.rules:
            del self.rules[name]
            return True
        return False

    def get_enabled_rules(self) -> list[RuleSpec]:
        """Get all enabled rules."""
        return [r for r in self.rules.values() if r.enabled]

    def check_file(
        self,
        file_path: Path,
        rules: list[RuleSpec] | None = None,
    ) -> RuleResult:
        """Check a single file against rules.

        Args:
            file_path: File to check
            rules: Rules to apply (defaults to all enabled)

        Returns:
            RuleResult with violations found
        """
        file_path = Path(file_path).resolve()
        rules = rules or self.get_enabled_rules()

        result = RuleResult(files_checked=1)

        # Read source
        try:
            source = file_path.read_text()
        except (OSError, UnicodeDecodeError) as e:
            if self.config.collect_errors:
                result.errors.append(f"Could not read {file_path}: {e}")
            return result

        # Detect context
        context = detect_context(file_path, source)

        # Filter rules by file pattern and context
        applicable_rules = self._filter_rules(rules, file_path, context)
        result.rules_applied = len(applicable_rules)

        if not applicable_rules:
            return result

        # Collect required backends
        required_backends = set()
        for rule in applicable_rules:
            required_backends.update(rule.backends)

        # Run backends
        backend_results = self._run_backends(file_path, source, required_backends, applicable_rules)

        # Run rules
        for rule in applicable_rules:
            try:
                violations = self._run_rule(rule, file_path, source, context, backend_results)
                result.violations.extend(violations)
            except (ValueError, KeyError, TypeError, AttributeError) as e:
                if self.config.collect_errors:
                    result.errors.append(f"Rule '{rule.name}' failed on {file_path}: {e}")
                if not self.config.continue_on_error:
                    raise

        return result

    def check_directory(
        self,
        directory: Path,
        rules: list[RuleSpec] | None = None,
    ) -> RuleResult:
        """Check all files in a directory.

        Args:
            directory: Directory to check
            rules: Rules to apply (defaults to all enabled)

        Returns:
            Aggregated RuleResult
        """
        directory = Path(directory).resolve()
        rules = rules or self.get_enabled_rules()

        # Discover files
        files = self._discover_files(directory)

        if not files:
            return RuleResult()

        result = RuleResult()

        if self.config.parallel and len(files) > 1:
            # Parallel execution
            with ThreadPoolExecutor(max_workers=self.config.max_workers) as executor:
                futures = {executor.submit(self.check_file, f, rules): f for f in files}

                for future in as_completed(futures):
                    file_result = future.result()
                    self._merge_results(result, file_result)
        else:
            # Sequential execution
            for file_path in files:
                file_result = self.check_file(file_path, rules)
                self._merge_results(result, file_result)

        return result

    def _discover_files(self, directory: Path) -> list[Path]:
        """Discover files matching include patterns."""
        files: list[Path] = []

        for pattern in self.config.include_patterns:
            files.extend(directory.glob(pattern))

        # Filter excluded
        filtered: list[Path] = []
        for f in files:
            rel_path = str(f.relative_to(directory))
            excluded = False
            for exclude in self.config.exclude_patterns:
                if fnmatch.fnmatch(rel_path, exclude) or fnmatch.fnmatch(str(f), exclude):
                    excluded = True
                    break
            if not excluded:
                filtered.append(f)

        return sorted(set(filtered))

    def _filter_rules(
        self,
        rules: list[RuleSpec],
        file_path: Path,
        context: CodeContext,
    ) -> list[RuleSpec]:
        """Filter rules applicable to this file and context."""
        applicable: list[RuleSpec] = []

        for rule in rules:
            # Check file patterns
            rel_path = str(file_path)
            matches_pattern = False
            for pattern in rule.file_patterns:
                if fnmatch.fnmatch(rel_path, pattern) or fnmatch.fnmatch(file_path.name, pattern):
                    matches_pattern = True
                    break

            if not matches_pattern:
                continue

            # Check context
            if not rule.applies_to_context(context):
                continue

            applicable.append(rule)

        return applicable

    def _run_backends(
        self,
        file_path: Path,
        source: str,
        backend_names: set[str],
        rules: list[RuleSpec],
    ) -> dict[str, BackendResult]:
        """Run required backends and collect results."""
        results: dict[str, BackendResult] = {}

        # Build pattern map: backend -> list of patterns needed
        patterns_by_backend: dict[str, list[str]] = {}
        for rule in rules:
            pattern = getattr(rule, "_pattern", None)
            if pattern:
                for backend_name in rule.backends:
                    if backend_name not in patterns_by_backend:
                        patterns_by_backend[backend_name] = []
                    patterns_by_backend[backend_name].append(pattern)

        for backend_name in backend_names:
            try:
                backend = get_backend(backend_name)

                # Get patterns for this backend
                patterns = patterns_by_backend.get(backend_name, [])

                if patterns:
                    # Run with each pattern and merge results
                    all_matches = []
                    all_errors = []
                    all_metadata: dict[str, Any] = {}

                    for pattern in patterns:
                        result = backend.analyze(file_path, pattern)
                        all_matches.extend(result.matches)
                        all_errors.extend(result.errors)
                        all_metadata.update(result.metadata)

                    results[backend_name] = BackendResult(
                        backend_name=backend_name,
                        matches=all_matches,
                        metadata=all_metadata,
                        errors=all_errors,
                    )
                else:
                    # Run without pattern (python backend, etc.)
                    results[backend_name] = backend.analyze(file_path)

            except (ValueError, KeyError, TypeError, AttributeError, OSError) as e:
                results[backend_name] = BackendResult(
                    backend_name=backend_name,
                    errors=[str(e)],
                )

        return results

    def _run_rule(
        self,
        rule: RuleSpec,
        file_path: Path,
        source: str,
        context: CodeContext,
        backend_results: dict[str, BackendResult],
    ) -> list[Violation]:
        """Execute a single rule and return violations."""
        # Build rule context
        rule_backends = {
            name: backend_results.get(
                name, BackendResult(backend_name=name, errors=["Backend not available"])
            )
            for name in rule.backends
        }

        ctx = RuleContext(
            file_path=file_path,
            source=source,
            code_context=context,
            backend_results=rule_backends,
        )

        # Run rule function
        violations = rule.func(ctx)

        # Fill in rule name and ensure severity
        for v in violations:
            v.rule_name = rule.name
            if v.severity is None:
                v.severity = rule.severity
            if not v.category:
                v.category = rule.category

        return violations

    def _merge_results(self, target: RuleResult, source: RuleResult) -> None:
        """Merge source result into target."""
        target.violations.extend(source.violations)
        target.files_checked += source.files_checked
        target.rules_applied = max(target.rules_applied, source.rules_applied)
        target.errors.extend(source.errors)


# =============================================================================
# Built-in rules (migrated from old rules.py)
# =============================================================================

# Define built-in rules using pattern_rule helper
_BUILTIN_RULES = [
    pattern_rule(
        "no-print",
        r"\bprint\s*\(",
        "Consider using logging instead of print statements",
        severity="info",
        category="best-practice",
        context="not:test",
    ),
    pattern_rule(
        "no-breakpoint",
        r"\bbreakpoint\s*\(",
        "Remove breakpoint() call before committing",
        severity="warning",
        category="debug",
    ),
    pattern_rule(
        "no-todo",
        r"#\s*TODO[:\s]",
        "TODO comment found",
        severity="info",
        category="documentation",
    ),
    pattern_rule(
        "no-fixme",
        r"#\s*FIXME[:\s]",
        "FIXME comment found - needs attention",
        severity="warning",
        category="documentation",
    ),
    pattern_rule(
        "no-bare-except",
        r"except\s*:",
        "Avoid bare except clauses",
        severity="warning",
        category="error-handling",
        context="not:test",
    ),
]


def get_builtin_rules() -> list[RuleSpec]:
    """Get all built-in rules."""
    return list(_BUILTIN_RULES)


def create_engine_with_builtins(
    include_builtins: bool = True,
    custom_rules: list[RuleSpec] | None = None,
    config: EngineConfig | None = None,
) -> RuleEngine:
    """Create a rule engine with optional built-in rules.

    Args:
        include_builtins: Include built-in rules
        custom_rules: Additional custom rules
        config: Engine configuration

    Returns:
        Configured RuleEngine
    """
    engine = RuleEngine(config)

    if include_builtins:
        engine.add_rules(get_builtin_rules())

    if custom_rules:
        engine.add_rules(custom_rules)

    return engine
