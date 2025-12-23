"""Security analysis via multi-tool orchestration.

Aggregates results from multiple security analysis tools:
- bandit: Python-specific security linting
- semgrep: SAST pattern matching with community rules
- (future) Snyk, CodeQL, ast-grep

Configuration via moss.toml [security] section:

    [security]
    min_severity = "medium"

    [security.bandit]
    enabled = true
    excludes = [".venv", "venv", "node_modules"]
    args = []

    [security.semgrep]
    enabled = true
    config = "auto"
    excludes = [".venv", "venv"]

Or via CLI:
    moss security [directory] [--tools bandit,semgrep] [--severity medium]
"""

from __future__ import annotations

import json
import logging
import shutil
import subprocess
import tomllib
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import IntEnum
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


@dataclass
class SecurityToolConfig:
    """Configuration for a security tool."""

    enabled: bool = True
    excludes: list[str] = field(default_factory=lambda: [".venv", "venv", "node_modules", ".git"])
    args: list[str] = field(default_factory=list)
    config: str | None = None  # Tool-specific config (e.g., semgrep ruleset)
    timeout: int = 300  # Timeout in seconds


@dataclass
class SecurityConfig:
    """Configuration for security analysis."""

    min_severity: str = "low"
    tools: dict[str, SecurityToolConfig] = field(default_factory=dict)

    @classmethod
    def load(cls, root: Path) -> SecurityConfig:
        """Load security config from moss.toml or .moss/security.toml.

        Config sources (in order of precedence):
        1. .moss/security.toml (dedicated security config)
        2. moss.toml [security] section
        3. pyproject.toml [tool.moss.security] section
        """
        config = cls()
        root = Path(root).resolve()

        # Try .moss/security.toml first
        security_toml = root / ".moss" / "security.toml"
        if security_toml.exists():
            config = cls._from_toml(security_toml)
            return config

        # Try moss.toml
        moss_toml = root / "moss.toml"
        if moss_toml.exists():
            try:
                data = tomllib.loads(moss_toml.read_text())
                if "security" in data:
                    config = cls._from_dict(data["security"])
                    return config
            except (OSError, tomllib.TOMLDecodeError) as e:
                logger.warning("Failed to load security config from moss.toml: %s", e)

        # Try pyproject.toml
        pyproject = root / "pyproject.toml"
        if pyproject.exists():
            try:
                data = tomllib.loads(pyproject.read_text())
                security_data = data.get("tool", {}).get("moss", {}).get("security", {})
                if security_data:
                    config = cls._from_dict(security_data)
                    return config
            except (OSError, tomllib.TOMLDecodeError) as e:
                logger.warning("Failed to load security config from pyproject.toml: %s", e)

        return config

    @classmethod
    def _from_toml(cls, path: Path) -> SecurityConfig:
        """Load from a dedicated security TOML file."""
        data = tomllib.loads(path.read_text())
        return cls._from_dict(data)

    @classmethod
    def _from_dict(cls, data: dict[str, Any]) -> SecurityConfig:
        """Create config from dictionary."""
        config = cls()

        if "min_severity" in data:
            config.min_severity = data["min_severity"]

        # Load per-tool configs
        for tool_name in ["bandit", "semgrep", "snyk", "codeql", "ast-grep"]:
            if tool_name in data:
                tool_data = data[tool_name]
                config.tools[tool_name] = SecurityToolConfig(
                    enabled=tool_data.get("enabled", True),
                    excludes=tool_data.get("excludes", [".venv", "venv", "node_modules", ".git"]),
                    args=tool_data.get("args", []),
                    config=tool_data.get("config"),
                    timeout=tool_data.get("timeout", 300),
                )

        return config

    def get_tool_config(self, tool_name: str) -> SecurityToolConfig:
        """Get config for a specific tool, with defaults."""
        return self.tools.get(tool_name, SecurityToolConfig())


