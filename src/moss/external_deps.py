"""External dependency analysis for Python projects.

Analyzes PyPI dependencies (not just internal imports):
- Parse pyproject.toml/requirements.txt/setup.py for dependencies
- Resolve full dependency tree (transitive dependencies)
- Show dependency weight (how many sub-dependencies each brings)
- Identify heavy/bloated dependencies
"""

from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

# Try to import tomllib (Python 3.11+) or tomli
try:
    import tomllib
except ImportError:
    try:
        import tomli as tomllib  # type: ignore[import-not-found]
    except ImportError:
        tomllib = None  # type: ignore[assignment]


@dataclass
class Dependency:
    """A single dependency with its constraints."""

    name: str
    version_spec: str = ""
    extras: list[str] = field(default_factory=list)
    is_dev: bool = False
    is_optional: bool = False
    optional_group: str = ""

    @property
    def normalized_name(self) -> str:
        """PEP 503 normalized name."""
        return re.sub(r"[-_.]+", "-", self.name).lower()

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "normalized_name": self.normalized_name,
            "version_spec": self.version_spec,
            "extras": self.extras,
            "is_dev": self.is_dev,
            "is_optional": self.is_optional,
            "optional_group": self.optional_group,
        }


@dataclass
class ResolvedDependency:
    """A dependency with its resolved transitive dependencies."""

    name: str
    version: str
    dependencies: list[ResolvedDependency] = field(default_factory=list)
    is_direct: bool = True

    @property
    def weight(self) -> int:
        """Total number of transitive dependencies (including self)."""
        return 1 + sum(d.weight for d in self.dependencies)

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "version": self.version,
            "weight": self.weight,
            "is_direct": self.is_direct,
            "dependencies": [d.to_dict() for d in self.dependencies],
        }


@dataclass
class Vulnerability:
    """A security vulnerability affecting a package."""

    id: str  # CVE or GHSA ID
    package: str
    severity: str  # LOW, MEDIUM, HIGH, CRITICAL
    summary: str
    affected_versions: str = ""
    fixed_version: str = ""
    url: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "package": self.package,
            "severity": self.severity,
            "summary": self.summary,
            "affected_versions": self.affected_versions,
            "fixed_version": self.fixed_version,
            "url": self.url,
        }


