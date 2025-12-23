"""Architectural weakness and gap detection.

Identifies potential issues in codebase architecture:
- Tight coupling between components
- Missing abstractions
- Inconsistent patterns
- Technical debt indicators
- Hardcoded assumptions
- Missing error handling

Usage:
    from moss.weaknesses import WeaknessAnalyzer

    analyzer = WeaknessAnalyzer(project_root)
    result = analyzer.analyze()

    # Via CLI:
    # moss weaknesses [directory] [--category coupling,abstractions]
"""

from __future__ import annotations

import ast
import logging
import re
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


class WeaknessCategory(Enum):
    """Categories of architectural weaknesses."""

    COUPLING = "coupling"  # Tight coupling between modules
    ABSTRACTION = "abstraction"  # Missing or wrong abstractions
    PATTERN = "pattern"  # Inconsistent patterns
    HARDCODED = "hardcoded"  # Hardcoded values that should be configurable
    ERROR_HANDLING = "error_handling"  # Missing or poor error handling
    COMPLEXITY = "complexity"  # High complexity, potential for simplification
    DUPLICATION = "duplication"  # Code duplication (structural)


class Severity(Enum):
    """Severity of a weakness."""

    INFO = "info"
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"


@dataclass
class Weakness:
    """A detected architectural weakness."""

    category: WeaknessCategory
    severity: Severity
    title: str
    description: str
    file_path: str | None = None
    line_start: int | None = None
    line_end: int | None = None
    suggestion: str | None = None
    related_files: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class WeaknessAnalysis:
    """Results from weakness analysis."""

    root: Path
    weaknesses: list[Weakness] = field(default_factory=list)
    summary: dict[str, int] = field(default_factory=dict)

    @property
    def by_category(self) -> dict[WeaknessCategory, list[Weakness]]:
        """Group weaknesses by category."""
        result: dict[WeaknessCategory, list[Weakness]] = {}
        for w in self.weaknesses:
            if w.category not in result:
                result[w.category] = []
            result[w.category].append(w)
        return result

    @property
    def by_severity(self) -> dict[Severity, list[Weakness]]:
        """Group weaknesses by severity."""
        result: dict[Severity, list[Weakness]] = {}
        for w in self.weaknesses:
            if w.severity not in result:
                result[w.severity] = []
            result[w.severity].append(w)
        return result

    def to_compact(self) -> str:
        """Format as compact text for LLM consumption."""
        by_cat = self.by_category
        cat_summary = ", ".join(f"{len(ws)} {cat.value}" for cat, ws in by_cat.items())
        lines = [f"Weakness Analysis: {len(self.weaknesses)} issues ({cat_summary or 'none'})"]

        for w in self.weaknesses[:5]:
            loc = f"{w.file_path}:{w.line_start}" if w.line_start else w.file_path or "global"
            lines.append(f"  [{w.severity.value}] {loc}: {w.title}")

        if len(self.weaknesses) > 5:
            lines.append(f"  ... and {len(self.weaknesses) - 5} more")

        return "\n".join(lines)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "root": str(self.root),
            "summary": {
                "total": len(self.weaknesses),
                "by_category": {cat.value: len(ws) for cat, ws in self.by_category.items()},
                "by_severity": {sev.value: len(ws) for sev, ws in self.by_severity.items()},
            },
            "weaknesses": [
                {
                    "category": w.category.value,
                    "severity": w.severity.value,
                    "title": w.title,
                    "description": w.description,
                    "file": w.file_path,
                    "line": w.line_start,
                    "suggestion": w.suggestion,
                    "related_files": w.related_files,
                }
                for w in self.weaknesses
            ],
        }


# =============================================================================
# Detectors
# =============================================================================