class Severity(IntEnum):
    """Severity levels for security findings."""

    INFO = 0
    LOW = 1
    MEDIUM = 2
    HIGH = 3
    CRITICAL = 4

    @classmethod
    def from_string(cls, s: str) -> Severity:
        """Parse severity from string."""
        mapping = {
            "info": cls.INFO,
            "low": cls.LOW,
            "medium": cls.MEDIUM,
            "med": cls.MEDIUM,
            "high": cls.HIGH,
            "critical": cls.CRITICAL,
            "crit": cls.CRITICAL,
            "error": cls.HIGH,
            "warning": cls.MEDIUM,
            "warn": cls.MEDIUM,
        }
        return mapping.get(s.lower(), cls.MEDIUM)


@dataclass
class Finding:
    """A security finding from any tool."""

    tool: str
    rule_id: str
    message: str
    severity: Severity
    file_path: str
    line_start: int
    line_end: int | None = None
    cwe: str | None = None  # CWE ID if available
    owasp: str | None = None  # OWASP category if available
    fix_suggestion: str | None = None
    confidence: str | None = None  # low, medium, high

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "tool": self.tool,
            "rule_id": self.rule_id,
            "message": self.message,
            "severity": self.severity.name.lower(),
            "file": self.file_path,
            "line": self.line_start,
            "line_end": self.line_end,
            "cwe": self.cwe,
            "owasp": self.owasp,
            "fix": self.fix_suggestion,
            "confidence": self.confidence,
        }

    @property
    def location_key(self) -> str:
        """Key for deduplication by location."""
        return f"{self.file_path}:{self.line_start}"


@dataclass
class SecurityAnalysis:
    """Results from security analysis."""

    root: Path
    findings: list[Finding] = field(default_factory=list)
    tools_run: list[str] = field(default_factory=list)
    tools_skipped: list[str] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)

    @property
    def critical_count(self) -> int:
        return sum(1 for f in self.findings if f.severity == Severity.CRITICAL)

    @property
    def high_count(self) -> int:
        return sum(1 for f in self.findings if f.severity == Severity.HIGH)

    @property
    def medium_count(self) -> int:
        return sum(1 for f in self.findings if f.severity == Severity.MEDIUM)

    @property
    def low_count(self) -> int:
        return sum(1 for f in self.findings if f.severity == Severity.LOW)

    def filter_by_severity(self, min_severity: Severity) -> list[Finding]:
        """Get findings at or above a severity threshold."""
        return [f for f in self.findings if f.severity >= min_severity]

    def dedupe(self) -> SecurityAnalysis:
        """Remove duplicate findings at the same location."""
        seen: dict[str, Finding] = {}
        for finding in self.findings:
            key = finding.location_key
            if key not in seen or finding.severity > seen[key].severity:
                seen[key] = finding

        return SecurityAnalysis(
            root=self.root,
            findings=list(seen.values()),
            tools_run=self.tools_run,
            tools_skipped=self.tools_skipped,
            errors=self.errors,
        )

    def to_compact(self) -> str:
        """Format as compact text for LLM consumption."""
        parts = []
        if self.critical_count:
            parts.append(f"{self.critical_count} critical")
        if self.high_count:
            parts.append(f"{self.high_count} high")
        if self.medium_count:
            parts.append(f"{self.medium_count} medium")
        if self.low_count:
            parts.append(f"{self.low_count} low")

        summary = ", ".join(parts) if parts else "no issues"
        lines = [f"Security Analysis: {summary} (tools: {', '.join(self.tools_run)})"]

        # Show top findings
        high_priority = [f for f in self.findings if f.severity >= Severity.HIGH]
        for finding in high_priority[:5]:
            sev = finding.severity.name.lower()
            loc = f"{finding.file}:{finding.line}" if finding.line else str(finding.file)
            lines.append(f"  [{sev}] {loc}: {finding.message[:60]}")

        if len(high_priority) > 5:
            lines.append(f"  ... and {len(high_priority) - 5} more high/critical findings")

        return "\n".join(lines)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "root": str(self.root),
            "summary": {
                "total": len(self.findings),
                "critical": self.critical_count,
                "high": self.high_count,
                "medium": self.medium_count,
                "low": self.low_count,
            },
            "tools_run": self.tools_run,
            "tools_skipped": self.tools_skipped,
            "errors": self.errors,
            "findings": [f.to_dict() for f in self.findings],
        }


