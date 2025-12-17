"""Configuration: Executable Python DSL for Moss configuration."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from moss.loop import LoopConfig
from moss.policy import (
    PathPolicy,
    Policy,
    QuarantinePolicy,
    RateLimitPolicy,
    VelocityPolicy,
)
from moss.validators import (
    PytestValidator,
    RuffValidator,
    SyntaxValidator,
    Validator,
    ValidatorChain,
)
from moss.views import ViewProvider, ViewRegistry


@dataclass
class ValidatorConfig:
    """Configuration for validators."""

    syntax: bool = True
    ruff: bool = True
    ruff_fix: bool = False
    pytest: bool = False
    pytest_args: list[str] = field(default_factory=list)
    custom: list[Validator] = field(default_factory=list)

    def build(self) -> ValidatorChain:
        """Build a ValidatorChain from this configuration."""
        chain = ValidatorChain()
        if self.syntax:
            chain.add(SyntaxValidator())
        if self.ruff:
            chain.add(RuffValidator(fix=self.ruff_fix))
        if self.pytest:
            chain.add(PytestValidator(args=self.pytest_args))
        for validator in self.custom:
            chain.add(validator)
        return chain


@dataclass
class PolicyConfig:
    """Configuration for policies."""

    velocity: bool = True
    velocity_stall_threshold: int = 3
    velocity_oscillation_threshold: int = 2

    quarantine: bool = True
    quarantine_repair_tools: set[str] = field(
        default_factory=lambda: {"repair", "fix_syntax", "raw_edit"}
    )

    rate_limit: bool = True
    rate_limit_per_minute: int = 60
    rate_limit_per_target: int = 10

    path: bool = True
    blocked_patterns: list[str] = field(
        default_factory=lambda: [".git", ".env", "__pycache__", "node_modules"]
    )
    blocked_paths: list[Path] = field(default_factory=list)

    custom: list[Policy] = field(default_factory=list)

    def build(self) -> list[Policy]:
        """Build a list of policies from this configuration."""
        policies: list[Policy] = []

        if self.velocity:
            policies.append(
                VelocityPolicy(
                    stall_threshold=self.velocity_stall_threshold,
                    oscillation_threshold=self.velocity_oscillation_threshold,
                )
            )

        if self.quarantine:
            policies.append(QuarantinePolicy(repair_tools=self.quarantine_repair_tools))

        if self.rate_limit:
            policies.append(
                RateLimitPolicy(
                    max_calls_per_minute=self.rate_limit_per_minute,
                    max_calls_per_target=self.rate_limit_per_target,
                )
            )

        if self.path:
            policies.append(
                PathPolicy(
                    blocked_patterns=self.blocked_patterns,
                    blocked_paths=self.blocked_paths,
                )
            )

        policies.extend(self.custom)
        return policies


@dataclass
class LoopConfigWrapper:
    """Wrapper for loop configuration with builder pattern."""

    max_iterations: int = 10
    stall_threshold: int = 3
    oscillation_threshold: int = 2
    timeout_seconds: int = 300
    auto_commit: bool = True

    def build(self) -> LoopConfig:
        """Build a LoopConfig from this configuration."""
        return LoopConfig(
            max_iterations=self.max_iterations,
            stall_threshold=self.stall_threshold,
            oscillation_threshold=self.oscillation_threshold,
            timeout_seconds=self.timeout_seconds,
            auto_commit=self.auto_commit,
        )


@dataclass
class MossConfig:
    """Main configuration for Moss.

    This is the primary configuration class that combines all subsystem
    configurations. It supports a fluent builder pattern for easy customization.
    """

    # Project settings
    project_root: Path = field(default_factory=Path.cwd)
    project_name: str = "moss-project"

    # Subsystem configurations
    validators: ValidatorConfig = field(default_factory=ValidatorConfig)
    policies: PolicyConfig = field(default_factory=PolicyConfig)
    loop: LoopConfigWrapper = field(default_factory=LoopConfigWrapper)

    # View providers
    view_providers: list[ViewProvider] = field(default_factory=list)

    # Static context (architecture docs, style guides)
    static_context: list[Path] = field(default_factory=list)

    # Hooks
    on_start: Callable[[MossConfig], None] | None = None
    on_error: Callable[[MossConfig, Exception], None] | None = None

    # Extension metadata
    extends: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def with_project(self, root: Path, name: str | None = None) -> MossConfig:
        """Set project settings."""
        self.project_root = root
        if name:
            self.project_name = name
        return self

    def with_validators(
        self,
        syntax: bool = True,
        ruff: bool = True,
        pytest: bool = False,
        **kwargs: Any,
    ) -> MossConfig:
        """Configure validators."""
        self.validators.syntax = syntax
        self.validators.ruff = ruff
        self.validators.pytest = pytest
        for key, value in kwargs.items():
            if hasattr(self.validators, key):
                setattr(self.validators, key, value)
        return self

    def with_policies(
        self,
        velocity: bool = True,
        quarantine: bool = True,
        rate_limit: bool = True,
        path: bool = True,
        **kwargs: Any,
    ) -> MossConfig:
        """Configure policies."""
        self.policies.velocity = velocity
        self.policies.quarantine = quarantine
        self.policies.rate_limit = rate_limit
        self.policies.path = path
        for key, value in kwargs.items():
            if hasattr(self.policies, key):
                setattr(self.policies, key, value)
        return self

    def with_loop(
        self,
        max_iterations: int | None = None,
        timeout_seconds: int | None = None,
        auto_commit: bool | None = None,
    ) -> MossConfig:
        """Configure the validation loop."""
        if max_iterations is not None:
            self.loop.max_iterations = max_iterations
        if timeout_seconds is not None:
            self.loop.timeout_seconds = timeout_seconds
        if auto_commit is not None:
            self.loop.auto_commit = auto_commit
        return self

    def with_static_context(self, *paths: Path) -> MossConfig:
        """Add static context files (architecture docs, style guides)."""
        self.static_context.extend(paths)
        return self

    def add_validator(self, validator: Validator) -> MossConfig:
        """Add a custom validator."""
        self.validators.custom.append(validator)
        return self

    def add_policy(self, policy: Policy) -> MossConfig:
        """Add a custom policy."""
        self.policies.custom.append(policy)
        return self

    def add_view_provider(self, provider: ViewProvider) -> MossConfig:
        """Add a custom view provider."""
        self.view_providers.append(provider)
        return self

    def validate(self) -> list[str]:
        """Validate the configuration.

        Returns a list of validation errors (empty if valid).
        """
        errors: list[str] = []

        if not self.project_root.exists():
            errors.append(f"Project root does not exist: {self.project_root}")

        for path in self.static_context:
            if not path.exists():
                errors.append(f"Static context file does not exist: {path}")

        if self.loop.max_iterations < 1:
            errors.append("max_iterations must be at least 1")

        if self.loop.timeout_seconds < 1:
            errors.append("timeout_seconds must be at least 1")

        return errors

    def build_validator_chain(self) -> ValidatorChain:
        """Build the validator chain from configuration."""
        return self.validators.build()

    def build_policies(self) -> list[Policy]:
        """Build the policy list from configuration."""
        return self.policies.build()

    def build_loop_config(self) -> LoopConfig:
        """Build the loop config from configuration."""
        return self.loop.build()

    def build_view_registry(self) -> ViewRegistry:
        """Build the view registry from configuration."""
        registry = ViewRegistry()
        for provider in self.view_providers:
            registry.register(provider)
        return registry


class Distro:
    """A composable preset of configuration.

    Distros allow packaging and sharing configuration presets that can
    be extended and composed.
    """

    def __init__(
        self,
        name: str,
        description: str = "",
        extends: list[Distro] | None = None,
    ):
        self.name = name
        self.description = description
        self.extends = extends or []
        self._modifiers: list[Callable[[MossConfig], MossConfig]] = []

    def modify(
        self, modifier: Callable[[MossConfig], MossConfig]
    ) -> Distro:
        """Add a configuration modifier."""
        self._modifiers.append(modifier)
        return self

    def apply(self, config: MossConfig) -> MossConfig:
        """Apply this distro to a configuration.

        First applies parent distros, then this distro's modifiers.
        """
        # Apply parent distros first
        for parent in self.extends:
            config = parent.apply(config)

        # Apply this distro's modifiers
        for modifier in self._modifiers:
            config = modifier(config)

        # Track lineage
        config.extends.append(self.name)

        return config

    def create_config(self) -> MossConfig:
        """Create a new config with this distro applied."""
        return self.apply(MossConfig())


# Built-in distros

def _python_modifier(config: MossConfig) -> MossConfig:
    """Python-focused configuration."""
    return (
        config
        .with_validators(syntax=True, ruff=True, pytest=False)
        .with_policies(velocity=True, quarantine=True)
    )


def _strict_modifier(config: MossConfig) -> MossConfig:
    """Strict configuration with all checks enabled."""
    return (
        config
        .with_validators(syntax=True, ruff=True, pytest=True)
        .with_policies(velocity=True, quarantine=True, rate_limit=True, path=True)
        .with_loop(max_iterations=5)  # Fail faster
    )


def _lenient_modifier(config: MossConfig) -> MossConfig:
    """Lenient configuration with minimal checks."""
    return (
        config
        .with_validators(syntax=True, ruff=False, pytest=False)
        .with_policies(velocity=False, quarantine=False, rate_limit=False, path=True)
        .with_loop(max_iterations=20)  # More attempts allowed
    )


def _fast_modifier(config: MossConfig) -> MossConfig:
    """Fast iteration configuration."""
    return (
        config
        .with_validators(syntax=True, ruff=False, pytest=False)
        .with_loop(max_iterations=3, timeout_seconds=60)
    )


# Pre-built distros
PYTHON_DISTRO = Distro("python", "Standard Python development").modify(_python_modifier)

STRICT_DISTRO = Distro(
    "strict",
    "Strict checks for production code",
    extends=[PYTHON_DISTRO],
).modify(_strict_modifier)

LENIENT_DISTRO = Distro(
    "lenient",
    "Lenient checks for prototyping",
    extends=[PYTHON_DISTRO],
).modify(_lenient_modifier)

FAST_DISTRO = Distro(
    "fast",
    "Fast iteration for quick fixes",
    extends=[PYTHON_DISTRO],
).modify(_fast_modifier)


# Distro registry
_DISTRO_REGISTRY: dict[str, Distro] = {
    "python": PYTHON_DISTRO,
    "strict": STRICT_DISTRO,
    "lenient": LENIENT_DISTRO,
    "fast": FAST_DISTRO,
}


def register_distro(distro: Distro) -> None:
    """Register a custom distro."""
    _DISTRO_REGISTRY[distro.name] = distro


def get_distro(name: str) -> Distro | None:
    """Get a distro by name."""
    return _DISTRO_REGISTRY.get(name)


def list_distros() -> list[str]:
    """List all registered distro names."""
    return list(_DISTRO_REGISTRY.keys())


def create_config(distro: str | Distro | None = None) -> MossConfig:
    """Create a configuration, optionally from a distro.

    Args:
        distro: Distro name (str), Distro instance, or None for default

    Returns:
        MossConfig instance
    """
    config = MossConfig()

    if distro is None:
        return config

    if isinstance(distro, str):
        distro_instance = get_distro(distro)
        if distro_instance is None:
            raise ValueError(f"Unknown distro: {distro}. Available: {list_distros()}")
        return distro_instance.apply(config)

    return distro.apply(config)


def load_config_file(path: Path) -> MossConfig:
    """Load configuration from a Python file.

    The file should define a `config` variable or a `configure()` function
    that returns a MossConfig.

    Example config file:
        from moss.config import create_config

        config = create_config("python").with_validators(pytest=True)

    Or:
        from moss.config import create_config

        def configure():
            return create_config("strict")
    """
    import importlib.util

    if not path.exists():
        raise FileNotFoundError(f"Config file not found: {path}")

    spec = importlib.util.spec_from_file_location("moss_config", path)
    if spec is None or spec.loader is None:
        raise ImportError(f"Cannot load config from: {path}")

    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    # Try to get config variable
    if hasattr(module, "config"):
        config = module.config
        if isinstance(config, MossConfig):
            return config
        raise TypeError(f"'config' must be MossConfig, got {type(config)}")

    # Try to call configure() function
    if hasattr(module, "configure"):
        configure_fn = module.configure
        if callable(configure_fn):
            config = configure_fn()
            if isinstance(config, MossConfig):
                return config
            raise TypeError(f"configure() must return MossConfig, got {type(config)}")

    raise ValueError(
        f"Config file must define 'config' variable or 'configure()' function: {path}"
    )
