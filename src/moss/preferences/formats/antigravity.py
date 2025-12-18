"""Google Antigravity format adapter.

Outputs preferences as .agent/rules/*.md files.
Antigravity uses a directory structure with separate rule files by category.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import ClassVar

from moss.preferences.models import ConfidenceLevel, PreferenceCategory, PreferenceSet


@dataclass
class RuleFile:
    """A single rule file for Antigravity."""

    filename: str
    content: str


@dataclass
class AntigravityAdapter:
    """Format adapter for Google Antigravity's .agent/rules/*.md."""

    min_confidence: ConfidenceLevel = ConfidenceLevel.MEDIUM

    # Category to filename mapping
    CATEGORY_FILES: ClassVar[dict[PreferenceCategory, str]] = {
        PreferenceCategory.ARCHITECTURE: "architecture.md",
        PreferenceCategory.WORKFLOW: "workflow.md",
        PreferenceCategory.AGENT_BEHAVIOR: "behavior.md",
        PreferenceCategory.DOMAIN: "conventions.md",
        PreferenceCategory.PROHIBITION: "prohibitions.md",
        PreferenceCategory.COMMUNICATION: "communication.md",
    }

    def format(self, prefs: PreferenceSet) -> str:
        """Format preferences as combined output.

        For actual file creation, use format_files() instead.
        This returns all rules combined with file markers.
        """
        files = self.format_files(prefs)

        lines = []
        for rule_file in files:
            lines.append(f"# .agent/rules/{rule_file.filename}")
            lines.append("")
            lines.append(rule_file.content)
            lines.append("")
            lines.append("---")
            lines.append("")

        return "\n".join(lines)

    def format_files(self, prefs: PreferenceSet) -> list[RuleFile]:
        """Format preferences as separate rule files.

        Returns a list of RuleFile objects, each representing
        a file that should be created in .agent/rules/.
        """
        files = []

        # Filter to minimum confidence
        filtered = prefs.by_confidence(self.min_confidence)

        for cat, filename in self.CATEGORY_FILES.items():
            cat_prefs = [p for p in filtered if p.category == cat]
            if not cat_prefs:
                continue

            content = self._format_category(cat, cat_prefs)
            files.append(RuleFile(filename=filename, content=content))

        return files

    def _format_category(self, category: PreferenceCategory, prefs: list) -> str:
        """Format a single category file."""
        lines = []

        # Category title
        title = category.value.replace("_", " ").title()
        lines.append(f"# {title}")
        lines.append("")

        for pref in prefs:
            rule = self._format_rule(pref.rule, category)
            lines.append(f"- {rule}")

        return "\n".join(lines)

    def _format_rule(self, rule: str, category: PreferenceCategory) -> str:
        """Format a rule for Antigravity."""
        rule = rule.strip()

        if rule:
            rule = rule[0].upper() + rule[1:]

        if category == PreferenceCategory.PROHIBITION:
            lower = rule.lower()
            if not lower.startswith(("do not", "don't", "never", "avoid")):
                rule = f"Never {rule[0].lower()}{rule[1:]}"

        return rule.rstrip(".")
