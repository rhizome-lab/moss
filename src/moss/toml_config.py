"""TOML-based configuration for Moss.

This module provides support for moss.toml configuration files, offering
a simpler declarative alternative to Python-based configuration.

Usage:
    from moss.toml_config import load_toml_config, find_config_file

    # Load from a specific file
    config = load_toml_config(Path("moss.toml"))

    # Auto-discover config file in directory hierarchy
    config_path = find_config_file(Path.cwd())
    if config_path:
        config = load_toml_config(config_path)

Example moss.toml:
    [project]
    name = "my-project"

    [validators]
    syntax = true
    ruff = true
    pytest = false

    [policies]
    velocity = true
    rate_limit = true
    blocked_patterns = [".git", ".env", "node_modules"]

    [loop]
    max_iterations = 10
    timeout_seconds = 300
"""

from __future__ import annotations

import tomllib
from pathlib import Path
from typing import Any

from moss.config import MossConfig

# Config file names to search for (in order of preference)
CONFIG_FILE_NAMES = ["moss.toml", ".mossrc.toml", "pyproject.toml"]


def find_config_file(
    start_dir: Path,
    config_names: list[str] | None = None,
) -> Path | None:
    """Find a config file by searching up the directory hierarchy.

    Args:
        start_dir: Directory to start searching from
        config_names: List of config file names to search for (default: CONFIG_FILE_NAMES)

    Returns:
        Path to the config file, or None if not found
    """
    config_names = config_names or CONFIG_FILE_NAMES
    current = start_dir.resolve()

    while True:
        for name in config_names:
            config_path = current / name
            if config_path.exists():
                # For pyproject.toml, check if it has a [tool.moss] section
                if name == "pyproject.toml":
                    if _has_moss_section(config_path):
                        return config_path
                else:
                    return config_path

        # Move up to parent directory
        parent = current.parent
        if parent == current:
            # Reached root
            break
        current = parent

    return None


def _has_moss_section(pyproject_path: Path) -> bool:
    """Check if pyproject.toml has a [tool.moss] section."""
    try:
        with open(pyproject_path, "rb") as f:
            data = tomllib.load(f)
        return "tool" in data and "moss" in data["tool"]
    except Exception:
        return False


def load_toml_config(path: Path) -> MossConfig:
    """Load a MossConfig from a TOML file.

    Supports moss.toml (full file) and pyproject.toml (under [tool.moss]).

    Args:
        path: Path to the config file

    Returns:
        Configured MossConfig instance
    """
    if not path.exists():
        raise FileNotFoundError(f"Config file not found: {path}")

    with open(path, "rb") as f:
        data = tomllib.load(f)

    # Extract moss config from pyproject.toml
    if path.name == "pyproject.toml":
        if "tool" not in data or "moss" not in data["tool"]:
            raise ValueError(f"No [tool.moss] section in {path}")
        data = data["tool"]["moss"]

    return _build_config_from_dict(data, path.parent)


