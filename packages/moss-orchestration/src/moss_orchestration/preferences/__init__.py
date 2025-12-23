"""Preference extraction from agent session logs.

Extract user preferences from AI coding assistant session logs.
Supports multiple agent formats (Claude Code, Gemini CLI, Cline, etc.)
and outputs to various instruction file formats.

Example:
    from moss_orchestration.preferences import extract_preferences, format_preferences

    # Extract from session logs
    prefs = extract_preferences(["session1.jsonl", "session2.jsonl"])

    # Format for Claude Code
    claude_md = format_preferences(prefs, format="claude")

    # Or with LLM synthesis
    from moss_orchestration.preferences import extract_and_synthesize
    result = extract_and_synthesize(["session.jsonl"], synthesize=True)
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

from moss_orchestration.preferences.extractors import (
    CorrectionsExtractor,
    ExplicitExtractor,
    WorkflowExtractor,
)
from moss_orchestration.preferences.formats import format_preferences, get_adapter
from moss_orchestration.preferences.models import (
    ConfidenceLevel,
    Evidence,
    ExtractionMethod,
    Preference,
    PreferenceCategory,
    PreferenceDiff,
    PreferenceSet,
    diff_preferences,
)
from moss_orchestration.preferences.parsing import LogFormat, ParsedSession, parse_session, parse_sessions


def extract_preferences(
    paths: Sequence[str | Path],
    *,
    log_format: LogFormat = LogFormat.AUTO,
    min_confidence: ConfidenceLevel = ConfidenceLevel.LOW,
) -> PreferenceSet:
    """Extract preferences from session log files.

    Runs all extractors (explicit, corrections, workflow) and combines results.

    Args:
        paths: Paths to session log files
        log_format: Log format (auto-detect if AUTO)
        min_confidence: Minimum confidence level to include

    Returns:
        PreferenceSet with extracted preferences
    """
    # Parse sessions
    sessions = parse_sessions([Path(p) for p in paths], format=log_format)

    # Run extractors
    explicit = ExplicitExtractor()
    corrections = CorrectionsExtractor()
    workflow = WorkflowExtractor()

    result = PreferenceSet(sources=[str(p) for p in paths])

    # Merge results from all extractors
    for extractor in [explicit, corrections, workflow]:
        extracted = extractor.extract(sessions)
        result = result.merge(extracted)

    # Filter by confidence
    if min_confidence != ConfidenceLevel.LOW:
        result.preferences = result.by_confidence(min_confidence)

    return result


def extract_and_synthesize(
    paths: list[str | Path],
    *,
    log_format: LogFormat = LogFormat.AUTO,
    synthesize: bool = False,
    provider: str | None = None,
    model: str | None = None,
    output_format: str = "generic",
) -> str:
    """Extract preferences and optionally synthesize with LLM.

    Args:
        paths: Paths to session log files
        log_format: Log format (auto-detect if AUTO)
        synthesize: Whether to use LLM synthesis
        provider: LLM provider name (for synthesis)
        model: Model name (for synthesis)
        output_format: Output format (claude, gemini, cursor, etc.)

    Returns:
        Formatted preference string
    """
    # Extract
    prefs = extract_preferences(paths, log_format=log_format)

    # Optionally synthesize
    if synthesize:
        from moss_orchestration.preferences.synthesis import synthesize_preferences

        result = synthesize_preferences(prefs, provider=provider, model=model)
        prefs = result.preferences

    # Format
    return format_preferences(prefs, output_format)


__all__ = [
    "ConfidenceLevel",
    "Evidence",
    "ExtractionMethod",
    "LogFormat",
    "ParsedSession",
    "Preference",
    "PreferenceCategory",
    "PreferenceDiff",
    "PreferenceSet",
    "diff_preferences",
    "extract_and_synthesize",
    "extract_preferences",
    "format_preferences",
    "get_adapter",
    "parse_session",
    "parse_sessions",
]
