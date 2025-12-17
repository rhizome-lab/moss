"""Tests for Configuration system."""

from pathlib import Path

import pytest

from moss.config import (
    FAST_DISTRO,
    LENIENT_DISTRO,
    PYTHON_DISTRO,
    STRICT_DISTRO,
    Distro,
    LoopConfigWrapper,
    MossConfig,
    PolicyConfig,
    ValidatorConfig,
    create_config,
    get_distro,
    list_distros,
    load_config_file,
    register_distro,
)
from moss.validators import SyntaxValidator


class TestValidatorConfig:
    """Tests for ValidatorConfig."""

    def test_default_config(self):
        config = ValidatorConfig()

        assert config.syntax is True
        assert config.ruff is True
        assert config.pytest is False

    def test_build_chain(self):
        config = ValidatorConfig(syntax=True, ruff=True, pytest=False)

        chain = config.build()

        names = [v.name for v in chain.validators]
        assert "syntax" in names
        assert "ruff" in names
        assert "pytest" not in names

    def test_custom_validators(self):
        custom = SyntaxValidator()  # Using as example
        config = ValidatorConfig(syntax=False, ruff=False, custom=[custom])

        chain = config.build()

        assert len(chain.validators) == 1


class TestPolicyConfig:
    """Tests for PolicyConfig."""

    def test_default_config(self):
        config = PolicyConfig()

        assert config.velocity is True
        assert config.quarantine is True
        assert config.rate_limit is True
        assert config.path is True

    def test_build_policies(self):
        config = PolicyConfig(velocity=True, quarantine=False, rate_limit=False, path=True)

        policies = config.build()
        names = [p.name for p in policies]

        assert "velocity" in names
        assert "quarantine" not in names
        assert "rate_limit" not in names
        assert "path" in names


class TestLoopConfigWrapper:
    """Tests for LoopConfigWrapper."""

    def test_default_config(self):
        wrapper = LoopConfigWrapper()

        assert wrapper.max_iterations == 10
        assert wrapper.timeout_seconds == 300
        assert wrapper.auto_commit is True

    def test_build_loop_config(self):
        wrapper = LoopConfigWrapper(max_iterations=5, auto_commit=False)

        loop_config = wrapper.build()

        assert loop_config.max_iterations == 5
        assert loop_config.auto_commit is False


class TestMossConfig:
    """Tests for MossConfig."""

    def test_default_config(self):
        config = MossConfig()

        assert config.project_name == "moss-project"
        assert config.validators is not None
        assert config.policies is not None
        assert config.loop is not None

    def test_fluent_builder(self):
        config = (
            MossConfig()
            .with_project(Path("/tmp/test"), "my-project")
            .with_validators(syntax=True, ruff=True, pytest=True)
            .with_policies(velocity=True, quarantine=False)
            .with_loop(max_iterations=5)
        )

        assert config.project_name == "my-project"
        assert config.validators.pytest is True
        assert config.policies.quarantine is False
        assert config.loop.max_iterations == 5

    def test_add_custom_validator(self):
        config = MossConfig().add_validator(SyntaxValidator())

        assert len(config.validators.custom) == 1

    def test_with_static_context(self, tmp_path: Path):
        doc1 = tmp_path / "arch.md"
        doc1.write_text("# Architecture")

        config = MossConfig().with_static_context(doc1)

        assert doc1 in config.static_context

    def test_validate_valid_config(self, tmp_path: Path):
        config = MossConfig().with_project(tmp_path, "test")

        errors = config.validate()

        assert len(errors) == 0

    def test_validate_missing_project_root(self):
        config = MossConfig().with_project(Path("/nonexistent/path"), "test")

        errors = config.validate()

        assert any("does not exist" in e for e in errors)

    def test_validate_invalid_loop_config(self, tmp_path: Path):
        config = MossConfig().with_project(tmp_path, "test")
        config.loop.max_iterations = 0

        errors = config.validate()

        assert any("max_iterations" in e for e in errors)

    def test_build_validator_chain(self):
        config = MossConfig().with_validators(syntax=True, ruff=True, pytest=False)

        chain = config.build_validator_chain()

        names = [v.name for v in chain.validators]
        assert "syntax" in names
        assert "ruff" in names

    def test_build_policies(self):
        config = MossConfig().with_policies(velocity=True, quarantine=True)

        policies = config.build_policies()
        names = [p.name for p in policies]

        assert "velocity" in names
        assert "quarantine" in names


