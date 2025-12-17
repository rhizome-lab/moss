"""Tests for Memory Layer."""

import pytest

from moss.memory import (
    Action,
    Episode,
    EpisodicStore,
    MemoryManager,
    Outcome,
    PatternMatcher,
    SemanticRule,
    SemanticStore,
    SimpleVectorIndex,
    StateSnapshot,
    create_memory_manager,
)


class TestStateSnapshot:
    """Tests for StateSnapshot."""

    def test_create_snapshot(self):
        snapshot = StateSnapshot.create(
            files=["src/main.py", "tests/test_main.py"],
            context="def hello(): pass",
            error_count=0,
        )

        assert snapshot.files == frozenset(["src/main.py", "tests/test_main.py"])
        assert len(snapshot.context_hash) == 16
        assert snapshot.error_count == 0

    def test_snapshot_with_metadata(self):
        snapshot = StateSnapshot.create(
            files=["a.py"],
            context="x = 1",
            metadata={"branch": "main", "commit": "abc123"},
        )

        assert ("branch", "main") in snapshot.metadata
        assert ("commit", "abc123") in snapshot.metadata


class TestAction:
    """Tests for Action."""

    def test_create_action(self):
        action = Action.create(
            tool="edit",
            target="src/main.py",
            description="Add function",
            content="new code",
        )

        assert action.tool == "edit"
        assert action.target == "src/main.py"
        assert ("content", "new code") in action.parameters

    def test_action_without_target(self):
        action = Action.create(tool="shell", description="Run tests")
        assert action.target is None


class TestEpisode:
    """Tests for Episode."""

    def test_create_episode(self):
        state = StateSnapshot.create(files=["a.py"], context="code")
        action = Action.create(tool="edit", target="a.py")

        episode = Episode.create(
            state=state,
            action=action,
            outcome=Outcome.SUCCESS,
            duration_ms=100,
        )

        assert len(episode.id) == 12
        assert episode.outcome == Outcome.SUCCESS
        assert episode.duration_ms == 100

    def test_episode_with_error(self):
        state = StateSnapshot.create(files=["a.py"], context="code")
        action = Action.create(tool="edit", target="a.py")

        episode = Episode.create(
            state=state,
            action=action,
            outcome=Outcome.FAILURE,
            error_message="Syntax error at line 5",
        )

        assert episode.outcome == Outcome.FAILURE
        assert episode.error_message == "Syntax error at line 5"

    def test_episode_with_tags(self):
        state = StateSnapshot.create(files=["a.py"], context="code")
        action = Action.create(tool="edit", target="a.py")

        episode = Episode.create(
            state=state,
            action=action,
            outcome=Outcome.SUCCESS,
            tags={"refactor", "python"},
        )

        assert "refactor" in episode.tags
        assert "python" in episode.tags


class TestSimpleVectorIndex:
    """Tests for SimpleVectorIndex."""

    @pytest.fixture
    def index(self):
        return SimpleVectorIndex()

    async def test_index_and_search(self, index: SimpleVectorIndex):
        await index.index("1", "python code function", {"type": "code"})
        await index.index("2", "python test unittest", {"type": "test"})
        await index.index("3", "javascript react component", {"type": "code"})

        results = await index.search("python function")

        assert len(results) > 0
        # First result should be the python code (more overlap)
        assert results[0][0] == "1"

    async def test_search_with_filter(self, index: SimpleVectorIndex):
        await index.index("1", "python code", {"type": "code"})
        await index.index("2", "python test", {"type": "test"})

        results = await index.search("python", filter={"type": "test"})

        assert len(results) == 1
        assert results[0][0] == "2"

    async def test_delete(self, index: SimpleVectorIndex):
        await index.index("1", "content", {})

        assert await index.delete("1")
        assert not await index.delete("1")  # Already deleted

        results = await index.search("content")
        assert len(results) == 0