@dataclass
class DependencyAnalysisResult:
    """Result of dependency analysis."""

    # Direct dependencies
    dependencies: list[Dependency] = field(default_factory=list)
    dev_dependencies: list[Dependency] = field(default_factory=list)
    optional_dependencies: dict[str, list[Dependency]] = field(default_factory=dict)

    # Resolved tree (if available)
    resolved_tree: list[ResolvedDependency] = field(default_factory=list)

    # Source files found
    sources: list[str] = field(default_factory=list)

    # Security vulnerabilities
    vulnerabilities: list[Vulnerability] = field(default_factory=list)

    @property
    def total_direct(self) -> int:
        return len(self.dependencies)

    @property
    def total_dev(self) -> int:
        return len(self.dev_dependencies)

    @property
    def total_optional(self) -> int:
        return sum(len(deps) for deps in self.optional_dependencies.values())

    @property
    def total_transitive(self) -> int:
        if not self.resolved_tree:
            return 0
        return sum(d.weight for d in self.resolved_tree) - len(self.resolved_tree)

    @property
    def heaviest_dependencies(self) -> list[ResolvedDependency]:
        """Dependencies sorted by weight (heaviest first)."""
        return sorted(self.resolved_tree, key=lambda d: -d.weight)

    def get_heavy_dependencies(self, threshold: int = 10) -> list[ResolvedDependency]:
        """Get dependencies exceeding weight threshold."""
        return [d for d in self.resolved_tree if d.weight >= threshold]

    @property
    def has_vulnerabilities(self) -> bool:
        return bool(self.vulnerabilities)

    @property
    def critical_vulns(self) -> list[Vulnerability]:
        return [v for v in self.vulnerabilities if v.severity == "CRITICAL"]

    @property
    def high_vulns(self) -> list[Vulnerability]:
        return [v for v in self.vulnerabilities if v.severity == "HIGH"]

    def to_dict(self, *, weight_threshold: int = 0) -> dict[str, Any]:
        heavy = self.get_heavy_dependencies(weight_threshold) if weight_threshold > 0 else []
        return {
            "stats": {
                "direct": self.total_direct,
                "dev": self.total_dev,
                "optional": self.total_optional,
                "transitive": self.total_transitive,
                "heavy_count": len(heavy),
                "vulnerabilities": len(self.vulnerabilities),
                "critical_vulns": len(self.critical_vulns),
                "high_vulns": len(self.high_vulns),
            },
            "sources": self.sources,
            "dependencies": [d.to_dict() for d in self.dependencies],
            "dev_dependencies": [d.to_dict() for d in self.dev_dependencies],
            "optional_dependencies": {
                group: [d.to_dict() for d in deps]
                for group, deps in self.optional_dependencies.items()
            },
            "resolved_tree": [d.to_dict() for d in self.resolved_tree],
            "heavy_dependencies": [d.to_dict() for d in heavy],
            "vulnerabilities": [v.to_dict() for v in self.vulnerabilities],
        }

    def to_markdown(self, *, weight_threshold: int = 0) -> str:
        """Format result as markdown.

        Args:
            weight_threshold: If >0, warn about deps with weight >= threshold
        """
        lines = ["# External Dependency Analysis", ""]

        # Stats
        lines.append("## Summary")
        lines.append("")
        lines.append(f"- **Direct dependencies:** {self.total_direct}")
        lines.append(f"- **Dev dependencies:** {self.total_dev}")
        lines.append(f"- **Optional dependencies:** {self.total_optional}")
        if self.resolved_tree:
            lines.append(f"- **Transitive dependencies:** {self.total_transitive}")
        lines.append(f"- **Sources:** {', '.join(self.sources)}")
        if self.vulnerabilities:
            lines.append(f"- **Vulnerabilities:** {len(self.vulnerabilities)} found")
        lines.append("")

        # Security vulnerabilities
        if self.vulnerabilities:
            lines.append("## Security Vulnerabilities")
            lines.append("")
            critical = self.critical_vulns
            high = self.high_vulns
            if critical:
                lines.append(f"**{len(critical)} CRITICAL** vulnerabilities found!")
                lines.append("")
            if high:
                lines.append(f"**{len(high)} HIGH** severity vulnerabilities found.")
                lines.append("")
            lines.append("| Severity | Package | ID | Summary |")
            lines.append("|----------|---------|-----|---------|")
            # Sort by severity (CRITICAL first)
            severity_order = {"CRITICAL": 0, "HIGH": 1, "MEDIUM": 2, "LOW": 3}
            for vuln in sorted(
                self.vulnerabilities, key=lambda v: severity_order.get(v.severity, 4)
            ):
                summary = vuln.summary[:60] + "..." if len(vuln.summary) > 60 else vuln.summary
                lines.append(f"| {vuln.severity} | {vuln.package} | {vuln.id} | {summary} |")
            lines.append("")

        # Heavy dependency warnings
        if weight_threshold > 0 and self.resolved_tree:
            heavy = self.get_heavy_dependencies(weight_threshold)
            if heavy:
                lines.append("## Heavy Dependencies Warning")
                lines.append("")
                lines.append(
                    f"The following {len(heavy)} dependencies exceed the weight "
                    f"threshold of {weight_threshold}:"
                )
                lines.append("")
                lines.append("| Package | Weight | Concern |")
                lines.append("|---------|--------|---------|")
                for dep in sorted(heavy, key=lambda d: -d.weight):
                    concern = "Very heavy" if dep.weight >= weight_threshold * 2 else "Heavy"
                    lines.append(f"| {dep.name} | {dep.weight} | {concern} |")
                lines.append("")
                lines.append(
                    "Consider if these dependencies are necessary, "
                    "or look for lighter alternatives."
                )
                lines.append("")

        # Direct dependencies
        if self.dependencies:
            lines.append("## Direct Dependencies")
            lines.append("")
            lines.append("| Package | Version Spec |")
            lines.append("|---------|--------------|")
            for dep in sorted(self.dependencies, key=lambda d: d.name.lower()):
                spec = dep.version_spec or "*"
                lines.append(f"| {dep.name} | {spec} |")
            lines.append("")

        # Dev dependencies
        if self.dev_dependencies:
            lines.append("## Dev Dependencies")
            lines.append("")
            lines.append("| Package | Version Spec |")
            lines.append("|---------|--------------|")
            for dep in sorted(self.dev_dependencies, key=lambda d: d.name.lower()):
                spec = dep.version_spec or "*"
                lines.append(f"| {dep.name} | {spec} |")
            lines.append("")

        # Optional dependencies
        if self.optional_dependencies:
            lines.append("## Optional Dependencies")
            lines.append("")
            for group, deps in sorted(self.optional_dependencies.items()):
                lines.append(f"### [{group}]")
                lines.append("")
                for dep in sorted(deps, key=lambda d: d.name.lower()):
                    spec = dep.version_spec or "*"
                    lines.append(f"- {dep.name} {spec}")
                lines.append("")

        # Heaviest dependencies
        if self.resolved_tree:
            lines.append("## Dependency Weight")
            lines.append("")
            lines.append("Sorted by total transitive dependencies:")
            lines.append("")
            lines.append("| Package | Version | Weight |")
            lines.append("|---------|---------|--------|")
            for dep in self.heaviest_dependencies[:15]:
                lines.append(f"| {dep.name} | {dep.version} | {dep.weight} |")
            lines.append("")

        return "\n".join(lines)