class HardcodedDetector(ast.NodeVisitor):
    """Detect hardcoded values that should be configurable."""

    # Patterns that suggest hardcoded values that should be config
    URL_PATTERN = re.compile(r"https?://[^\s\"']+")
    IP_PATTERN = re.compile(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b")
    PORT_PATTERN = re.compile(r"\bport\s*[=:]\s*\d+", re.IGNORECASE)
    PATH_PATTERN = re.compile(r'["\'](?:/[^/"\']+){3,}["\']')  # Absolute paths with 3+ components

    def __init__(self, source: str, file_path: str) -> None:
        self.source = source
        self.file_path = file_path
        self.findings: list[dict[str, Any]] = []

    def visit_Constant(self, node: ast.Constant) -> None:
        if isinstance(node.value, str) and len(node.value) > 5:
            value = node.value

            # Skip docstrings and common patterns
            if value.startswith("http://localhost") or value.startswith("https://localhost"):
                pass  # Often legitimate for tests
            elif self.URL_PATTERN.search(value):
                self.findings.append(
                    {
                        "type": "hardcoded_url",
                        "value": value[:50] + "..." if len(value) > 50 else value,
                        "line": node.lineno,
                    }
                )
            elif self.IP_PATTERN.search(value) and "127.0.0.1" not in value:
                self.findings.append(
                    {
                        "type": "hardcoded_ip",
                        "value": value,
                        "line": node.lineno,
                    }
                )
            elif self.PATH_PATTERN.search(f'"{value}"') and "/home/" not in value:
                # Absolute paths that aren't home dirs
                self.findings.append(
                    {
                        "type": "hardcoded_path",
                        "value": value,
                        "line": node.lineno,
                    }
                )

        self.generic_visit(node)


class ErrorHandlingDetector(ast.NodeVisitor):
    """Detect missing or poor error handling patterns."""

    def __init__(self, source: str, file_path: str) -> None:
        self.source = source
        self.file_path = file_path
        self.findings: list[dict[str, Any]] = []

    def visit_ExceptHandler(self, node: ast.ExceptHandler) -> None:
        # Check for bare except or except Exception with pass
        if node.type is None:
            self.findings.append(
                {
                    "type": "bare_except",
                    "line": node.lineno,
                    "description": "Bare except catches all exceptions including KeyboardInterrupt",
                }
            )
        elif isinstance(node.type, ast.Name) and node.type.id == "Exception":
            # Check if body is just pass
            if len(node.body) == 1 and isinstance(node.body[0], ast.Pass):
                self.findings.append(
                    {
                        "type": "swallowed_exception",
                        "line": node.lineno,
                        "description": "Exception caught and silently ignored",
                    }
                )

        self.generic_visit(node)


class AbstractionDetector(ast.NodeVisitor):
    """Detect missing abstraction opportunities."""

    def __init__(self, source: str, file_path: str) -> None:
        self.source = source
        self.file_path = file_path
        self.findings: list[dict[str, Any]] = []
        self._similar_functions: dict[str, list[dict[str, Any]]] = {}

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        # Long parameter lists suggest missing abstraction
        if len(node.args.args) > 7:
            self.findings.append(
                {
                    "type": "long_param_list",
                    "name": node.name,
                    "param_count": len(node.args.args),
                    "line": node.lineno,
                }
            )

        # Very long functions
        if node.end_lineno and (node.end_lineno - node.lineno) > 100:
            self.findings.append(
                {
                    "type": "long_function",
                    "name": node.name,
                    "lines": node.end_lineno - node.lineno,
                    "line": node.lineno,
                }
            )

        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        # Same checks for async functions
        if len(node.args.args) > 7:
            self.findings.append(
                {
                    "type": "long_param_list",
                    "name": node.name,
                    "param_count": len(node.args.args),
                    "line": node.lineno,
                }
            )

        if node.end_lineno and (node.end_lineno - node.lineno) > 100:
            self.findings.append(
                {
                    "type": "long_function",
                    "name": node.name,
                    "lines": node.end_lineno - node.lineno,
                    "line": node.lineno,
                }
            )

        self.generic_visit(node)

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        # God classes - too many methods
        method_count = sum(
            1 for item in node.body if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef))
        )

        if method_count > 20:
            self.findings.append(
                {
                    "type": "god_class",
                    "name": node.name,
                    "method_count": method_count,
                    "line": node.lineno,
                }
            )

        self.generic_visit(node)


# =============================================================================
# Main Analyzer
# =============================================================================