def _build_config_from_dict(data: dict[str, Any], project_root: Path) -> MossConfig:
    """Build a MossConfig from a dictionary of settings.

    Args:
        data: Configuration dictionary
        project_root: Project root directory

    Returns:
        Configured MossConfig instance
    """
    config = MossConfig()
    config.project_root = project_root

    # Project settings
    if "project" in data:
        project = data["project"]
        if "name" in project:
            config.project_name = project["name"]
        if "root" in project:
            config.project_root = project_root / project["root"]

    # Distro extension
    if "extends" in data:
        from moss.config import get_distro

        distro_name = data["extends"]
        distro = get_distro(distro_name)
        if distro is None:
            raise ValueError(f"Unknown distro: {distro_name}")
        config = distro.apply(config)

    # Validator settings
    if "validators" in data:
        validators = data["validators"]
        if "syntax" in validators:
            config.validators.syntax = validators["syntax"]
        if "ruff" in validators:
            config.validators.ruff = validators["ruff"]
        if "ruff_fix" in validators:
            config.validators.ruff_fix = validators["ruff_fix"]
        if "pytest" in validators:
            config.validators.pytest = validators["pytest"]
        if "pytest_args" in validators:
            config.validators.pytest_args = validators["pytest_args"]

    # Policy settings
    if "policies" in data:
        policies = data["policies"]
        if "velocity" in policies:
            config.policies.velocity = policies["velocity"]
        if "velocity_stall_threshold" in policies:
            config.policies.velocity_stall_threshold = policies["velocity_stall_threshold"]
        if "velocity_oscillation_threshold" in policies:
            config.policies.velocity_oscillation_threshold = policies[
                "velocity_oscillation_threshold"
            ]
        if "quarantine" in policies:
            config.policies.quarantine = policies["quarantine"]
        if "rate_limit" in policies:
            config.policies.rate_limit = policies["rate_limit"]
        if "rate_limit_per_minute" in policies:
            config.policies.rate_limit_per_minute = policies["rate_limit_per_minute"]
        if "rate_limit_per_target" in policies:
            config.policies.rate_limit_per_target = policies["rate_limit_per_target"]
        if "path" in policies:
            config.policies.path = policies["path"]
        if "blocked_patterns" in policies:
            config.policies.blocked_patterns = policies["blocked_patterns"]
        if "blocked_paths" in policies:
            config.policies.blocked_paths = [project_root / p for p in policies["blocked_paths"]]

    # Loop settings
    if "loop" in data:
        loop = data["loop"]
        if "max_iterations" in loop:
            config.loop.max_iterations = loop["max_iterations"]
        if "stall_threshold" in loop:
            config.loop.stall_threshold = loop["stall_threshold"]
        if "oscillation_threshold" in loop:
            config.loop.oscillation_threshold = loop["oscillation_threshold"]
        if "timeout_seconds" in loop:
            config.loop.timeout_seconds = loop["timeout_seconds"]
        if "auto_commit" in loop:
            config.loop.auto_commit = loop["auto_commit"]

    # Static context paths
    if "static_context" in data:
        for path_str in data["static_context"]:
            config.static_context.append(project_root / path_str)

    # Metadata
    if "metadata" in data:
        config.metadata.update(data["metadata"])

    return config


def merge_configs(base: MossConfig, override: MossConfig) -> MossConfig:
    """Merge two configs, with override taking precedence.

    This is useful for per-directory overrides where a subdirectory
    can override settings from a parent config.

    Args:
        base: Base configuration
        override: Override configuration

    Returns:
        Merged configuration
    """
    # Start with a copy of base config
    merged = MossConfig()

    # Project settings - override wins
    merged.project_root = override.project_root or base.project_root
    if override.project_name != "moss-project":
        merged.project_name = override.project_name
    else:
        merged.project_name = base.project_name

    # Validators - override wins for each setting
    merged.validators.syntax = override.validators.syntax
    merged.validators.ruff = override.validators.ruff
    merged.validators.ruff_fix = override.validators.ruff_fix
    merged.validators.pytest = override.validators.pytest
    merged.validators.pytest_args = override.validators.pytest_args or base.validators.pytest_args
    merged.validators.custom = base.validators.custom + override.validators.custom

    # Policies - override wins for each setting
    merged.policies.velocity = override.policies.velocity
    merged.policies.velocity_stall_threshold = override.policies.velocity_stall_threshold
    merged.policies.velocity_oscillation_threshold = (
        override.policies.velocity_oscillation_threshold
    )
    merged.policies.quarantine = override.policies.quarantine
    merged.policies.rate_limit = override.policies.rate_limit
    merged.policies.rate_limit_per_minute = override.policies.rate_limit_per_minute
    merged.policies.rate_limit_per_target = override.policies.rate_limit_per_target
    merged.policies.path = override.policies.path
    merged.policies.blocked_patterns = (
        override.policies.blocked_patterns or base.policies.blocked_patterns
    )
    merged.policies.blocked_paths = override.policies.blocked_paths or base.policies.blocked_paths
    merged.policies.custom = base.policies.custom + override.policies.custom

    # Loop - override wins
    merged.loop.max_iterations = override.loop.max_iterations
    merged.loop.stall_threshold = override.loop.stall_threshold
    merged.loop.oscillation_threshold = override.loop.oscillation_threshold
    merged.loop.timeout_seconds = override.loop.timeout_seconds
    merged.loop.auto_commit = override.loop.auto_commit

    # View providers - combine both
    merged.view_providers = base.view_providers + override.view_providers

    # Static context - combine both
    merged.static_context = base.static_context + override.static_context

    # Hooks - override wins
    merged.on_start = override.on_start or base.on_start
    merged.on_error = override.on_error or base.on_error

    # Extends - combine
    merged.extends = base.extends + override.extends

    # Metadata - merge with override taking precedence
    merged.metadata = {**base.metadata, **override.metadata}

    return merged


