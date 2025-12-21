"""Test-based code validator.

Runs pytest (or jest for JavaScript) to validate generated code against
specification tests. This is the critical "TestExecutorValidator" component
that was missing from the synthesis framework.
"""

from __future__ import annotations

import asyncio
import logging
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

logger = logging.getLogger(__name__)


class PytestValidator:
    """Validator that runs pytest/jest to validate generated code.

    This validator:
    1. Creates a temporary test file combining code + tests
    2. Runs pytest (or jest) in a subprocess
    3. Parses output to determine pass/fail
    4. Optionally generates counterexamples from failures

    Configuration:
    - test_runner: "pytest" or "jest"
    - timeout: Maximum test execution time (seconds)
    - working_dir: Directory to run tests in (for imports)
    """

    def __init__(
        self,
        test_runner: str = "pytest",
        timeout: int = 30,
        working_dir: Path | None = None,
    ) -> None:
        """Initialize test validator.

        Args:
            test_runner: Test runner to use ("pytest" or "jest")
            timeout: Test timeout in seconds
            working_dir: Working directory for test execution
        """
        self._test_runner = test_runner
        self._timeout = timeout
        self._working_dir = working_dir or Path.cwd()

        self._metadata = ValidatorMetadata(
            name=f"{test_runner}",
            validator_type=ValidatorType.TEST,
            languages=(
                frozenset(["python"])
                if test_runner == "pytest"
                else frozenset(["javascript", "typescript"])
            ),
            priority=100,  # High priority - tests are authoritative
            description=f"Run {test_runner} to validate generated code",
            can_generate_counterexample=True,
        )

    @property
    def metadata(self) -> ValidatorMetadata:
        """Return validator metadata."""
        return self._metadata

    def can_validate(self, spec: Specification, code: str) -> bool:
        """Check if we can validate this code.

        Returns True if:
        - Specification has tests defined, OR
        - Specification has examples that can be converted to tests
        """
        # Has explicit tests
        if spec.tests:
            return True

        # Has examples we can convert to tests
        if spec.examples:
            return True

        # Has test code embedded in constraints
        for constraint in spec.constraints:
            if "assert" in constraint.lower() or "test" in constraint.lower():
                return True

        return False

    def _generate_test_code(
        self,
        spec: Specification,
        code: str,
    ) -> str:
        """Generate test code combining implementation and tests.

        Args:
            spec: The specification with tests/examples
            code: The generated code

        Returns:
            Complete test file content
        """
        lines = []

        # Imports
        lines.append("import pytest")
        lines.append("")

        # The generated code
        lines.append("# Generated code")
        lines.append(code)
        lines.append("")

        # Tests from specification
        if spec.tests:
            lines.append("# Tests from specification")
            for i, test in enumerate(spec.tests):
                if isinstance(test, str):
                    lines.append(test)
                else:
                    # Assume it's a dict with test info
                    lines.append(f"def test_spec_{i}():")
                    lines.append(f"    {test}")
            lines.append("")

        # Tests from examples
        if spec.examples:
            lines.append("# Tests from examples")
            lines.append("class TestExamples:")

            # Try to extract function name from code
            func_name = self._extract_function_name(code)

            for i, (inp, expected) in enumerate(spec.examples):
                lines.append(f"    def test_example_{i}(self):")
                if func_name:
                    # Generate assertion calling the function
                    if isinstance(inp, tuple):
                        args_str = ", ".join(repr(a) for a in inp)
                    else:
                        args_str = repr(inp)
                    lines.append(f"        result = {func_name}({args_str})")
                    lines.append(
                        f"        assert result == {expected!r}, "
                        f"f'Expected {expected!r}, got {{result!r}}'"
                    )
                else:
                    # Can't determine function, just document
                    lines.append(f"        # Input: {inp!r}")
                    lines.append(f"        # Expected: {expected!r}")
                    lines.append("        pass  # TODO: call function")
                lines.append("")

        return "\n".join(lines)

    def _extract_function_name(self, code: str) -> str | None:
        """Extract the main function name from code."""
        # Look for def or async def
        match = re.search(r"(?:async\s+)?def\s+(\w+)\s*\(", code)
        if match:
            name = match.group(1)
            # Skip test functions and private functions
            if not name.startswith("test_") and not name.startswith("_"):
                return name
        return None

    async def validate(
        self,
        spec: Specification,
        code: str,
        context: Context,
    ) -> ValidationResult:
        """Validate code by running tests.

        Args:
            spec: The specification
            code: The generated code
            context: Available resources

        Returns:
            ValidationResult with test results
        """
        # Generate combined test file
        test_code = self._generate_test_code(spec, code)

        # Create temporary file
        with tempfile.NamedTemporaryFile(
            mode="w",
            suffix="_test.py" if self._test_runner == "pytest" else ".test.js",
            delete=False,
        ) as f:
            f.write(test_code)
            test_file = Path(f.name)

        try:
            # Run tests
            if self._test_runner == "pytest":
                result = await self._run_pytest(test_file)
            else:
                result = await self._run_jest(test_file)

            return result

        finally:
            # Clean up temporary file
            try:
                test_file.unlink()
            except OSError:
                pass

    async def _run_pytest(self, test_file: Path) -> ValidationResult:
        """Run pytest on the test file."""
        cmd = [
            "python",
            "-m",
            "pytest",
            str(test_file),
            "-v",
            "--tb=short",
            f"--timeout={self._timeout}",
        ]

        try:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                cwd=self._working_dir,
            )

            stdout, stderr = await asyncio.wait_for(
                proc.communicate(),
                timeout=self._timeout + 5,
            )

            output = stdout.decode() + stderr.decode()

            # Parse pytest output
            return self._parse_pytest_output(output, proc.returncode or 0)

        except TimeoutError:
            return ValidationResult(
                success=False,
                issues=["Test execution timed out"],
                metadata={"timeout": self._timeout},
            )
        except FileNotFoundError:
            return ValidationResult(
                success=False,
                issues=["pytest not found - install with: pip install pytest"],
            )
        except (OSError, subprocess.SubprocessError) as e:
            return ValidationResult(
                success=False,
                issues=[f"Test execution failed: {e}"],
            )

    def _parse_pytest_output(self, output: str, returncode: int) -> ValidationResult:
        """Parse pytest output to extract results."""
        issues: list[str] = []
        passed = 0
        failed = 0
        total = 0

        # Look for summary line: "X passed, Y failed"
        summary_match = re.search(
            r"(\d+)\s+passed(?:,\s+(\d+)\s+failed)?",
            output,
        )
        if summary_match:
            passed = int(summary_match.group(1))
            failed = int(summary_match.group(2) or 0)
            total = passed + failed

        # Extract failure messages
        failure_matches = re.findall(
            r"FAILED\s+\S+\s+-\s+(.+?)(?=\n(?:FAILED|PASSED|=|$))",
            output,
            re.DOTALL,
        )
        for failure in failure_matches:
            issues.append(failure.strip()[:200])  # Limit length

        # Extract assertion errors
        assert_matches = re.findall(
            r"AssertionError:\s*(.+?)(?=\n|$)",
            output,
        )
        for assertion in assert_matches:
            if assertion not in issues:
                issues.append(f"Assertion failed: {assertion}")

        # Check for syntax errors
        if "SyntaxError" in output:
            syntax_match = re.search(r"SyntaxError:\s*(.+?)(?=\n|$)", output)
            if syntax_match:
                issues.append(f"Syntax error: {syntax_match.group(1)}")

        success = returncode == 0 and failed == 0

        return ValidationResult(
            success=success,
            passed_checks=passed,
            total_checks=total if total > 0 else 1,
            issues=issues,
            metadata={
                "runner": "pytest",
                "output": output[:1000],  # Truncate
            },
        )

    async def _run_jest(self, test_file: Path) -> ValidationResult:
        """Run jest on the test file."""
        cmd = [
            "npx",
            "jest",
            str(test_file),
            "--no-cache",
            f"--testTimeout={self._timeout * 1000}",
        ]

        try:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                cwd=self._working_dir,
            )

            stdout, stderr = await asyncio.wait_for(
                proc.communicate(),
                timeout=self._timeout + 5,
            )

            output = stdout.decode() + stderr.decode()

            # Parse jest output (simplified)
            success = proc.returncode == 0
            issues = []
            if not success:
                # Extract failure info
                issues = [line for line in output.split("\n") if "FAIL" in line or "Error" in line]

            return ValidationResult(
                success=success,
                passed_checks=1 if success else 0,
                total_checks=1,
                issues=issues[:5],  # Limit issues
                metadata={
                    "runner": "jest",
                    "output": output[:1000],
                },
            )

        except (OSError, subprocess.SubprocessError) as e:
            return ValidationResult(
                success=False,
                issues=[f"Jest execution failed: {e}"],
            )

    async def generate_counterexample(
        self,
        spec: Specification,
        code: str,
        context: Context,
    ) -> tuple[Any, Any] | None:
        """Generate a counterexample from failing tests.

        Returns:
            (input, expected_output) pair where the code fails
        """
        # Run validation first
        result = await self.validate(spec, code, context)

        if result.success:
            return None

        # Try to extract counterexample from issues
        for issue in result.issues:
            # Look for assertion failure pattern
            match = re.search(
                r"Expected\s+(.+?),?\s+got\s+(.+)",
                issue,
            )
            if match:
                expected = match.group(1)
                # Note: match.group(2) contains 'actual' but we don't need it
                # Try to find the input that caused this
                for inp, exp in spec.examples:
                    if repr(exp) in expected:
                        return inp, exp
                # Return what we found
                return ("unknown_input", expected)

        # Fall back to first failing example
        if spec.examples and not result.success:
            return spec.examples[0]

        return None


# Protocol compliance check
assert isinstance(PytestValidator(), SynthesisValidator)

# Backwards compatibility alias
TestValidator = PytestValidator