class SecurityTool(ABC):
    """Base class for security analysis tools."""

    name: str = "unknown"

    @abstractmethod
    def is_available(self) -> bool:
        """Check if the tool is installed and available."""
        ...

    @abstractmethod
    def analyze(self, root: Path, config: SecurityToolConfig | None = None) -> list[Finding]:
        """Run the tool and return findings.

        Args:
            root: Project root directory
            config: Tool-specific configuration (optional)
        """
        ...


class BanditTool(SecurityTool):
    """Bandit - Python security linter."""

    name = "bandit"

    def is_available(self) -> bool:
        return shutil.which("bandit") is not None

    def analyze(self, root: Path, config: SecurityToolConfig | None = None) -> list[Finding]:
        """Run bandit on Python files."""
        config = config or SecurityToolConfig()
        findings = []

        # Build exclude list from config
        excludes = ",".join([*config.excludes, "__pycache__", "dist", "build"])

        try:
            cmd = [
                "bandit",
                "-r",
                str(root),
                "-f",
                "json",
                "-q",  # quiet, don't print to stderr
                "--exclude",
                excludes,
                *config.args,
            ]

            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=config.timeout,
            )

            if result.stdout:
                data = json.loads(result.stdout)
                for item in data.get("results", []):
                    findings.append(
                        Finding(
                            tool=self.name,
                            rule_id=item.get("test_id", "unknown"),
                            message=item.get("issue_text", ""),
                            severity=Severity.from_string(item.get("issue_severity", "medium")),
                            file_path=item.get("filename", ""),
                            line_start=item.get("line_number", 0),
                            line_end=item.get("end_col_offset"),
                            cwe=self._get_cwe(item.get("test_id")),
                            confidence=item.get("issue_confidence", "").lower(),
                        )
                    )

        except subprocess.TimeoutExpired:
            logger.warning("Bandit timed out")
        except json.JSONDecodeError as e:
            logger.warning("Failed to parse bandit output: %s", e)
        except (OSError, subprocess.SubprocessError) as e:
            logger.warning("Bandit failed: %s", e)

        return findings

    def _get_cwe(self, test_id: str) -> str | None:
        """Map bandit test IDs to CWE numbers."""
        # Common mappings
        cwe_map = {
            "B101": "CWE-703",  # assert used
            "B102": "CWE-78",  # exec used
            "B103": "CWE-732",  # chmod
            "B104": "CWE-400",  # bind all interfaces
            "B105": "CWE-259",  # hardcoded password
            "B106": "CWE-259",  # hardcoded password
            "B107": "CWE-259",  # hardcoded password
            "B108": "CWE-377",  # insecure temp file
            "B110": "CWE-703",  # try/except pass
            "B112": "CWE-703",  # try/except continue
            "B201": "CWE-502",  # flask debug
            "B301": "CWE-502",  # pickle
            "B302": "CWE-502",  # marshal
            "B303": "CWE-327",  # md5/sha1
            "B304": "CWE-327",  # insecure cipher
            "B305": "CWE-327",  # insecure cipher mode
            "B306": "CWE-295",  # mktemp
            "B307": "CWE-78",  # eval
            "B308": "CWE-79",  # mark_safe
            "B309": "CWE-295",  # httpsconnection
            "B310": "CWE-330",  # urllib urlopen
            "B311": "CWE-330",  # random
            "B312": "CWE-295",  # telnetlib
            "B313": "CWE-611",  # xml parsing
            "B314": "CWE-611",  # xml parsing
            "B315": "CWE-611",  # xml parsing
            "B316": "CWE-611",  # xml parsing
            "B317": "CWE-611",  # xml parsing
            "B318": "CWE-611",  # xml parsing
            "B319": "CWE-611",  # xml parsing
            "B320": "CWE-611",  # xml parsing
            "B321": "CWE-295",  # ftplib
            "B323": "CWE-295",  # ssl unverified
            "B324": "CWE-327",  # hashlib insecure
            "B501": "CWE-295",  # requests no verify
            "B502": "CWE-295",  # ssl no verify
            "B503": "CWE-295",  # ssl bad version
            "B504": "CWE-295",  # ssl bad cipher
            "B505": "CWE-327",  # weak cryptographic key
            "B506": "CWE-94",  # yaml load
            "B507": "CWE-295",  # ssh no host key verify
            "B601": "CWE-78",  # paramiko exec
            "B602": "CWE-78",  # subprocess shell
            "B603": "CWE-78",  # subprocess no shell
            "B604": "CWE-78",  # any other function
            "B605": "CWE-78",  # os.system
            "B606": "CWE-78",  # os.popen
            "B607": "CWE-78",  # partial path
            "B608": "CWE-89",  # sql injection
            "B609": "CWE-78",  # wildcard injection
            "B610": "CWE-94",  # django extra
            "B611": "CWE-94",  # django raw
            "B701": "CWE-94",  # jinja2 autoescape
            "B702": "CWE-79",  # mako templates
            "B703": "CWE-79",  # django xss
        }
        return cwe_map.get(test_id)


