"""Tests for the linter plugin architecture."""

import shutil
from pathlib import Path

import pytest

from moss.plugins.linters import (
    LinterIssue,
    LinterMetadata,
    LinterPlugin,
    LinterRegistry,
    LinterResult,
    MypyPlugin,
    RuffPlugin,
    SARIFAdapter,
    Severity,
    get_linter_registry,
    reset_linter_registry,
)

# Check if mypy is available for conditional skipping
HAS_MYPY = shutil.which("mypy") is not None

# =============================================================================
# Severity Tests
# =============================================================================


class TestSeverity:
    @pytest.mark.parametrize(
        "sarif_level,expected",
        [
            ("error", Severity.ERROR),
            ("warning", Severity.WARNING),
            ("note", Severity.INFO),
            ("none", Severity.HINT),
            ("ERROR", Severity.ERROR),
            ("Warning", Severity.WARNING),
            ("unknown", Severity.WARNING),  # Default
        ],
    )
    def test_from_sarif(self, sarif_level: str, expected: Severity):
        assert Severity.from_sarif(sarif_level) == expected

    @pytest.mark.parametrize(
        "string,expected",
        [
            ("error", Severity.ERROR),
            ("e", Severity.ERROR),
            ("fatal", Severity.ERROR),
            ("critical", Severity.ERROR),
            ("warning", Severity.WARNING),
            ("w", Severity.WARNING),
            ("warn", Severity.WARNING),
            ("info", Severity.INFO),
            ("i", Severity.INFO),
            ("information", Severity.INFO),
            ("notice", Severity.INFO),
            ("hint", Severity.HINT),
            ("unknown", Severity.HINT),  # Default
        ],
    )
    def test_from_string(self, string: str, expected: Severity):
        assert Severity.from_string(string) == expected


# =============================================================================
# LinterIssue Tests
# =============================================================================


class TestLinterIssue:
    def test_create_minimal(self):
        issue = LinterIssue(message="Test", severity=Severity.ERROR)
        assert issue.message == "Test"
        assert issue.severity == Severity.ERROR
        assert issue.file is None
        assert issue.line is None
        assert issue.column is None
        assert issue.rule_id is None

    def test_create_full(self):
        issue = LinterIssue(
            message="Test message",
            severity=Severity.WARNING,
            file=Path("test.py"),
            line=10,
            column=5,
            end_line=10,
            end_column=20,
            rule_id="E501",
            source="ruff",
            fix="suggested fix",
        )
        assert issue.message == "Test message"
        assert issue.severity == Severity.WARNING
        assert issue.file == Path("test.py")
        assert issue.line == 10
        assert issue.column == 5
        assert issue.end_line == 10
        assert issue.end_column == 20
        assert issue.rule_id == "E501"
        assert issue.source == "ruff"
        assert issue.fix == "suggested fix"

    def test_str_full(self):
        issue = LinterIssue(
            message="Line too long",
            severity=Severity.ERROR,
            file=Path("test.py"),
            line=10,
            column=5,
            rule_id="E501",
        )
        s = str(issue)
        assert "test.py:10:5" in s
        assert "[E501]" in s
        assert "Line too long" in s

    def test_str_minimal(self):
        issue = LinterIssue(message="Test", severity=Severity.ERROR)
        assert str(issue) == "Test"

    def test_frozen(self):
        issue = LinterIssue(message="Test", severity=Severity.ERROR)
        with pytest.raises(AttributeError):
            issue.message = "Changed"  # type: ignore


# =============================================================================
# LinterResult Tests
# =============================================================================


class TestLinterResult:
    def test_create_success(self):
        result = LinterResult(success=True)
        assert result.success is True
        assert result.issues == []
        assert result.tool_name == ""
        assert result.tool_version == ""
        assert result.execution_time_ms == 0.0
        assert result.metadata == {}

    def test_create_with_issues(self):
        issues = [
            LinterIssue(message="Error", severity=Severity.ERROR),
            LinterIssue(message="Warning", severity=Severity.WARNING),
        ]
        result = LinterResult(success=False, issues=issues, tool_name="test")
        assert result.success is False
        assert len(result.issues) == 2
        assert result.tool_name == "test"

    def test_errors_property(self):
        issues = [
            LinterIssue(message="Error 1", severity=Severity.ERROR),
            LinterIssue(message="Warning", severity=Severity.WARNING),
            LinterIssue(message="Error 2", severity=Severity.ERROR),
        ]
        result = LinterResult(success=False, issues=issues)
        assert len(result.errors) == 2
        assert result.error_count == 2

    def test_warnings_property(self):
        issues = [
            LinterIssue(message="Error", severity=Severity.ERROR),
            LinterIssue(message="Warning 1", severity=Severity.WARNING),
            LinterIssue(message="Warning 2", severity=Severity.WARNING),
        ]
        result = LinterResult(success=False, issues=issues)
        assert len(result.warnings) == 2
        assert result.warning_count == 2


