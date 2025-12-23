"""Graceful error handling with actionable feedback.

Categorizes errors and provides recovery suggestions. Goal is to never crash
unexpectedly - either handle gracefully or provide clear guidance.

Usage:
    from moss_orchestration.errors import MossError, handle_error, ErrorCategory

    try:
        risky_operation()
    except Exception as e:
        result = handle_error(e)
        print(result.message)
        if result.suggestion:
            print(f"Try: {result.suggestion}")
"""

from __future__ import annotations

import builtins
from dataclasses import dataclass
from enum import Enum, auto
from pathlib import Path
from typing import Any


class ErrorCategory(Enum):
    """Categories of errors for appropriate handling."""

    FILE_NOT_FOUND = auto()  # Missing files
    PERMISSION_DENIED = auto()  # Access issues
    PARSE_ERROR = auto()  # Syntax/parsing failures
    IMPORT_ERROR = auto()  # Missing dependencies
    TIMEOUT = auto()  # Operation timeouts
    NETWORK = auto()  # Network/connection issues
    CONFIG = auto()  # Configuration problems
    VALIDATION = auto()  # Invalid input/state
    RESOURCE = auto()  # Resource exhaustion (memory, disk)
    INTERNAL = auto()  # Unexpected internal errors
    CANCELLED = auto()  # User cancellation
    EXTERNAL = auto()  # External tool/process failures


@dataclass
class ErrorResult:
    """Structured error result with context and suggestions."""

    category: ErrorCategory
    message: str
    original: Exception | None = None
    suggestion: str | None = None
    context: dict[str, Any] | None = None
    recoverable: bool = True

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "category": self.category.name,
            "message": self.message,
            "suggestion": self.suggestion,
            "recoverable": self.recoverable,
            "context": self.context,
        }

    def to_compact(self) -> str:
        """Format as compact string."""
        parts = [f"[{self.category.name}] {self.message}"]
        if self.suggestion:
            parts.append(f"  Try: {self.suggestion}")
        return "\n".join(parts)


class MossError(Exception):
    """Base exception for moss with structured error handling."""

    def __init__(
        self,
        message: str,
        category: ErrorCategory = ErrorCategory.INTERNAL,
        suggestion: str | None = None,
        context: dict[str, Any] | None = None,
        recoverable: bool = True,
    ):
        super().__init__(message)
        self.category = category
        self.suggestion = suggestion
        self.context = context or {}
        self.recoverable = recoverable

    def to_result(self) -> ErrorResult:
        """Convert to ErrorResult."""
        return ErrorResult(
            category=self.category,
            message=str(self),
            original=self,
            suggestion=self.suggestion,
            context=self.context,
            recoverable=self.recoverable,
        )


# Specific error types for common cases
class FileNotFoundMossError(MossError):
    """File or path not found."""

    def __init__(self, path: str | Path, context: dict[str, Any] | None = None):
        super().__init__(
            f"File not found: {path}",
            category=ErrorCategory.FILE_NOT_FOUND,
            suggestion="Check the path exists and spelling is correct",
            context={"path": str(path), **(context or {})},
        )


class ParseError(MossError):
    """Failed to parse file content."""

    def __init__(
        self,
        path: str | Path,
        line: int | None = None,
        detail: str = "",
        context: dict[str, Any] | None = None,
    ):
        loc = f" at line {line}" if line else ""
        msg = f"Parse error in {path}{loc}"
        if detail:
            msg += f": {detail}"
        super().__init__(
            msg,
            category=ErrorCategory.PARSE_ERROR,
            suggestion="Check file syntax. Run linter for details.",
            context={"path": str(path), "line": line, **(context or {})},
        )


class DependencyError(MossError):
    """Missing optional dependency."""

    def __init__(self, package: str, feature: str = "", context: dict[str, Any] | None = None):
        msg = f"Missing dependency: {package}"
        suggestion = f"pip install {package}"
        if feature:
            suggestion = f"pip install 'moss[{feature}]'"
        super().__init__(
            msg,
            category=ErrorCategory.IMPORT_ERROR,
            suggestion=suggestion,
            context={"package": package, "feature": feature, **(context or {})},
        )


class ConfigError(MossError):
    """Configuration file or setting issue."""

    def __init__(
        self,
        message: str,
        file: str | None = None,
        context: dict[str, Any] | None = None,
    ):
        super().__init__(
            message,
            category=ErrorCategory.CONFIG,
            suggestion="Check .moss/config.toml or pyproject.toml [tool.moss]",
            context={"file": file, **(context or {})},
        )


class TimeoutError(MossError):
    """Operation timed out."""

    def __init__(
        self,
        operation: str,
        timeout_seconds: float,
        context: dict[str, Any] | None = None,
    ):
        super().__init__(
            f"{operation} timed out after {timeout_seconds}s",
            category=ErrorCategory.TIMEOUT,
            suggestion="Increase timeout or check if operation is stuck",
            context={"operation": operation, "timeout": timeout_seconds, **(context or {})},
        )