class SemgrepTool(SecurityTool):
    """Semgrep - SAST pattern matching."""

    name = "semgrep"

    def is_available(self) -> bool:
        return shutil.which("semgrep") is not None

    def analyze(self, root: Path, config: SecurityToolConfig | None = None) -> list[Finding]:
        """Run semgrep with security rules."""
        config = config or SecurityToolConfig()
        findings = []

        # Build command with excludes from config
        cmd = [
            "semgrep",
            "--config",
            config.config or "auto",  # Use configured ruleset or auto-detect
            "--json",
            "--quiet",
        ]

        # Add excludes from config
        for exclude in config.excludes:
            cmd.extend(["--exclude", exclude])

        # Add any extra args
        cmd.extend(config.args)
        cmd.append(str(root))

        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=config.timeout,
            )

            if result.stdout:
                data = json.loads(result.stdout)
                for item in data.get("results", []):
                    extra = item.get("extra", {})
                    metadata = extra.get("metadata", {})

                    findings.append(
                        Finding(
                            tool=self.name,
                            rule_id=item.get("check_id", "unknown"),
                            message=extra.get("message", ""),
                            severity=Severity.from_string(extra.get("severity", "medium")),
                            file_path=item.get("path", ""),
                            line_start=item.get("start", {}).get("line", 0),
                            line_end=item.get("end", {}).get("line"),
                            cwe=self._extract_cwe(metadata),
                            owasp=metadata.get("owasp"),
                            fix_suggestion=extra.get("fix"),
                            confidence=metadata.get("confidence", "").lower(),
                        )
                    )

        except subprocess.TimeoutExpired:
            logger.warning("Semgrep timed out")
        except json.JSONDecodeError as e:
            logger.warning("Failed to parse semgrep output: %s", e)
        except (OSError, subprocess.SubprocessError) as e:
            logger.warning("Semgrep failed: %s", e)

        return findings

    def _extract_cwe(self, metadata: dict) -> str | None:
        """Extract CWE from semgrep metadata."""
        cwe = metadata.get("cwe")
        if isinstance(cwe, list) and cwe:
            return cwe[0]
        if isinstance(cwe, str):
            return cwe
        return None


# Registry of available tools
SECURITY_TOOLS: list[SecurityTool] = [
    BanditTool(),
    SemgrepTool(),
]