class WeaknessAnalyzer:
    """Analyzes a codebase for architectural weaknesses."""

    def __init__(
        self,
        root: Path,
        categories: list[str] | None = None,
    ) -> None:
        """Initialize the analyzer.

        Args:
            root: Project root directory
            categories: Categories to check (None = all)
        """
        self.root = Path(root).resolve()
        all_categories = [c.value for c in WeaknessCategory]
        self.categories = categories or all_categories

    def analyze(self) -> WeaknessAnalysis:
        """Run weakness analysis on the codebase."""
        result = WeaknessAnalysis(root=self.root)

        # Find all Python files
        python_files = list(self.root.rglob("*.py"))
        exclude_parts = [".venv", "venv", "node_modules", ".git", "__pycache__", "dist", "build"]
        python_files = [
            f for f in python_files if not any(part in str(f) for part in exclude_parts)
        ]

        # Analyze coupling if requested
        if "coupling" in self.categories:
            self._analyze_coupling(result, python_files)

        # Analyze individual files
        for file_path in python_files:
            try:
                source = file_path.read_text()
                tree = ast.parse(source)
                rel_path = str(file_path.relative_to(self.root))

                if "hardcoded" in self.categories:
                    self._analyze_hardcoded(result, source, rel_path, tree)

                if "error_handling" in self.categories:
                    self._analyze_error_handling(result, source, rel_path, tree)

                if "abstraction" in self.categories:
                    self._analyze_abstractions(result, source, rel_path, tree)

            except (OSError, UnicodeDecodeError, SyntaxError) as e:
                logger.debug("Failed to analyze %s: %s", file_path, e)

        # Analyze patterns if requested
        if "pattern" in self.categories:
            self._analyze_patterns(result)

        return result

    def _analyze_coupling(self, result: WeaknessAnalysis, python_files: list[Path]) -> None:
        """Analyze module coupling."""
        from moss.patterns import CouplingAnalyzer

        # Build import graph
        imports_from: dict[str, list[str]] = {}
        imported_by: dict[str, list[str]] = {}

        for file_path in python_files:
            try:
                source = file_path.read_text()
                tree = ast.parse(source)
                rel_path = str(file_path.relative_to(self.root))
                module_name = rel_path.replace("/", ".").replace(".py", "")

                analyzer = CouplingAnalyzer(source, module_name)
                analyzer.visit(tree)

                imports_from[module_name] = analyzer.imports

                for imported in analyzer.imports:
                    if imported not in imported_by:
                        imported_by[imported] = []
                    imported_by[imported].append(module_name)

            except (OSError, UnicodeDecodeError, SyntaxError):
                pass

        # Detect coupling issues
        stdlib_prefixes = (
            "os",
            "sys",
            "re",
            "json",
            "typing",
            "collections",
            "dataclasses",
            "pathlib",
            "logging",
            "enum",
            "abc",
            "functools",
            "itertools",
            "datetime",
            "asyncio",
            "contextlib",
        )
        for module, imports in imports_from.items():
            # High fan-out: imports too many modules
            internal_imports = [i for i in imports if not i.startswith(stdlib_prefixes)]
            if len(internal_imports) > 15:
                result.weaknesses.append(
                    Weakness(
                        category=WeaknessCategory.COUPLING,
                        severity=Severity.MEDIUM,
                        title=f"High fan-out: {module}",
                        description=(
                            f"Module imports {len(internal_imports)} other modules, "
                            "suggesting it may be doing too much"
                        ),
                        file_path=module.replace(".", "/") + ".py",
                        suggestion="Consider splitting into smaller, focused modules",
                        metadata={"import_count": len(internal_imports)},
                    )
                )

        # High fan-in: too many modules depend on this
        for module, dependents in imported_by.items():
            if len(dependents) > 15 and not module.startswith(("moss.",)):
                # Skip if it's an internal module (expected to be widely used)
                pass  # Internal modules being widely imported is expected
            elif len(dependents) > 20:
                result.weaknesses.append(
                    Weakness(
                        category=WeaknessCategory.COUPLING,
                        severity=Severity.LOW,
                        title=f"High fan-in: {module}",
                        description=f"Module is imported by {len(dependents)} modules",
                        suggestion="Consider if it's doing too much or should be split",
                        related_files=dependents[:5],
                        metadata={"dependent_count": len(dependents)},
                    )
                )

        # Circular dependencies would require more sophisticated analysis
        # (not implemented yet - would need to build full dependency graph)

    def _analyze_hardcoded(
        self,
        result: WeaknessAnalysis,
        source: str,
        rel_path: str,
        tree: ast.AST,
    ) -> None:
        """Analyze for hardcoded values."""
        detector = HardcodedDetector(source, rel_path)
        detector.visit(tree)

        for finding in detector.findings:
            severity = Severity.LOW
            if finding["type"] == "hardcoded_url":
                severity = Severity.MEDIUM

            result.weaknesses.append(
                Weakness(
                    category=WeaknessCategory.HARDCODED,
                    severity=severity,
                    title=f"Hardcoded {finding['type'].replace('hardcoded_', '')}",
                    description=f"Value: {finding['value']}",
                    file_path=rel_path,
                    line_start=finding["line"],
                    suggestion="Consider using configuration or environment variables",
                )
            )

    def _analyze_error_handling(
        self,
        result: WeaknessAnalysis,
        source: str,
        rel_path: str,
        tree: ast.AST,
    ) -> None:
        """Analyze error handling patterns."""
        detector = ErrorHandlingDetector(source, rel_path)
        detector.visit(tree)

        for finding in detector.findings:
            severity = Severity.MEDIUM if finding["type"] == "bare_except" else Severity.LOW

            result.weaknesses.append(
                Weakness(
                    category=WeaknessCategory.ERROR_HANDLING,
                    severity=severity,
                    title=finding["type"].replace("_", " ").title(),
                    description=finding["description"],
                    file_path=rel_path,
                    line_start=finding["line"],
                    suggestion="Be specific about exceptions to catch and handle them",
                )
            )

    def _analyze_abstractions(
        self,
        result: WeaknessAnalysis,
        source: str,
        rel_path: str,
        tree: ast.AST,
    ) -> None:
        """Analyze for missing abstractions."""
        detector = AbstractionDetector(source, rel_path)
        detector.visit(tree)

        for finding in detector.findings:
            if finding["type"] == "long_param_list":
                result.weaknesses.append(
                    Weakness(
                        category=WeaknessCategory.ABSTRACTION,
                        severity=Severity.MEDIUM,
                        title=f"Long parameter list: {finding['name']}",
                        description=f"Function has {finding['param_count']} parameters",
                        file_path=rel_path,
                        line_start=finding["line"],
                        suggestion="Consider grouping parameters into a dataclass or config object",
                    )
                )
            elif finding["type"] == "long_function":
                result.weaknesses.append(
                    Weakness(
                        category=WeaknessCategory.ABSTRACTION,
                        severity=Severity.MEDIUM,
                        title=f"Long function: {finding['name']}",
                        description=f"Function is {finding['lines']} lines long",
                        file_path=rel_path,
                        line_start=finding["line"],
                        suggestion="Consider breaking into smaller, focused functions",
                    )
                )
            elif finding["type"] == "god_class":
                result.weaknesses.append(
                    Weakness(
                        category=WeaknessCategory.ABSTRACTION,
                        severity=Severity.HIGH,
                        title=f"God class: {finding['name']}",
                        description=f"Class has {finding['method_count']} methods",
                        file_path=rel_path,
                        line_start=finding["line"],
                        suggestion="Consider splitting into smaller, focused classes",
                    )
                )

    def _analyze_patterns(self, result: WeaknessAnalysis) -> None:
        """Analyze for pattern inconsistencies."""
        from moss.patterns import PatternAnalyzer

        try:
            analyzer = PatternAnalyzer(self.root)
            analysis = analyzer.analyze()

            # Check for inconsistent use of patterns
            # e.g., some factories use one pattern, others use different pattern

            # Check if there are similar factories that could be unified
            if len(analysis.factories) > 5:
                result.weaknesses.append(
                    Weakness(
                        category=WeaknessCategory.PATTERN,
                        severity=Severity.INFO,
                        title="Multiple factory patterns",
                        description=(
                            f"Found {len(analysis.factories)} factories - "
                            "consider if they could share a common interface"
                        ),
                        suggestion="Review if factories follow consistent patterns",
                    )
                )

            # Check coupling suggestions from pattern analysis
            for suggestion in analysis.suggestions:
                if "imports" in suggestion.lower():
                    result.weaknesses.append(
                        Weakness(
                            category=WeaknessCategory.COUPLING,
                            severity=Severity.LOW,
                            title="Coupling issue from pattern analysis",
                            description=suggestion,
                        )
                    )

        except (OSError, SyntaxError, ValueError) as e:
            logger.debug("Pattern analysis failed: %s", e)