# =============================================================================
# LinterMetadata Tests
# =============================================================================


class TestLinterMetadata:
    def test_create_with_defaults(self):
        meta = LinterMetadata(name="test", tool_name="Test Tool")
        assert meta.name == "test"
        assert meta.tool_name == "Test Tool"
        assert meta.languages == frozenset()
        assert meta.category == "linter"
        assert meta.priority == 0
        assert meta.version == "0.1.0"
        assert meta.description == ""
        assert meta.required_tool is None
        assert meta.supports_fix is False
        assert meta.supports_sarif is False

    def test_create_full(self):
        meta = LinterMetadata(
            name="ruff",
            tool_name="Ruff",
            languages=frozenset(["python"]),
            category="linter",
            priority=100,
            version="1.0.0",
            description="Fast linter",
            required_tool="ruff",
            supports_fix=True,
            supports_sarif=True,
        )
        assert meta.name == "ruff"
        assert "python" in meta.languages
        assert meta.supports_fix is True
        assert meta.supports_sarif is True

    def test_frozen(self):
        meta = LinterMetadata(name="test", tool_name="Test")
        with pytest.raises(AttributeError):
            meta.name = "changed"  # type: ignore


# =============================================================================
# RuffPlugin Tests
# =============================================================================


class TestRuffPlugin:
    @pytest.fixture
    def plugin(self):
        return RuffPlugin()

    def test_metadata(self, plugin: RuffPlugin):
        meta = plugin.metadata
        assert meta.name == "ruff"
        assert meta.tool_name == "Ruff"
        assert "python" in meta.languages
        assert meta.category == "linter"
        assert meta.priority == 100
        assert meta.required_tool == "ruff"
        assert meta.supports_fix is True
        assert meta.supports_sarif is True

    def test_is_available(self, plugin: RuffPlugin):
        # Ruff should be available in our dev environment
        assert plugin.is_available() is True

    def test_get_version(self, plugin: RuffPlugin):
        version = plugin.get_version()
        assert version is not None
        # Version should look like "0.x.x"
        parts = version.split(".")
        assert len(parts) >= 2

    @pytest.mark.asyncio
    async def test_run_clean_file(self, plugin: RuffPlugin, tmp_path: Path):
        """Test running ruff on a clean file."""
        clean_file = tmp_path / "clean.py"
        clean_file.write_text("def foo():\n    pass\n")

        result = await plugin.run(clean_file)
        assert result.success is True
        assert result.tool_name == "ruff"
        assert result.execution_time_ms > 0

    @pytest.mark.asyncio
    async def test_run_file_with_errors(self, plugin: RuffPlugin, tmp_path: Path):
        """Test running ruff on a file with errors."""
        bad_file = tmp_path / "bad.py"
        bad_file.write_text("import os\nprint(x)\n")

        result = await plugin.run(bad_file)
        # Has F401 (unused import) and F821 (undefined name)
        assert len(result.issues) > 0
        assert result.tool_name == "ruff"

    def test_implements_protocol(self, plugin: RuffPlugin):
        assert isinstance(plugin, LinterPlugin)


# =============================================================================
# MypyPlugin Tests
# =============================================================================


