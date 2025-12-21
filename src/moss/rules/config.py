"""Configuration loading for rules.

Loads rules from TOML configuration files:
- moss.toml [rules] section
- .moss/rules.toml
- pyproject.toml [tool.moss.rules] section

TOML format supports the new multi-backend architecture:

    [[rules]]
    name = "no-print"
    pattern = "\\bprint\\s*\\("
    message = "Use logging instead of print"
    backend = "regex"  # or "ast-grep", "python"
    severity = "warning"
    context = "not:test"  # Optional: skip test code

For ast-grep patterns:

    [[rules]]
    name = "no-star-import"
    pattern = "from $MOD import *"
    message = "Avoid star imports"
    backend = "ast-grep"
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .decorator import pattern_rule


def load_rules_from_toml(path: Path) -> list[Any]:
    """Load custom rules from a TOML file.

    Args:
        path: Path to TOML file

    Returns:
        List of RuleSpec objects
    """
    import tomllib

    content = path.read_text()
    data = tomllib.loads(content)

    rules = []
    for rule_data in data.get("rules", []):
        spec = _rule_from_dict(rule_data)
        if spec:
            rules.append(spec)

    return rules


def load_rules_from_config(directory: Path) -> list[Any]:
    """Load rules from configuration files.

    Looks for rules in:
    1. moss.toml [rules] section
    2. .moss/rules.toml
    3. pyproject.toml [tool.moss.rules] section

    Args:
        directory: Project directory

    Returns:
        List of RuleSpec objects
    """
    import tomllib

    rules: list[Any] = []
    directory = Path(directory).resolve()

    # Check moss.toml
    moss_toml = directory / "moss.toml"
    if moss_toml.exists():
        try:
            data = tomllib.loads(moss_toml.read_text())
            for rule_data in data.get("rules", []):
                spec = _rule_from_dict(rule_data)
                if spec:
                    rules.append(spec)
        except (OSError, tomllib.TOMLDecodeError):
            pass

    # Check .moss/rules.toml
    rules_toml = directory / ".moss" / "rules.toml"
    if rules_toml.exists():
        try:
            rules.extend(load_rules_from_toml(rules_toml))
        except (OSError, tomllib.TOMLDecodeError):
            pass

    # Check pyproject.toml
    pyproject = directory / "pyproject.toml"
    if pyproject.exists():
        try:
            data = tomllib.loads(pyproject.read_text())
            tool_moss = data.get("tool", {}).get("moss", {})
            for rule_data in tool_moss.get("rules", []):
                spec = _rule_from_dict(rule_data)
                if spec:
                    rules.append(spec)
        except (OSError, tomllib.TOMLDecodeError):
            pass

    return rules


def _rule_from_dict(data: dict[str, Any]) -> Any | None:
    """Create a RuleSpec from a dictionary.

    Supported fields:
        name: Rule name (required)
        pattern: Pattern to match (required)
        message: Violation message (required)
        backend: "regex" (default), "ast-grep", or "python"
        severity: "info", "warning" (default), or "error"
        category: Category string
        context: Context filter like "not:test"
        file_pattern: Glob pattern for applicable files
        enabled: Whether rule is enabled (default True)
        fix: Optional fix suggestion
    """
    try:
        name = data["name"]
        pattern = data["pattern"]
        message = data["message"]
    except KeyError:
        return None

    backend = data.get("backend", "regex")
    severity = data.get("severity", "warning")
    category = data.get("category", "custom")
    context = data.get("context")
    file_pattern = data.get("file_pattern", "**/*.py")
    fix = data.get("fix")

    # Use pattern_rule helper to create RuleSpec
    return pattern_rule(
        name=name,
        pattern=pattern,
        message=message,
        backend=backend,
        severity=severity,
        category=category,
        context=context,
        file_pattern=file_pattern,
        fix=fix,
    )
