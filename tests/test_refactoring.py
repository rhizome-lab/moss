"""Tests for multi-file refactoring module."""

from pathlib import Path

import pytest


@pytest.fixture
def workspace(tmp_path: Path) -> Path:
    """Create a test workspace with Python files."""
    # Create a simple module structure
    (tmp_path / "main.py").write_text("""
from utils import helper_func

def main():
    result = helper_func()
    return result
""")

    (tmp_path / "utils.py").write_text('''
def helper_func():
    """A helper function."""
    return 42

def other_func():
    return helper_func() + 1
''')

    (tmp_path / "tests" / "__init__.py").parent.mkdir()
    (tmp_path / "tests" / "__init__.py").write_text("")
    (tmp_path / "tests" / "test_main.py").write_text("""
from main import main
from utils import helper_func

def test_main():
    assert main() == 42

def test_helper():
    assert helper_func() == 42
""")

    return tmp_path


class TestRefactoringScope:
    """Tests for RefactoringScope."""

    def test_scope_values(self):
        from moss_intelligence.refactoring import RefactoringScope

        assert RefactoringScope.FILE.value == "file"
        assert RefactoringScope.DIRECTORY.value == "directory"
        assert RefactoringScope.WORKSPACE.value == "workspace"


class TestFileChange:
    """Tests for FileChange."""

    def test_create_file_change(self, tmp_path: Path):
        from moss_intelligence.refactoring import FileChange

        path = tmp_path / "test.py"
        change = FileChange(
            path=path,
            original_content="old content",
            new_content="new content",
        )

        assert change.path == path
        assert change.has_changes is True

    def test_no_changes(self, tmp_path: Path):
        from moss_intelligence.refactoring import FileChange

        path = tmp_path / "test.py"
        change = FileChange(
            path=path,
            original_content="same",
            new_content="same",
        )

        assert change.has_changes is False

    def test_to_diff(self, tmp_path: Path):
        from moss_intelligence.refactoring import FileChange

        path = tmp_path / "test.py"
        change = FileChange(
            path=path,
            original_content="line1\nline2\n",
            new_content="line1\nmodified\n",
        )

        diff = change.to_diff()
        assert "-line2" in diff
        assert "+modified" in diff


class TestRefactoringResult:
    """Tests for RefactoringResult."""

    def test_success_result(self):
        from moss_intelligence.refactoring import RefactoringResult

        result = RefactoringResult(success=True)

        assert result.success is True
        assert result.total_changes == 0
        assert result.errors == []

    def test_with_changes(self, tmp_path: Path):
        from moss_intelligence.refactoring import FileChange, RefactoringResult

        change = FileChange(
            path=tmp_path / "test.py",
            original_content="old",
            new_content="new",
        )
        result = RefactoringResult(success=True, changes=[change])

        assert result.total_changes == 1


class TestRenameRefactoring:
    """Tests for RenameRefactoring."""

    def test_rename_function(self):
        from moss_intelligence.refactoring import RenameRefactoring

        refactoring = RenameRefactoring(old_name="old_func", new_name="new_func")

        content = """
def old_func():
    return 1

result = old_func()
"""
        new_content = refactoring.apply_to_file(Path("test.py"), content)

        assert new_content is not None
        assert "new_func" in new_content
        assert "old_func" not in new_content

    def test_rename_class(self):
        from moss_intelligence.refactoring import RenameRefactoring

        refactoring = RenameRefactoring(
            old_name="OldClass", new_name="NewClass", symbol_type="class"
        )

        content = """
class OldClass:
    pass

instance = OldClass()
"""
        new_content = refactoring.apply_to_file(Path("test.py"), content)

        assert new_content is not None
        assert "NewClass" in new_content
        assert "OldClass" not in new_content

    def test_rename_preserves_other_code(self):
        from moss_intelligence.refactoring import RenameRefactoring

        refactoring = RenameRefactoring(old_name="target", new_name="renamed")

        content = """
def target():
    return 1

def other():
    return 2
"""
        new_content = refactoring.apply_to_file(Path("test.py"), content)

        assert new_content is not None
        assert "renamed" in new_content
        assert "other" in new_content


