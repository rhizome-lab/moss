"""Linter Plugin Protocol: Unified interface for external tools.

This module provides a plugin architecture for integrating external
linting, type checking, and analysis tools with Moss.

Key components:
- LinterMetadata: Describes a linter plugin's capabilities
- LinterPlugin: Protocol that linter plugins must implement
- LinterResult: Standardized output format
- SARIFAdapter: Universal adapter for SARIF-outputting tools
- LinterRegistry: Discovers and manages linter plugins
"""

from __future__ import annotations

import asyncio
import json
import logging
import shutil
from dataclasses import dataclass, field
from enum import Enum, auto
from pathlib import Path
from typing import TYPE_CHECKING, Any, Protocol, runtime_checkable

if TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)


# =============================================================================
# Data Types
# =============================================================================


class Severity(Enum):
    """Issue severity levels."""

    ERROR = auto()
    WARNING = auto()
    INFO = auto()
    HINT = auto()

    @classmethod
    def from_sarif(cls, level: str) -> Severity:
        """Convert SARIF level to Severity."""
        mapping = {
            "error": cls.ERROR,
            "warning": cls.WARNING,
            "note": cls.INFO,
            "none": cls.HINT,
        }
        return mapping.get(level.lower(), cls.WARNING)

    @classmethod
    def from_string(cls, s: str) -> Severity:
        """Convert common severity strings."""
        s_lower = s.lower()
        if s_lower in ("error", "e", "fatal", "critical"):
            return cls.ERROR
        elif s_lower in ("warning", "w", "warn"):
            return cls.WARNING
        elif s_lower in ("info", "i", "information", "notice"):
            return cls.INFO
        return cls.HINT


@dataclass(frozen=True)
class LinterIssue:
    """A single issue reported by a linter.

    Attributes:
        message: Human-readable issue description
        severity: Issue severity level
        file: File where the issue was found
        line: Line number (1-indexed)
        column: Column number (1-indexed)
        end_line: End line for multi-line issues
        end_column: End column for multi-line issues
        rule_id: Rule/code identifier (e.g., "E501", "mypy-error")
        source: Tool that reported the issue
        fix: Suggested fix, if available
    """

    message: str
    severity: Severity
    file: Path | None = None
    line: int | None = None
    column: int | None = None
    end_line: int | None = None
    end_column: int | None = None
    rule_id: str | None = None
    source: str | None = None
    fix: str | None = None

    def __str__(self) -> str:
        parts = []
        if self.file:
            loc = str(self.file)
            if self.line:
                loc += f":{self.line}"
                if self.column:
                    loc += f":{self.column}"
            parts.append(loc)
        if self.rule_id:
            parts.append(f"[{self.rule_id}]")
        parts.append(self.message)
        return " ".join(parts)


@dataclass
class LinterResult:
    """Result of running a linter.

    Attributes:
        success: True if no errors were found
        issues: List of issues found
        tool_name: Name of the tool that ran
        tool_version: Version of the tool
        execution_time_ms: How long the tool took to run
        metadata: Additional tool-specific data
    """

    success: bool
    issues: list[LinterIssue] = field(default_factory=list)
    tool_name: str = ""
    tool_version: str = ""
    execution_time_ms: float = 0.0
    metadata: dict[str, Any] = field(default_factory=dict)

    @property
    def errors(self) -> list[LinterIssue]:
        """Get all error-level issues."""
        return [i for i in self.issues if i.severity == Severity.ERROR]

    @property
    def warnings(self) -> list[LinterIssue]:
        """Get all warning-level issues."""
        return [i for i in self.issues if i.severity == Severity.WARNING]

    @property
    def error_count(self) -> int:
        """Count of errors."""
        return len(self.errors)

    @property
    def warning_count(self) -> int:
        """Count of warnings."""
        return len(self.warnings)


# =============================================================================
# Plugin Metadata
# =============================================================================


