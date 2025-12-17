"""Tests for TOML configuration module."""

from pathlib import Path

import pytest

from moss.config import MossConfig
from moss.toml_config import (
    config_to_toml,
    find_config_file,
    load_toml_config,
    merge_configs,
)


class TestFindConfigFile:
    """Tests for find_config_file."""

    def test_finds_moss_toml(self, tmp_path: Path):
        config_file = tmp_path / "moss.toml"
        config_file.write_text('[project]\nname = "test"')

        result = find_config_file(tmp_path)

        assert result == config_file

    def test_finds_mossrc_toml(self, tmp_path: Path):
        config_file = tmp_path / ".mossrc.toml"
        config_file.write_text('[project]\nname = "test"')

        result = find_config_file(tmp_path)

        assert result == config_file

    def test_prefers_moss_toml_over_mossrc(self, tmp_path: Path):
        moss_toml = tmp_path / "moss.toml"
        mossrc_toml = tmp_path / ".mossrc.toml"
        moss_toml.write_text('[project]\nname = "moss"')
        mossrc_toml.write_text('[project]\nname = "mossrc"')

        result = find_config_file(tmp_path)

        assert result == moss_toml

    def test_finds_pyproject_toml_with_moss_section(self, tmp_path: Path):
        pyproject = tmp_path / "pyproject.toml"
        pyproject.write_text('[tool.moss]\nname = "test"')

        result = find_config_file(tmp_path)

        assert result == pyproject

    def test_ignores_pyproject_without_moss_section(self, tmp_path: Path):
        pyproject = tmp_path / "pyproject.toml"
        pyproject.write_text("[tool.ruff]\nline-length = 100")

        result = find_config_file(tmp_path)

        assert result is None

    def test_searches_parent_directories(self, tmp_path: Path):
        config_file = tmp_path / "moss.toml"
        config_file.write_text('[project]\nname = "test"')
        subdir = tmp_path / "src" / "module"
        subdir.mkdir(parents=True)

        result = find_config_file(subdir)

        assert result == config_file

    def test_returns_none_when_not_found(self, tmp_path: Path):
        result = find_config_file(tmp_path)
        assert result is None


class TestLoadTomlConfig:
    """Tests for load_toml_config."""

    def test_loads_basic_config(self, tmp_path: Path):
        config_file = tmp_path / "moss.toml"
        config_file.write_text("""
[project]
name = "my-project"

[validators]
syntax = true
ruff = false
pytest = true

[policies]
velocity = true
rate_limit = false

[loop]
max_iterations = 5
timeout_seconds = 120
""")

        config = load_toml_config(config_file)

        assert config.project_name == "my-project"
        assert config.validators.syntax is True
        assert config.validators.ruff is False
        assert config.validators.pytest is True
        assert config.policies.velocity is True
        assert config.policies.rate_limit is False
        assert config.loop.max_iterations == 5
        assert config.loop.timeout_seconds == 120

    def test_loads_from_pyproject_toml(self, tmp_path: Path):
        pyproject = tmp_path / "pyproject.toml"
        pyproject.write_text("""
[tool.moss]
[tool.moss.project]
name = "pyproject-project"

[tool.moss.validators]
pytest = true
""")

        config = load_toml_config(pyproject)

        assert config.project_name == "pyproject-project"
        assert config.validators.pytest is True

    def test_loads_with_distro(self, tmp_path: Path):
        config_file = tmp_path / "moss.toml"
        config_file.write_text("""
extends = "strict"

[project]
name = "strict-project"
""")

        config = load_toml_config(config_file)

        assert config.project_name == "strict-project"
        # Strict distro enables pytest
        assert config.validators.pytest is True

    def test_loads_blocked_patterns(self, tmp_path: Path):
        config_file = tmp_path / "moss.toml"
        config_file.write_text("""
[policies]
blocked_patterns = [".git", ".venv", "node_modules"]
""")

        config = load_toml_config(config_file)

        assert ".git" in config.policies.blocked_patterns
        assert ".venv" in config.policies.blocked_patterns
        assert "node_modules" in config.policies.blocked_patterns

    def test_loads_static_context(self, tmp_path: Path):
        docs_dir = tmp_path / "docs"
        docs_dir.mkdir()

        config_file = tmp_path / "moss.toml"
        config_file.write_text("""
static_context = ["docs/architecture.md", "README.md"]
""")

        config = load_toml_config(config_file)

        assert len(config.static_context) == 2
        assert config.static_context[0] == tmp_path / "docs/architecture.md"

    def test_raises_on_missing_file(self, tmp_path: Path):
        with pytest.raises(FileNotFoundError):
            load_toml_config(tmp_path / "nonexistent.toml")

    def test_raises_on_invalid_distro(self, tmp_path: Path):
        config_file = tmp_path / "moss.toml"
        config_file.write_text('extends = "nonexistent"')

        with pytest.raises(ValueError, match="Unknown distro"):
            load_toml_config(config_file)

    def test_raises_on_pyproject_without_moss_section(self, tmp_path: Path):
        pyproject = tmp_path / "pyproject.toml"
        pyproject.write_text("[tool.ruff]\nline-length = 100")

        with pytest.raises(ValueError, match=r"No \[tool.moss\] section"):
            load_toml_config(pyproject)

    def test_sets_project_root_to_config_parent(self, tmp_path: Path):
        config_file = tmp_path / "moss.toml"
        config_file.write_text('[project]\nname = "test"')

        config = load_toml_config(config_file)

        assert config.project_root == tmp_path


