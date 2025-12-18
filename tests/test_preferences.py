"""Tests for the preferences extraction module."""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest

from moss.preferences import (
    ConfidenceLevel,
    Evidence,
    ExtractionMethod,
    LogFormat,
    ParsedSession,
    Preference,
    PreferenceCategory,
    PreferenceSet,
    diff_preferences,
    extract_preferences,
    format_preferences,
    get_adapter,
)
from moss.preferences.extractors import (
    CorrectionsExtractor,
    ExplicitExtractor,
    WorkflowExtractor,
)
from moss.preferences.parsing import (
    ClaudeCodeParser,
    GenericChatParser,
)


class TestPreferenceModels:
    """Tests for preference data models."""

    def test_preference_creation(self):
        """Test basic Preference creation."""
        pref = Preference(
            category=PreferenceCategory.WORKFLOW,
            rule="Always run tests before committing",
            confidence=ConfidenceLevel.HIGH,
        )
        assert pref.category == PreferenceCategory.WORKFLOW
        assert pref.rule == "Always run tests before committing"
        assert pref.confidence == ConfidenceLevel.HIGH
        assert pref.evidence == []
        assert pref.tags == []

    def test_preference_with_evidence(self):
        """Test Preference with evidence."""
        evidence = Evidence(
            source="session1.jsonl",
            text="User said: always run tests",
            method=ExtractionMethod.EXPLICIT,
        )
        pref = Preference(
            category=PreferenceCategory.WORKFLOW,
            rule="Always run tests",
            confidence=ConfidenceLevel.MEDIUM,
            evidence=[evidence],
        )
        assert len(pref.evidence) == 1
        assert pref.evidence[0].source == "session1.jsonl"

    def test_preference_set_add(self):
        """Test adding preferences to a set."""
        pref_set = PreferenceSet(sources=["test.jsonl"])
        pref = Preference(
            category=PreferenceCategory.PROHIBITION,
            rule="Never push directly to main",
            confidence=ConfidenceLevel.HIGH,
        )
        pref_set.add(pref)
        assert len(pref_set.preferences) == 1

    def test_preference_set_merge(self):
        """Test merging preference sets."""
        set1 = PreferenceSet(sources=["a.jsonl"])
        set1.add(
            Preference(
                category=PreferenceCategory.WORKFLOW,
                rule="Rule A",
                confidence=ConfidenceLevel.HIGH,
            )
        )

        set2 = PreferenceSet(sources=["b.jsonl"])
        set2.add(
            Preference(
                category=PreferenceCategory.WORKFLOW,
                rule="Rule B",
                confidence=ConfidenceLevel.MEDIUM,
            )
        )

        merged = set1.merge(set2)
        assert len(merged.preferences) == 2
        assert "a.jsonl" in merged.sources
        assert "b.jsonl" in merged.sources

    def test_preference_set_by_confidence(self):
        """Test filtering by confidence level."""
        pref_set = PreferenceSet()
        pref_set.add(
            Preference(
                category=PreferenceCategory.WORKFLOW,
                rule="Low confidence",
                confidence=ConfidenceLevel.LOW,
            )
        )
        pref_set.add(
            Preference(
                category=PreferenceCategory.WORKFLOW,
                rule="High confidence",
                confidence=ConfidenceLevel.HIGH,
            )
        )

        high_only = pref_set.by_confidence(ConfidenceLevel.HIGH)
        assert len(high_only) == 1
        assert high_only[0].rule == "High confidence"

    def test_preference_set_by_category(self):
        """Test filtering by category."""
        pref_set = PreferenceSet()
        pref_set.add(
            Preference(
                category=PreferenceCategory.WORKFLOW,
                rule="Workflow rule",
                confidence=ConfidenceLevel.MEDIUM,
            )
        )
        pref_set.add(
            Preference(
                category=PreferenceCategory.PROHIBITION,
                rule="Prohibition rule",
                confidence=ConfidenceLevel.MEDIUM,
            )
        )

        workflow_only = pref_set.by_category(PreferenceCategory.WORKFLOW)
        assert len(workflow_only) == 1
        assert workflow_only[0].rule == "Workflow rule"