class TestMoveRefactoring:
    """Tests for MoveRefactoring."""

    def test_update_imports(self, tmp_path: Path):
        from moss_intelligence.refactoring import MoveRefactoring

        refactoring = MoveRefactoring(
            source_file=Path("old_module.py"),
            target_file=Path("new_module.py"),
            symbol_name="my_func",
        )

        content = "from old_module import my_func\n"
        new_content = refactoring.apply_to_file(tmp_path / "test.py", content)

        assert new_content is not None
        assert "new_module" in new_content
        assert "old_module" not in new_content


class TestExtractRefactoring:
    """Tests for ExtractRefactoring."""

    def test_extract_simple(self):
        from moss_intelligence.refactoring import ExtractRefactoring

        refactoring = ExtractRefactoring(
            start_line=3,
            end_line=4,
            new_name="extracted",
        )

        content = """def main():
    x = 1
    y = 2
    z = x + y
    return z
"""
        new_content = refactoring.apply_to_file(Path("test.py"), content)

        assert new_content is not None
        assert "def extracted" in new_content


class TestRefactorer:
    """Tests for Refactorer."""

    def test_create_refactorer(self, workspace: Path):
        from moss_intelligence.refactoring import Refactorer

        refactorer = Refactorer(workspace)

        assert refactorer.workspace == workspace

    @pytest.mark.asyncio
    async def test_apply_rename(self, workspace: Path):
        from moss_intelligence.refactoring import Refactorer, RefactoringScope, RenameRefactoring

        refactorer = Refactorer(workspace)
        refactoring = RenameRefactoring(
            old_name="helper_func",
            new_name="renamed_helper",
            scope=RefactoringScope.WORKSPACE,
        )

        result = await refactorer.apply(refactoring, dry_run=True)

        assert result.success is True
        # Should affect multiple files
        assert len(result.changes) > 0

    def test_preview(self, workspace: Path):
        from moss_intelligence.refactoring import Refactorer, RefactoringScope, RenameRefactoring

        refactorer = Refactorer(workspace)
        refactoring = RenameRefactoring(
            old_name="helper_func",
            new_name="renamed_helper",
            scope=RefactoringScope.WORKSPACE,
        )

        refactorer.preview(refactoring)

        # Dry run - files should not be modified
        utils_content = (workspace / "utils.py").read_text()
        assert "helper_func" in utils_content
        assert "renamed_helper" not in utils_content

    def test_generate_diff(self, workspace: Path):
        from moss_intelligence.refactoring import Refactorer, RefactoringScope, RenameRefactoring

        refactorer = Refactorer(workspace)
        refactoring = RenameRefactoring(
            old_name="helper_func",
            new_name="renamed_helper",
            scope=RefactoringScope.WORKSPACE,
        )

        result = refactorer.preview(refactoring)
        diff = refactorer.generate_diff(result)

        # The diff contains the changes
        assert "renamed_helper" in diff
        assert len(diff) > 0


class TestRenameSymbol:
    """Tests for rename_symbol convenience function."""

    @pytest.mark.asyncio
    async def test_rename_in_workspace(self, workspace: Path):
        from moss_intelligence.refactoring import rename_symbol

        result = await rename_symbol(
            workspace=workspace,
            old_name="helper_func",
            new_name="new_helper",
            dry_run=True,
        )

        assert result.success is True


class TestMoveSymbol:
    """Tests for move_symbol convenience function."""

    @pytest.mark.asyncio
    async def test_move_updates_imports(self, workspace: Path):
        from moss_intelligence.refactoring import move_symbol

        result = await move_symbol(
            workspace=workspace,
            symbol_name="helper_func",
            source_file=workspace / "utils.py",
            target_file=workspace / "helpers.py",
            dry_run=True,
        )

        assert result.success is True


