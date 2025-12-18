"""Validators: Domain-specific verification for the silent loop.

Validators can be registered via entry points or programmatically.

Entry point group: moss.validators

Example plugin registration in pyproject.toml:
    [project.entry-points."moss.validators"]
    my_validator = "my_package.validators:MyValidator"
"""

from __future__ import annotations

import asyncio
import logging
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum, auto
from importlib.metadata import entry_points
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


class ValidationSeverity(Enum):
    """Severity levels for validation issues."""

    ERROR = auto()
    WARNING = auto()
    INFO = auto()


@dataclass(frozen=True)
class ValidationIssue:
    """A single validation issue."""

    message: str
    severity: ValidationSeverity
    file: Path | None = None
    line: int | None = None
    column: int | None = None
    code: str | None = None  # e.g., "E501" for ruff
    source: str | None = None  # e.g., "ruff", "pytest"

    def __str__(self) -> str:
        parts = []
        if self.file:
            loc = str(self.file)
            if self.line:
                loc += f":{self.line}"
                if self.column:
                    loc += f":{self.column}"
            parts.append(loc)
        if self.code:
            parts.append(f"[{self.code}]")
        parts.append(self.message)
        return " ".join(parts)


@dataclass
class ValidationResult:
    """Result of running a validator."""

    success: bool
    issues: list[ValidationIssue] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    @property
    def errors(self) -> list[ValidationIssue]:
        return [i for i in self.issues if i.severity == ValidationSeverity.ERROR]

    @property
    def warnings(self) -> list[ValidationIssue]:
        return [i for i in self.issues if i.severity == ValidationSeverity.WARNING]

    @property
    def error_count(self) -> int:
        return len(self.errors)

    @property
    def warning_count(self) -> int:
        return len(self.warnings)

    def to_compact(self) -> str:
        """Return compact format for LLM consumption."""
        status = "✓ valid" if self.success else "✗ invalid"
        parts = [status]
        if self.error_count:
            parts.append(f"{self.error_count} errors")
        if self.warning_count:
            parts.append(f"{self.warning_count} warnings")
        return " | ".join(parts)


class Validator(ABC):
    """Abstract base for validators."""

    @property
    @abstractmethod
    def name(self) -> str:
        """Validator name for logging."""
        ...

    @abstractmethod
    async def validate(self, path: Path) -> ValidationResult:
        """Validate a file or directory."""
        ...


class SyntaxValidator(Validator):
    """Validate Python syntax using ast.parse."""

    @property
    def name(self) -> str:
        return "syntax"

    async def validate(self, path: Path) -> ValidationResult:
        issues = []

        if path.is_file():
            files = [path]
        else:
            files = list(path.rglob("*.py"))

        for file in files:
            try:
                import ast

                source = file.read_text()
                ast.parse(source)
            except SyntaxError as e:
                issues.append(
                    ValidationIssue(
                        message=e.msg or "Syntax error",
                        severity=ValidationSeverity.ERROR,
                        file=file,
                        line=e.lineno,
                        column=e.offset,
                        source="syntax",
                    )
                )

        return ValidationResult(
            success=len(issues) == 0,
            issues=issues,
            metadata={"files_checked": len(files)},
        )


class RuffValidator(Validator):
    """Validate using ruff linter."""

    def __init__(self, fix: bool = False):
        self.fix = fix

    @property
    def name(self) -> str:
        return "ruff"

    async def validate(self, path: Path) -> ValidationResult:
        cmd = ["ruff", "check", str(path), "--output-format=json"]
        if self.fix:
            cmd.append("--fix")

        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _stderr = await proc.communicate()

        issues = []
        if stdout:
            import json

            try:
                results = json.loads(stdout.decode())
                for item in results:
                    issues.append(
                        ValidationIssue(
                            message=item.get("message", ""),
                            severity=ValidationSeverity.ERROR,
                            file=Path(item.get("filename", "")),
                            line=item.get("location", {}).get("row"),
                            column=item.get("location", {}).get("column"),
                            code=item.get("code"),
                            source="ruff",
                        )
                    )
            except json.JSONDecodeError:
                pass

        return ValidationResult(
            success=proc.returncode == 0,
            issues=issues,
            metadata={"returncode": proc.returncode},
        )


