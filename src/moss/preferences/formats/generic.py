"""Generic format adapters.

Plain markdown and JSON output formats.
"""

from __future__ import annotations

import json
from dataclasses import dataclass

from moss.preferences.models import PreferenceCategory, PreferenceSet


@dataclass
class GenericAdapter:
    """Generic markdown format adapter."""

    include_evidence: bool = False
    include_confidence: bool = True

    def format(self, prefs: PreferenceSet) -> str:
        """Format as plain markdown."""
        lines = ["# Agent Preferences", ""]

        if prefs.sources:
            lines.append(f"*Extracted from {len(prefs.sources)} session(s)*")
            lines.append("")

        # Group by category
        for cat in PreferenceCategory:
            cat_prefs = prefs.by_category(cat)
            if not cat_prefs:
                continue

            # Format category name
            cat_name = cat.value.replace("_", " ").title()
            lines.append(f"## {cat_name}")
            lines.append("")

            for pref in cat_prefs:
                # Confidence indicator
                if self.include_confidence:
                    indicator = {
                        "high": "●",
                        "medium": "◐",
                        "low": "○",
                    }.get(pref.confidence.value, "○")
                    lines.append(f"- {indicator} {pref.rule}")
                else:
                    lines.append(f"- {pref.rule}")

                # Evidence
                if self.include_evidence and pref.evidence:
                    for ev in pref.evidence[:2]:
                        lines.append(f"  - *{ev.method.value}*: {ev.text[:100]}...")

            lines.append("")

        # Legend
        if self.include_confidence:
            lines.append("---")
            lines.append("*● high confidence, ◐ medium confidence, ○ low confidence*")

        return "\n".join(lines)


@dataclass
class JSONAdapter:
    """JSON format adapter."""

    indent: int = 2
    include_evidence: bool = True

    def format(self, prefs: PreferenceSet) -> str:
        """Format as JSON."""
        data = prefs.to_dict()

        # Optionally strip evidence for smaller output
        if not self.include_evidence:
            for cat_prefs in data.get("by_category", {}).values():
                for pref in cat_prefs:
                    pref.pop("evidence", None)

        return json.dumps(data, indent=self.indent)
