"""Validators: Domain-specific verification for the silent loop."""

from __future__ import annotations

import asyncio
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum, auto
from pathlib import Path
from typing import Any


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


def create_python_validator_chain(*, include_tests: bool = False) -> ValidatorChain:
    """Create a standard Python validation chain."""
    chain = ValidatorChain()
    chain.add(SyntaxValidator())
    chain.add(RuffValidator())
    if include_tests:
        chain.add(PytestValidator())
    return chain
