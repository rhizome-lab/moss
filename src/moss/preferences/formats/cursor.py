"""Cursor format adapter.

Outputs preferences as .cursorrules content.
"""

from __future__ import annotations

from dataclasses import dataclass

from moss.preferences.models import ConfidenceLevel, PreferenceCategory, PreferenceSet


@dataclass
class CursorAdapter:
    """Format adapter for Cursor's .cursorrules."""

    min_confidence: ConfidenceLevel = ConfidenceLevel.MEDIUM

    def format(self, prefs: PreferenceSet) -> str:
        """Format preferences for .cursorrules.

        Cursor uses a simpler format, typically just a list of rules.
        The format is plain text with rules that Cursor's AI follows.
        """
        lines = []

        # Filter to minimum confidence
        filtered = prefs.by_confidence(self.min_confidence)

        # Cursor rules are typically more direct and less structured
        # Group important categories first

        # Start with prohibitions (most important)
        prohibitions = [p for p in filtered if p.category == PreferenceCategory.PROHIBITION]
        if prohibitions:
            lines.append("# Restrictions")
            lines.append("")
            for pref in prohibitions:
                rule = self._format_rule(pref.rule, pref.category)
                lines.append(f"- {rule}")
            lines.append("")

        # Architecture and code style
        arch_prefs = [p for p in filtered if p.category == PreferenceCategory.ARCHITECTURE]
        if arch_prefs:
            lines.append("# Code Style")
            lines.append("")
            for pref in arch_prefs:
                rule = self._format_rule(pref.rule, pref.category)
                lines.append(f"- {rule}")
            lines.append("")

        # Workflow
        workflow_prefs = [p for p in filtered if p.category == PreferenceCategory.WORKFLOW]
        if workflow_prefs:
            lines.append("# Workflow")
            lines.append("")
            for pref in workflow_prefs:
                rule = self._format_rule(pref.rule, pref.category)
                lines.append(f"- {rule}")
            lines.append("")

        # Other categories combined
        other_cats = [
            PreferenceCategory.AGENT_BEHAVIOR,
            PreferenceCategory.DOMAIN,
            PreferenceCategory.COMMUNICATION,
        ]
        other_prefs = [p for p in filtered if p.category in other_cats]
        if other_prefs:
            lines.append("# General")
            lines.append("")
            for pref in other_prefs:
                rule = self._format_rule(pref.rule, pref.category)
                lines.append(f"- {rule}")
            lines.append("")

        return "\n".join(lines)

    def _format_rule(self, rule: str, category: PreferenceCategory) -> str:
        """Format a rule for Cursor."""
        rule = rule.strip()

        if rule:
            rule = rule[0].upper() + rule[1:]

        # Cursor prefers direct, imperative rules
        if category == PreferenceCategory.PROHIBITION:
            lower = rule.lower()
            if not lower.startswith(("do not", "don't", "never", "avoid")):
                rule = f"Do not {rule[0].lower()}{rule[1:]}"

        return rule.rstrip(".")