@dataclass(frozen=True)
class LinterMetadata:
    """Metadata describing a linter plugin.

    Attributes:
        name: Unique identifier (e.g., "ruff", "mypy")
        tool_name: Human-readable tool name
        languages: Languages this linter supports
        category: Category (linter, type-checker, formatter, security)
        priority: Selection priority when multiple linters available
        version: Plugin version
        description: Human-readable description
        required_tool: External tool binary name (for availability check)
        supports_fix: Whether the tool can auto-fix issues
        supports_sarif: Whether the tool supports SARIF output
    """

    name: str
    tool_name: str
    languages: frozenset[str] = field(default_factory=frozenset)
    category: str = "linter"
    priority: int = 0
    version: str = "0.1.0"
    description: str = ""
    required_tool: str | None = None
    supports_fix: bool = False
    supports_sarif: bool = False


# =============================================================================
# Plugin Protocol
# =============================================================================


@runtime_checkable
class LinterPlugin(Protocol):
    """Protocol for linter plugins.

    Plugins integrate external tools (ruff, mypy, eslint, etc.) with Moss,
    providing a unified interface for running linters and parsing output.
    """

    @property
    def metadata(self) -> LinterMetadata:
        """Plugin metadata describing capabilities."""
        ...

    def is_available(self) -> bool:
        """Check if the required tool is installed.

        Returns:
            True if the tool is available and can be run
        """
        ...

    def get_version(self) -> str | None:
        """Get the version of the installed tool.

        Returns:
            Version string, or None if not available
        """
        ...

    async def run(
        self,
        path: Path,
        *,
        fix: bool = False,
        config: dict[str, Any] | None = None,
    ) -> LinterResult:
        """Run the linter on a file or directory.

        Args:
            path: File or directory to lint
            fix: Whether to apply auto-fixes (if supported)
            config: Tool-specific configuration

        Returns:
            LinterResult with all issues found
        """
        ...


# =============================================================================
# Built-in Plugins
# =============================================================================


