"""Core types and protocols for the rules system.

The rules system provides a multi-backend architecture for custom code analysis.
Each backend provides a different analysis capability:

- **regex**: Simple pattern matching (fast, no AST)
- **ast-grep**: Structural pattern matching (AST-aware)
- **pyright**: Type-aware analysis (requires type stubs)
- **deps**: Cross-file dependency analysis (uses moss deps)
- **python**: Arbitrary Python checks (escape hatch)

Rules can compose multiple backends when needed:

    @rule(backend=["ast-grep", "pyright"])
    def my_rule(ctx: RuleContext) -> list[Violation]:
        ast_matches = ctx.backend("ast-grep").matches
        type_info = ctx.backend("pyright").types
        # Combine information from both backends
        ...
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import TYPE_CHECKING, Any, Protocol, runtime_checkable

if TYPE_CHECKING:
    from collections.abc import Callable


class Severity(Enum):
    """Severity level for rule violations."""

    INFO = "info"
    WARNING = "warning"
    ERROR = "error"


class CodeContext(Enum):
    """Context classification for code.

    Used to scope rules to specific types of code (e.g., skip test code).
    """

    LIBRARY = "library"  # Main source code
    TEST = "test"  # Test files
    EXAMPLE = "example"  # Example/demo code
    CLI = "cli"  # Command-line interface code
    CONFIG = "config"  # Configuration/setup code
    GENERATED = "generated"  # Auto-generated code
    UNKNOWN = "unknown"


@dataclass(frozen=True)
class Location:
    """Source code location."""

    file_path: Path
    line: int
    column: int
    end_line: int | None = None
    end_column: int | None = None

    def __str__(self) -> str:
        return f"{self.file_path}:{self.line}:{self.column}"


@dataclass
class Match:
    """A pattern match from a backend."""

    location: Location
    text: str
    metadata: dict[str, Any] = field(default_factory=dict)

    @property
    def file_path(self) -> Path:
        return self.location.file_path

    @property
    def line(self) -> int:
        return self.location.line


@dataclass
class Violation:
    """A rule violation found in code."""

    rule_name: str
    message: str
    location: Location
    severity: Severity = Severity.WARNING
    category: str = "custom"
    fix: str | None = None
    context_lines: str = ""
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "rule": self.rule_name,
            "message": self.message,
            "file": str(self.location.file_path),
            "line": self.location.line,
            "column": self.location.column,
            "severity": self.severity.value,
            "category": self.category,
            "fix": self.fix,
        }


@dataclass
class BackendResult:
    """Result from a backend analysis."""

    backend_name: str
    matches: list[Match] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)
    errors: list[str] = field(default_factory=list)


@runtime_checkable
class Backend(Protocol):
    """Protocol for analysis backends.

    Backends provide specific analysis capabilities that rules can use.
    Each backend knows how to analyze files in its own way.
    """

    @property
    def name(self) -> str:
        """Backend name (e.g., 'ast-grep', 'pyright')."""
        ...

    def analyze(
        self,
        file_path: Path,
        pattern: str | None = None,
        **options: Any,
    ) -> BackendResult:
        """Analyze a file.

        Args:
            file_path: Path to analyze
            pattern: Optional pattern (backend-specific format)
            **options: Backend-specific options

        Returns:
            BackendResult with matches and metadata
        """
        ...

    def supports_pattern(self, pattern: str) -> bool:
        """Check if this backend can handle the given pattern."""
        ...


class BaseBackend(ABC):
    """Base class for backend implementations."""

    @property
    @abstractmethod
    def name(self) -> str:
        """Backend identifier."""
        ...

    @abstractmethod
    def analyze(
        self,
        file_path: Path,
        pattern: str | None = None,
        **options: Any,
    ) -> BackendResult:
        """Run analysis on a file."""
        ...

    def supports_pattern(self, pattern: str) -> bool:
        """Default: support all patterns."""
        return True


@dataclass
class RuleSpec:
    """Specification for a rule.

    Created by the @rule decorator, contains all metadata needed
    to run the rule.
    """

    name: str
    description: str
    func: Callable[[RuleContext], list[Violation]]
    backends: list[str]  # Required backend names
    severity: Severity = Severity.WARNING
    category: str = "custom"
    contexts: list[CodeContext] | None = None  # None = all contexts
    exclude_contexts: list[CodeContext] | None = None
    file_patterns: list[str] = field(default_factory=lambda: ["**/*.py"])
    enabled: bool = True
    tags: list[str] = field(default_factory=list)

    def applies_to_context(self, context: CodeContext) -> bool:
        """Check if this rule applies to the given code context."""
        # Check exclusions first
        if self.exclude_contexts and context in self.exclude_contexts:
            return False
        # Then check inclusions
        if self.contexts is None:
            return True
        return context in self.contexts


@dataclass
class RuleContext:
    """Context provided to rule functions.

    Contains the file being analyzed and results from all required backends.
    Rules use this to access backend results and create violations.
    """

    file_path: Path
    source: str
    code_context: CodeContext
    backend_results: dict[str, BackendResult] = field(default_factory=dict)

    def backend(self, name: str) -> BackendResult:
        """Get results from a specific backend.

        Raises:
            KeyError: If backend wasn't run for this rule
        """
        if name not in self.backend_results:
            raise KeyError(
                f"Backend '{name}' not available. Available: {list(self.backend_results.keys())}"
            )
        return self.backend_results[name]

    def has_backend(self, name: str) -> bool:
        """Check if backend results are available."""
        return name in self.backend_results

    def violation(
        self,
        message: str,
        location: Location,
        severity: Severity | None = None,
        fix: str | None = None,
        **metadata: Any,
    ) -> Violation:
        """Create a violation (helper method for rule functions)."""
        return Violation(
            rule_name="",  # Filled in by engine
            message=message,
            location=location,
            severity=severity or Severity.WARNING,
            fix=fix,
            metadata=metadata,
        )

    def location(
        self,
        line: int,
        column: int = 1,
        end_line: int | None = None,
        end_column: int | None = None,
    ) -> Location:
        """Create a location in the current file."""
        return Location(
            file_path=self.file_path,
            line=line,
            column=column,
            end_line=end_line,
            end_column=end_column,
        )


@dataclass
class RuleResult:
    """Result of running rules on a codebase."""

    violations: list[Violation] = field(default_factory=list)
    files_checked: int = 0
    rules_applied: int = 0
    errors: list[str] = field(default_factory=list)

    def by_severity(self, severity: Severity) -> list[Violation]:
        """Filter violations by severity."""
        return [v for v in self.violations if v.severity == severity]

    def by_rule(self, rule_name: str) -> list[Violation]:
        """Filter violations by rule name."""
        return [v for v in self.violations if v.rule_name == rule_name]

    def by_file(self, file_path: Path) -> list[Violation]:
        """Filter violations by file."""
        return [v for v in self.violations if v.location.file_path == file_path]

    @property
    def error_count(self) -> int:
        return len(self.by_severity(Severity.ERROR))

    @property
    def warning_count(self) -> int:
        return len(self.by_severity(Severity.WARNING))

    @property
    def info_count(self) -> int:
        return len(self.by_severity(Severity.INFO))

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "files_checked": self.files_checked,
            "rules_applied": self.rules_applied,
            "total_violations": len(self.violations),
            "by_severity": {
                "error": self.error_count,
                "warning": self.warning_count,
                "info": self.info_count,
            },
            "violations": [v.to_dict() for v in self.violations],
            "errors": self.errors,
        }