class TestAnalyzeVariables:
    """Tests for variable analysis."""

    def test_analyze_used_variables(self):
        from moss_intelligence.refactoring import _analyze_used_variables

        code = "result = x + y"
        used = _analyze_used_variables(code)

        assert "x" in used
        assert "y" in used
        assert "result" not in used  # assigned, not used

    def test_analyze_assigned_variables(self):
        from moss_intelligence.refactoring import _analyze_assigned_variables

        code = "x = 1\ny = 2"
        assigned = _analyze_assigned_variables(code)

        assert "x" in assigned
        assert "y" in assigned


class TestPathToModule:
    """Tests for path to module conversion."""

    def test_simple_path(self):
        from moss_intelligence.refactoring import _path_to_module

        path = Path("utils.py")
        module = _path_to_module(path)

        assert module == "utils"

    def test_nested_path(self):
        from moss_intelligence.refactoring import _path_to_module

        path = Path("package/subpackage/module.py")
        module = _path_to_module(path)

        assert module == "package.subpackage.module"


# =============================================================================
# Inline Refactoring Tests
# =============================================================================


class TestInlineRefactoring:
    """Tests for InlineRefactoring."""

    def test_inline_simple_function(self):
        from moss_intelligence.refactoring import InlineRefactoring

        refactoring = InlineRefactoring(name="double")

        content = """
def double(x):
    return x * 2

result = double(5)
"""
        new_content = refactoring.apply_to_file(Path("test.py"), content)

        assert new_content is not None
        assert "double" not in new_content  # Function removed
        assert "5 * 2" in new_content  # Call inlined

    def test_inline_function_multiple_args(self):
        from moss_intelligence.refactoring import InlineRefactoring

        refactoring = InlineRefactoring(name="add")

        content = """
def add(a, b):
    return a + b

result = add(3, 4)
"""
        new_content = refactoring.apply_to_file(Path("test.py"), content)

        assert new_content is not None
        assert "3 + 4" in new_content

    def test_inline_variable(self):
        from moss_intelligence.refactoring import InlineRefactoring

        refactoring = InlineRefactoring(name="config")

        content = """
config = {"debug": True}
print(config)
"""
        new_content = refactoring.apply_to_file(Path("test.py"), content)

        assert new_content is not None
        assert "config = " not in new_content  # Assignment removed
        # AST unparsing uses single quotes for dict keys
        assert "'debug'" in new_content

    def test_inline_preserve_definition(self):
        from moss_intelligence.refactoring import InlineRefactoring

        refactoring = InlineRefactoring(name="value", remove_definition=False)

        content = """
value = 42
result = value + 1
"""
        new_content = refactoring.apply_to_file(Path("test.py"), content)

        assert new_content is not None
        # Definition preserved
        assert "value = 42" in new_content
        # Usage inlined
        assert "42 + 1" in new_content

    def test_inline_no_definition_found(self):
        from moss_intelligence.refactoring import InlineRefactoring

        refactoring = InlineRefactoring(name="nonexistent")

        content = "x = 1"
        new_content = refactoring.apply_to_file(Path("test.py"), content)

        assert new_content is None


class TestInlineSymbol:
    """Tests for inline_symbol convenience function."""

    @pytest.mark.asyncio
    async def test_inline_function(self, tmp_path: Path):
        from moss_intelligence.refactoring import inline_symbol

        code_file = tmp_path / "code.py"
        code_file.write_text("""
def helper(x):
    return x * 2

result = helper(10)
""")

        result = await inline_symbol(code_file, "helper", dry_run=True)

        assert result.success is True
        assert len(result.changes) > 0


# =============================================================================
# Codemod Tests
# =============================================================================