@pytest.mark.skipif(not HAS_MYPY, reason="mypy not installed")
class TestMypyPlugin:
    @pytest.fixture
    def plugin(self):
        return MypyPlugin()

    def test_metadata(self, plugin: MypyPlugin):
        meta = plugin.metadata
        assert meta.name == "mypy"
        assert meta.tool_name == "Mypy"
        assert "python" in meta.languages
        assert meta.category == "type-checker"
        assert meta.required_tool == "mypy"
        assert meta.supports_fix is False

    def test_is_available(self, plugin: MypyPlugin):
        # Mypy should be available in our dev environment
        assert plugin.is_available() is True

    def test_get_version(self, plugin: MypyPlugin):
        version = plugin.get_version()
        assert version is not None
        parts = version.split(".")
        assert len(parts) >= 2

    @pytest.mark.asyncio
    async def test_run_clean_file(self, plugin: MypyPlugin, tmp_path: Path):
        """Test running mypy on a clean file."""
        clean_file = tmp_path / "clean.py"
        clean_file.write_text("def foo(x: int) -> int:\n    return x + 1\n")

        result = await plugin.run(clean_file)
        assert result.success is True
        assert result.tool_name == "mypy"
        assert result.execution_time_ms > 0

    @pytest.mark.asyncio
    async def test_run_file_with_errors(self, plugin: MypyPlugin, tmp_path: Path):
        """Test running mypy on a file with type errors."""
        bad_file = tmp_path / "bad.py"
        bad_file.write_text('def foo(x: int) -> int:\n    return "not an int"\n')

        result = await plugin.run(bad_file)
        assert result.success is False
        assert len(result.issues) > 0
        # Should find incompatible return type
        assert any("incompatible" in i.message.lower() for i in result.issues)

    def test_implements_protocol(self, plugin: MypyPlugin):
        assert isinstance(plugin, LinterPlugin)


# =============================================================================
# SARIFAdapter Tests
# =============================================================================


class TestSARIFAdapter:
    @pytest.fixture
    def adapter(self):
        return SARIFAdapter("test-tool", ["echo", "{}"])

    def test_metadata(self, adapter: SARIFAdapter):
        meta = adapter.metadata
        assert meta.name == "sarif-test-tool"
        assert meta.tool_name == "test-tool"
        assert meta.supports_sarif is True

    def test_parse_sarif_empty(self, adapter: SARIFAdapter):
        issues = adapter._parse_sarif("{}")
        assert issues == []

    def test_parse_sarif_with_results(self, adapter: SARIFAdapter):
        sarif = {
            "runs": [
                {
                    "tool": {
                        "driver": {
                            "name": "TestTool",
                            "rules": [
                                {
                                    "id": "TEST001",
                                    "shortDescription": {"text": "Test rule"},
                                }
                            ],
                        }
                    },
                    "results": [
                        {
                            "ruleId": "TEST001",
                            "message": {"text": "Test issue"},
                            "level": "error",
                            "locations": [
                                {
                                    "physicalLocation": {
                                        "artifactLocation": {"uri": "test.py"},
                                        "region": {
                                            "startLine": 10,
                                            "startColumn": 5,
                                        },
                                    }
                                }
                            ],
                        }
                    ],
                }
            ]
        }

        import json

        issues = adapter._parse_sarif(json.dumps(sarif))

        assert len(issues) == 1
        issue = issues[0]
        assert issue.message == "Test issue"
        assert issue.severity == Severity.ERROR
        assert issue.rule_id == "TEST001"
        assert issue.line == 10
        assert issue.column == 5
        assert issue.source == "TestTool"

    def test_parse_sarif_invalid_json(self, adapter: SARIFAdapter):
        issues = adapter._parse_sarif("not valid json")
        assert issues == []

    def test_implements_protocol(self, adapter: SARIFAdapter):
        assert isinstance(adapter, LinterPlugin)


# =============================================================================
# LinterRegistry Tests
# =============================================================================


class TestLinterRegistry:
    @pytest.fixture
    def registry(self):
        return LinterRegistry()

    @pytest.fixture
    def mock_plugin(self):
        """Create a mock linter plugin."""

        class MockLinterPlugin:
            @property
            def metadata(self) -> LinterMetadata:
                return LinterMetadata(
                    name="mock",
                    tool_name="Mock",
                    languages=frozenset(["python"]),
                    category="linter",
                    priority=50,
                )

            def is_available(self) -> bool:
                return True

            def get_version(self) -> str | None:
                return "1.0.0"

            async def run(
                self,
                path: Path,
                *,
                fix: bool = False,
                config: dict | None = None,
            ) -> LinterResult:
                return LinterResult(success=True, tool_name="mock")

        return MockLinterPlugin()

    def test_register_plugin(self, registry: LinterRegistry, mock_plugin):
        registry.register(mock_plugin)
        assert registry.get("mock") is mock_plugin
        assert len(registry.get_all()) == 1

    def test_register_duplicate_raises(self, registry: LinterRegistry, mock_plugin):
        registry.register(mock_plugin)
        with pytest.raises(ValueError, match="already registered"):
            registry.register(mock_plugin)

    def test_get_for_language(self, registry: LinterRegistry):
        registry.register_builtins()

        python_linters = registry.get_for_language("python")
        assert len(python_linters) >= 2  # ruff and mypy
        # Should be sorted by priority
        assert python_linters[0].metadata.priority >= python_linters[1].metadata.priority

    def test_get_for_category(self, registry: LinterRegistry):
        registry.register_builtins()

        linters = registry.get_for_category("linter")
        type_checkers = registry.get_for_category("type-checker")

        assert any(p.metadata.name == "ruff" for p in linters)
        assert any(p.metadata.name == "mypy" for p in type_checkers)

    def test_get_available(self, registry: LinterRegistry):
        registry.register_builtins()

        available = registry.get_available()
        # At least ruff should be available (mypy is optional)
        assert len(available) >= 1
        assert any(p.metadata.name == "ruff" for p in available)

    def test_register_builtins(self, registry: LinterRegistry):
        registry.register_builtins()

        assert registry.get("ruff") is not None
        assert registry.get("mypy") is not None