class ValidationError(MossError):
    """Invalid input or state."""

    def __init__(self, message: str, context: dict[str, Any] | None = None):
        super().__init__(
            message,
            category=ErrorCategory.VALIDATION,
            context=context,
        )


# Error classification and suggestion lookup
_ERROR_PATTERNS: list[tuple[type, ErrorCategory, str | None]] = [
    (builtins.FileNotFoundError, ErrorCategory.FILE_NOT_FOUND, "Check path exists"),
    (builtins.PermissionError, ErrorCategory.PERMISSION_DENIED, "Check file permissions"),
    (builtins.SyntaxError, ErrorCategory.PARSE_ERROR, "Check file syntax"),
    (builtins.ImportError, ErrorCategory.IMPORT_ERROR, None),  # Handled specially
    (builtins.ModuleNotFoundError, ErrorCategory.IMPORT_ERROR, None),
    (builtins.KeyboardInterrupt, ErrorCategory.CANCELLED, None),
    (builtins.ConnectionError, ErrorCategory.NETWORK, "Check network connection"),
    (builtins.OSError, ErrorCategory.RESOURCE, None),  # Catch-all for OS issues
    (builtins.ValueError, ErrorCategory.VALIDATION, None),
    (builtins.TypeError, ErrorCategory.VALIDATION, None),
]

# Import error suggestions
_IMPORT_SUGGESTIONS: dict[str, str] = {
    "mcp": "pip install 'moss[mcp]'",
    "litellm": "pip install 'moss[llm]'",
    "anthropic": "pip install anthropic",
    "openai": "pip install openai",
    "tree_sitter": "pip install tree-sitter",
    "pygments": "pip install pygments",
    "rich": "pip install rich",
    "yaml": "pip install pyyaml",
    "toml": "pip install toml",
    "pytest": "pip install pytest",
}


def _get_import_suggestion(error: ImportError | ModuleNotFoundError) -> str | None:
    """Get suggestion for import error."""
    msg = str(error).lower()
    for pkg, suggestion in _IMPORT_SUGGESTIONS.items():
        if pkg in msg:
            return suggestion
    return "pip install <missing-package>"


def handle_error(error: Exception, context: dict[str, Any] | None = None) -> ErrorResult:
    """Convert any exception to a structured ErrorResult.

    This is the main entry point for graceful error handling. It:
    1. Categorizes the error
    2. Extracts useful information
    3. Provides actionable suggestions when possible
    """
    # Already a MossError - just convert
    if isinstance(error, MossError):
        result = error.to_result()
        if context:
            result.context = {**(result.context or {}), **context}
        return result

    # Match against known patterns
    for error_type, category, suggestion in _ERROR_PATTERNS:
        if isinstance(error, error_type):
            # Special handling for import errors
            if category == ErrorCategory.IMPORT_ERROR:
                suggestion = _get_import_suggestion(error)  # type: ignore

            return ErrorResult(
                category=category,
                message=str(error),
                original=error,
                suggestion=suggestion,
                context=context,
                recoverable=category != ErrorCategory.INTERNAL,
            )

    # Unknown error - treat as internal
    return ErrorResult(
        category=ErrorCategory.INTERNAL,
        message=str(error),
        original=error,
        suggestion="This may be a bug. Please report at github.com/moss/issues",
        context=context,
        recoverable=False,
    )


def safe_execute(func, *args, default=None, **kwargs):
    """Execute a function, returning default on any error.

    Useful for operations where failure is acceptable.
    """
    try:
        return func(*args, **kwargs)
    except Exception:
        return default


async def safe_execute_async(coro, default=None):
    """Execute a coroutine, returning default on any error."""
    try:
        return await coro
    except Exception:
        return default


class ErrorCollector:
    """Collect multiple errors without stopping execution.

    Useful for batch operations where you want to process everything
    and report all errors at the end.
    """

    def __init__(self):
        self.errors: list[ErrorResult] = []
        self.successes: int = 0

    def record(self, error: Exception, context: dict[str, Any] | None = None) -> None:
        """Record an error."""
        self.errors.append(handle_error(error, context))

    def success(self) -> None:
        """Record a successful operation."""
        self.successes += 1

    def has_errors(self) -> bool:
        """Check if any errors were recorded."""
        return len(self.errors) > 0

    def summary(self) -> str:
        """Get summary of collected errors."""
        total = self.successes + len(self.errors)
        if not self.errors:
            return f"All {total} operations succeeded"

        lines = [f"{self.successes}/{total} succeeded, {len(self.errors)} errors:"]
        for i, err in enumerate(self.errors[:10], 1):
            lines.append(f"  {i}. {err.to_compact()}")
        if len(self.errors) > 10:
            lines.append(f"  ... and {len(self.errors) - 10} more")
        return "\n".join(lines)