class SecurityAnalyzer:
    """Orchestrates multiple security analysis tools."""

    def __init__(
        self,
        root: Path,
        tools: list[str] | None = None,
        min_severity: Severity = Severity.LOW,
        config: SecurityConfig | None = None,
    ):
        """Initialize the analyzer.

        Args:
            root: Project root directory
            tools: List of tool names to use (None = all available)
            min_severity: Minimum severity to report
            config: Security configuration (loads from file if None)
        """
        self.root = Path(root).resolve()
        self.requested_tools = tools
        self.min_severity = min_severity
        self.config = config or SecurityConfig.load(self.root)

        # Override min_severity from config if not explicitly set
        if min_severity == Severity.LOW and self.config.min_severity:
            self.min_severity = Severity.from_string(self.config.min_severity)

    def analyze(self, dedupe: bool = True) -> SecurityAnalysis:
        """Run all available security tools.

        Args:
            dedupe: Remove duplicate findings at same location

        Returns:
            SecurityAnalysis with aggregated findings
        """
        result = SecurityAnalysis(root=self.root)

        for tool in SECURITY_TOOLS:
            # Skip if not requested via CLI
            if self.requested_tools and tool.name not in self.requested_tools:
                continue

            # Check config for enabled status
            tool_config = self.config.get_tool_config(tool.name)
            if tool.name in self.config.tools and not tool_config.enabled:
                result.tools_skipped.append(f"{tool.name} (disabled in config)")
                continue

            if not tool.is_available():
                result.tools_skipped.append(f"{tool.name} (not installed)")
                logger.debug("Skipping %s (not installed)", tool.name)
                continue

            logger.info("Running %s...", tool.name)
            try:
                findings = tool.analyze(self.root, tool_config)
                result.findings.extend(findings)
                result.tools_run.append(tool.name)
                logger.info("%s found %d issues", tool.name, len(findings))
            except (OSError, subprocess.SubprocessError, json.JSONDecodeError) as e:
                result.errors.append(f"{tool.name}: {e}")
                logger.error("%s failed: %s", tool.name, e)

        # Filter by severity
        result.findings = [f for f in result.findings if f.severity >= self.min_severity]

        # Sort by severity (critical first), then file, then line
        result.findings.sort(key=lambda f: (-f.severity, f.file_path, f.line_start))

        if dedupe:
            result = result.dedupe()

        return result


def format_security_analysis(analysis: SecurityAnalysis) -> str:
    """Format security analysis as markdown."""
    lines = ["## Security Analysis", ""]

    # Summary
    total = len(analysis.findings)
    if total == 0 and not analysis.tools_run:
        lines.append("No security tools available. Install bandit or semgrep:")
        lines.append("  pip install bandit")
        lines.append("  pip install semgrep")
        return "\n".join(lines)

    lines.append(f"**Tools run:** {', '.join(analysis.tools_run) or 'none'}")
    if analysis.tools_skipped:
        lines.append(f"**Tools skipped:** {', '.join(analysis.tools_skipped)} (not installed)")
    lines.append("")

    if analysis.errors:
        lines.append("**Errors:**")
        for err in analysis.errors:
            lines.append(f"  - {err}")
        lines.append("")

    # Counts
    if total == 0:
        lines.append("No security issues found.")
        return "\n".join(lines)

    lines.append("### Summary")
    lines.append(f"- Critical: {analysis.critical_count}")
    lines.append(f"- High: {analysis.high_count}")
    lines.append(f"- Medium: {analysis.medium_count}")
    lines.append(f"- Low: {analysis.low_count}")
    lines.append(f"- **Total: {total}**")
    lines.append("")

    # Group by severity
    lines.append("### Findings")
    lines.append("")

    current_severity = None
    for finding in analysis.findings:
        if finding.severity != current_severity:
            current_severity = finding.severity
            lines.append(f"#### {current_severity.name}")
            lines.append("")

        cwe_info = f" ({finding.cwe})" if finding.cwe else ""
        lines.append(f"**{finding.rule_id}**{cwe_info} - {finding.tool}")
        lines.append(f"  `{finding.file_path}:{finding.line_start}`")
        lines.append(f"  {finding.message}")
        if finding.fix_suggestion:
            lines.append(f"  *Fix:* {finding.fix_suggestion}")
        lines.append("")

    return "\n".join(lines)


def analyze_security(
    root: Path | str,
    tools: list[str] | None = None,
    min_severity: str = "low",
) -> SecurityAnalysis:
    """Convenience function to run security analysis.

    Args:
        root: Project root directory
        tools: List of tool names (None = all available)
        min_severity: Minimum severity ("low", "medium", "high", "critical")

    Returns:
        SecurityAnalysis with findings
    """
    analyzer = SecurityAnalyzer(
        Path(root),
        tools=tools,
        min_severity=Severity.from_string(min_severity),
    )
    return analyzer.analyze()