# =============================================================================
# Global Registry Tests
# =============================================================================


class TestGlobalLinterRegistry:
    def setup_method(self):
        reset_linter_registry()

    def teardown_method(self):
        reset_linter_registry()

    def test_get_registry_creates_singleton(self):
        registry1 = get_linter_registry()
        registry2 = get_linter_registry()
        assert registry1 is registry2

    def test_get_registry_has_builtins(self):
        registry = get_linter_registry()
        assert registry.get("ruff") is not None
        assert registry.get("mypy") is not None

    def test_reset_clears_singleton(self):
        registry1 = get_linter_registry()
        reset_linter_registry()
        registry2 = get_linter_registry()
        assert registry1 is not registry2


# =============================================================================
# LinterPlugin Protocol Tests
# =============================================================================


class TestLinterPluginProtocol:
    def test_ruff_implements_protocol(self):
        assert isinstance(RuffPlugin(), LinterPlugin)

    def test_mypy_implements_protocol(self):
        assert isinstance(MypyPlugin(), LinterPlugin)

    def test_sarif_adapter_implements_protocol(self):
        adapter = SARIFAdapter("test", ["echo"])
        assert isinstance(adapter, LinterPlugin)


# =============================================================================
# LinterValidatorAdapter Tests
# =============================================================================


class TestLinterValidatorAdapter:
    @pytest.fixture
    def ruff_adapter(self):
        from moss.validators import LinterValidatorAdapter

        return LinterValidatorAdapter(RuffPlugin())

    def test_create_adapter(self, ruff_adapter):
        from moss.validators import Validator

        assert isinstance(ruff_adapter, Validator)
        assert ruff_adapter.name == "ruff"

    def test_create_with_invalid_plugin(self):
        from moss.validators import LinterValidatorAdapter

        with pytest.raises(TypeError, match="Expected LinterPlugin"):
            LinterValidatorAdapter("not a plugin")

    @pytest.mark.asyncio
    async def test_validate_clean_file(self, ruff_adapter, tmp_path: Path):
        clean_file = tmp_path / "clean.py"
        clean_file.write_text("def foo():\n    pass\n")

        result = await ruff_adapter.validate(clean_file)
        assert result.success is True
        assert len(result.issues) == 0
        assert "tool_name" in result.metadata
        assert result.metadata["tool_name"] == "ruff"

    @pytest.mark.asyncio
    async def test_validate_file_with_errors(self, ruff_adapter, tmp_path: Path):
        from moss.validators import ValidationIssue, ValidationSeverity

        bad_file = tmp_path / "bad.py"
        bad_file.write_text("import os\nprint(x)\n")

        result = await ruff_adapter.validate(bad_file)
        assert len(result.issues) > 0
        # Issues should be ValidationIssue instances
        assert all(isinstance(i, ValidationIssue) for i in result.issues)
        # Should have errors
        assert any(i.severity == ValidationSeverity.ERROR for i in result.issues)

    @pytest.mark.asyncio
    async def test_adapter_in_chain(self, tmp_path: Path):
        """Test using adapter in a ValidatorChain."""
        from moss.validators import (
            LinterValidatorAdapter,
            SyntaxValidator,
            ValidatorChain,
        )

        chain = ValidatorChain()
        chain.add(SyntaxValidator())
        chain.add(LinterValidatorAdapter(RuffPlugin()))

        clean_file = tmp_path / "clean.py"
        clean_file.write_text("def foo():\n    pass\n")

        result = await chain.validate(clean_file)
        assert result.success is True
        assert "syntax" in result.metadata["validators"]
        assert "ruff" in result.metadata["validators"]