def format_weakness_analysis(analysis: WeaknessAnalysis) -> str:
    """Format weakness analysis as markdown."""
    lines = ["## Architectural Weakness Analysis", ""]

    # Summary
    by_sev = analysis.by_severity
    lines.append("### Summary")
    lines.append(f"- Total weaknesses: {len(analysis.weaknesses)}")
    lines.append(f"- High severity: {len(by_sev.get(Severity.HIGH, []))}")
    lines.append(f"- Medium severity: {len(by_sev.get(Severity.MEDIUM, []))}")
    lines.append(f"- Low severity: {len(by_sev.get(Severity.LOW, []))}")
    lines.append("")

    # By category
    by_cat = analysis.by_category
    for category in WeaknessCategory:
        weaknesses = by_cat.get(category, [])
        if weaknesses:
            lines.append(f"### {category.value.replace('_', ' ').title()} ({len(weaknesses)})")
            for w in weaknesses:
                severity_marker = {"high": "[!]", "medium": "[~]", "low": "[.]", "info": "[i]"}.get(
                    w.severity.value, ""
                )
                location = f" (`{w.file_path}:{w.line_start}`)" if w.file_path else ""
                lines.append(f"- {severity_marker} **{w.title}**{location}")
                lines.append(f"  {w.description}")
                if w.suggestion:
                    lines.append(f"  💡 {w.suggestion}")
            lines.append("")

    return "\n".join(lines)


