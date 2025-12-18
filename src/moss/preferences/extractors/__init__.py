"""Preference extractors.

Each extractor analyzes sessions for a specific type of preference signal:
- explicit: Direct instructions ("always X", "never Y")
- corrections: User edits after assistant actions
- workflow: Patterns in how the user works with the agent

Extractors can be registered via entry points or programmatically.

Entry point group: moss.preferences.extractors

Example plugin registration in pyproject.toml:
    [project.entry-points."moss.preferences.extractors"]
    my_extractor = "my_package.extractors:MyExtractor"
"""

from __future__ import annotations

from importlib.metadata import entry_points
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


# Extractor registry
_EXTRACTORS: dict[str, type[Extractor]] = {}


def register_extractor(name: str, extractor_class: type[Extractor]) -> None:
    """Register a preference extractor.

    Args:
        name: Extractor name (e.g., "explicit", "corrections")
        extractor_class: Extractor class implementing Extractor protocol
    """
    _EXTRACTORS[name] = extractor_class


def get_extractor(name: str) -> Extractor:
    """Get an extractor instance by name.

    Args:
        name: Extractor name

    Returns:
        Extractor instance

    Raises:
        ValueError: If extractor not found
    """
    if name not in _EXTRACTORS:
        available = ", ".join(_EXTRACTORS.keys())
        raise ValueError(f"Extractor '{name}' not found. Available: {available}")
    return _EXTRACTORS[name]()


def list_extractors() -> list[str]:
    """List all registered extractor names."""
    return list(_EXTRACTORS.keys())


def get_all_extractors() -> list[Extractor]:
    """Get instances of all registered extractors."""
    return [cls() for cls in _EXTRACTORS.values()]


def _discover_entry_points() -> None:
    """Discover and register extractors from entry points."""
    try:
        eps = entry_points(group="moss.preferences.extractors")
        for ep in eps:
            try:
                extractor_class = ep.load()
                if ep.name not in _EXTRACTORS:
                    register_extractor(ep.name, extractor_class)
            except Exception:
                pass
    except Exception:
        pass


def _register_builtin_extractors() -> None:
    """Register built-in extractors."""
    register_extractor("explicit", ExplicitExtractor)
    register_extractor("corrections", CorrectionsExtractor)
    register_extractor("workflow", WorkflowExtractor)


# Auto-register on import
_register_builtin_extractors()
_discover_entry_points()

__all__ = [
    "CorrectionsExtractor",
    "ExplicitExtractor",
    "Extractor",
    "WorkflowExtractor",
    "get_all_extractors",
    "get_extractor",
    "list_extractors",
    "register_extractor",
]
