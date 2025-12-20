"""Tests for the edit module."""

from __future__ import annotations

from pathlib import Path

import pytest

from moss.edit import (
    EditContext,
    TaskComplexity,
    analyze_complexity,
    is_direct_edit,
    is_localized_change,
    is_multi_file_change,
    is_new_feature,
    is_novel_problem,
)


class TestComplexityAnalysis:
    """Tests for complexity analysis functions."""

    def test_simple_rename(self):
        """Rename tasks should be simple."""
        assert (
            analyze_complexity("rename bar to bar", EditContext(project_root=Path(".")))
            == TaskComplexity.SIMPLE
        )

    def test_simple_fix_typo(self):
        """Fix typo tasks should be simple."""
        assert (
            analyze_complexity("fix typo in function name", EditContext(project_root=Path(".")))
            == TaskComplexity.SIMPLE
        )

    def test_simple_remove_unused(self):
        """Remove unused code should be simple."""
        assert (
            analyze_complexity("remove unused imports", EditContext(project_root=Path(".")))
            == TaskComplexity.SIMPLE
        )

    def test_simple_add_import(self):
        """Add import should be simple."""
        assert (
            analyze_complexity("add import for datetime", EditContext(project_root=Path(".")))
            == TaskComplexity.SIMPLE
        )

    def test_medium_add_function(self):
        """Add function should be medium."""
        assert (
            analyze_complexity(
                "add function to calculate total", EditContext(project_root=Path("."))
            )
            == TaskComplexity.MEDIUM
        )

    def test_medium_refactor(self):
        """Refactor tasks should be medium."""
        assert (
            analyze_complexity("refactor UserManager class", EditContext(project_root=Path(".")))
            == TaskComplexity.MEDIUM
        )

    def test_medium_extract(self):
        """Extract tasks should be medium."""
        assert (
            analyze_complexity(
                "extract function from process_data", EditContext(project_root=Path("."))
            )
            == TaskComplexity.MEDIUM
        )

    def test_medium_add_logging(self):
        """Add logging should be medium."""
        assert (
            analyze_complexity("add logging to API calls", EditContext(project_root=Path(".")))
            == TaskComplexity.MEDIUM
        )

    def test_complex_implement(self):
        """Implement tasks should be complex."""
        assert (
            analyze_complexity("implement user authentication", EditContext(project_root=Path(".")))
            == TaskComplexity.COMPLEX
        )

    def test_complex_add_feature(self):
        """Add feature tasks should be complex."""
        assert (
            analyze_complexity(
                "add feature for exporting data", EditContext(project_root=Path("."))
            )
            == TaskComplexity.COMPLEX
        )

    def test_complex_create_api(self):
        """Create API tasks should be complex."""
        assert (
            analyze_complexity(
                "create api for user management", EditContext(project_root=Path("."))
            )
            == TaskComplexity.COMPLEX
        )

    def test_complex_multi_file(self):
        """Multi-file tasks should be complex."""
        assert (
            analyze_complexity("update types across all files", EditContext(project_root=Path(".")))
            == TaskComplexity.COMPLEX
        )

    def test_novel_design(self):
        """Design tasks should be novel."""
        assert (
            analyze_complexity(
                "design new plugin architecture", EditContext(project_root=Path("."))
            )
            == TaskComplexity.NOVEL
        )

    def test_novel_from_scratch(self):
        """From scratch tasks should be novel."""
        assert (
            analyze_complexity(
                "build authentication from scratch", EditContext(project_root=Path("."))
            )
            == TaskComplexity.NOVEL
        )

    def test_short_task_is_simple(self):
        """Very short tasks default to simple."""
        assert (
            analyze_complexity("fix it", EditContext(project_root=Path(".")))
            == TaskComplexity.SIMPLE
        )

    def test_long_task_is_complex(self):
        """Long detailed tasks default to complex."""
        long_task = " ".join(["word"] * 40)
        assert (
            analyze_complexity(long_task, EditContext(project_root=Path(".")))
            == TaskComplexity.COMPLEX
        )


class TestHelperFunctions:
    """Tests for helper classification functions."""

    def test_is_direct_edit(self):
        """Test is_direct_edit function."""
        assert is_direct_edit("rename bar to bar")
        assert is_direct_edit("fix typo")
        assert not is_direct_edit("implement new feature")

    def test_is_localized_change(self):
        """Test is_localized_change function."""
        assert is_localized_change("add function to utils")
        assert is_localized_change("refactor the class")
        assert not is_localized_change("rename bar to bar")

    def test_is_multi_file_change(self):
        """Test is_multi_file_change function."""
        assert is_multi_file_change("update across all files")
        assert is_multi_file_change("project-wide refactoring")
        assert not is_multi_file_change("fix this function")

    def test_is_new_feature(self):
        """Test is_new_feature function."""
        assert is_new_feature("implement user login")
        assert is_new_feature("add feature for exports")
        assert is_new_feature("create new endpoint")
        assert not is_new_feature("fix bug in login")

    def test_is_novel_problem(self):
        """Test is_novel_problem function."""
        assert is_novel_problem("design new architecture")
        assert is_novel_problem("architect the system")
        assert not is_novel_problem("add a function")