class PytestValidator(Validator):
    """Validate using pytest."""

    def __init__(self, args: list[str] | None = None):
        self.args = args or []

    @property
    def name(self) -> str:
        return "pytest"

    async def validate(self, path: Path) -> ValidationResult:
        cmd = ["python", "-m", "pytest", str(path), "-v", "--tb=short", *self.args]

        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _stderr = await proc.communicate()

        output = stdout.decode()
        issues = []

        # Parse pytest output for failures
        if proc.returncode != 0:
            # Extract failed test names
            for line in output.splitlines():
                if line.startswith("FAILED"):
                    issues.append(
                        ValidationIssue(
                            message=line,
                            severity=ValidationSeverity.ERROR,
                            source="pytest",
                        )
                    )

            # If no specific failures found, add generic error
            if not issues:
                issues.append(
                    ValidationIssue(
                        message="pytest failed",
                        severity=ValidationSeverity.ERROR,
                        source="pytest",
                    )
                )

        # Count passed/failed
        passed = output.count(" passed")
        failed = output.count(" failed")

        return ValidationResult(
            success=proc.returncode == 0,
            issues=issues,
            metadata={
                "returncode": proc.returncode,
                "passed": passed,
                "failed": failed,
                "output": output,
            },
        )


class CommandValidator(Validator):
    """Generic validator that runs a shell command."""

    def __init__(self, name: str, command: list[str], success_codes: list[int] | None = None):
        self._name = name
        self.command = command
        self.success_codes = success_codes or [0]

    @property
    def name(self) -> str:
        return self._name

    async def validate(self, path: Path) -> ValidationResult:
        # Substitute {path} in command
        cmd = [arg.replace("{path}", str(path)) for arg in self.command]

        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await proc.communicate()

        success = proc.returncode in self.success_codes
        issues = []

        if not success:
            issues.append(
                ValidationIssue(
                    message=f"Command failed with exit code {proc.returncode}",
                    severity=ValidationSeverity.ERROR,
                    source=self._name,
                )
            )
            # Add stderr as additional issue if present
            if stderr:
                issues.append(
                    ValidationIssue(
                        message=stderr.decode().strip()[:500],
                        severity=ValidationSeverity.ERROR,
                        source=self._name,
                    )
                )

        return ValidationResult(
            success=success,
            issues=issues,
            metadata={
                "returncode": proc.returncode,
                "stdout": stdout.decode(),
                "stderr": stderr.decode(),
            },
        )


class ValidatorChain:
    """Run multiple validators in sequence."""

    def __init__(self, validators: list[Validator] | None = None):
        self.validators = validators or []

    def add(self, validator: Validator) -> None:
        self.validators.append(validator)

    async def validate(self, path: Path, *, stop_on_error: bool = True) -> ValidationResult:
        """Run all validators.

        Args:
            path: Path to validate
            stop_on_error: If True, stop after first validator with errors

        Returns:
            Combined ValidationResult
        """
        all_issues: list[ValidationIssue] = []
        all_success = True
        metadata: dict[str, Any] = {"validators": {}}

        for validator in self.validators:
            result = await validator.validate(path)
            all_issues.extend(result.issues)
            metadata["validators"][validator.name] = {
                "success": result.success,
                "errors": result.error_count,
                "warnings": result.warning_count,
            }

            if not result.success:
                all_success = False
                if stop_on_error:
                    break

        return ValidationResult(
            success=all_success,
            issues=all_issues,
            metadata=metadata,
        )


