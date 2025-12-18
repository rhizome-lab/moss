"""Claude Code format adapter.

Outputs preferences as CLAUDE.md content.
"""

from __future__ import annotations

from dataclasses import dataclass

from moss.preferences.models import ConfidenceLevel, PreferenceCategory, PreferenceSet


@dataclass
class ClaudeAdapter:
    """Format adapter for Claude Code's CLAUDE.md."""

    min_confidence: ConfidenceLevel = ConfidenceLevel.MEDIUM
    include_header: bool = True

    def format(self, prefs: PreferenceSet) -> str:
        """Format preferences for CLAUDE.md.

        Claude Code reads CLAUDE.md for project-specific instructions.
        The format should be clear, actionable instructions.
        """
        lines = []

        if self.include_header:
            lines.extend(
                [
                    "# Project Preferences",
                    "",
                    "*Auto-extracted from session history. Review and edit as needed.*",
                    "",
                ]
            )

        # Filter to minimum confidence
        filtered = prefs.by_confidence(self.min_confidence)

        # Group by category with Claude-appropriate section names
        category_sections = {
            PreferenceCategory.ARCHITECTURE: "Code Architecture",
            PreferenceCategory.WORKFLOW: "Workflow",
            PreferenceCategory.AGENT_BEHAVIOR: "Agent Behavior",
            PreferenceCategory.DOMAIN: "Project Conventions",
            PreferenceCategory.PROHIBITION: "Do NOT",
            PreferenceCategory.COMMUNICATION: "Communication Style",
        }

        for cat, section_name in category_sections.items():
            cat_prefs = [p for p in filtered if p.category == cat]
            if not cat_prefs:
                continue

            lines.append(f"## {section_name}")
            lines.append("")

            for pref in cat_prefs:
                # Format as actionable instruction
                rule = self._format_rule(pref.rule, cat)
                lines.append(f"- {rule}")

            lines.append("")

        return "\n".join(lines)

    def _format_rule(self, rule: str, category: PreferenceCategory) -> str:
        """Format a rule for Claude Code.

        Ensures rules are actionable and properly phrased.
        """
        rule = rule.strip()

        # Ensure first letter is capitalized
        if rule:
            rule = rule[0].upper() + rule[1:]

        # For prohibitions, ensure they start with "Do not" or "Never"
        if category == PreferenceCategory.PROHIBITION:
            lower = rule.lower()
            if not lower.startswith(("do not", "don't", "never", "avoid")):
                rule = f"Do not {rule[0].lower()}{rule[1:]}"

        # Remove trailing period for consistency
        rule = rule.rstrip(".")

        return rule
