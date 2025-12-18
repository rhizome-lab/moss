"""Preference data models.

Defines the core data structures for extracted preferences.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class PreferenceCategory(Enum):
    """Categories of agent preferences.

    These represent things that can't be expressed in linter configs:
    - Architectural patterns and decisions
    - Agent workflow and behavior
    - Domain-specific conventions
    - Explicit prohibitions
    """

    ARCHITECTURE = "architecture"  # Code structure, patterns, design decisions
    WORKFLOW = "workflow"  # How the agent should work (commit frequency, testing, etc.)
    AGENT_BEHAVIOR = "agent_behavior"  # Agent-specific instructions (verbosity, tools, etc.)
    DOMAIN = "domain"  # Project-specific conventions (naming, formats, etc.)
    PROHIBITION = "prohibition"  # Explicit "never do X" rules
    COMMUNICATION = "communication"  # How agent should communicate (tone, format, etc.)


class ConfidenceLevel(Enum):
    """Confidence in an extracted preference."""

    LOW = "low"  # Single occurrence, might be situational
    MEDIUM = "medium"  # Multiple occurrences or moderate signal
    HIGH = "high"  # Explicit instruction or strong pattern


class ExtractionMethod(Enum):
    """How the preference was extracted."""

    EXPLICIT = "explicit"  # User directly stated the preference
    CORRECTION = "correction"  # Inferred from user correcting agent
    PATTERN = "pattern"  # Inferred from repeated behavior
    WORKFLOW = "workflow"  # Inferred from workflow patterns


@dataclass
class Evidence:
    """Evidence supporting an extracted preference."""

    source: str  # File path or session identifier
    text: str  # The actual text/content
    method: ExtractionMethod
    timestamp: str | None = None  # ISO timestamp if available

    def to_dict(self) -> dict[str, Any]:
        return {
            "source": self.source,
            "text": self.text[:200] + "..." if len(self.text) > 200 else self.text,
            "method": self.method.value,
            "timestamp": self.timestamp,
        }


@dataclass
class Preference:
    """A single extracted preference.

    Represents one rule or guideline that should be communicated to an AI agent.
    """

    category: PreferenceCategory
    rule: str  # The preference as a natural language rule
    confidence: ConfidenceLevel
    evidence: list[Evidence] = field(default_factory=list)
    tags: list[str] = field(default_factory=list)  # Additional categorization

    # For tracking/deduplication
    id: str | None = None  # Unique identifier
    supersedes: list[str] = field(default_factory=list)  # IDs of preferences this replaces

    @property
    def evidence_count(self) -> int:
        """Number of evidence items supporting this preference."""
        return len(self.evidence)

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "category": self.category.value,
            "rule": self.rule,
            "confidence": self.confidence.value,
            "evidence_count": self.evidence_count,
            "evidence": [e.to_dict() for e in self.evidence[:5]],  # Limit for output
            "tags": self.tags,
        }

    def to_markdown(self) -> str:
        """Format as a markdown list item."""
        confidence_emoji = {
            ConfidenceLevel.HIGH: "🟢",
            ConfidenceLevel.MEDIUM: "🟡",
            ConfidenceLevel.LOW: "🔴",
        }
        emoji = confidence_emoji.get(self.confidence, "")
        return f"- {emoji} {self.rule}"


@dataclass
class PreferenceSet:
    """A collection of preferences extracted from one or more sessions.

    Groups preferences by category and provides aggregation methods.
    """

    preferences: list[Preference] = field(default_factory=list)
    sources: list[str] = field(default_factory=list)  # Session files analyzed
    metadata: dict[str, Any] = field(default_factory=dict)

    def add(self, pref: Preference) -> None:
        """Add a preference to the set."""
        self.preferences.append(pref)

    def by_category(self, category: PreferenceCategory) -> list[Preference]:
        """Get preferences in a specific category."""
        return [p for p in self.preferences if p.category == category]

    def by_confidence(self, min_confidence: ConfidenceLevel) -> list[Preference]:
        """Get preferences at or above a confidence level."""
        levels = [ConfidenceLevel.LOW, ConfidenceLevel.MEDIUM, ConfidenceLevel.HIGH]
        min_idx = levels.index(min_confidence)
        return [p for p in self.preferences if levels.index(p.confidence) >= min_idx]

    def high_confidence(self) -> list[Preference]:
        """Get only high-confidence preferences."""
        return self.by_confidence(ConfidenceLevel.HIGH)

    def merge(self, other: PreferenceSet) -> PreferenceSet:
        """Merge two preference sets, deduplicating rules."""
        merged = PreferenceSet(
            sources=list(set(self.sources + other.sources)),
            metadata={**self.metadata, **other.metadata},
        )

        # Simple deduplication by rule text
        seen_rules: set[str] = set()
        for pref in self.preferences + other.preferences:
            rule_key = pref.rule.lower().strip()
            if rule_key not in seen_rules:
                seen_rules.add(rule_key)
                merged.add(pref)

        return merged

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        by_cat: dict[str, list[dict]] = {}
        for cat in PreferenceCategory:
            prefs = self.by_category(cat)
            if prefs:
                by_cat[cat.value] = [p.to_dict() for p in prefs]

        return {
            "sources": self.sources,
            "total_preferences": len(self.preferences),
            "by_category": by_cat,
            "metadata": self.metadata,
        }

    def to_markdown(self) -> str:
        """Format as markdown document."""
        lines = ["# Extracted Preferences", ""]

        if self.sources:
            lines.append(f"*Extracted from {len(self.sources)} session(s)*")
            lines.append("")

        for cat in PreferenceCategory:
            prefs = self.by_category(cat)
            if prefs:
                # Format category name nicely
                cat_name = cat.value.replace("_", " ").title()
                lines.append(f"## {cat_name}")
                lines.append("")
                for pref in prefs:
                    lines.append(pref.to_markdown())
                lines.append("")

        return "\n".join(lines)


@dataclass
class PreferenceDiff:
    """Difference between two preference sets.

    Used to track preference drift over time.
    """

    added: list[Preference] = field(default_factory=list)
    removed: list[Preference] = field(default_factory=list)
    changed: list[tuple[Preference, Preference]] = field(default_factory=list)  # (old, new)

    @property
    def has_changes(self) -> bool:
        """Check if there are any differences."""
        return bool(self.added or self.removed or self.changed)

    def to_dict(self) -> dict[str, Any]:
        return {
            "added": [p.to_dict() for p in self.added],
            "removed": [p.to_dict() for p in self.removed],
            "changed": [{"old": old.to_dict(), "new": new.to_dict()} for old, new in self.changed],
        }

    def to_markdown(self) -> str:
        """Format as markdown."""
        lines = ["# Preference Changes", ""]

        if self.added:
            lines.append("## Added")
            lines.append("")
            for pref in self.added:
                lines.append(f"+ {pref.rule}")
            lines.append("")

        if self.removed:
            lines.append("## Removed")
            lines.append("")
            for pref in self.removed:
                lines.append(f"- {pref.rule}")
            lines.append("")

        if self.changed:
            lines.append("## Changed")
            lines.append("")
            for old, new in self.changed:
                lines.append(f"- {old.rule}")
                lines.append(f"+ {new.rule}")
                lines.append("")

        if not self.has_changes:
            lines.append("*No changes detected*")

        return "\n".join(lines)


def diff_preferences(old: PreferenceSet, new: PreferenceSet) -> PreferenceDiff:
    """Compare two preference sets and return differences.

    Args:
        old: Previous preference set
        new: Current preference set

    Returns:
        PreferenceDiff with added, removed, and changed preferences
    """
    diff = PreferenceDiff()

    # Build lookup by rule (normalized)
    def normalize(rule: str) -> str:
        return rule.lower().strip()

    old_by_rule = {normalize(p.rule): p for p in old.preferences}
    new_by_rule = {normalize(p.rule): p for p in new.preferences}

    old_rules = set(old_by_rule.keys())
    new_rules = set(new_by_rule.keys())

    # Added
    for rule in new_rules - old_rules:
        diff.added.append(new_by_rule[rule])

    # Removed
    for rule in old_rules - new_rules:
        diff.removed.append(old_by_rule[rule])

    # Changed (same rule but different confidence or category)
    for rule in old_rules & new_rules:
        old_pref = old_by_rule[rule]
        new_pref = new_by_rule[rule]
        if old_pref.confidence != new_pref.confidence or old_pref.category != new_pref.category:
            diff.changed.append((old_pref, new_pref))

    return diff
