"""SARIF output format for CI/CD integration.

SARIF (Static Analysis Results Interchange Format) is a standard JSON format
for static analysis results. This module provides export to SARIF v2.1.0
which is supported by:
- GitHub Code Scanning
- Azure DevOps
- VS Code SARIF Viewer
- Many CI/CD platforms

Usage:
    from moss_orchestration.sarif import generate_sarif, write_sarif
    from moss_orchestration.rules_single import create_engine_with_builtins

    # Run analysis
    engine = create_engine_with_builtins()
    result = engine.check_directory(Path("."))

    # Generate SARIF
    sarif = generate_sarif(result, tool_name="moss", version="0.1.0")
    write_sarif(sarif, Path("results.sarif"))
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from moss_orchestration.rules_single import RuleResult, Severity, Violation

# SARIF specification version
SARIF_VERSION = "2.1.0"
SARIF_SCHEMA = (
    "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json"
)


@dataclass
class SARIFConfig:
    """Configuration for SARIF output."""

    tool_name: str = "moss"
    tool_version: str = "0.1.0"
    tool_information_uri: str = ""
    include_snippets: bool = True
    include_fingerprints: bool = True
    base_path: Path | None = None  # For relative URIs


def generate_sarif(
    result: RuleResult,
    config: SARIFConfig | None = None,
) -> dict[str, Any]:
    """Generate SARIF output from rule results.

    Args:
        result: Rule checking result
        config: SARIF configuration

    Returns:
        SARIF document as dictionary
    """
    config = config or SARIFConfig()

    # Build rules dictionary for tool component
    rules_dict: dict[str, dict] = {}
    for violation in result.violations:
        rule_id = violation.rule_name
        if rule_id not in rules_dict:
            rules_dict[rule_id] = _build_rule_descriptor(violation)

    # Build results array
    results = [_build_result(v, config) for v in result.violations]

    # Build the SARIF document
    sarif = {
        "$schema": SARIF_SCHEMA,
        "version": SARIF_VERSION,
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": config.tool_name,
                        "version": config.tool_version,
                        "informationUri": config.tool_information_uri or "",
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

    return sarif


def _build_rule_descriptor(violation: Violation) -> dict[str, Any]:
    """Build a SARIF rule descriptor from a violation."""
    severity_map = {
        Severity.ERROR: "error",
        Severity.WARNING: "warning",
        Severity.INFO: "note",
    }

    descriptor: dict[str, Any] = {
        "id": violation.rule_name,
        "name": violation.rule_name.replace("-", " ").title(),
        "shortDescription": {"text": violation.message},
        "defaultConfiguration": {
            "level": severity_map.get(violation.severity, "warning"),
        },
        "properties": {
            "category": violation.category,
        },
    }

    # Note: documentation field doesn't exist in new Violation structure
    # If we add it in metadata, we can extract it here

    if violation.fix:
        descriptor["help"] = {"text": violation.fix}

    return descriptor


def _build_result(violation: Violation, config: SARIFConfig) -> dict[str, Any]:
    """Build a SARIF result from a violation."""
    severity_map = {
        Severity.ERROR: "error",
        Severity.WARNING: "warning",
        Severity.INFO: "note",
    }

    # Build file URI
    file_path = violation.location.file_path
    if config.base_path:
        try:
            file_path = file_path.relative_to(config.base_path)
        except ValueError:
            pass

    result: dict[str, Any] = {
        "ruleId": violation.rule_name,
        "level": severity_map.get(violation.severity, "warning"),
        "message": {"text": violation.message},
        "locations": [
            {
                "physicalLocation": {
                    "artifactLocation": {"uri": str(file_path)},
                    "region": {
                        "startLine": violation.location.line,
                        "startColumn": violation.location.column,
                    },
                }
            }
        ],
    }

    # Add snippet if available and configured
    if config.include_snippets and violation.context_lines:
        result["locations"][0]["physicalLocation"]["region"]["snippet"] = {
            "text": violation.context_lines
        }

    # Add fingerprint for deduplication
    if config.include_fingerprints:
        # Simple fingerprint based on rule + location
        fingerprint = f"{violation.rule_name}:{file_path}:{violation.location.line}"
        result["fingerprints"] = {"primaryLocationLineHash": fingerprint}

    return result


def write_sarif(sarif: dict[str, Any], output_path: Path) -> None:
    """Write SARIF document to file.

    Args:
        sarif: SARIF document dictionary
        output_path: Path to output file
    """
    output_path.write_text(json.dumps(sarif, indent=2))


def sarif_from_rules_result(
    result: RuleResult,
    tool_name: str = "moss",
    version: str = "0.1.0",
    base_path: Path | None = None,
) -> str:
    """Generate SARIF JSON string from rule results.

    Convenience function for common use case.

    Args:
        result: Rule checking result
        tool_name: Name of the analysis tool
        version: Tool version
        base_path: Base path for relative file URIs

    Returns:
        SARIF JSON string
    """
    config = SARIFConfig(
        tool_name=tool_name,
        tool_version=version,
        base_path=base_path,
    )
    sarif = generate_sarif(result, config)
    return json.dumps(sarif, indent=2)