class TestMergeConfigs:
    """Tests for merge_configs."""

    def test_merge_basic(self):
        base = MossConfig()
        base.project_name = "base"
        base.validators.pytest = False

        override = MossConfig()
        override.project_name = "override"
        override.validators.pytest = True

        merged = merge_configs(base, override)

        assert merged.project_name == "override"
        assert merged.validators.pytest is True

    def test_base_values_preserved_when_override_default(self):
        base = MossConfig()
        base.project_name = "base"

        override = MossConfig()
        # project_name is default "moss-project"

        merged = merge_configs(base, override)

        assert merged.project_name == "base"

    def test_custom_validators_combined(self):
        from moss.validators import SyntaxValidator

        base = MossConfig()
        base.validators.custom = [SyntaxValidator()]

        override = MossConfig()
        override.validators.custom = [SyntaxValidator()]

        merged = merge_configs(base, override)

        assert len(merged.validators.custom) == 2

    def test_static_context_combined(self):
        base = MossConfig()
        base.static_context = [Path("base.md")]

        override = MossConfig()
        override.static_context = [Path("override.md")]

        merged = merge_configs(base, override)

        assert len(merged.static_context) == 2
        assert Path("base.md") in merged.static_context
        assert Path("override.md") in merged.static_context

    def test_metadata_merged(self):
        base = MossConfig()
        base.metadata = {"key1": "value1", "key2": "base"}

        override = MossConfig()
        override.metadata = {"key2": "override", "key3": "value3"}

        merged = merge_configs(base, override)

        assert merged.metadata["key1"] == "value1"
        assert merged.metadata["key2"] == "override"
        assert merged.metadata["key3"] == "value3"


class TestConfigToToml:
    """Tests for config_to_toml."""

    def test_generates_valid_toml(self):
        config = MossConfig()
        config.project_name = "test-project"

        toml = config_to_toml(config)

        assert "[project]" in toml
        assert 'name = "test-project"' in toml
        assert "[validators]" in toml
        assert "[policies]" in toml
        assert "[loop]" in toml

    def test_includes_all_settings(self):
        config = MossConfig()
        config.validators.pytest = True
        config.loop.max_iterations = 20

        toml = config_to_toml(config)

        assert "pytest = true" in toml
        assert "max_iterations = 20" in toml

    def test_formats_lists(self):
        config = MossConfig()
        config.policies.blocked_patterns = [".git", ".venv"]

        toml = config_to_toml(config)

        assert 'blocked_patterns = [".git", ".venv"]' in toml