class TestCodemodPattern:
    """Tests for CodemodPattern."""

    def test_regex_match(self):
        from moss_intelligence.refactoring import CodemodPattern

        pattern = CodemodPattern(r"print\((?P<arg>.*?)\)", is_regex=True)
        matches = pattern.match("print(x)\nprint(y)")

        assert len(matches) == 2
        assert matches[0]["arg"] == "x"
        assert matches[1]["arg"] == "y"

    def test_placeholder_match(self):
        from moss_intelligence.refactoring import CodemodPattern

        pattern = CodemodPattern("assertEqual($x, $y)")
        matches = pattern.match("assertEqual(a, b)\nassertEqual(1, 2)")

        assert len(matches) == 2

    def test_pattern_to_regex(self):
        from moss_intelligence.refactoring import CodemodPattern

        pattern = CodemodPattern("func($a, $b)")
        regex = pattern._pattern_to_regex()

        assert "(?P<a>" in regex
        assert "(?P<b>" in regex


class TestCodemodRule:
    """Tests for CodemodRule."""

    def test_apply_regex_rule(self):
        from moss_intelligence.refactoring import CodemodPattern, CodemodRule

        rule = CodemodRule(
            name="test",
            description="Test rule",
            pattern=CodemodPattern(r"print\((.*?)\)", is_regex=True),
            replacement=r"logger.info(\1)",
        )

        content = "print(x)\nprint(y)"
        new_content, count = rule.apply(content)

        assert count == 2
        assert "logger.info(x)" in new_content
        assert "logger.info(y)" in new_content

    def test_apply_placeholder_rule(self):
        from moss_intelligence.refactoring import CodemodPattern, CodemodRule

        rule = CodemodRule(
            name="test",
            description="Test rule",
            pattern=CodemodPattern("assertEqual($x, $y)"),
            replacement="assert $x == $y",
        )

        content = "assertEqual(a, b)"
        new_content, count = rule.apply(content)

        assert count == 1
        assert "assert a == b" in new_content


class TestCodemod:
    """Tests for Codemod."""

    def test_create_codemod(self):
        from moss_intelligence.refactoring import Codemod

        codemod = Codemod(name="test", description="Test codemod")

        assert codemod.name == "test"
        assert len(codemod.rules) == 0

    def test_add_rule_chaining(self):
        from moss_intelligence.refactoring import Codemod

        codemod = (
            Codemod(name="test").add_rule("rule1", "old1", "new1").add_rule("rule2", "old2", "new2")
        )

        assert len(codemod.rules) == 2
        assert codemod.rules[0].name == "rule1"
        assert codemod.rules[1].name == "rule2"


class TestCodemodRunner:
    """Tests for CodemodRunner."""

    @pytest.mark.asyncio
    async def test_run_codemod(self, tmp_path: Path):
        from moss_intelligence.refactoring import Codemod, CodemodRunner

        # Create test file
        (tmp_path / "test.py").write_text("print(x)\nprint(y)")

        codemod = Codemod(name="test").add_rule(
            name="replace_print",
            pattern=r"print\((.*?)\)",
            replacement=r"logger.info(\1)",
            is_regex=True,
        )

        runner = CodemodRunner(tmp_path)
        result = await runner.run(codemod, dry_run=True)

        assert result.success is True
        assert len(result.changes) == 1

    @pytest.mark.asyncio
    async def test_run_codemod_multiple_files(self, tmp_path: Path):
        from moss_intelligence.refactoring import Codemod, CodemodRunner

        # Create test files
        (tmp_path / "a.py").write_text("print(1)")
        (tmp_path / "b.py").write_text("print(2)")

        codemod = Codemod(name="test").add_rule(
            name="replace_print",
            pattern=r"print",
            replacement="log",
            is_regex=True,
        )

        runner = CodemodRunner(tmp_path)
        result = await runner.run(codemod, dry_run=True)

        assert result.success is True
        assert len(result.changes) == 2

    @pytest.mark.asyncio
    async def test_exclude_patterns(self, tmp_path: Path):
        from moss_intelligence.refactoring import Codemod, CodemodRunner

        # Create test files
        (tmp_path / "src.py").write_text("print(1)")
        venv = tmp_path / "venv"
        venv.mkdir()
        (venv / "lib.py").write_text("print(2)")

        codemod = Codemod(name="test").add_rule(
            name="replace_print",
            pattern=r"print",
            replacement="log",
            is_regex=True,
        )

        runner = CodemodRunner(tmp_path)
        result = await runner.run(codemod, dry_run=True)

        # Should only change src.py, not venv/lib.py
        assert len(result.changes) == 1
        assert result.changes[0].path == tmp_path / "src.py"


