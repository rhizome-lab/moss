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
from typing import Any, ClassVar

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
class License:
    """License information for a package."""

    package: str
    license: str  # SPDX identifier or license text
    license_category: str = ""  # permissive, copyleft, proprietary, unknown

    def to_dict(self) -> dict[str, Any]:
        return {
            "package": self.package,
            "license": self.license,
            "license_category": self.license_category,
        }


@dataclass
class LicenseIssue:
    """A license compatibility issue."""

    package: str
    license: str
    issue: str  # Description of the issue
    severity: str = "WARNING"  # WARNING or ERROR

    def to_dict(self) -> dict[str, Any]:
        return {
            "package": self.package,
            "license": self.license,
            "issue": self.issue,
            "severity": self.severity,
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

    # License information
    licenses: list[License] = field(default_factory=list)
    license_issues: list[LicenseIssue] = field(default_factory=list)

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

    @property
    def has_license_issues(self) -> bool:
        return bool(self.license_issues)

    @property
    def copyleft_licenses(self) -> list[License]:
        return [lic for lic in self.licenses if lic.license_category == "copyleft"]

    @property
    def unknown_licenses(self) -> list[License]:
        return [lic for lic in self.licenses if lic.license_category == "unknown"]

    def to_compact(self) -> str:
        """Format result as compact single-line summary (token-efficient).

        Example: deps: 5 direct, 2 dev, 12 trans | vulns: 1 HIGH | licenses: 2 issues
        """
        parts = []

        # Dependencies
        deps_parts = []
        if self.total_direct:
            deps_parts.append(f"{self.total_direct} direct")
        if self.total_dev:
            deps_parts.append(f"{self.total_dev} dev")
        if self.total_transitive:
            deps_parts.append(f"{self.total_transitive} trans")
        if deps_parts:
            parts.append(f"deps: {', '.join(deps_parts)}")
        else:
            parts.append("deps: 0")

        # Vulnerabilities
        if self.vulnerabilities:
            crit = len(self.critical_vulns)
            high = len(self.high_vulns)
            if crit:
                parts.append(f"vulns: {crit} CRIT")
            elif high:
                parts.append(f"vulns: {high} HIGH")
            else:
                parts.append(f"vulns: {len(self.vulnerabilities)}")
        else:
            parts.append("vulns: 0")

        # Licenses
        if self.license_issues:
            parts.append(f"licenses: {len(self.license_issues)} issues")
        elif self.licenses:
            parts.append("licenses: ok")

        return " | ".join(parts)

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
                "license_issues": len(self.license_issues),
                "copyleft_licenses": len(self.copyleft_licenses),
                "unknown_licenses": len(self.unknown_licenses),
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
            "licenses": [lic.to_dict() for lic in self.licenses],
            "license_issues": [issue.to_dict() for issue in self.license_issues],
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
        if self.license_issues:
            lines.append(f"- **License issues:** {len(self.license_issues)} found")
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

        # License issues
        if self.license_issues:
            lines.append("## License Issues")
            lines.append("")
            lines.append("| Severity | Package | License | Issue |")
            lines.append("|----------|---------|---------|-------|")
            for issue in self.license_issues:
                lines.append(
                    f"| {issue.severity} | {issue.package} | {issue.license} | {issue.issue} |"
                )
            lines.append("")

        # License summary (if licenses were checked)
        if self.licenses:
            copyleft = self.copyleft_licenses
            unknown = self.unknown_licenses
            if copyleft or unknown:
                lines.append("## License Summary")
                lines.append("")
                if copyleft:
                    lines.append(f"**{len(copyleft)} copyleft** licenses found:")
                    lines.append("")
                    for lic in copyleft:
                        lines.append(f"- {lic.package}: {lic.license}")
                    lines.append("")
                if unknown:
                    lines.append(f"**{len(unknown)} unknown** licenses found:")
                    lines.append("")
                    for lic in unknown:
                        lines.append(f"- {lic.package}: {lic.license}")
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
        self,
        *,
        resolve: bool = False,
        check_vulns: bool = False,
        check_licenses: bool = False,
    ) -> DependencyAnalysisResult:
        """Analyze project dependencies.

        Args:
            resolve: If True, resolve full transitive dependency tree
            check_vulns: If True, check for known vulnerabilities via OSV API
            check_licenses: If True, check license compatibility

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

        # Try package.json (npm/Node.js)
        package_json = self.root / "package.json"
        if package_json.exists():
            self._parse_package_json(package_json, result)
            result.sources.append("package.json")

        # Resolve transitive dependencies if requested
        if resolve:
            result.resolved_tree = self._resolve_dependencies(result.dependencies)

        # Check for vulnerabilities if requested
        if check_vulns:
            result.vulnerabilities = self._check_vulnerabilities(result.dependencies)

        # Check licenses if requested
        if check_licenses:
            licenses, issues = self._check_licenses(result.dependencies)
            result.licenses = licenses
            result.license_issues = issues

        return result

    def _parse_pyproject(self, path: Path, result: DependencyAnalysisResult) -> None:
        """Parse pyproject.toml for dependencies."""
        if tomllib is None:
            return

        try:
            content = path.read_text()
            data = tomllib.loads(content)
        except (OSError, tomllib.TOMLDecodeError):
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
        except OSError:
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

    def _parse_package_json(self, path: Path, result: DependencyAnalysisResult) -> None:
        """Parse package.json for npm dependencies."""
        import json

        try:
            content = path.read_text()
            data = json.loads(content)
        except (OSError, json.JSONDecodeError):
            return

        # Regular dependencies
        deps = data.get("dependencies", {})
        for name, version_spec in deps.items():
            dep = self._parse_npm_dependency(name, version_spec)
            if dep:
                result.dependencies.append(dep)

        # Dev dependencies
        dev_deps = data.get("devDependencies", {})
        for name, version_spec in dev_deps.items():
            dep = self._parse_npm_dependency(name, version_spec, is_dev=True)
            if dep:
                result.dev_dependencies.append(dep)

        # Optional dependencies
        optional_deps = data.get("optionalDependencies", {})
        if optional_deps:
            opt_list = []
            for name, version_spec in optional_deps.items():
                dep = self._parse_npm_dependency(name, version_spec)
                if dep:
                    dep.is_optional = True
                    dep.optional_group = "optional"
                    opt_list.append(dep)
            if opt_list:
                result.optional_dependencies["optional"] = opt_list

        # Peer dependencies (treated as optional)
        peer_deps = data.get("peerDependencies", {})
        if peer_deps:
            peer_list = []
            for name, version_spec in peer_deps.items():
                dep = self._parse_npm_dependency(name, version_spec)
                if dep:
                    dep.is_optional = True
                    dep.optional_group = "peer"
                    peer_list.append(dep)
            if peer_list:
                result.optional_dependencies["peer"] = peer_list

    def _parse_npm_dependency(
        self, name: str, version_spec: str, *, is_dev: bool = False
    ) -> Dependency | None:
        """Parse an npm dependency name and version spec."""
        if not name:
            return None

        # Clean up version spec
        # npm uses: ^1.0.0, ~1.0.0, >=1.0.0, 1.0.0, *, latest, git urls, etc.
        version_spec = version_spec.strip()

        # Skip git/url/file dependencies for now
        if version_spec.startswith(("git:", "git+", "http:", "https:", "file:")):
            version_spec = "(git/url)"

        return Dependency(name=name, version_spec=version_spec, is_dev=is_dev)

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
            except (OSError, subprocess.SubprocessError):
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

            except (
                urllib.error.URLError,
                TimeoutError,
                json.JSONDecodeError,
                ValueError,
                KeyError,
            ):
                continue

        return vulnerabilities

    # License categories based on common SPDX identifiers
    PERMISSIVE_LICENSES: ClassVar[set[str]] = {
        "mit",
        "apache-2.0",
        "apache 2.0",
        "bsd-2-clause",
        "bsd-3-clause",
        "bsd",
        "isc",
        "unlicense",
        "cc0-1.0",
        "wtfpl",
        "zlib",
        "public domain",
        "psf-2.0",
        "python-2.0",
    }

    COPYLEFT_LICENSES: ClassVar[set[str]] = {
        "gpl-2.0",
        "gpl-3.0",
        "gpl-2.0-only",
        "gpl-3.0-only",
        "gpl-2.0-or-later",
        "gpl-3.0-or-later",
        "agpl-3.0",
        "agpl-3.0-only",
        "lgpl-2.1",
        "lgpl-3.0",
        "lgpl-2.1-only",
        "lgpl-3.0-only",
        "mpl-2.0",
        "eupl-1.2",
        "gpl",
        "agpl",
        "lgpl",
    }

    def _check_licenses(
        self, dependencies: list[Dependency]
    ) -> tuple[list[License], list[LicenseIssue]]:
        """Check dependencies for license information and compatibility.

        Returns tuple of (licenses, issues).
        """
        licenses = []
        issues = []

        for dep in dependencies:
            try:
                # Get license info from pip
                result = subprocess.run(
                    ["pip", "show", dep.name],
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                if result.returncode != 0:
                    continue

                license_str = ""
                for line in result.stdout.splitlines():
                    if line.startswith("License:"):
                        license_str = line.split(":", 1)[1].strip()
                        break

                if not license_str or license_str.lower() == "unknown":
                    licenses.append(
                        License(
                            package=dep.name,
                            license="Unknown",
                            license_category="unknown",
                        )
                    )
                    issues.append(
                        LicenseIssue(
                            package=dep.name,
                            license="Unknown",
                            issue="License not specified",
                            severity="WARNING",
                        )
                    )
                    continue

                # Categorize license
                license_lower = license_str.lower()
                category = "unknown"

                # Check permissive
                for perm in self.PERMISSIVE_LICENSES:
                    if perm in license_lower:
                        category = "permissive"
                        break

                # Check copyleft
                if category == "unknown":
                    for copy in self.COPYLEFT_LICENSES:
                        if copy in license_lower:
                            category = "copyleft"
                            # Add warning for copyleft in non-dev deps
                            if not dep.is_dev:
                                issues.append(
                                    LicenseIssue(
                                        package=dep.name,
                                        license=license_str,
                                        issue="Copyleft license may require source disclosure",
                                        severity="WARNING",
                                    )
                                )
                            break

                licenses.append(
                    License(
                        package=dep.name,
                        license=license_str,
                        license_category=category,
                    )
                )

            except (OSError, subprocess.SubprocessError):
                continue

        return licenses, issues


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
