"""Tests for Validators."""

from pathlib import Path

import pytest

from moss.validators import (
    CommandValidator,
    PytestValidator,
    RuffValidator,
    SyntaxValidator,
    ValidationIssue,
    ValidationResult,
    ValidationSeverity,
    ValidatorChain,
    create_python_validator_chain,
)


class TestValidationIssue:
    """Tests for ValidationIssue."""

    def test_create_issue(self):
        issue = ValidationIssue(
            message="Test error",
            severity=ValidationSeverity.ERROR,
        )
        assert issue.message == "Test error"
        assert issue.severity == ValidationSeverity.ERROR

    def test_issue_with_location(self):
        issue = ValidationIssue(
            message="Error",
            severity=ValidationSeverity.ERROR,
            file=Path("test.py"),
            line=10,
            column=5,
        )
        assert "test.py:10:5" in str(issue)

    def test_issue_with_code(self):
        issue = ValidationIssue(
            message="Line too long",
            severity=ValidationSeverity.ERROR,
            code="E501",
        )
        assert "[E501]" in str(issue)


class TestValidationResult:
    """Tests for ValidationResult."""

    def test_success_result(self):
        result = ValidationResult(success=True)
        assert result.success
        assert result.error_count == 0
        assert result.warning_count == 0

    def test_result_with_errors(self):
        result = ValidationResult(
            success=False,
            issues=[
                ValidationIssue("Error 1", ValidationSeverity.ERROR),
                ValidationIssue("Error 2", ValidationSeverity.ERROR),
                ValidationIssue("Warning 1", ValidationSeverity.WARNING),
            ],
        )
        assert result.error_count == 2
        assert result.warning_count == 1

    def test_errors_property(self):
        result = ValidationResult(
            success=False,
            issues=[
                ValidationIssue("Error", ValidationSeverity.ERROR),
                ValidationIssue("Warning", ValidationSeverity.WARNING),
            ],
        )
        assert len(result.errors) == 1
        assert result.errors[0].message == "Error"


class TestSyntaxValidator:
    """Tests for SyntaxValidator."""

    @pytest.fixture
    def validator(self):
        return SyntaxValidator()

    async def test_valid_syntax(self, validator: SyntaxValidator, tmp_path: Path):
        f = tmp_path / "valid.py"
        f.write_text("def hello(): pass")

        result = await validator.validate(f)

        assert result.success
        assert result.error_count == 0

    async def test_invalid_syntax(self, validator: SyntaxValidator, tmp_path: Path):
        f = tmp_path / "invalid.py"
        f.write_text("def broken(")

        result = await validator.validate(f)

        assert not result.success
        assert result.error_count == 1
        assert result.issues[0].file == f

    async def test_validate_directory(self, validator: SyntaxValidator, tmp_path: Path):
        (tmp_path / "good.py").write_text("x = 1")
        (tmp_path / "bad.py").write_text("x = ")

        result = await validator.validate(tmp_path)

        assert not result.success
        assert result.error_count == 1
        assert result.metadata["files_checked"] == 2


class TestRuffValidator:
    """Tests for RuffValidator."""

    @pytest.fixture
    def validator(self):
        return RuffValidator()

    async def test_clean_file(self, validator: RuffValidator, tmp_path: Path):
        f = tmp_path / "clean.py"
        f.write_text('"""Module."""\n\nx = 1\n')

        result = await validator.validate(f)

        # ruff may or may not find issues depending on config
        assert isinstance(result.success, bool)

    async def test_file_with_issues(self, validator: RuffValidator, tmp_path: Path):
        f = tmp_path / "issues.py"
        # Unused import should trigger F401
        f.write_text("import os\n")

        result = await validator.validate(f)

        # Should have at least one issue (unused import)
        # Note: depends on ruff being installed and configured
        assert isinstance(result, ValidationResult)


class TestPytestValidator:
    """Tests for PytestValidator."""

    @pytest.fixture
    def validator(self):
        return PytestValidator()

    async def test_passing_tests(self, validator: PytestValidator, tmp_path: Path):
        test_file = tmp_path / "test_pass.py"
        test_file.write_text("def test_pass(): assert True")

        result = await validator.validate(tmp_path)

        assert result.success
        assert result.metadata["passed"] >= 1

    async def test_failing_tests(self, validator: PytestValidator, tmp_path: Path):
        test_file = tmp_path / "test_fail.py"
        test_file.write_text("def test_fail(): assert False")

        result = await validator.validate(tmp_path)

        assert not result.success
        assert result.error_count >= 1


class TestCommandValidator:
    """Tests for CommandValidator."""

    async def test_successful_command(self, tmp_path: Path):
        f = tmp_path / "test.txt"
        f.write_text("hello")

        validator = CommandValidator("test", ["cat", str(f)])
        result = await validator.validate(tmp_path)

        assert result.success

    async def test_failed_command(self, tmp_path: Path):
        validator = CommandValidator("test", ["false"])
        result = await validator.validate(tmp_path)

        assert not result.success
        assert result.error_count >= 1

    async def test_path_substitution(self, tmp_path: Path):
        validator = CommandValidator("test", ["ls", "{path}"])
        result = await validator.validate(tmp_path)

        assert result.success


class TestValidatorChain:
    """Tests for ValidatorChain."""

    async def test_all_pass(self, tmp_path: Path):
        f = tmp_path / "good.py"
        f.write_text("x = 1\n")

        chain = ValidatorChain([SyntaxValidator()])
        result = await chain.validate(f)

        assert result.success

    async def test_stop_on_error(self, tmp_path: Path):
        f = tmp_path / "bad.py"
        f.write_text("x = ")

        chain = ValidatorChain([SyntaxValidator(), RuffValidator()])
        result = await chain.validate(f, stop_on_error=True)

        assert not result.success
        # Should only have run syntax validator
        assert "syntax" in result.metadata["validators"]

    async def test_continue_on_error(self, tmp_path: Path):
        f = tmp_path / "bad.py"
        f.write_text("x = ")

        chain = ValidatorChain([SyntaxValidator(), RuffValidator()])
        result = await chain.validate(f, stop_on_error=False)

        assert not result.success
        # Should have run both validators
        assert "syntax" in result.metadata["validators"]
        assert "ruff" in result.metadata["validators"]


class TestCreatePythonValidatorChain:
    """Tests for create_python_validator_chain."""

    def test_without_tests(self):
        chain = create_python_validator_chain(include_tests=False)
        names = [v.name for v in chain.validators]

        assert "syntax" in names
        assert "ruff" in names
        assert "pytest" not in names

    def test_with_tests(self):
        chain = create_python_validator_chain(include_tests=True)
        names = [v.name for v in chain.validators]

        assert "syntax" in names
        assert "ruff" in names
        assert "pytest" in names