def load_config_with_overrides(path: Path) -> MossConfig:
    """Load config with per-directory overrides.

    Searches up the directory tree for config files and merges them,
    with child configs overriding parent configs.

    Args:
        path: Starting directory to search from

    Returns:
        Merged MossConfig
    """
    configs: list[tuple[Path, MossConfig]] = []
    current = path.resolve()

    # Collect all configs up the tree
    while True:
        config_path = find_config_file(current, CONFIG_FILE_NAMES)
        if config_path and config_path.parent == current:
            configs.append((config_path, load_toml_config(config_path)))

        parent = current.parent
        if parent == current:
            break
        current = parent

    if not configs:
        return MossConfig()

    # Merge configs from root to leaf (so child overrides parent)
    configs.reverse()  # Root first
    result = configs[0][1]
    for _, config in configs[1:]:
        result = merge_configs(result, config)

    return result


def config_to_toml(config: MossConfig) -> str:
    """Convert a MossConfig to TOML format.

    Args:
        config: Configuration to convert

    Returns:
        TOML-formatted string
    """
    lines = []

    # Project section
    lines.append("[project]")
    lines.append(f'name = "{config.project_name}"')
    lines.append("")

    # Validators section
    lines.append("[validators]")
    lines.append(f"syntax = {str(config.validators.syntax).lower()}")
    lines.append(f"ruff = {str(config.validators.ruff).lower()}")
    lines.append(f"ruff_fix = {str(config.validators.ruff_fix).lower()}")
    lines.append(f"pytest = {str(config.validators.pytest).lower()}")
    if config.validators.pytest_args:
        args_str = ", ".join(f'"{a}"' for a in config.validators.pytest_args)
        lines.append(f"pytest_args = [{args_str}]")
    lines.append("")

    # Policies section
    lines.append("[policies]")
    lines.append(f"velocity = {str(config.policies.velocity).lower()}")
    lines.append(f"quarantine = {str(config.policies.quarantine).lower()}")
    lines.append(f"rate_limit = {str(config.policies.rate_limit).lower()}")
    lines.append(f"path = {str(config.policies.path).lower()}")
    if config.policies.blocked_patterns:
        patterns_str = ", ".join(f'"{p}"' for p in config.policies.blocked_patterns)
        lines.append(f"blocked_patterns = [{patterns_str}]")
    lines.append("")

    # Loop section
    lines.append("[loop]")
    lines.append(f"max_iterations = {config.loop.max_iterations}")
    lines.append(f"timeout_seconds = {config.loop.timeout_seconds}")
    lines.append(f"auto_commit = {str(config.loop.auto_commit).lower()}")
    lines.append("")

    # Static context
    if config.static_context:
        paths_str = ", ".join(f'"{p}"' for p in config.static_context)
        lines.append(f"static_context = [{paths_str}]")
        lines.append("")

    return "\n".join(lines)