def analyze_weaknesses(
    root: Path | str,
    categories: list[str] | None = None,
) -> WeaknessAnalysis:
    """Convenience function to analyze weaknesses.

    Args:
        root: Project root directory
        categories: Categories to check (None = all)

    Returns:
        WeaknessAnalysis with detected weaknesses
    """
    analyzer = WeaknessAnalyzer(Path(root), categories=categories)
    return analyzer.analyze()


# =============================================================================
# SARIF Output Support
# =============================================================================


def generate_sarif_from_weaknesses(
    analysis: WeaknessAnalysis,
    tool_name: str = "moss-weaknesses",
    tool_version: str = "0.1.0",
) -> dict[str, Any]:
    """Generate SARIF output from weakness analysis.

    SARIF (Static Analysis Results Interchange Format) is a standard
    for static analysis results supported by GitHub, Azure DevOps, etc.

    Args:
        analysis: Weakness analysis result
        tool_name: Name for the tool component
        tool_version: Tool version string

    Returns:
        SARIF document as dictionary
    """
    from datetime import UTC, datetime

    # SARIF schema info
    sarif_schema = (
        "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/"
        "Schemata/sarif-schema-2.1.0.json"
    )

    # Build rules dictionary
    rules_dict: dict[str, dict[str, Any]] = {}
    for weakness in analysis.weaknesses:
        rule_id = _weakness_to_rule_id(weakness)
        if rule_id not in rules_dict:
            rules_dict[rule_id] = _build_weakness_rule_descriptor(weakness)

    # Build results
    results = [_build_weakness_result(w, analysis.root) for w in analysis.weaknesses]

    return {
        "$schema": sarif_schema,
        "version": "2.1.0",
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": tool_name,
                        "version": tool_version,
                        "rules": list(rules_dict.values()),
                    }
                },
                "results": results,
                "invocations": [
                    {
                        "executionSuccessful": True,
                        "endTimeUtc": datetime.now(UTC).isoformat(),
                    }
                ],
            }
        ],
    }


def _weakness_to_rule_id(weakness: Weakness) -> str:
    """Generate a stable rule ID for a weakness type."""
    # Create a consistent rule ID from category and title pattern
    title_slug = weakness.title.split(":")[0].strip().lower().replace(" ", "-")
    return f"moss-weakness/{weakness.category.value}/{title_slug}"


