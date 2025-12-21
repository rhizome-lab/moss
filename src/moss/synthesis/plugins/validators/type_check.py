"""Type-based code validator.

Uses mypy or pyright to validate generated code against type signatures
specified in the specification.
"""

from __future__ import annotations

import asyncio
import re
import subprocess
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING, Any

from moss.synthesis.plugins.protocols import (
    SynthesisValidator,
    ValidationResult,
    ValidatorMetadata,
    ValidatorType,
)

if TYPE_CHECKING:
    from moss.synthesis.types import Context, Specification


class TypeValidator:
    """Validator that uses mypy/pyright for type checking.

    This validator:
    1. Creates a temporary file with the code
    2. Runs mypy or pyright
    3. Parses output for type errors

    Particularly useful when specification includes type_signature.
    """

    def __init__(
        self,
        type_checker: str = "mypy",
        strict: bool = False,
        timeout: int = 30,
    ) -> None:
        """Initialize type validator.

        Args:
            type_checker: Type checker to use ("mypy" or "pyright")
            strict: Enable strict mode
            timeout: Timeout in seconds
        """
        self._type_checker = type_checker
        self._strict = strict
        self._timeout = timeout

        self._metadata = ValidatorMetadata(
            name=type_checker,
            validator_type=ValidatorType.TYPE,
            languages=frozenset(["python"]),
            priority=50,  # Medium priority
            description=f"Type checking with {type_checker}",
            can_generate_counterexample=False,
        )

    @property
    def metadata(self) -> ValidatorMetadata:
        """Return validator metadata."""
        return self._metadata

    def can_validate(self, spec: Specification, code: str) -> bool:
        """Check if we should type-check this code.

        Returns True if:
        - Specification has type_signature, OR
        - Code contains type annotations (excluding comments)
        """
        if spec.type_signature:
            return True

        # Check for type annotations in actual code lines (not comments)
        for line in code.split("\n"):
            # Skip comments and empty lines
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue

            # Remove inline comments before checking
            if "#" in line:
                line = line[: line.index("#")]

            # Check for type annotations
            type_patterns = [
                r":\s*[A-Z]\w*",  # : Type (capitalized)
                r":\s*(?:int|str|float|bool|list|dict|tuple|set|None)\b",  # : builtin
                r"->\s*\w+",  # -> return_type
                r"typing\.",  # from typing import
                r"\w+\[",  # Generic types like list[int]
            ]

            for pattern in type_patterns:
                if re.search(pattern, line):
                    return True

        return False

    async def validate(
        self,
        spec: Specification,
        code: str,
        context: Context,
    ) -> ValidationResult:
        """Validate code with type checker.

        Args:
            spec: The specification
            code: The generated code
            context: Available resources

        Returns:
            ValidationResult with type errors
        """
        # Create temporary file
        with tempfile.NamedTemporaryFile(
            mode="w",
            suffix=".py",
            delete=False,
        ) as f:
            f.write(code)
            code_file = Path(f.name)

        try:
            if self._type_checker == "mypy":
                return await self._run_mypy(code_file)
            else:
                return await self._run_pyright(code_file)

        finally:
            try:
                code_file.unlink()
            except OSError:
                pass

    async def _run_mypy(self, code_file: Path) -> ValidationResult:
        """Run mypy on the code file."""
        cmd = [
            "python",
            "-m",
            "mypy",
            str(code_file),
            "--no-error-summary",
            "--show-column-numbers",
        ]

        if self._strict:
            cmd.append("--strict")

        try:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )

            stdout, _stderr = await asyncio.wait_for(
                proc.communicate(),
                timeout=self._timeout,
            )

            output = stdout.decode()

            # Parse mypy output
            return self._parse_mypy_output(output, proc.returncode or 0)

        except TimeoutError:
            return ValidationResult(
                success=False,
                issues=["Type checking timed out"],
            )
        except FileNotFoundError:
            return ValidationResult(
                success=False,
                issues=["mypy not found - install with: pip install mypy"],
            )
        except (OSError, subprocess.SubprocessError) as e:
            return ValidationResult(
                success=False,
                issues=[f"Type checking failed: {e}"],
            )

    def _parse_mypy_output(self, output: str, returncode: int) -> ValidationResult:
        """Parse mypy output to extract errors."""
        issues: list[str] = []

        # Parse error lines
        for line in output.split("\n"):
            if ": error:" in line:
                # Extract just the error message
                match = re.search(r": error: (.+)$", line)
                if match:
                    issues.append(match.group(1))

        success = returncode == 0 and len(issues) == 0

        return ValidationResult(
            success=success,
            passed_checks=0 if issues else 1,
            total_checks=1,
            issues=issues,
            metadata={
                "checker": "mypy",
                "output": output[:500],
            },
        )

    async def _run_pyright(self, code_file: Path) -> ValidationResult:
        """Run pyright on the code file."""
        cmd = [
            "npx",
            "pyright",
            str(code_file),
            "--outputjson",
        ]

        try:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )

            stdout, _stderr = await asyncio.wait_for(
                proc.communicate(),
                timeout=self._timeout,
            )

            output = stdout.decode()

            # Parse JSON output
            import json

            try:
                result = json.loads(output)
                errors = result.get("generalDiagnostics", [])
                issues = [e.get("message", "") for e in errors if e.get("severity") == "error"]
            except json.JSONDecodeError:
                issues = [line for line in output.split("\n") if "error" in line.lower()]

            success = proc.returncode == 0

            return ValidationResult(
                success=success,
                passed_checks=0 if issues else 1,
                total_checks=1,
                issues=issues,
                metadata={"checker": "pyright"},
            )

        except (OSError, subprocess.SubprocessError) as e:
            return ValidationResult(
                success=False,
                issues=[f"Pyright execution failed: {e}"],
            )

    async def generate_counterexample(
        self,
        spec: Specification,
        code: str,
        context: Context,
    ) -> tuple[Any, Any] | None:
        """Type validators cannot generate counterexamples."""
        return None


# Protocol compliance check
assert isinstance(TypeValidator(), SynthesisValidator)
