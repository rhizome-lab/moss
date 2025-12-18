"""Corrections extractor.

Extracts preferences by analyzing when users correct or modify
what the assistant did. This is high-signal data.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from moss.preferences.models import (
    ConfidenceLevel,
    Evidence,
    ExtractionMethod,
    Preference,
    PreferenceCategory,
    PreferenceSet,
)
from moss.preferences.parsing import ParsedSession, TurnPair

# Patterns that indicate correction intent
CORRECTION_PATTERNS = [
    (r"no[,.]?\s+(.+)", "negation"),
    (r"actually[,]?\s+(.+)", "revision"),
    (r"that'?s\s+(?:not|wrong)", "rejection"),
    (r"should\s+(?:be|have)\s+(.+)", "correction"),
    (r"change\s+(?:it|that|this)\s+to\s+(.+)", "change_request"),
    (r"(?:use|do)\s+(.+?)\s+instead", "alternative"),
    (r"i\s+(?:meant|wanted|said)\s+(.+)", "clarification"),
    (r"not\s+(.+?)[,.]?\s+(?:but|use)\s+(.+)", "substitution"),
    (r"don'?t\s+(.+)", "prohibition"),
    (r"stop\s+(.+)", "prohibition"),
    (r"why\s+did\s+you\s+(.+)", "questioning"),
    (r"i\s+didn'?t\s+(?:ask|want)\s+(.+)", "unwanted"),
    (r"remove\s+(?:the\s+)?(.+)", "removal"),
    (r"undo\s+(.+)", "reversal"),
]


@dataclass
class CorrectionInstance:
    """A detected correction instance."""

    pattern_type: str
    user_text: str
    assistant_context: str
    files_affected: list[str] = field(default_factory=list)
    source: str = ""


@dataclass
class CorrectionsExtractor:
    """Extract preferences from user corrections."""

    def extract(self, sessions: list[ParsedSession]) -> PreferenceSet:
        """Extract preferences from correction patterns.

        Args:
            sessions: List of parsed sessions

        Returns:
            PreferenceSet with inferred preferences from corrections
        """
        result = PreferenceSet(sources=[str(s.path) for s in sessions])

        # Collect all corrections
        corrections: list[CorrectionInstance] = []
        for session in sessions:
            pairs = session.turn_pairs()
            for pair in pairs:
                if pair.is_correction and pair.user:
                    corr = self._analyze_correction(pair, str(session.path))
                    if corr:
                        corrections.append(corr)

        # Group similar corrections
        grouped = self._group_corrections(corrections)

        # Convert to preferences
        for group in grouped:
            pref = self._create_preference(group)
            if pref:
                result.add(pref)

        return result

    def _analyze_correction(self, pair: TurnPair, source: str) -> CorrectionInstance | None:
        """Analyze a correction to extract the pattern."""
        if not pair.user:
            return None

        user_text = pair.user.content.lower().strip()

        # Try to match correction patterns
        for pattern, pattern_type in CORRECTION_PATTERNS:
            match = re.search(pattern, user_text, re.IGNORECASE)
            if match:
                return CorrectionInstance(
                    pattern_type=pattern_type,
                    user_text=pair.user.content,
                    assistant_context=pair.assistant.content[:500],
                    files_affected=pair.assistant.files_written,
                    source=source,
                )

        # Check for file overlap (user edited same file)
        if pair.user.files_written:
            assistant_files = set(pair.assistant.files_written)
            user_files = set(pair.user.files_written)
            if assistant_files & user_files:
                return CorrectionInstance(
                    pattern_type="file_edit",
                    user_text=pair.user.content,
                    assistant_context=pair.assistant.content[:500],
                    files_affected=list(assistant_files & user_files),
                    source=source,
                )

        return None

    def _group_corrections(
        self, corrections: list[CorrectionInstance]
    ) -> list[list[CorrectionInstance]]:
        """Group similar corrections together."""
        if not corrections:
            return []

        # Simple grouping by pattern type and key phrases
        groups: dict[str, list[CorrectionInstance]] = {}

        for corr in corrections:
            # Create a grouping key
            key_parts = [corr.pattern_type]

            # Add key phrases from user text
            key_phrases = self._extract_key_phrases(corr.user_text)
            if key_phrases:
                key_parts.append(key_phrases[0])

            key = ":".join(key_parts)

            if key not in groups:
                groups[key] = []
            groups[key].append(corr)

        return list(groups.values())

    def _extract_key_phrases(self, text: str) -> list[str]:
        """Extract key phrases from correction text."""
        # Remove common words and extract meaningful phrases
        text = text.lower()

        # Remove filler words
        fillers = [
            "the",
            "a",
            "an",
            "is",
            "are",
            "was",
            "were",
            "be",
            "been",
            "being",
            "it",
            "that",
            "this",
            "to",
            "of",
            "for",
            "with",
            "in",
            "on",
            "at",
        ]

        words = re.findall(r"\b\w+\b", text)
        meaningful = [w for w in words if w not in fillers and len(w) > 2]

        return meaningful[:3]  # Return top 3 meaningful words

    def _create_preference(self, group: list[CorrectionInstance]) -> Preference | None:
        """Create a preference from a group of corrections."""
        if not group:
            return None

        # Determine confidence based on group size
        if len(group) >= 3:
            confidence = ConfidenceLevel.HIGH
        elif len(group) >= 2:
            confidence = ConfidenceLevel.MEDIUM
        else:
            confidence = ConfidenceLevel.LOW

        # Analyze the pattern type
        first = group[0]
        pattern_type = first.pattern_type

        # Generate a rule based on the correction type
        rule = self._generate_rule(group)
        if not rule:
            return None

        # Determine category
        category = self._determine_category(group)

        # Build evidence
        evidence = [
            Evidence(
                source=corr.source,
                text=corr.user_text[:200],
                method=ExtractionMethod.CORRECTION,
            )
            for corr in group[:5]  # Limit evidence
        ]

        return Preference(
            category=category,
            rule=rule,
            confidence=confidence,
            evidence=evidence,
            tags=[f"correction:{pattern_type}"],
        )

    def _generate_rule(self, group: list[CorrectionInstance]) -> str | None:
        """Generate a natural language rule from corrections."""
        if not group:
            return None

        first = group[0]
        pattern_type = first.pattern_type

        # Try to extract the core instruction
        user_text = first.user_text

        if pattern_type == "prohibition":
            # Extract what they don't want
            match = re.search(r"(?:don'?t|stop)\s+(.+?)(?:\.|$)", user_text, re.IGNORECASE)
            if match:
                action = match.group(1).strip()
                return f"Do not {action.lower()}"

        elif pattern_type == "substitution":
            match = re.search(
                r"not\s+(.+?)[,.]?\s+(?:but|use)\s+(.+?)(?:\.|$)", user_text, re.IGNORECASE
            )
            if match:
                wrong, right = match.group(1).strip(), match.group(2).strip()
                return f"Use {right} instead of {wrong}"

        elif pattern_type == "alternative":
            match = re.search(r"(?:use|do)\s+(.+?)\s+instead", user_text, re.IGNORECASE)
            if match:
                return f"Prefer {match.group(1).strip()}"

        elif pattern_type == "correction":
            match = re.search(r"should\s+(?:be|have)\s+(.+?)(?:\.|$)", user_text, re.IGNORECASE)
            if match:
                return match.group(1).strip().capitalize()

        elif pattern_type == "file_edit":
            # User edited the same file - less specific
            files = first.files_affected
            if files:
                return f"Review changes to {', '.join(files)} before finalizing"

        # Fallback: use key phrases
        phrases = self._extract_key_phrases(user_text)
        if phrases:
            return f"User corrected regarding: {' '.join(phrases)}"

        return None

    def _determine_category(self, group: list[CorrectionInstance]) -> PreferenceCategory:
        """Determine the category based on correction content."""
        # Check for patterns in the correction text
        all_text = " ".join(c.user_text.lower() for c in group)

        if any(word in all_text for word in ["commit", "push", "test", "build", "branch", "merge"]):
            return PreferenceCategory.WORKFLOW

        if any(
            word in all_text for word in ["pattern", "class", "function", "module", "structure"]
        ):
            return PreferenceCategory.ARCHITECTURE

        if any(
            word in all_text for word in ["never", "don't", "stop", "remove", "delete", "avoid"]
        ):
            return PreferenceCategory.PROHIBITION

        if any(word in all_text for word in ["verbose", "concise", "explain", "ask", "wait"]):
            return PreferenceCategory.AGENT_BEHAVIOR

        # Default
        return PreferenceCategory.AGENT_BEHAVIOR