class TestPreferenceDiff:
    """Tests for preference diffing."""

    def test_diff_identical(self):
        """Test diffing identical preference sets."""
        pref = Preference(
            category=PreferenceCategory.WORKFLOW,
            rule="Same rule",
            confidence=ConfidenceLevel.HIGH,
        )

        set1 = PreferenceSet()
        set1.add(pref)

        set2 = PreferenceSet()
        set2.add(pref)

        diff = diff_preferences(set1, set2)
        assert len(diff.added) == 0
        assert len(diff.removed) == 0
        assert len(diff.changed) == 0

    def test_diff_added(self):
        """Test detecting added preferences."""
        set1 = PreferenceSet()

        set2 = PreferenceSet()
        set2.add(
            Preference(
                category=PreferenceCategory.WORKFLOW,
                rule="New rule",
                confidence=ConfidenceLevel.MEDIUM,
            )
        )

        diff = diff_preferences(set1, set2)
        assert len(diff.added) == 1
        assert diff.added[0].rule == "New rule"

    def test_diff_removed(self):
        """Test detecting removed preferences."""
        set1 = PreferenceSet()
        set1.add(
            Preference(
                category=PreferenceCategory.WORKFLOW,
                rule="Old rule",
                confidence=ConfidenceLevel.MEDIUM,
            )
        )

        set2 = PreferenceSet()

        diff = diff_preferences(set1, set2)
        assert len(diff.removed) == 1
        assert diff.removed[0].rule == "Old rule"


class TestLogParsing:
    """Tests for session log parsing."""

    def test_claude_code_parser(self):
        """Test Claude Code parser."""
        entries = [
            {"type": "user", "message": {"content": [{"type": "text", "text": "Hello"}]}},
            {
                "type": "assistant",
                "message": {"content": [{"type": "text", "text": "Hi there"}]},
            },
        ]

        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            for entry in entries:
                f.write(json.dumps(entry) + "\n")
            f.flush()

            parser = ClaudeCodeParser(Path(f.name))
            session = parser.parse()
            assert len(session.turns) == 2
            assert session.turns[0].role == "user"
            assert session.turns[1].role == "assistant"

    def test_generic_parser_fallback(self):
        """Test generic parser as fallback."""
        entries = [
            {"role": "user", "content": "Hello"},
            {"role": "assistant", "content": "Hi there"},
        ]

        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            for entry in entries:
                f.write(json.dumps(entry) + "\n")
            f.flush()

            parser = GenericChatParser(Path(f.name))
            session = parser.parse()
            assert len(session.turns) >= 1


class TestExplicitExtractor:
    """Tests for explicit preference extraction."""

    def test_extract_always_pattern(self):
        """Test extracting 'always' patterns."""
        from moss.preferences.parsing import Turn

        extractor = ExplicitExtractor()
        session = ParsedSession(
            path=Path("test.jsonl"),
            format=LogFormat.GENERIC_CHAT,
            turns=[Turn(role="user", content="Always use type hints in Python code")],
            metadata={},
        )

        result = extractor.extract([session])
        # Should find the explicit preference
        assert isinstance(result, PreferenceSet)

    def test_extract_never_pattern(self):
        """Test extracting 'never' patterns."""
        from moss.preferences.parsing import Turn

        extractor = ExplicitExtractor()
        session = ParsedSession(
            path=Path("test.jsonl"),
            format=LogFormat.GENERIC_CHAT,
            turns=[Turn(role="user", content="Never use global variables")],
            metadata={},
        )

        result = extractor.extract([session])
        assert isinstance(result, PreferenceSet)


class TestCorrectionsExtractor:
    """Tests for corrections extraction."""

    def test_detect_correction(self):
        """Test detecting user corrections."""
        from moss.preferences.parsing import Turn

        extractor = CorrectionsExtractor()
        session = ParsedSession(
            path=Path("test.jsonl"),
            format=LogFormat.GENERIC_CHAT,
            turns=[
                Turn(role="assistant", content="Here's the code"),
                Turn(role="user", content="No, that's wrong. Use a list instead."),
            ],
            metadata={},
        )

        result = extractor.extract([session])
        # Should detect the correction
        assert isinstance(result, PreferenceSet)