class ExternalDependencyAnalyzer:
    """Analyze external dependencies for a Python project."""

    def __init__(self, root: Path):
        """Initialize analyzer.

        Args:
            root: Project root directory
        """
        self.root = root.resolve()

    def analyze(
        self, *, resolve: bool = False, check_vulns: bool = False
    ) -> DependencyAnalysisResult:
        """Analyze project dependencies.

        Args:
            resolve: If True, resolve full transitive dependency tree
            check_vulns: If True, check for known vulnerabilities via OSV API

        Returns:
            DependencyAnalysisResult with all dependency information
        """
        result = DependencyAnalysisResult()

        # Try pyproject.toml first
        pyproject = self.root / "pyproject.toml"
        if pyproject.exists():
            self._parse_pyproject(pyproject, result)
            result.sources.append("pyproject.toml")

        # Try requirements.txt
        requirements = self.root / "requirements.txt"
        if requirements.exists():
            self._parse_requirements(requirements, result)
            result.sources.append("requirements.txt")

        # Try requirements-dev.txt
        requirements_dev = self.root / "requirements-dev.txt"
        if requirements_dev.exists():
            self._parse_requirements(requirements_dev, result, is_dev=True)
            result.sources.append("requirements-dev.txt")

        # Resolve transitive dependencies if requested
        if resolve:
            result.resolved_tree = self._resolve_dependencies(result.dependencies)

        # Check for vulnerabilities if requested
        if check_vulns:
            result.vulnerabilities = self._check_vulnerabilities(result.dependencies)

        return result

    def _parse_pyproject(self, path: Path, result: DependencyAnalysisResult) -> None:
        """Parse pyproject.toml for dependencies."""
        if tomllib is None:
            return

        try:
            content = path.read_text()
            data = tomllib.loads(content)
        except Exception:
            return

        project = data.get("project", {})

        # Main dependencies
        deps = project.get("dependencies", [])
        for dep_str in deps:
            dep = self._parse_dependency_string(dep_str)
            if dep:
                result.dependencies.append(dep)

        # Optional dependencies
        optional = project.get("optional-dependencies", {})
        for group, deps_list in optional.items():
            group_deps = []
            for dep_str in deps_list:
                dep = self._parse_dependency_string(dep_str)
                if dep:
                    dep.is_optional = True
                    dep.optional_group = group
                    # Check if it's a dev-like group
                    if group.lower() in ("dev", "test", "testing", "development"):
                        dep.is_dev = True
                        result.dev_dependencies.append(dep)
                    else:
                        group_deps.append(dep)
            if group_deps:
                result.optional_dependencies[group] = group_deps

    def _parse_requirements(
        self, path: Path, result: DependencyAnalysisResult, *, is_dev: bool = False
    ) -> None:
        """Parse requirements.txt style file."""
        try:
            content = path.read_text()
        except Exception:
            return

        for line in content.splitlines():
            line = line.strip()
            # Skip comments and empty lines
            if not line or line.startswith("#"):
                continue
            # Skip -r includes for now
            if line.startswith("-r") or line.startswith("-e"):
                continue

            dep = self._parse_dependency_string(line)
            if dep:
                dep.is_dev = is_dev
                if is_dev:
                    result.dev_dependencies.append(dep)
                else:
                    result.dependencies.append(dep)

    def _parse_dependency_string(self, dep_str: str) -> Dependency | None:
        """Parse a dependency string like 'requests>=2.0,<3.0' or 'package[extra1,extra2]'."""
        dep_str = dep_str.strip()
        if not dep_str:
            return None

        # Handle extras: package[extra1,extra2]
        extras: list[str] = []
        if "[" in dep_str:
            match = re.match(r"([^[]+)\[([^\]]+)\](.*)", dep_str)
            if match:
                name_part = match.group(1)
                extras = [e.strip() for e in match.group(2).split(",")]
                version_part = match.group(3)
                dep_str = name_part + version_part
            else:
                return None

        # Split name and version spec
        # Handles: package>=1.0, package==1.0, package~=1.0, package!=1.0
        match = re.match(r"([a-zA-Z0-9_-]+)(.*)", dep_str)
        if not match:
            return None

        name = match.group(1)
        version_spec = match.group(2).strip()

        # Clean up version spec (remove comments, environment markers)
        if ";" in version_spec:
            version_spec = version_spec.split(";")[0].strip()
        if "#" in version_spec:
            version_spec = version_spec.split("#")[0].strip()

        return Dependency(name=name, version_spec=version_spec, extras=extras)

    def _resolve_dependencies(self, dependencies: list[Dependency]) -> list[ResolvedDependency]:
        """Resolve transitive dependencies using pip.

        This requires pip to be available and may be slow for large projects.
        """
        resolved = []

        for dep in dependencies:
            try:
                # Use pip show to get installed package info
                result = subprocess.run(
                    ["pip", "show", dep.name],
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                if result.returncode != 0:
                    continue

                # Parse pip show output
                version = ""
                requires = []
                for line in result.stdout.splitlines():
                    if line.startswith("Version:"):
                        version = line.split(":", 1)[1].strip()
                    elif line.startswith("Requires:"):
                        req_str = line.split(":", 1)[1].strip()
                        if req_str:
                            requires = [r.strip() for r in req_str.split(",")]

                # Recursively resolve sub-dependencies (limited depth)
                sub_deps = []
                for req in requires:
                    sub_result = subprocess.run(
                        ["pip", "show", req],
                        capture_output=True,
                        text=True,
                        timeout=10,
                    )
                    if sub_result.returncode == 0:
                        sub_version = ""
                        for line in sub_result.stdout.splitlines():
                            if line.startswith("Version:"):
                                sub_version = line.split(":", 1)[1].strip()
                                break
                        sub_deps.append(
                            ResolvedDependency(name=req, version=sub_version, is_direct=False)
                        )

                resolved.append(
                    ResolvedDependency(
                        name=dep.name,
                        version=version,
                        dependencies=sub_deps,
                        is_direct=True,
                    )
                )
            except Exception:
                continue

        return resolved

    def _check_vulnerabilities(self, dependencies: list[Dependency]) -> list[Vulnerability]:
        """Check dependencies for known vulnerabilities using OSV API.

        Uses the Open Source Vulnerabilities database (https://osv.dev).
        """
        import json
        import urllib.error
        import urllib.request

        vulnerabilities = []
        osv_api = "https://api.osv.dev/v1/query"

        for dep in dependencies:
            # Get installed version via pip
            try:
                result = subprocess.run(
                    ["pip", "show", dep.name],
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                if result.returncode != 0:
                    continue

                version = ""
                for line in result.stdout.splitlines():
                    if line.startswith("Version:"):
                        version = line.split(":", 1)[1].strip()
                        break

                if not version:
                    continue

                # Query OSV API
                query = {
                    "package": {"name": dep.normalized_name, "ecosystem": "PyPI"},
                    "version": version,
                }

                req = urllib.request.Request(
                    osv_api,
                    data=json.dumps(query).encode("utf-8"),
                    headers={"Content-Type": "application/json"},
                    method="POST",
                )

                with urllib.request.urlopen(req, timeout=10) as response:
                    data = json.loads(response.read().decode("utf-8"))

                for vuln_data in data.get("vulns", []):
                    # Determine severity
                    severity = "UNKNOWN"
                    for severity_item in vuln_data.get("severity", []):
                        if severity_item.get("type") == "CVSS_V3":
                            score = float(severity_item.get("score", "0").split("/")[0])
                            if score >= 9.0:
                                severity = "CRITICAL"
                            elif score >= 7.0:
                                severity = "HIGH"
                            elif score >= 4.0:
                                severity = "MEDIUM"
                            else:
                                severity = "LOW"
                            break
                    else:
                        # Fallback: check database-specific severity
                        db_specific = vuln_data.get("database_specific", {})
                        if db_specific.get("severity"):
                            severity = db_specific["severity"].upper()

                    # Get affected version info
                    affected_str = ""
                    fixed_str = ""
                    for affected in vuln_data.get("affected", []):
                        pkg_name = affected.get("package", {}).get("name", "").lower()
                        if pkg_name == dep.normalized_name:
                            ranges = affected.get("ranges", [])
                            for r in ranges:
                                events = r.get("events", [])
                                for event in events:
                                    if "fixed" in event:
                                        fixed_str = event["fixed"]
                                        break

                    # Get URL
                    url = ""
                    for ref in vuln_data.get("references", []):
                        if ref.get("type") == "ADVISORY":
                            url = ref.get("url", "")
                            break
                    if not url:
                        url = f"https://osv.dev/vulnerability/{vuln_data.get('id', '')}"

                    vulnerabilities.append(
                        Vulnerability(
                            id=vuln_data.get("id", "UNKNOWN"),
                            package=dep.name,
                            severity=severity,
                            summary=vuln_data.get("summary", "No summary available"),
                            affected_versions=affected_str,
                            fixed_version=fixed_str,
                            url=url,
                        )
                    )

            except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
                continue
            except Exception:
                continue

        return vulnerabilities


def create_external_dependency_analyzer(
    root: Path | None = None,
) -> ExternalDependencyAnalyzer:
    """Factory function to create an ExternalDependencyAnalyzer.

    Args:
        root: Project root (default: current directory)

    Returns:
        Configured ExternalDependencyAnalyzer instance
    """
    if root is None:
        root = Path.cwd()
    return ExternalDependencyAnalyzer(root)
