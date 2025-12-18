"""Gemini CLI format adapter.

Outputs preferences as GEMINI.md content.
"""

from __future__ import annotations

from dataclasses import dataclass

from moss.preferences.models import ConfidenceLevel, PreferenceCategory, PreferenceSet


@dataclass
class GeminiAdapter:
    """Format adapter for Gemini CLI's GEMINI.md."""

    min_confidence: ConfidenceLevel = ConfidenceLevel.MEDIUM
    include_header: bool = True

    def format(self, prefs: PreferenceSet) -> str:
        """Format preferences for GEMINI.md.

        Gemini CLI reads GEMINI.md for project-specific instructions.
        Similar to Claude Code but may have different conventions.
        """
        lines = []

        if self.include_header:
            lines.extend(
                [
                    "# Project Instructions",
                    "",
                    "*Auto-extracted preferences. Review and customize as needed.*",
                    "",
                ]
            )

        # Filter to minimum confidence
        filtered = prefs.by_confidence(self.min_confidence)

        # Group by category
        category_sections = {
            PreferenceCategory.ARCHITECTURE: "Architecture Guidelines",
            PreferenceCategory.WORKFLOW: "Development Workflow",
            PreferenceCategory.AGENT_BEHAVIOR: "Behavior Guidelines",
            PreferenceCategory.DOMAIN: "Project-Specific Rules",
            PreferenceCategory.PROHIBITION: "Restrictions",
            PreferenceCategory.COMMUNICATION: "Response Style",
        }

        for cat, section_name in category_sections.items():
            cat_prefs = [p for p in filtered if p.category == cat]
            if not cat_prefs:
                continue

            lines.append(f"## {section_name}")
            lines.append("")

            for pref in cat_prefs:
                rule = self._format_rule(pref.rule, cat)
                lines.append(f"- {rule}")

            lines.append("")

        return "\n".join(lines)

    def _format_rule(self, rule: str, category: PreferenceCategory) -> str:
        """Format a rule for Gemini CLI."""
        rule = rule.strip()

        # Capitalize first letter
        if rule:
            rule = rule[0].upper() + rule[1:]

        # Ensure prohibitions are clear
        if category == PreferenceCategory.PROHIBITION:
            lower = rule.lower()
            if not lower.startswith(("do not", "don't", "never", "avoid", "must not")):
                rule = f"Must not {rule[0].lower()}{rule[1:]}"

        return rule.rstrip(".")
