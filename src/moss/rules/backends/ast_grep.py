"""ast-grep structural pattern matching backend.

Uses ast-grep (sg) for AST-aware pattern matching. This is more powerful
than regex because it understands code structure:

    # This pattern matches function calls, not string literals containing "print"
    pattern = "print($ARGS)"

ast-grep uses a YAML rule format or inline patterns. This backend
supports both:

    @rule(backend="ast-grep")
    def no_print(ctx: RuleContext) -> list[Violation]:
        # Pattern passed via rule spec
        for match in ctx.backend("ast-grep").matches:
            ...

See: https://ast-grep.github.io/
"""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path
from typing import Any

from ..base import BackendResult, BaseBackend, Location, Match
from . import register_backend


@register_backend
class AstGrepBackend(BaseBackend):
    """ast-grep structural pattern matching backend."""

    def __init__(self) -> None:
        self._sg_path: str | None = None

    @property
    def name(self) -> str:
        return "ast-grep"

    @property
    def sg_path(self) -> str | None:
        """Get path to sg binary, or None if not available."""
        if self._sg_path is None:
            self._sg_path = shutil.which("sg") or shutil.which("ast-grep")
        return self._sg_path

    def is_available(self) -> bool:
        """Check if ast-grep is installed."""
        return self.sg_path is not None

    def analyze(
        self,
        file_path: Path,
        pattern: str | None = None,
        **options: Any,
    ) -> BackendResult:
        """Find structural matches in a file using ast-grep.

        Args:
            file_path: File to analyze
            pattern: ast-grep pattern (e.g., "print($ARGS)")
            **options:
                - language: str = "python"
                - rule_file: Path to YAML rule file (alternative to pattern)

        Returns:
            BackendResult with Match objects
        """
        if not self.is_available():
            return BackendResult(
                backend_name=self.name,
                matches=[],
                errors=["ast-grep (sg) not found. Install with: cargo install ast-grep"],
            )

        if pattern is None and "rule_file" not in options:
            return BackendResult(backend_name=self.name, matches=[])

        language = options.get("language", "python")

        try:
            if "rule_file" in options:
                result = self._run_with_rule_file(file_path, options["rule_file"])
            else:
                result = self._run_with_pattern(file_path, pattern, language)  # type: ignore
            return result
        except (OSError, subprocess.SubprocessError, subprocess.TimeoutExpired) as e:
            return BackendResult(
                backend_name=self.name,
                matches=[],
                errors=[f"ast-grep error: {e}"],
            )

    def _run_with_pattern(self, file_path: Path, pattern: str, language: str) -> BackendResult:
        """Run ast-grep with an inline pattern."""
        cmd = [
            self.sg_path,  # type: ignore
            "--pattern",
            pattern,
            "--lang",
            language,
            "--json",
            str(file_path),
        ]

        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)

        if result.returncode != 0 and result.stderr:
            return BackendResult(
                backend_name=self.name,
                matches=[],
                errors=[f"ast-grep error: {result.stderr}"],
            )

        return self._parse_json_output(result.stdout, file_path)

    def _run_with_rule_file(self, file_path: Path, rule_file: Path) -> BackendResult:
        """Run ast-grep with a YAML rule file."""
        cmd = [
            self.sg_path,  # type: ignore
            "scan",
            "--rule",
            str(rule_file),
            "--json",
            str(file_path),
        ]

        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)

        if result.returncode != 0 and result.stderr:
            return BackendResult(
                backend_name=self.name,
                matches=[],
                errors=[f"ast-grep error: {result.stderr}"],
            )

        return self._parse_json_output(result.stdout, file_path)

    def _parse_json_output(self, output: str, default_path: Path) -> BackendResult:
        """Parse ast-grep JSON output into Match objects."""
        matches: list[Match] = []

        if not output.strip():
            return BackendResult(backend_name=self.name, matches=[])

        try:
            # ast-grep outputs one JSON object per line (JSON Lines format)
            for line in output.strip().split("\n"):
                if not line.strip():
                    continue
                data = json.loads(line)
                match = self._parse_match(data, default_path)
                if match:
                    matches.append(match)
        except json.JSONDecodeError as e:
            return BackendResult(
                backend_name=self.name,
                matches=[],
                errors=[f"Failed to parse ast-grep output: {e}"],
            )

        return BackendResult(backend_name=self.name, matches=matches)

    def _parse_match(self, data: dict[str, Any], default_path: Path) -> Match | None:
        """Parse a single match from ast-grep JSON."""
        # ast-grep JSON format:
        # {
        #   "text": "matched text",
        #   "range": {"start": {"line": 1, "column": 0}, "end": {...}},
        #   "file": "path/to/file.py",
        #   "metaVariables": {"ARGS": {...}},
        #   ...
        # }
        try:
            range_data = data.get("range", {})
            start = range_data.get("start", {})
            end = range_data.get("end", {})

            file_path = Path(data.get("file", str(default_path)))

            location = Location(
                file_path=file_path,
                line=start.get("line", 1),
                column=start.get("column", 0) + 1,  # Convert to 1-indexed
                end_line=end.get("line"),
                end_column=end.get("column", 0) + 1 if end.get("column") else None,
            )

            # Extract meta-variables (captured groups)
            meta_vars = {}
            for name, var_data in data.get("metaVariables", {}).items():
                if isinstance(var_data, dict):
                    meta_vars[name] = var_data.get("text", "")
                else:
                    meta_vars[name] = str(var_data)

            return Match(
                location=location,
                text=data.get("text", ""),
                metadata={
                    "metaVariables": meta_vars,
                    "rule": data.get("ruleId"),
                },
            )
        except (ValueError, KeyError, TypeError):
            return None

    def supports_pattern(self, pattern: str) -> bool:
        """Check if pattern looks like an ast-grep pattern."""
        # ast-grep patterns typically contain $ for metavariables
        # or look like code snippets
        return "$" in pattern or any(
            kw in pattern for kw in ["def ", "class ", "if ", "for ", "while ", "import "]
        )


def create_rule_yaml(
    rule_id: str,
    pattern: str,
    message: str,
    *,
    language: str = "python",
    severity: str = "warning",
    fix: str | None = None,
) -> str:
    """Create ast-grep YAML rule content.

    Args:
        rule_id: Unique rule identifier
        pattern: ast-grep pattern
        message: Violation message
        language: Target language
        severity: hint, info, warning, error
        fix: Optional fix pattern

    Returns:
        YAML rule content
    """
    rule = {
        "id": rule_id,
        "language": language,
        "severity": severity,
        "message": message,
        "rule": {"pattern": pattern},
    }

    if fix:
        rule["fix"] = fix

    import yaml

    return yaml.dump(rule, default_flow_style=False)