class TestEditContext:
    """Tests for EditContext."""

    def test_default_context(self):
        """Test default context values."""
        ctx = EditContext(project_root=Path("/test"))
        assert ctx.project_root == Path("/test")
        assert ctx.target_file is None
        assert ctx.target_symbol is None
        assert ctx.language == "python"
        assert ctx.constraints == []

    def test_context_with_target(self):
        """Test context with target file."""
        ctx = EditContext(
            project_root=Path("/test"),
            target_file=Path("/test/src/main.py"),
            target_symbol="process_data",
        )
        assert ctx.target_file == Path("/test/src/main.py")
        assert ctx.target_symbol == "process_data"


class TestEditAsync:
    """Async tests for edit functions."""

    @pytest.mark.asyncio
    async def test_structural_edit_rename(self):
        """Test structural edit for rename returns correct method."""
        from moss.edit import structural_edit

        ctx = EditContext(project_root=Path("."))
        result = await structural_edit("rename something_else to something_else", ctx)

        # Returns a result (may succeed with 0 changes or fail)
        assert result.method == "structural"

    @pytest.mark.asyncio
    async def test_structural_edit_unknown_type(self):
        """Test structural edit for unknown task type."""
        from moss.edit import structural_edit

        ctx = EditContext(project_root=Path("."))
        result = await structural_edit("do something weird", ctx)

        assert not result.success
        assert "not yet implemented" in result.error.lower()

    @pytest.mark.asyncio
    async def test_multi_agent_edit_not_implemented(self):
        """Test multi-agent edit returns not implemented."""
        from moss.edit import multi_agent_edit

        ctx = EditContext(project_root=Path("."))
        result = await multi_agent_edit("refactor the class", ctx)

        assert not result.success
        assert "not yet implemented" in result.error.lower()
        assert result.method == "multi_agent"


class TestExtractSpecification:
    """Tests for specification extraction."""

    def test_basic_extraction(self):
        """Test basic specification extraction."""
        from moss.edit import extract_specification

        ctx = EditContext(project_root=Path("."))
        spec = extract_specification("implement user login", ctx)

        assert spec.description == "implement user login"
        assert spec.type_signature is None

    def test_extraction_with_symbol(self):
        """Test extraction includes symbol context."""
        from moss.edit import extract_specification

        ctx = EditContext(project_root=Path("."), target_symbol="UserManager")
        spec = extract_specification("add validation", ctx)

        assert "UserManager" in spec.description

    def test_extraction_with_constraints(self):
        """Test extraction includes constraints."""
        from moss.edit import extract_specification

        ctx = EditContext(
            project_root=Path("."),
            constraints=["must handle errors", "should be async"],
        )
        spec = extract_specification("implement login", ctx)

        assert len(spec.constraints) >= 2

    def test_extraction_with_must_clauses(self):
        """Test extraction extracts must/should clauses."""
        from moss.edit import extract_specification

        ctx = EditContext(project_root=Path("."))
        spec = extract_specification(
            "implement login, must validate input, should log attempts", ctx
        )

        # Should extract "must validate input" and "should log attempts"
        assert len(spec.constraints) >= 2


class TestEditAPI:
    """Tests for the EditAPI."""

    def test_write_file(self, tmp_path):
        """Test writing a new file."""
        from moss.edit import EditAPI

        api = EditAPI(tmp_path)
        file_path = "test.txt"
        content = "hello world"

        result = api.write_file(file_path, content)

        assert result.success
        assert (tmp_path / file_path).read_text() == content
        assert result.new_size == len(content)

    def test_replace_text(self, tmp_path):
        """Test replacing text in a file."""
        from moss.edit import EditAPI

        api = EditAPI(tmp_path)
        file_path = "test.txt"
        (tmp_path / file_path).write_text("hello world\nhello universe")

        # Replace all
        result = api.replace_text(file_path, "hello", "hi")
        assert result.success
        assert (tmp_path / file_path).read_text() == "hi world\nhi universe"

        # Replace specific occurrence
        (tmp_path / file_path).write_text("a b a c")
        result = api.replace_text(file_path, "a", "x", occurrence=2)
        assert result.success
        assert (tmp_path / file_path).read_text() == "a b x c"

    def test_insert_line(self, tmp_path):
        """Test inserting lines."""
        from moss.edit import EditAPI

        api = EditAPI(tmp_path)
        file_path = "test.txt"
        (tmp_path / file_path).write_text("line 1\nline 3")

        # Insert at line
        result = api.insert_line(file_path, "line 2", at_line=2)
        assert result.success
        assert (tmp_path / file_path).read_text() == "line 1\nline 2\nline 3"

        # Insert after pattern
        result = api.insert_line(file_path, "line 4", after_pattern="line 3")
        assert result.success
        assert (tmp_path / file_path).read_text() == "line 1\nline 2\nline 3\nline 4\n"

        # Append
        result = api.insert_line(file_path, "line 5")
        assert result.success
        assert (tmp_path / file_path).read_text().endswith("line 5\n")