class RuffPlugin:
    """Ruff linter plugin.

    Ruff is an extremely fast Python linter written in Rust.
    """

    @property
    def metadata(self) -> LinterMetadata:
        return LinterMetadata(
            name="ruff",
            tool_name="Ruff",
            languages=frozenset(["python"]),
            category="linter",
            priority=100,  # High priority for Python
            description="Extremely fast Python linter",
            required_tool="ruff",
            supports_fix=True,
            supports_sarif=True,
        )

    def is_available(self) -> bool:
        return shutil.which("ruff") is not None

    def get_version(self) -> str | None:
        try:
            import subprocess

            result = subprocess.run(
                ["ruff", "--version"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if result.returncode == 0:
                # Output: "ruff 0.x.x"
                return result.stdout.strip().split()[-1]
        except (OSError, subprocess.SubprocessError, subprocess.TimeoutExpired):
            pass
        return None

    async def run(
        self,
        path: Path,
        *,
        fix: bool = False,
        config: dict[str, Any] | None = None,
    ) -> LinterResult:
        import time

        start = time.time()

        cmd = ["ruff", "check", str(path), "--output-format=json"]
        if fix:
            cmd.append("--fix")

        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _stderr = await proc.communicate()

        execution_time = (time.time() - start) * 1000

        issues: list[LinterIssue] = []
        try:
            data = json.loads(stdout.decode()) if stdout else []
            for item in data:
                issues.append(
                    LinterIssue(
                        message=item.get("message", ""),
                        severity=Severity.ERROR,  # Ruff issues are errors
                        file=Path(item.get("filename", "")),
                        line=item.get("location", {}).get("row"),
                        column=item.get("location", {}).get("column"),
                        end_line=item.get("end_location", {}).get("row"),
                        end_column=item.get("end_location", {}).get("column"),
                        rule_id=item.get("code"),
                        source="ruff",
                        fix=(item.get("fix") or {}).get("message"),
                    )
                )
        except json.JSONDecodeError:
            pass

        return LinterResult(
            success=len(issues) == 0,
            issues=issues,
            tool_name="ruff",
            tool_version=self.get_version() or "",
            execution_time_ms=execution_time,
        )


class MypyPlugin:
    """Mypy type checker plugin.

    Mypy is a static type checker for Python.
    """

    @property
    def metadata(self) -> LinterMetadata:
        return LinterMetadata(
            name="mypy",
            tool_name="Mypy",
            languages=frozenset(["python"]),
            category="type-checker",
            priority=90,
            description="Static type checker for Python",
            required_tool="mypy",
            supports_fix=False,
            supports_sarif=False,
        )

    def is_available(self) -> bool:
        return shutil.which("mypy") is not None

    def get_version(self) -> str | None:
        try:
            import subprocess

            result = subprocess.run(
                ["mypy", "--version"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if result.returncode == 0:
                # Output: "mypy 1.x.x (compiled: yes)"
                parts = result.stdout.strip().split()
                if len(parts) >= 2:
                    return parts[1]
        except (OSError, subprocess.SubprocessError, subprocess.TimeoutExpired):
            pass
        return None

    async def run(
        self,
        path: Path,
        *,
        fix: bool = False,
        config: dict[str, Any] | None = None,
    ) -> LinterResult:
        import time

        start = time.time()

        cmd = ["mypy", str(path), "--show-column-numbers", "--no-error-summary"]

        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _stderr = await proc.communicate()

        execution_time = (time.time() - start) * 1000

        issues: list[LinterIssue] = []
        output = stdout.decode() if stdout else ""

        for line in output.strip().split("\n"):
            if not line:
                continue
            # Format: file.py:line:col: severity: message  [error-code]
            try:
                parts = line.split(":", 4)
                if len(parts) >= 5:
                    file_path = parts[0]
                    line_no = int(parts[1]) if parts[1].isdigit() else None
                    col_no = int(parts[2]) if parts[2].isdigit() else None
                    severity_msg = parts[3].strip() + ":" + parts[4]
                    sev_parts = severity_msg.split(":", 1)
                    sev = Severity.from_string(sev_parts[0].strip())
                    msg = sev_parts[1].strip() if len(sev_parts) > 1 else ""

                    # Extract error code if present
                    rule_id = None
                    if msg.endswith("]"):
                        idx = msg.rfind("[")
                        if idx != -1:
                            rule_id = msg[idx + 1 : -1]
                            msg = msg[:idx].strip()

                    issues.append(
                        LinterIssue(
                            message=msg,
                            severity=sev,
                            file=Path(file_path),
                            line=line_no,
                            column=col_no,
                            rule_id=rule_id,
                            source="mypy",
                        )
                    )
            except (ValueError, IndexError):
                continue

        return LinterResult(
            success=len([i for i in issues if i.severity == Severity.ERROR]) == 0,
            issues=issues,
            tool_name="mypy",
            tool_version=self.get_version() or "",
            execution_time_ms=execution_time,
        )


# =============================================================================
# SARIF Adapter
# =============================================================================


class SARIFAdapter:
    """Universal adapter for tools that output SARIF.

    SARIF (Static Analysis Results Interchange Format) is a standard
    JSON-based format for static analysis tools.
    """

    def __init__(self, tool_name: str, command: list[str]):
        """Create a SARIF adapter.

        Args:
            tool_name: Name of the tool
            command: Command to run (should output SARIF to stdout)
        """
        self.tool_name = tool_name
        self.command = command

    @property
    def metadata(self) -> LinterMetadata:
        return LinterMetadata(
            name=f"sarif-{self.tool_name}",
            tool_name=self.tool_name,
            category="linter",
            supports_sarif=True,
        )

    def is_available(self) -> bool:
        if not self.command:
            return False
        return shutil.which(self.command[0]) is not None

    def get_version(self) -> str | None:
        return None

    async def run(
        self,
        path: Path,
        *,
        fix: bool = False,
        config: dict[str, Any] | None = None,
    ) -> LinterResult:
        import time

        start = time.time()

        # Substitute {path} in command
        cmd = [arg.replace("{path}", str(path)) for arg in self.command]

        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _stderr = await proc.communicate()

        execution_time = (time.time() - start) * 1000

        issues = self._parse_sarif(stdout.decode() if stdout else "")

        return LinterResult(
            success=len([i for i in issues if i.severity == Severity.ERROR]) == 0,
            issues=issues,
            tool_name=self.tool_name,
            execution_time_ms=execution_time,
        )

    def _parse_sarif(self, sarif_json: str) -> list[LinterIssue]:
        """Parse SARIF JSON into LinterIssues."""
        issues: list[LinterIssue] = []

        try:
            data = json.loads(sarif_json)
        except json.JSONDecodeError:
            return issues

        for run in data.get("runs", []):
            tool_name = run.get("tool", {}).get("driver", {}).get("name", self.tool_name)
            rules = {r["id"]: r for r in run.get("tool", {}).get("driver", {}).get("rules", [])}

            for result in run.get("results", []):
                rule_id = result.get("ruleId", "")
                message = result.get("message", {}).get("text", "")

                # Get additional info from rule definition
                rule = rules.get(rule_id, {})
                if not message:
                    message = rule.get("shortDescription", {}).get("text", "")

                severity = Severity.from_sarif(result.get("level", "warning"))

                # Get location
                locations = result.get("locations", [])
                file_path = None
                line = None
                column = None
                end_line = None
                end_column = None

                if locations:
                    loc = locations[0].get("physicalLocation", {})
                    artifact = loc.get("artifactLocation", {})
                    file_path = artifact.get("uri")
                    if file_path and file_path.startswith("file://"):
                        file_path = file_path[7:]

                    region = loc.get("region", {})
                    line = region.get("startLine")
                    column = region.get("startColumn")
                    end_line = region.get("endLine")
                    end_column = region.get("endColumn")

                # Get fix if available
                fix_text = None
                fixes = result.get("fixes", [])
                if fixes:
                    changes = fixes[0].get("artifactChanges", [])
                    if changes:
                        replacements = changes[0].get("replacements", [])
                        if replacements:
                            fix_text = replacements[0].get("insertedContent", {}).get("text")

                issues.append(
                    LinterIssue(
                        message=message,
                        severity=severity,
                        file=Path(file_path) if file_path else None,
                        line=line,
                        column=column,
                        end_line=end_line,
                        end_column=end_column,
                        rule_id=rule_id,
                        source=tool_name,
                        fix=fix_text,
                    )
                )

        return issues


# =============================================================================
# Linter Registry
# =============================================================================


class LinterRegistry:
    """Registry for discovering and managing linter plugins.

    The registry supports:
    - Manual plugin registration
    - Automatic discovery via entry points
    - Language-based plugin selection
    - Availability checking
    """

    def __init__(self) -> None:
        """Initialize an empty registry."""
        self._plugins: dict[str, LinterPlugin] = {}
        self._by_language: dict[str, list[LinterPlugin]] = {}
        self._by_category: dict[str, list[LinterPlugin]] = {}
        self._discovered = False

    def register(self, plugin: LinterPlugin) -> None:
        """Register a linter plugin.

        Args:
            plugin: The plugin to register

        Raises:
            ValueError: If a plugin with the same name exists
        """
        meta = plugin.metadata
        name = meta.name

        if name in self._plugins:
            raise ValueError(f"Linter '{name}' is already registered")

        self._plugins[name] = plugin

        # Index by language
        for lang in meta.languages:
            if lang not in self._by_language:
                self._by_language[lang] = []
            self._by_language[lang].append(plugin)
            self._by_language[lang].sort(key=lambda p: p.metadata.priority, reverse=True)

        # Index by category
        cat = meta.category
        if cat not in self._by_category:
            self._by_category[cat] = []
        self._by_category[cat].append(plugin)

        logger.debug("Registered linter: %s (category=%s)", name, cat)

    def get(self, name: str) -> LinterPlugin | None:
        """Get a linter by name."""
        return self._plugins.get(name)

    def get_for_language(self, language: str) -> list[LinterPlugin]:
        """Get all linters that support a language.

        Args:
            language: Language identifier (e.g., "python")

        Returns:
            List of plugins, sorted by priority
        """
        return list(self._by_language.get(language, []))

    def get_for_category(self, category: str) -> list[LinterPlugin]:
        """Get all linters in a category.

        Args:
            category: Category (linter, type-checker, formatter, security)

        Returns:
            List of plugins
        """
        return list(self._by_category.get(category, []))

    def get_available(self) -> list[LinterPlugin]:
        """Get all linters that have their tools installed."""
        return [p for p in self._plugins.values() if p.is_available()]

    def get_all(self) -> list[LinterPlugin]:
        """Get all registered linters."""
        return list(self._plugins.values())

    def discover_plugins(self) -> int:
        """Discover linter plugins via entry points.

        Looks for entry points in the "moss.linters" group.

        Returns:
            Number of plugins discovered
        """
        if self._discovered:
            return 0

        count = 0

        try:
            from importlib.metadata import entry_points

            eps = entry_points(group="moss.linters")

            for ep in eps:
                try:
                    plugin_factory = ep.load()
                    plugin = plugin_factory()

                    if isinstance(plugin, LinterPlugin):
                        self.register(plugin)
                        count += 1
                        logger.info("Discovered linter: %s", ep.name)
                except (ImportError, AttributeError, TypeError) as e:
                    logger.warning("Failed to load linter '%s': %s", ep.name, e)

        except ImportError:
            logger.debug("importlib.metadata not available")

        self._discovered = True
        return count

    def register_builtins(self) -> None:
        """Register built-in linter plugins."""
        builtins: list[LinterPlugin] = [
            RuffPlugin(),
            MypyPlugin(),
        ]

        for plugin in builtins:
            if plugin.metadata.name not in self._plugins:
                self.register(plugin)


# =============================================================================
# Global Registry
# =============================================================================

_linter_registry: LinterRegistry | None = None


def get_linter_registry() -> LinterRegistry:
    """Get the global linter registry.

    Creates and initializes on first call.
    """
    global _linter_registry

    if _linter_registry is None:
        _linter_registry = LinterRegistry()
        _linter_registry.discover_plugins()
        _linter_registry.register_builtins()

    return _linter_registry


def reset_linter_registry() -> None:
    """Reset the global linter registry (for testing)."""
    global _linter_registry
    _linter_registry = None


# =============================================================================
# Convenience Functions
# =============================================================================


async def lint(
    path: Path,
    language: str | None = None,
    *,
    fix: bool = False,
) -> list[LinterResult]:
    """Run all appropriate linters on a path.

    Args:
        path: File or directory to lint
        language: Language hint (auto-detected if None)
        fix: Whether to apply auto-fixes

    Returns:
        List of results from all linters
    """
    from moss_orchestration.plugins import detect_language

    registry = get_linter_registry()

    if language is None and path.is_file():
        language = detect_language(path)

    plugins = []
    if language:
        plugins = registry.get_for_language(language)
    else:
        plugins = registry.get_available()

    results = []
    for plugin in plugins:
        if plugin.is_available():
            try:
                result = await plugin.run(path, fix=fix)
                results.append(result)
            except (OSError, asyncio.SubprocessError) as e:
                logger.warning("Linter %s failed: %s", plugin.metadata.name, e)

    return results


__all__ = [
    "LinterIssue",
    "LinterMetadata",
    "LinterPlugin",
    "LinterRegistry",
    "LinterResult",
    "MypyPlugin",
    "RuffPlugin",
    "SARIFAdapter",
    "Severity",
    "get_linter_registry",
    "lint",
    "reset_linter_registry",
]
