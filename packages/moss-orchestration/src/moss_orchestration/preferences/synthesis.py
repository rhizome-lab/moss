"""LLM synthesis for preference extraction.

Optionally uses an LLM to convert structured preference data into
natural language rules suitable for agent instruction files.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from moss_orchestration.preferences.models import (
    ConfidenceLevel,
    Preference,
    PreferenceCategory,
    PreferenceSet,
)

if TYPE_CHECKING:
    from moss_llm import LLMProvider


SYNTHESIS_SYSTEM_PROMPT = """You are helping extract coding preferences from conversation analysis.

Given structured preference data extracted from coding sessions, generate clear, actionable rules
that can be added to an AI agent's instruction file (like CLAUDE.md or GEMINI.md).

Guidelines:
- Write rules as imperative instructions ("Do X", "Never Y", "Always Z")
- Be specific and actionable, not vague
- Combine similar rules into one when appropriate
- Remove redundant or contradictory rules
- Keep rules concise (1-2 sentences max)
- Group rules by category
- Prioritize high-confidence rules

Output format:
Return a markdown document with sections for each category, containing bullet-pointed rules.
"""

SYNTHESIS_USER_TEMPLATE = """Here are preferences extracted from {session_count} coding sessions:

{preferences_summary}

Please synthesize these into clear, actionable rules for an AI coding assistant.
Focus on the most important patterns and remove redundancy.
Group by category: Architecture, Workflow, Agent Behavior, Domain Conventions,
Prohibitions, Communication.
"""


@dataclass
class SynthesisResult:
    """Result of LLM synthesis."""

    content: str
    preferences: PreferenceSet
    model: str | None = None
    tokens_used: int = 0


@dataclass
class Synthesizer:
    """Synthesize preferences using an LLM."""

    provider: LLMProvider | None = None
    provider_name: str | None = None
    model: str | None = None

    def __post_init__(self) -> None:
        if self.provider is None:
            # Lazy load provider
            from moss_llm import get_provider

            self.provider = get_provider(self.provider_name, model=self.model)

    def synthesize(self, prefs: PreferenceSet) -> SynthesisResult:
        """Synthesize preferences into natural language rules.

        Args:
            prefs: PreferenceSet with extracted preferences

        Returns:
            SynthesisResult with synthesized content
        """
        if not prefs.preferences:
            return SynthesisResult(
                content="# No Preferences Found\n\nNo preferences extracted.",
                preferences=prefs,
            )

        # Build summary of preferences
        summary = self._build_summary(prefs)

        # Create prompt
        prompt = SYNTHESIS_USER_TEMPLATE.format(
            session_count=len(prefs.sources),
            preferences_summary=summary,
        )

        # Call LLM
        response = self.provider.complete(prompt, system=SYNTHESIS_SYSTEM_PROMPT)

        # Parse response and create updated preference set
        synthesized_prefs = self._parse_response(response.content, prefs)

        return SynthesisResult(
            content=response.content,
            preferences=synthesized_prefs,
            model=response.model,
            tokens_used=response.input_tokens + response.output_tokens,
        )

    def _build_summary(self, prefs: PreferenceSet) -> str:
        """Build a summary of preferences for the LLM."""
        lines = []

        for cat in PreferenceCategory:
            cat_prefs = prefs.by_category(cat)
            if not cat_prefs:
                continue

            cat_name = cat.value.replace("_", " ").title()
            lines.append(f"## {cat_name}")
            lines.append("")

            for pref in cat_prefs:
                confidence = pref.confidence.value
                evidence_count = pref.evidence_count
                lines.append(f"- [{confidence}, {evidence_count} evidence] {pref.rule}")

            lines.append("")

        return "\n".join(lines)

    def _parse_response(self, content: str, original: PreferenceSet) -> PreferenceSet:
        """Parse LLM response back into a PreferenceSet.

        This is a best-effort parse - we extract rules from the markdown
        and create new preferences with high confidence (since they're LLM-curated).
        """
        result = PreferenceSet(
            sources=original.sources,
            metadata={**original.metadata, "synthesized": True},
        )

        # Parse markdown sections
        current_category = PreferenceCategory.AGENT_BEHAVIOR
        category_map = {
            "architecture": PreferenceCategory.ARCHITECTURE,
            "workflow": PreferenceCategory.WORKFLOW,
            "agent behavior": PreferenceCategory.AGENT_BEHAVIOR,
            "behavior": PreferenceCategory.AGENT_BEHAVIOR,
            "domain": PreferenceCategory.DOMAIN,
            "domain conventions": PreferenceCategory.DOMAIN,
            "conventions": PreferenceCategory.DOMAIN,
            "prohibitions": PreferenceCategory.PROHIBITION,
            "restrictions": PreferenceCategory.PROHIBITION,
            "communication": PreferenceCategory.COMMUNICATION,
            "response style": PreferenceCategory.COMMUNICATION,
        }

        for line in content.split("\n"):
            line = line.strip()

            # Check for section headers
            if line.startswith("##"):
                section_name = line.lstrip("#").strip().lower()
                for key, cat in category_map.items():
                    if key in section_name:
                        current_category = cat
                        break

            # Check for list items (rules)
            elif line.startswith(("-", "*", "•")):
                rule = line.lstrip("-*• ").strip()
                if rule and len(rule) > 5:  # Minimal length check
                    result.add(
                        Preference(
                            category=current_category,
                            rule=rule,
                            confidence=ConfidenceLevel.HIGH,  # LLM-curated = high confidence
                            tags=["synthesized"],
                        )
                    )

        return result


def synthesize_preferences(
    prefs: PreferenceSet,
    provider: str | None = None,
    model: str | None = None,
) -> SynthesisResult:
    """Convenience function to synthesize preferences.

    Args:
        prefs: PreferenceSet to synthesize
        provider: LLM provider name (uses default if None)
        model: Model name (uses provider default if None)

    Returns:
        SynthesisResult with synthesized content
    """
    synthesizer = Synthesizer(provider_name=provider, model=model)
    return synthesizer.synthesize(prefs)