def _build_weakness_rule_descriptor(weakness: Weakness) -> dict[str, Any]:
    """Build a SARIF rule descriptor from a weakness."""
    severity_map = {
        Severity.HIGH: "error",
        Severity.MEDIUM: "warning",
        Severity.LOW: "note",
        Severity.INFO: "note",
    }

    descriptor: dict[str, Any] = {
        "id": _weakness_to_rule_id(weakness),
        "name": weakness.title.split(":")[0].strip(),
        "shortDescription": {"text": weakness.description[:200]},
        "defaultConfiguration": {
            "level": severity_map.get(weakness.severity, "warning"),
        },
        "properties": {
            "category": weakness.category.value,
            "severity": weakness.severity.value,
        },
    }

    if weakness.suggestion:
        descriptor["help"] = {"text": weakness.suggestion}

    return descriptor


def _build_weakness_result(weakness: Weakness, root: Path) -> dict[str, Any]:
    """Build a SARIF result from a weakness."""
    severity_map = {
        Severity.HIGH: "error",
        Severity.MEDIUM: "warning",
        Severity.LOW: "note",
        Severity.INFO: "note",
    }

    result: dict[str, Any] = {
        "ruleId": _weakness_to_rule_id(weakness),
        "level": severity_map.get(weakness.severity, "warning"),
        "message": {"text": f"{weakness.title}: {weakness.description}"},
    }

    # Add location if available
    if weakness.file_path:
        result["locations"] = [
            {
                "physicalLocation": {
                    "artifactLocation": {"uri": weakness.file_path},
                    "region": {
                        "startLine": weakness.line_start or 1,
                        "startColumn": 1,
                    },
                }
            }
        ]

        # Add end line if available
        if weakness.line_end:
            result["locations"][0]["physicalLocation"]["region"]["endLine"] = weakness.line_end

    # Add fingerprint for deduplication
    loc_str = f"{weakness.file_path}:{weakness.line_start}" if weakness.file_path else "project"
    fingerprint = f"{_weakness_to_rule_id(weakness)}:{loc_str}"
    result["fingerprints"] = {"primaryLocationLineHash": fingerprint}

    # Add fix suggestion if available
    if weakness.suggestion:
        result["fixes"] = [
            {
                "description": {"text": weakness.suggestion},
            }
        ]

    # Add related locations if there are related files
    if weakness.related_files:
        result["relatedLocations"] = [
            {
                "physicalLocation": {
                    "artifactLocation": {"uri": f},
                },
                "message": {"text": "Related file"},
            }
            for f in weakness.related_files[:5]  # Limit to 5
        ]

    return result


def weaknesses_to_sarif(
    analysis: WeaknessAnalysis,
    output_path: Path | None = None,
    tool_name: str = "moss-weaknesses",
    tool_version: str = "0.1.0",
) -> str:
    """Generate SARIF JSON from weakness analysis.

    Args:
        analysis: Weakness analysis result
        output_path: Optional path to write SARIF file
        tool_name: Name for the tool component
        tool_version: Tool version string

    Returns:
        SARIF JSON string
    """
    import json

    sarif = generate_sarif_from_weaknesses(analysis, tool_name, tool_version)
    sarif_json = json.dumps(sarif, indent=2)

    if output_path:
        output_path.write_text(sarif_json)

    return sarif_json


# =============================================================================
# Auto-Fix Support
# =============================================================================


@dataclass
class WeaknessFix:
    """A suggested fix for a weakness."""

    weakness: Weakness
    fix_type: str  # "auto", "semi-auto", "manual"
    description: str
    commands: list[str] = field(default_factory=list)  # CLI commands to run
    code_changes: list[dict[str, Any]] = field(default_factory=list)  # Structured changes


def get_fixable_weaknesses(analysis: WeaknessAnalysis) -> list[WeaknessFix]:
    """Get weaknesses that have automated or semi-automated fixes.

    Returns:
        List of WeaknessFix objects for fixable weaknesses
    """
    fixes = []

    for weakness in analysis.weaknesses:
        fix = _get_weakness_fix(weakness)
        if fix:
            fixes.append(fix)

    return fixes