class LinterValidatorAdapter(Validator):
    """Adapter that wraps a LinterPlugin as a Validator.

    This allows using the new plugin-based linters through the existing
    Validator interface, enabling integration with ValidatorChain and
    other validation infrastructure.
    """

    def __init__(self, plugin: Any):
        """Create adapter for a LinterPlugin.

        Args:
            plugin: A LinterPlugin instance
        """
        from moss.plugins.linters import LinterPlugin

        if not isinstance(plugin, LinterPlugin):
            raise TypeError(f"Expected LinterPlugin, got {type(plugin).__name__}")
        self._plugin = plugin

    @property
    def name(self) -> str:
        return self._plugin.metadata.name

    async def validate(self, path: Path) -> ValidationResult:
        """Run the linter plugin and convert result to ValidationResult."""
        from moss.plugins.linters import Severity

        result = await self._plugin.run(path)

        # Convert LinterIssue to ValidationIssue
        issues = []
        for issue in result.issues:
            severity_map = {
                Severity.ERROR: ValidationSeverity.ERROR,
                Severity.WARNING: ValidationSeverity.WARNING,
                Severity.INFO: ValidationSeverity.INFO,
                Severity.HINT: ValidationSeverity.INFO,
            }
            issues.append(
                ValidationIssue(
                    message=issue.message,
                    severity=severity_map.get(issue.severity, ValidationSeverity.INFO),
                    file=issue.file,
                    line=issue.line,
                    column=issue.column,
                    code=issue.rule_id,
                    source=issue.source,
                )
            )

        return ValidationResult(
            success=result.success,
            issues=issues,
            metadata={
                "tool_name": result.tool_name,
                "tool_version": result.tool_version,
                "execution_time_ms": result.execution_time_ms,
            },
        )


def create_python_validator_chain(*, include_tests: bool = False) -> ValidatorChain:
    """Create a standard Python validation chain."""
    chain = ValidatorChain()
    chain.add(SyntaxValidator())
    chain.add(RuffValidator())
    if include_tests:
        chain.add(PytestValidator())
    return chain


# =============================================================================
# Validator Registry
# =============================================================================

# Registry of validator classes
_VALIDATORS: dict[str, type[Validator]] = {}


def register_validator(name: str, validator_class: type[Validator]) -> None:
    """Register a validator class.

    Args:
        name: Validator name (e.g., "syntax", "ruff")
        validator_class: Validator class (not instance)
    """
    _VALIDATORS[name] = validator_class


def get_validator(name: str, **kwargs: Any) -> Validator:
    """Get a validator instance by name.

    Args:
        name: Validator name
        **kwargs: Arguments to pass to validator constructor

    Returns:
        Validator instance

    Raises:
        ValueError: If validator not found
    """
    if name not in _VALIDATORS:
        available = ", ".join(_VALIDATORS.keys())
        raise ValueError(f"Validator '{name}' not found. Available: {available}")
    return _VALIDATORS[name](**kwargs)


def list_validators() -> list[str]:
    """List all registered validator names."""
    return list(_VALIDATORS.keys())


def get_all_validators() -> list[Validator]:
    """Get instances of all registered validators (with default args)."""
    return [cls() for cls in _VALIDATORS.values()]


def _discover_entry_points() -> None:
    """Discover and register validators from entry points."""
    try:
        eps = entry_points(group="moss.validators")
        for ep in eps:
            try:
                validator_class = ep.load()
                if ep.name not in _VALIDATORS:
                    register_validator(ep.name, validator_class)
                    logger.debug("Discovered validator: %s", ep.name)
            except Exception as e:
                logger.warning("Failed to load validator '%s': %s", ep.name, e)
    except Exception:
        pass


def _register_builtin_validators() -> None:
    """Register built-in validators."""
    register_validator("syntax", SyntaxValidator)
    register_validator("ruff", RuffValidator)
    register_validator("pytest", PytestValidator)


# Auto-register on import
_register_builtin_validators()
_discover_entry_points()


__all__ = [
    "CommandValidator",
    "LinterValidatorAdapter",
    "PytestValidator",
    "RuffValidator",
    "SyntaxValidator",
    "ValidationIssue",
    "ValidationResult",
    "ValidationSeverity",
    "Validator",
    "ValidatorChain",
    "create_python_validator_chain",
    "get_all_validators",
    "get_validator",
    "list_validators",
    "register_validator",
]