class TestBuiltinCodemods:
    """Tests for built-in codemod factories."""

    def test_create_deprecation_codemod(self):
        from moss_intelligence.refactoring import create_deprecation_codemod

        codemod = create_deprecation_codemod("old_module", "new_module", "OldClass", "NewClass")

        assert codemod.name == "deprecate_OldClass"
        assert len(codemod.rules) == 2  # import + usage

    def test_deprecation_codemod_same_name(self):
        from moss_intelligence.refactoring import create_deprecation_codemod

        codemod = create_deprecation_codemod("old_module", "new_module", "MyClass")

        # Only import rule when name doesn't change
        assert len(codemod.rules) == 1

    def test_create_api_migration_codemod(self):
        from moss_intelligence.refactoring import create_api_migration_codemod

        codemod = create_api_migration_codemod("assertEqual($x, $y)", "assert $x == $y")

        assert codemod.name == "api_migration"
        assert len(codemod.rules) == 1

    @pytest.mark.asyncio
    async def test_api_migration_applies(self, tmp_path: Path):
        from moss_intelligence.refactoring import CodemodRunner, create_api_migration_codemod

        (tmp_path / "test.py").write_text("assertEqual(a, b)\nassertEqual(1, 2)")

        codemod = create_api_migration_codemod("assertEqual($x, $y)", "assert $x == $y")

        runner = CodemodRunner(tmp_path)
        result = await runner.run(codemod, dry_run=True)

        assert result.success is True
        assert len(result.changes) == 1
        assert "assert a == b" in result.changes[0].new_content


# =============================================================================
# Integration Tests
# =============================================================================


class TestRefactoringIntegration:
    """Integration tests for refactoring operations."""

    @pytest.mark.asyncio
    async def test_rename_and_verify(self, workspace: Path):
        """Test rename refactoring updates all references."""
        from moss_intelligence.refactoring import Refactorer, RefactoringScope, RenameRefactoring

        refactorer = Refactorer(workspace)
        refactoring = RenameRefactoring(
            old_name="helper_func",
            new_name="utility_func",
            scope=RefactoringScope.WORKSPACE,
        )

        # Apply the refactoring
        result = await refactorer.apply(refactoring, dry_run=False)

        assert result.success is True

        # Verify changes
        utils_content = (workspace / "utils.py").read_text()
        assert "utility_func" in utils_content
        assert "helper_func" not in utils_content

        main_content = (workspace / "main.py").read_text()
        assert "utility_func" in main_content

    @pytest.mark.asyncio
    async def test_extract_then_inline(self, tmp_path: Path):
        """Test extracting code then inlining it back."""
        from moss_intelligence.refactoring import extract_function, inline_symbol

        code_file = tmp_path / "code.py"
        original = """def main():
    x = 1
    y = 2
    z = x + y
    return z
"""
        code_file.write_text(original)

        # Extract lines 3-4
        result = await extract_function(code_file, 3, 4, "compute", dry_run=False)
        assert result.success

        # Now inline it back
        result = await inline_symbol(code_file, "compute", dry_run=True)
        # Note: inline may not perfectly restore original due to AST unparsing
        assert result.success