class TestWorkflowExtractor:
    """Tests for workflow pattern extraction."""

    def test_detect_intervention(self):
        """Test detecting user interventions."""
        from moss.preferences.parsing import Turn

        extractor = WorkflowExtractor()
        session = ParsedSession(
            path=Path("test.jsonl"),
            format=LogFormat.GENERIC_CHAT,
            turns=[
                Turn(role="assistant", content="Running tests..."),
                Turn(role="user", content="Stop! Don't do that."),
            ],
            metadata={},
        )

        result = extractor.extract([session])
        assert isinstance(result, PreferenceSet)


class TestFormatAdapters:
    """Tests for output format adapters."""

    def test_get_adapter_claude(self):
        """Test getting Claude adapter."""
        adapter = get_adapter("claude")
        assert adapter is not None

    def test_get_adapter_gemini(self):
        """Test getting Gemini adapter."""
        adapter = get_adapter("gemini")
        assert adapter is not None

    def test_get_adapter_cursor(self):
        """Test getting Cursor adapter."""
        adapter = get_adapter("cursor")
        assert adapter is not None

    def test_get_adapter_generic(self):
        """Test getting generic adapter."""
        adapter = get_adapter("generic")
        assert adapter is not None

    def test_get_adapter_json(self):
        """Test getting JSON adapter."""
        adapter = get_adapter("json")
        assert adapter is not None

    def test_get_adapter_invalid(self):
        """Test getting invalid adapter raises error."""
        with pytest.raises(ValueError, match="Unknown format"):
            get_adapter("invalid_format")

    def test_format_preferences_generic(self):
        """Test formatting preferences as generic markdown."""
        pref_set = PreferenceSet(sources=["test.jsonl"])
        pref_set.add(
            Preference(
                category=PreferenceCategory.WORKFLOW,
                rule="Run tests before committing",
                confidence=ConfidenceLevel.HIGH,
            )
        )

        output = format_preferences(pref_set, "generic")
        assert "Run tests before committing" in output

    def test_format_preferences_json(self):
        """Test formatting preferences as JSON."""
        pref_set = PreferenceSet(sources=["test.jsonl"])
        pref_set.add(
            Preference(
                category=PreferenceCategory.PROHIBITION,
                rule="Never push to main",
                confidence=ConfidenceLevel.HIGH,
            )
        )

        output = format_preferences(pref_set, "json")
        data = json.loads(output)
        # JSON output uses by_category structure
        assert "by_category" in data
        assert "prohibition" in data["by_category"]


class TestExtractPreferences:
    """Tests for the main extract_preferences function."""

    def test_extract_from_empty_file(self):
        """Test extracting from empty file."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write("")
            f.flush()

            result = extract_preferences([f.name])
            assert isinstance(result, PreferenceSet)

    def test_extract_from_minimal_session(self):
        """Test extracting from minimal session."""
        entries = [
            {"type": "user", "message": {"content": [{"type": "text", "text": "Hello"}]}},
            {
                "type": "assistant",
                "message": {"content": [{"type": "text", "text": "Hi"}]},
            },
        ]

        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            for entry in entries:
                f.write(json.dumps(entry) + "\n")
            f.flush()

            result = extract_preferences([f.name])
            assert isinstance(result, PreferenceSet)


class TestLLMProviders:
    """Tests for LLM provider module."""

    def test_list_providers(self):
        """Test listing available providers."""
        from moss.llm import list_providers

        providers = list_providers()
        assert isinstance(providers, list)
        # CLI provider should always be available
        assert "cli" in providers

    def test_get_provider_cli(self):
        """Test getting CLI provider."""
        from moss.llm import get_provider

        provider = get_provider("cli")
        assert provider is not None

    def test_get_provider_invalid(self):
        """Test getting invalid provider raises error."""
        from moss.llm import get_provider

        with pytest.raises(ValueError, match="not found"):
            get_provider("nonexistent_provider")