def _get_weakness_fix(weakness: Weakness) -> WeaknessFix | None:
    """Generate a fix suggestion for a weakness if possible."""
    # Bare except can be auto-fixed
    if weakness.category == WeaknessCategory.ERROR_HANDLING:
        if "bare except" in weakness.title.lower():
            return WeaknessFix(
                weakness=weakness,
                fix_type="semi-auto",
                description="Replace bare except with specific exception type",
                code_changes=[
                    {
                        "file": weakness.file_path,
                        "line": weakness.line_start,
                        "old": "except:",
                        "new": "except Exception:  # TODO: specify exception type",
                    }
                ],
            )
        if "swallowed" in weakness.title.lower():
            return WeaknessFix(
                weakness=weakness,
                fix_type="semi-auto",
                description="Add logging to swallowed exception",
                code_changes=[
                    {
                        "file": weakness.file_path,
                        "line": weakness.line_start,
                        "suggestion": "Add: logger.exception('Error occurred') or re-raise",
                    }
                ],
            )

    # Hardcoded values can sometimes be auto-fixed
    if weakness.category == WeaknessCategory.HARDCODED:
        if weakness.file_path and weakness.line_start:
            # Suggest environment variable
            return WeaknessFix(
                weakness=weakness,
                fix_type="semi-auto",
                description="Extract hardcoded value to configuration",
                code_changes=[
                    {
                        "file": weakness.file_path,
                        "line": weakness.line_start,
                        "suggestion": "Replace with os.environ.get('CONFIG_VAR') or config file",
                    }
                ],
            )

    # Long functions / god classes - suggest extraction
    if weakness.category == WeaknessCategory.ABSTRACTION:
        if "long function" in weakness.title.lower():
            return WeaknessFix(
                weakness=weakness,
                fix_type="manual",
                description="Consider extracting helper functions",
                commands=[
                    f"moss skeleton {weakness.file_path}",  # View structure
                    f"moss complexity {weakness.file_path}",  # Find complex parts
                ],
            )
        if "god class" in weakness.title.lower():
            return WeaknessFix(
                weakness=weakness,
                fix_type="manual",
                description="Consider splitting into smaller, focused classes",
                commands=[
                    f"moss skeleton {weakness.file_path}",  # View structure
                    f"moss deps {weakness.file_path}",  # See dependencies
                ],
            )
        if "long param" in weakness.title.lower():
            return WeaknessFix(
                weakness=weakness,
                fix_type="semi-auto",
                description="Group parameters into a dataclass",
                code_changes=[
                    {
                        "file": weakness.file_path,
                        "line": weakness.line_start,
                        "suggestion": "Create @dataclass with related parameters",
                    }
                ],
            )

    return None


def format_weakness_fixes(fixes: list[WeaknessFix]) -> str:
    """Format weakness fixes as markdown."""
    if not fixes:
        return "No auto-fixable weaknesses found."

    lines = ["## Suggested Fixes", ""]

    # Group by fix type
    auto_fixes = [f for f in fixes if f.fix_type == "auto"]
    semi_fixes = [f for f in fixes if f.fix_type == "semi-auto"]
    manual_fixes = [f for f in fixes if f.fix_type == "manual"]

    if auto_fixes:
        lines.append("### Auto-fixable")
        for fix in auto_fixes:
            loc = f"{fix.weakness.file_path}:{fix.weakness.line_start}"
            lines.append(f"- **{fix.weakness.title}** ({loc})")
            lines.append(f"  {fix.description}")
            for change in fix.code_changes:
                if "old" in change and "new" in change:
                    lines.append(f"  Replace: `{change['old']}` → `{change['new']}`")
        lines.append("")

    if semi_fixes:
        lines.append("### Semi-automated (review required)")
        for fix in semi_fixes:
            if fix.weakness.file_path:
                loc = f"{fix.weakness.file_path}:{fix.weakness.line_start}"
            else:
                loc = "N/A"
            lines.append(f"- **{fix.weakness.title}** ({loc})")
            lines.append(f"  {fix.description}")
            for change in fix.code_changes:
                if "suggestion" in change:
                    lines.append(f"  💡 {change['suggestion']}")
        lines.append("")

    if manual_fixes:
        lines.append("### Manual fixes (guidance)")
        for fix in manual_fixes:
            if fix.weakness.file_path:
                loc = f"{fix.weakness.file_path}:{fix.weakness.line_start}"
            else:
                loc = "N/A"
            lines.append(f"- **{fix.weakness.title}** ({loc})")
            lines.append(f"  {fix.description}")
            for cmd in fix.commands:
                lines.append(f"  Run: `{cmd}`")
        lines.append("")

    return "\n".join(lines)