class TestDistro:
    """Tests for Distro."""

    def test_create_distro(self):
        distro = Distro("test", "A test distro")

        assert distro.name == "test"
        assert distro.description == "A test distro"

    def test_distro_modifier(self):
        distro = Distro("test").modify(lambda c: c.with_validators(pytest=True))

        config = distro.create_config()

        assert config.validators.pytest is True

    def test_distro_inheritance(self):
        parent = Distro("parent").modify(lambda c: c.with_validators(syntax=True))
        child = Distro("child", extends=[parent]).modify(lambda c: c.with_validators(pytest=True))

        config = child.create_config()

        assert config.validators.syntax is True
        assert config.validators.pytest is True
        assert "parent" in config.extends
        assert "child" in config.extends

    def test_distro_apply_preserves_order(self):
        # Parent sets max_iterations to 5
        parent = Distro("parent").modify(lambda c: c.with_loop(max_iterations=5))
        # Child overrides to 3
        child = Distro("child", extends=[parent]).modify(lambda c: c.with_loop(max_iterations=3))

        config = child.create_config()

        # Child should win
        assert config.loop.max_iterations == 3


class TestBuiltInDistros:
    """Tests for built-in distros."""

    def test_python_distro(self):
        config = PYTHON_DISTRO.create_config()

        assert config.validators.syntax is True
        assert config.validators.ruff is True
        assert "python" in config.extends

    def test_strict_distro(self):
        config = STRICT_DISTRO.create_config()

        assert config.validators.pytest is True
        assert config.loop.max_iterations == 5
        assert "strict" in config.extends

    def test_lenient_distro(self):
        config = LENIENT_DISTRO.create_config()

        assert config.validators.ruff is False
        assert config.policies.velocity is False
        assert config.loop.max_iterations == 20

    def test_fast_distro(self):
        config = FAST_DISTRO.create_config()

        assert config.loop.max_iterations == 3
        assert config.loop.timeout_seconds == 60


class TestDistroRegistry:
    """Tests for distro registry functions."""

    def test_list_distros(self):
        distros = list_distros()

        assert "python" in distros
        assert "strict" in distros
        assert "lenient" in distros
        assert "fast" in distros

    def test_get_distro(self):
        distro = get_distro("python")

        assert distro is not None
        assert distro.name == "python"

    def test_get_unknown_distro(self):
        distro = get_distro("nonexistent")

        assert distro is None

    def test_register_custom_distro(self):
        custom = Distro("custom", "Custom distro")
        register_distro(custom)

        assert get_distro("custom") is custom


class TestCreateConfig:
    """Tests for create_config function."""

    def test_create_default_config(self):
        config = create_config()

        assert isinstance(config, MossConfig)

    def test_create_config_from_distro_name(self):
        config = create_config("python")

        assert "python" in config.extends

    def test_create_config_from_distro_instance(self):
        config = create_config(STRICT_DISTRO)

        assert "strict" in config.extends

    def test_create_config_unknown_distro(self):
        with pytest.raises(ValueError, match="Unknown distro"):
            create_config("nonexistent")


class TestLoadConfigFile:
    """Tests for load_config_file function."""

    def test_load_config_with_variable(self, tmp_path: Path):
        config_file = tmp_path / "moss_config.py"
        config_file.write_text("""
from moss.config import MossConfig

config = MossConfig()
config.project_name = "loaded-project"
""")

        config = load_config_file(config_file)

        assert config.project_name == "loaded-project"

    def test_load_config_with_function(self, tmp_path: Path):
        config_file = tmp_path / "moss_config.py"
        config_file.write_text("""
from moss.config import MossConfig

def configure():
    config = MossConfig()
    config.project_name = "from-function"
    return config
""")

        config = load_config_file(config_file)

        assert config.project_name == "from-function"

    def test_load_config_file_not_found(self, tmp_path: Path):
        with pytest.raises(FileNotFoundError):
            load_config_file(tmp_path / "nonexistent.py")

    def test_load_config_invalid_file(self, tmp_path: Path):
        config_file = tmp_path / "invalid.py"
        config_file.write_text("x = 1")  # No config or configure

        with pytest.raises(ValueError, match="must define"):
            load_config_file(config_file)
