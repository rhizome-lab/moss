"""Preference extractors.

Each extractor analyzes sessions for a specific type of preference signal:
- explicit: Direct instructions ("always X", "never Y")
- corrections: User edits after assistant actions
- workflow: Patterns in how the user works with the agent
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Protocol

from moss.preferences.extractors.corrections import CorrectionsExtractor
from moss.preferences.extractors.explicit import ExplicitExtractor
from moss.preferences.extractors.workflow import WorkflowExtractor

if TYPE_CHECKING:
    from moss.preferences.models import PreferenceSet
    from moss.preferences.parsing import ParsedSession


class Extractor(Protocol):
    """Protocol for preference extractors."""

    def extract(self, sessions: list[ParsedSession]) -> PreferenceSet:
        """Extract preferences from parsed sessions.

        Args:
            sessions: List of parsed session data

        Returns:
            PreferenceSet with extracted preferences
        """
        ...


__all__ = [
    "CorrectionsExtractor",
    "ExplicitExtractor",
    "Extractor",
    "WorkflowExtractor",
]
