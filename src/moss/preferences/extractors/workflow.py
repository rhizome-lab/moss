"""Workflow extractor.

Extracts preferences from workflow patterns:
- When does the user intervene?
- What tools cause friction?
- What are the commit/test patterns?
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass

from moss.preferences.models import (
    ConfidenceLevel,
    Evidence,
    ExtractionMethod,
    Preference,
    PreferenceCategory,
    PreferenceSet,
)
from moss.preferences.parsing import ParsedSession, Turn


@dataclass
class ToolFriction:
    """Detected friction with a tool."""

    tool_name: str
    error_count: int
    retry_count: int
    intervention_count: int  # User intervened after tool use

    @property
    def friction_score(self) -> float:
        """Calculate overall friction score."""
        return self.error_count * 2 + self.retry_count * 1.5 + self.intervention_count


@dataclass
class WorkflowPattern:
    """A detected workflow pattern."""

    pattern_type: str
    frequency: int
    examples: list[str]


@dataclass
class WorkflowExtractor:
    """Extract preferences from workflow patterns."""

    min_pattern_frequency: int = 2  # Minimum occurrences to consider a pattern

    def extract(self, sessions: list[ParsedSession]) -> PreferenceSet:
        """Extract workflow preferences.

        Args:
            sessions: List of parsed sessions

        Returns:
            PreferenceSet with workflow preferences
        """
        result = PreferenceSet(sources=[str(s.path) for s in sessions])

        # Analyze tool friction
        friction = self._analyze_tool_friction(sessions)
        for pref in self._friction_to_preferences(friction, sessions):
            result.add(pref)

        # Analyze intervention patterns
        interventions = self._analyze_interventions(sessions)
        for pref in self._interventions_to_preferences(interventions, sessions):
            result.add(pref)

        # Analyze workflow sequences
        patterns = self._analyze_workflow_patterns(sessions)
        for pref in self._patterns_to_preferences(patterns, sessions):
            result.add(pref)

        return result

    def _analyze_tool_friction(self, sessions: list[ParsedSession]) -> dict[str, ToolFriction]:
        """Analyze which tools cause friction."""
        friction: dict[str, ToolFriction] = {}

        for session in sessions:
            # Track tool calls and errors
            tool_errors: Counter[str] = Counter()
            tool_retries: Counter[str] = Counter()
            tool_interventions: Counter[str] = Counter()

            turns = session.turns
            for i, turn in enumerate(turns):
                if turn.role != "assistant":
                    continue

                for tc in turn.tool_calls:
                    # Check for errors
                    for tr in turn.tool_results:
                        if tr.tool_use_id == tc.id and tr.is_error:
                            tool_errors[tc.name] += 1

                # Check if next turn is user intervention after tool use
                if turn.has_tool_calls and i + 1 < len(turns):
                    next_turn = turns[i + 1]
                    if next_turn.role == "user" and self._is_intervention(next_turn):
                        for tc in turn.tool_calls:
                            tool_interventions[tc.name] += 1

            # Check for retries (same tool called multiple times in sequence)
            for i, turn in enumerate(turns):
                if turn.role != "assistant" or not turn.tool_calls:
                    continue
                if i + 2 < len(turns):
                    # Check if same tool is called again soon
                    tools_used = {tc.name for tc in turn.tool_calls}
                    for j in range(i + 1, min(i + 3, len(turns))):
                        if turns[j].role == "assistant":
                            for tc in turns[j].tool_calls:
                                if tc.name in tools_used:
                                    tool_retries[tc.name] += 1

            # Aggregate
            all_tools = (
                set(tool_errors.keys()) | set(tool_retries.keys()) | set(tool_interventions.keys())
            )
            for tool in all_tools:
                if tool not in friction:
                    friction[tool] = ToolFriction(
                        tool_name=tool,
                        error_count=0,
                        retry_count=0,
                        intervention_count=0,
                    )
                friction[tool].error_count += tool_errors[tool]
                friction[tool].retry_count += tool_retries[tool]
                friction[tool].intervention_count += tool_interventions[tool]

        return friction

    def _is_intervention(self, turn: Turn) -> bool:
        """Check if a user turn is an intervention/correction."""
        text = turn.content.lower()
        intervention_signals = [
            "no",
            "stop",
            "wait",
            "actually",
            "wrong",
            "that's not",
            "don't",
            "shouldn't",
            "cancel",
            "undo",
        ]
        return any(signal in text for signal in intervention_signals)

    def _friction_to_preferences(
        self, friction: dict[str, ToolFriction], sessions: list[ParsedSession]
    ) -> list[Preference]:
        """Convert friction analysis to preferences."""
        preferences = []

        # Only report high-friction tools
        high_friction = [f for f in friction.values() if f.friction_score >= 3]

        for fric in sorted(high_friction, key=lambda f: f.friction_score, reverse=True):
            if fric.error_count >= 2:
                rule = (
                    f"The {fric.tool_name} tool frequently causes errors. "
                    "Consider alternatives or be more careful with its use."
                )
                pref = Preference(
                    category=PreferenceCategory.AGENT_BEHAVIOR,
                    rule=rule,
                    confidence=ConfidenceLevel.MEDIUM
                    if fric.error_count >= 3
                    else ConfidenceLevel.LOW,
                    evidence=[
                        Evidence(
                            source=str(sessions[0].path) if sessions else "unknown",
                            text=f"{fric.error_count} errors, {fric.retry_count} retries",
                            method=ExtractionMethod.WORKFLOW,
                        )
                    ],
                    tags=["tool_friction"],
                )
                preferences.append(pref)

            if fric.intervention_count >= 2:
                rule = (
                    f"User frequently intervenes after {fric.tool_name} tool use. "
                    "Consider asking for confirmation before using this tool."
                )
                pref = Preference(
                    category=PreferenceCategory.AGENT_BEHAVIOR,
                    rule=rule,
                    confidence=ConfidenceLevel.MEDIUM
                    if fric.intervention_count >= 3
                    else ConfidenceLevel.LOW,
                    evidence=[
                        Evidence(
                            source=str(sessions[0].path) if sessions else "unknown",
                            text=f"{fric.intervention_count} interventions",
                            method=ExtractionMethod.WORKFLOW,
                        )
                    ],
                    tags=["tool_friction", "needs_confirmation"],
                )
                preferences.append(pref)

        return preferences

    def _analyze_interventions(self, sessions: list[ParsedSession]) -> Counter[str]:
        """Analyze what triggers user interventions."""
        intervention_triggers: Counter[str] = Counter()

        for session in sessions:
            turns = session.turns
            for i, turn in enumerate(turns):
                if turn.role != "user":
                    continue

                if not self._is_intervention(turn):
                    continue

                # Check what preceded this intervention
                if i > 0:
                    prev = turns[i - 1]
                    if prev.role == "assistant":
                        # Categorize what the assistant was doing
                        if prev.tool_calls:
                            for tc in prev.tool_calls:
                                intervention_triggers[f"tool:{tc.name}"] += 1
                        else:
                            # Text response intervention
                            intervention_triggers["text_response"] += 1

        return intervention_triggers

    def _interventions_to_preferences(
        self, triggers: Counter[str], sessions: list[ParsedSession]
    ) -> list[Preference]:
        """Convert intervention analysis to preferences."""
        preferences = []

        for trigger, count in triggers.most_common(5):
            if count < self.min_pattern_frequency:
                continue

            if trigger.startswith("tool:"):
                tool_name = trigger.split(":")[1]
                rule = f"Ask for confirmation before using {tool_name} tool"
            else:
                rule = "Ask for confirmation before proceeding with significant changes"

            pref = Preference(
                category=PreferenceCategory.AGENT_BEHAVIOR,
                rule=rule,
                confidence=ConfidenceLevel.MEDIUM if count >= 3 else ConfidenceLevel.LOW,
                evidence=[
                    Evidence(
                        source=str(sessions[0].path) if sessions else "unknown",
                        text=f"User intervened {count} times after {trigger}",
                        method=ExtractionMethod.WORKFLOW,
                    )
                ],
                tags=["intervention_pattern"],
            )
            preferences.append(pref)

        return preferences

    def _analyze_workflow_patterns(self, sessions: list[ParsedSession]) -> list[WorkflowPattern]:
        """Analyze workflow sequences for patterns."""
        patterns: list[WorkflowPattern] = []

        # Track tool sequences
        tool_sequences: Counter[tuple[str, ...]] = Counter()

        for session in sessions:
            sequence: list[str] = []
            for turn in session.turns:
                if turn.role == "assistant":
                    for tc in turn.tool_calls:
                        sequence.append(tc.name)
                        # Track pairs and triples
                        if len(sequence) >= 2:
                            tool_sequences[tuple(sequence[-2:])] += 1
                        if len(sequence) >= 3:
                            tool_sequences[tuple(sequence[-3:])] += 1

        # Find common sequences
        for seq, count in tool_sequences.most_common(10):
            if count >= self.min_pattern_frequency and len(seq) >= 2:
                patterns.append(
                    WorkflowPattern(
                        pattern_type="tool_sequence",
                        frequency=count,
                        examples=[" → ".join(seq)],
                    )
                )

        return patterns

    def _patterns_to_preferences(
        self, patterns: list[WorkflowPattern], sessions: list[ParsedSession]
    ) -> list[Preference]:
        """Convert workflow patterns to preferences."""
        preferences = []

        for pattern in patterns:
            if pattern.frequency < self.min_pattern_frequency:
                continue

            # Generate rules from common patterns
            if pattern.pattern_type == "tool_sequence":
                # Look for "Read then Edit" patterns
                example = pattern.examples[0] if pattern.examples else ""
                if "Read" in example and "Edit" in example:
                    pref = Preference(
                        category=PreferenceCategory.WORKFLOW,
                        rule="Always read a file before editing it",
                        confidence=ConfidenceLevel.HIGH
                        if pattern.frequency >= 5
                        else ConfidenceLevel.MEDIUM,
                        evidence=[
                            Evidence(
                                source=str(sessions[0].path) if sessions else "unknown",
                                text=f"Pattern observed {pattern.frequency} times: {example}",
                                method=ExtractionMethod.PATTERN,
                            )
                        ],
                        tags=["workflow_pattern"],
                    )
                    preferences.append(pref)

        return preferences