class TestEpisodicStore:
    """Tests for EpisodicStore."""

    @pytest.fixture
    def store(self):
        return EpisodicStore()

    @pytest.fixture
    def sample_episode(self):
        state = StateSnapshot.create(files=["src/main.py"], context="def main(): pass")
        action = Action.create(tool="edit", target="src/main.py")
        return Episode.create(state=state, action=action, outcome=Outcome.SUCCESS)

    async def test_store_and_get(self, store: EpisodicStore, sample_episode: Episode):
        id = await store.store(sample_episode)

        retrieved = await store.get(id)
        assert retrieved is not None
        assert retrieved.id == sample_episode.id

    async def test_delete(self, store: EpisodicStore, sample_episode: Episode):
        id = await store.store(sample_episode)

        assert await store.delete(id)
        assert await store.get(id) is None

    async def test_find_similar(self, store: EpisodicStore):
        # Store some episodes
        for i in range(5):
            state = StateSnapshot.create(
                files=[f"src/module{i}.py"], context=f"code {i}"
            )
            action = Action.create(tool="edit", target=f"src/module{i}.py")
            episode = Episode.create(state=state, action=action, outcome=Outcome.SUCCESS)
            await store.store(episode)

        # Search
        query_state = StateSnapshot.create(files=["src/module0.py"], context="query")
        query_action = Action.create(tool="edit", target="src/module0.py")

        results = await store.find_similar(query_state, query_action, limit=3)

        assert len(results) <= 3

    async def test_find_failures(self, store: EpisodicStore):
        # Store success and failure episodes
        for outcome in [Outcome.SUCCESS, Outcome.FAILURE, Outcome.FAILURE]:
            state = StateSnapshot.create(files=["a.py"], context="code")
            action = Action.create(tool="edit", target="a.py")
            episode = Episode.create(state=state, action=action, outcome=outcome)
            await store.store(episode)

        failures = await store.find_failures()

        assert len(failures) == 2
        assert all(ep.outcome == Outcome.FAILURE for ep in failures)

    async def test_find_by_tag(self, store: EpisodicStore):
        state = StateSnapshot.create(files=["a.py"], context="code")
        action = Action.create(tool="edit", target="a.py")
        episode = Episode.create(
            state=state, action=action, outcome=Outcome.SUCCESS, tags={"important"}
        )
        await store.store(episode)

        results = await store.find_by_tag("important")

        assert len(results) == 1
        assert "important" in results[0].tags

    async def test_stats(self, store: EpisodicStore, sample_episode: Episode):
        await store.store(sample_episode)

        stats = store.stats()

        assert stats["total"] == 1
        assert stats["by_outcome"]["SUCCESS"] == 1
        assert "edit" in stats["by_tool"]

    async def test_max_episodes_eviction(self):
        store = EpisodicStore(max_episodes=3)

        # Store 5 episodes
        for i in range(5):
            state = StateSnapshot.create(files=[f"file{i}.py"], context=f"code{i}")
            action = Action.create(tool="edit", target=f"file{i}.py")
            episode = Episode.create(state=state, action=action, outcome=Outcome.SUCCESS)
            await store.store(episode)

        assert store.count == 3  # Only 3 remain


class TestSemanticRule:
    """Tests for SemanticRule."""

    def test_matches_pattern(self):
        rule = SemanticRule(
            id="test",
            pattern="python syntax error",
            action="Check syntax",
            confidence=0.8,
            supporting_episodes=[],
        )

        assert rule.matches("There was a python syntax error in the file")
        assert not rule.matches("JavaScript runtime error")

    def test_record_match(self):
        rule = SemanticRule(
            id="test",
            pattern="test",
            action="action",
            confidence=0.8,
            supporting_episodes=[],
        )

        assert rule.match_count == 0
        rule.record_match()
        assert rule.match_count == 1
        assert rule.last_matched is not None


