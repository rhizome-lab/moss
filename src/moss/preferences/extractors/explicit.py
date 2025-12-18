"""Explicit instruction extractor.

Extracts preferences from direct user instructions containing
keywords like "always", "never", "prefer", "don't", etc.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from moss.preferences.models import (
    ConfidenceLevel,
    Evidence,
    ExtractionMethod,
    Preference,
    PreferenceCategory,
    PreferenceSet,
)
from moss.preferences.parsing import ParsedSession

# Patterns that indicate explicit preferences
EXPLICIT_PATTERNS = [
    # High confidence - direct instructions
    (r"\balways\s+(.+?)(?:\.|$)", ConfidenceLevel.HIGH),
    (r"\bnever\s+(.+?)(?:\.|$)", ConfidenceLevel.HIGH),
    (r"\bdon'?t\s+(?:ever\s+)?(.+?)(?:\.|$)", ConfidenceLevel.HIGH),
    (r"\bdo\s+not\s+(.+?)(?:\.|$)", ConfidenceLevel.HIGH),
    (r"\bmust\s+(?:always\s+)?(.+?)(?:\.|$)", ConfidenceLevel.HIGH),
    (r"\bmust\s+not\s+(.+?)(?:\.|$)", ConfidenceLevel.HIGH),
    # Medium confidence - preferences
    (r"\bprefer\s+(.+?)(?:\.|$)", ConfidenceLevel.MEDIUM),
    (r"\bplease\s+(?:always\s+)?(.+?)(?:\.|$)", ConfidenceLevel.MEDIUM),
    (r"\bmake\s+sure\s+(?:to\s+)?(.+?)(?:\.|$)", ConfidenceLevel.MEDIUM),
    (r"\bensure\s+(?:that\s+)?(.+?)(?:\.|$)", ConfidenceLevel.MEDIUM),
    (r"\bi\s+want\s+(?:you\s+to\s+)?(.+?)(?:\.|$)", ConfidenceLevel.MEDIUM),
    (r"\bi\s+(?:would\s+)?like\s+(?:you\s+to\s+)?(.+?)(?:\.|$)", ConfidenceLevel.MEDIUM),
    # Low confidence - suggestions
    (r"\btry\s+to\s+(.+?)(?:\.|$)", ConfidenceLevel.LOW),
    (r"\bit(?:'s|s)\s+better\s+(?:to\s+)?(.+?)(?:\.|$)", ConfidenceLevel.LOW),
    (r"\bavoid\s+(.+?)(?:\.|$)", ConfidenceLevel.MEDIUM),
]

# Category detection patterns
CATEGORY_PATTERNS = {
    PreferenceCategory.ARCHITECTURE: [
        r"\b(?:pattern|architecture|design|structure|module|class|function|component)\b",
        r"\b(?:composition|inheritance|abstraction|interface|dependency)\b",
        r"\b(?:separation|coupling|cohesion|encapsulation)\b",
    ],
    PreferenceCategory.WORKFLOW: [
        r"\b(?:commit|push|pull|branch|merge|test|build|deploy)\b",
        r"\b(?:before|after|first|then|step|process)\b",
        r"\b(?:review|check|verify|validate|confirm)\b",
    ],
    PreferenceCategory.AGENT_BEHAVIOR: [
        r"\b(?:ask|prompt|wait|pause|stop|continue|proceed)\b",
        r"\b(?:verbose|concise|brief|detailed|explain)\b",
        r"\b(?:tool|read|write|edit|search|create)\b",
    ],
    PreferenceCategory.DOMAIN: [
        r"\b(?:naming|convention|format|style|timestamp|id|uuid)\b",
        r"\b(?:error|exception|result|return|response)\b",
        r"\b(?:api|endpoint|route|handler|middleware)\b",
    ],
    PreferenceCategory.PROHIBITION: [
        r"\b(?:never|don'?t|do\s+not|must\s+not|forbidden|prohibited)\b",
        r"\b(?:avoid|skip|ignore|remove|delete)\b",
    ],
    PreferenceCategory.COMMUNICATION: [
        r"\b(?:emoji|tone|format|markdown|response|message)\b",
        r"\b(?:concise|verbose|brief|detailed|short|long)\b",
    ],
}


@dataclass
class ExplicitExtractor:
    """Extract preferences from explicit user instructions."""

    min_instruction_length: int = 10  # Minimum chars for a valid instruction
    max_instruction_length: int = 500  # Maximum chars to consider

    def extract(self, sessions: list[ParsedSession]) -> PreferenceSet:
        """Extract explicit preferences from user messages.

        Args:
            sessions: List of parsed sessions

        Returns:
            PreferenceSet with extracted explicit preferences
        """
        result = PreferenceSet(sources=[str(s.path) for s in sessions])

        for session in sessions:
            for turn in session.user_messages():
                preferences = self._extract_from_text(turn.content, str(session.path))
                for pref in preferences:
                    result.add(pref)

        # Deduplicate and merge similar preferences
        result = self._deduplicate(result)

        return result

    def _extract_from_text(self, text: str, source: str) -> list[Preference]:
        """Extract preferences from a single text."""
        preferences = []

        # Normalize text
        text = text.strip()
        if len(text) < self.min_instruction_length:
            return preferences

        for pattern, confidence in EXPLICIT_PATTERNS:
            matches = re.finditer(pattern, text, re.IGNORECASE | re.MULTILINE)
            for match in matches:
                instruction = match.group(1).strip() if match.lastindex else match.group(0)

                # Skip if too short or too long
                if (
                    len(instruction) < self.min_instruction_length
                    or len(instruction) > self.max_instruction_length
                ):
                    continue

                # Clean up instruction
                instruction = self._clean_instruction(instruction)
                if not instruction:
                    continue

                # Determine category
                category = self._categorize(instruction, text)

                # Create preference
                pref = Preference(
                    category=category,
                    rule=instruction,
                    confidence=confidence,
                    evidence=[
                        Evidence(
                            source=source,
                            text=text[: min(200, len(text))],
                            method=ExtractionMethod.EXPLICIT,
                        )
                    ],
                )
                preferences.append(pref)

        return preferences

    def _clean_instruction(self, text: str) -> str:
        """Clean up an extracted instruction."""
        # Remove leading/trailing punctuation and whitespace
        text = text.strip(" \t\n.,;:")

        # Remove common filler words at the start
        filler_starts = ["that you", "to", "that", "you"]
        for filler in filler_starts:
            if text.lower().startswith(filler + " "):
                text = text[len(filler) + 1 :]

        # Capitalize first letter
        if text:
            text = text[0].upper() + text[1:]

        return text.strip()

    def _categorize(self, instruction: str, context: str) -> PreferenceCategory:
        """Determine the category of an instruction."""
        full_text = f"{instruction} {context}".lower()

        # Check prohibition first (strongest signal from keywords)
        for pattern in CATEGORY_PATTERNS[PreferenceCategory.PROHIBITION]:
            if re.search(pattern, instruction.lower()):
                return PreferenceCategory.PROHIBITION

        # Check other categories
        scores: dict[PreferenceCategory, int] = {}
        for category, patterns in CATEGORY_PATTERNS.items():
            if category == PreferenceCategory.PROHIBITION:
                continue
            score = sum(1 for p in patterns if re.search(p, full_text))
            if score > 0:
                scores[category] = score

        if scores:
            return max(scores, key=scores.get)  # type: ignore

        # Default to agent_behavior for uncategorized
        return PreferenceCategory.AGENT_BEHAVIOR

    def _deduplicate(self, pref_set: PreferenceSet) -> PreferenceSet:
        """Deduplicate similar preferences, boosting confidence for repeated ones."""
        # Group by normalized rule
        groups: dict[str, list[Preference]] = {}
        for pref in pref_set.preferences:
            key = self._normalize_rule(pref.rule)
            if key not in groups:
                groups[key] = []
            groups[key].append(pref)

        # Merge groups
        result = PreferenceSet(sources=pref_set.sources)
        for prefs in groups.values():
            if len(prefs) == 1:
                result.add(prefs[0])
            else:
                # Merge: take highest confidence, combine evidence
                merged = prefs[0]
                for pref in prefs[1:]:
                    if pref.confidence.value > merged.confidence.value:
                        merged.confidence = pref.confidence
                    merged.evidence.extend(pref.evidence)

                # Boost confidence if seen multiple times
                if len(prefs) >= 3 and merged.confidence != ConfidenceLevel.HIGH:
                    merged.confidence = ConfidenceLevel.HIGH
                elif len(prefs) >= 2 and merged.confidence == ConfidenceLevel.LOW:
                    merged.confidence = ConfidenceLevel.MEDIUM

                result.add(merged)

        return result

    def _normalize_rule(self, rule: str) -> str:
        """Normalize a rule for deduplication."""
        # Lowercase, remove extra whitespace, remove punctuation
        text = rule.lower()
        text = re.sub(r"\s+", " ", text)
        text = re.sub(r"[^\w\s]", "", text)
        return text.strip()