class TestSemanticStore:
    """Tests for SemanticStore."""

    @pytest.fixture
    def store(self):
        return SemanticStore()

    def test_add_and_get_rule(self, store: SemanticStore):
        rule = SemanticRule(
            id="rule1",
            pattern="edit python file",
            action="Run linter",
            confidence=0.9,
            supporting_episodes=[],
        )

        store.add_rule(rule)

        retrieved = store.get_rule("rule1")
        assert retrieved is not None
        assert retrieved.pattern == "edit python file"

    def test_remove_rule(self, store: SemanticStore):
        rule = SemanticRule(
            id="rule1",
            pattern="test",
            action="action",
            confidence=0.8,
            supporting_episodes=[],
        )
        store.add_rule(rule)

        assert store.remove_rule("rule1")
        assert store.get_rule("rule1") is None

    def test_find_matching_rules(self, store: SemanticStore):
        rule1 = SemanticRule(
            id="r1",
            pattern="python syntax",
            action="Run syntax check",
            confidence=0.9,
            supporting_episodes=[],
        )
        rule2 = SemanticRule(
            id="r2",
            pattern="javascript",
            action="Run eslint",
            confidence=0.8,
            supporting_episodes=[],
        )
        store.add_rule(rule1)
        store.add_rule(rule2)

        matches = store.find_matching_rules("python syntax error in file")

        assert len(matches) == 1
        assert matches[0].id == "r1"

    def test_min_confidence_filter(self, store: SemanticStore):
        rule = SemanticRule(
            id="r1",
            pattern="test",
            action="action",
            confidence=0.4,  # Low confidence
            supporting_episodes=[],
        )
        store.add_rule(rule)

        # Default min_confidence is 0.5
        matches = store.find_matching_rules("test context")
        assert len(matches) == 0

        # Lower threshold
        matches = store.find_matching_rules("test context", min_confidence=0.3)
        assert len(matches) == 1


class TestPatternMatcher:
    """Tests for PatternMatcher."""

    @pytest.fixture
    def stores(self):
        return EpisodicStore(), SemanticStore()

    @pytest.fixture
    def matcher(self, stores):
        episodic, semantic = stores
        return PatternMatcher(episodic, semantic, min_occurrences=2, min_confidence=0.5)

    async def test_analyze_failures_creates_rules(self, stores, matcher: PatternMatcher):
        episodic, semantic = stores

        # Create multiple failures with same pattern
        for i in range(3):
            state = StateSnapshot.create(files=["src/main.py"], context=f"code{i}")
            action = Action.create(tool="edit", target="src/main.py")
            episode = Episode.create(
                state=state,
                action=action,
                outcome=Outcome.FAILURE,
                error_message="Syntax error",
            )
            await episodic.store(episode)

        new_rules = await matcher.analyze_failures()

        assert len(new_rules) >= 1
        assert len(semantic.rules) >= 1


class TestMemoryManager:
    """Tests for MemoryManager."""

    @pytest.fixture
    def manager(self):
        return create_memory_manager()

    async def test_record_episode(self, manager: MemoryManager):
        state = StateSnapshot.create(files=["a.py"], context="code")
        action = Action.create(tool="edit", target="a.py")

        episode = await manager.record_episode(
            state=state,
            action=action,
            outcome=Outcome.SUCCESS,
            duration_ms=50,
        )

        assert episode.id is not None
        assert manager.episodic.count == 1

    async def test_get_context(self, manager: MemoryManager):
        # Record some episodes first
        state = StateSnapshot.create(files=["src/main.py"], context="code")
        action = Action.create(tool="edit", target="src/main.py")

        await manager.record_episode(
            state=state, action=action, outcome=Outcome.FAILURE, error_message="Error"
        )
        await manager.record_episode(
            state=state, action=action, outcome=Outcome.FAILURE, error_message="Error"
        )

        # Get context for similar action
        context = await manager.get_context(state, action)

        # Should find the similar episodes
        assert len(context.relevant_episodes) > 0

    async def test_add_manual_rule(self, manager: MemoryManager):
        rule_id = manager.add_rule(
            pattern="edit python", action="Run ruff first", confidence=0.9
        )

        rule = manager.semantic.get_rule(rule_id)
        assert rule is not None
        assert rule.pattern == "edit python"

    async def test_context_to_text(self, manager: MemoryManager):
        # Add a rule that will match the action
        manager.add_rule(pattern="edit test", action="Be careful!", confidence=0.8)

        # Get context that matches the rule (search is: "edit test.py test.py")
        state = StateSnapshot.create(files=["test.py"], context="some context")
        action = Action.create(tool="edit", target="test.py")

        context = await manager.get_context(state, action)
        text = context.to_text()

        assert "Relevant learned rules" in text
        assert "Be careful!" in text


class TestCreateMemoryManager:
    """Tests for create_memory_manager."""

    def test_creates_manager(self):
        manager = create_memory_manager()
        assert manager is not None
        assert manager.episodic is not None
        assert manager.semantic is not None
