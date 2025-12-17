"""Tests for context memory management."""

from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path

import pytest

from moss.context_memory import (
    ChatlogStore,
    ChatMessage,
    ChatSession,
    ContentHash,
    DocumentSummary,
    DocumentSummaryStore,
    SimpleSummarizer,
    get_chatlog_store,
    get_document_store,
    reset_stores,
)


@pytest.fixture(autouse=True)
def reset_global_stores():
    """Reset global stores before each test."""
    reset_stores()
    yield
    reset_stores()


# =============================================================================
# Content Hash Tests
# =============================================================================


class TestContentHash:
    """Tests for ContentHash."""

    def test_hash_from_content(self):
        content = "Hello, world!"
        hash_obj = ContentHash.from_content(content)
        assert hash_obj.hash is not None
        assert len(hash_obj.hash) == 16
        assert hash_obj.content_type == "file"
        assert hash_obj.size_bytes == len(content.encode())

    def test_same_content_same_hash(self):
        content = "Same content"
        hash1 = ContentHash.from_content(content)
        hash2 = ContentHash.from_content(content)
        assert hash1.hash == hash2.hash

    def test_different_content_different_hash(self):
        hash1 = ContentHash.from_content("Content A")
        hash2 = ContentHash.from_content("Content B")
        assert hash1.hash != hash2.hash


# =============================================================================
# Document Summary Tests
# =============================================================================


class TestDocumentSummary:
    """Tests for DocumentSummary."""

    def test_merkle_hash_without_children(self):
        content_hash = ContentHash.from_content("test")
        summary = DocumentSummary(
            content_hash=content_hash,
            summary="Test summary",
            key_points=["Point 1"],
        )
        assert summary.merkle_hash is not None

    def test_merkle_hash_with_children(self):
        parent_hash = ContentHash.from_content("parent")
        child_hash = ContentHash.from_content("child")

        child = DocumentSummary(
            content_hash=child_hash,
            summary="Child summary",
            key_points=[],
        )
        parent = DocumentSummary(
            content_hash=parent_hash,
            summary="Parent summary",
            key_points=[],
            children=[child],
        )

        # Merkle hash should include child
        assert parent.merkle_hash is not None
        assert parent.merkle_hash != parent.content_hash.hash


# =============================================================================
# Simple Summarizer Tests
# =============================================================================


class TestSimpleSummarizer:
    """Tests for SimpleSummarizer."""

    @pytest.mark.asyncio
    async def test_summarize_short_text(self):
        summarizer = SimpleSummarizer()
        text = "This is a short text."
        summary = await summarizer.summarize(text)
        # SimpleSummarizer adds a period at the end
        assert "short text" in summary

    @pytest.mark.asyncio
    async def test_summarize_long_text(self):
        summarizer = SimpleSummarizer()
        text = ". ".join([f"Sentence number {i}" for i in range(100)])
        summary = await summarizer.summarize(text, max_length=50)
        assert len(summary) < len(text)

    @pytest.mark.asyncio
    async def test_extract_key_points_with_keywords(self):
        summarizer = SimpleSummarizer()
        text = "First point. This is important. Another sentence. You must do this."
        points = await summarizer.extract_key_points(text)
        assert any("important" in p.lower() for p in points)
        assert any("must" in p.lower() for p in points)

    @pytest.mark.asyncio
    async def test_extract_key_points_fallback(self):
        summarizer = SimpleSummarizer()
        text = "First. Second. Third. Fourth. Fifth. Sixth."
        points = await summarizer.extract_key_points(text, max_points=3)
        assert len(points) <= 3


# =============================================================================
# Document Summary Store Tests
# =============================================================================


class TestDocumentSummaryStore:
    """Tests for DocumentSummaryStore."""

    @pytest.mark.asyncio
    async def test_get_or_create(self):
        store = DocumentSummaryStore()
        content = "This is test content for summarization."
        summary = await store.get_or_create(content)

        assert summary is not None
        assert summary.summary is not None
        assert summary.content_hash.hash is not None

    @pytest.mark.asyncio
    async def test_caching(self):
        store = DocumentSummaryStore()
        content = "Cached content test."

        summary1 = await store.get_or_create(content)
        summary2 = await store.get_or_create(content)

        assert summary1.merkle_hash == summary2.merkle_hash

    @pytest.mark.asyncio
    async def test_force_refresh(self):
        store = DocumentSummaryStore()
        content = "Content to refresh."

        summary1 = await store.get_or_create(content)
        summary2 = await store.get_or_create(content, force_refresh=True)

        # Should create new summary
        assert summary1.created_at <= summary2.created_at

    @pytest.mark.asyncio
    async def test_get_by_path(self):
        store = DocumentSummaryStore()
        content = "Path indexed content."
        path = "/test/file.md"

        await store.get_or_create(content, source_path=path)
        summary = store.get_by_path(path)

        assert summary is not None
        assert summary.source_path == path

    @pytest.mark.asyncio
    async def test_invalidate(self):
        store = DocumentSummaryStore()
        path = "/test/invalidate.md"

        await store.get_or_create("Content", source_path=path)
        assert store.get_by_path(path) is not None

        store.invalidate(path)
        assert store.get_by_path(path) is None

    @pytest.mark.asyncio
    async def test_summarize_directory(self, tmp_path: Path):
        store = DocumentSummaryStore()

        # Create test files
        (tmp_path / "doc1.md").write_text("# Document 1\nThis is important content.")
        (tmp_path / "doc2.txt").write_text("Document 2 content here.")
        (tmp_path / "code.py").write_text("def hello(): pass")  # Not matched

        summaries = await store.summarize_directory(tmp_path)

        # Should find md and txt files
        assert len(summaries) == 2

    def test_stats(self):
        store = DocumentSummaryStore()
        stats = store.stats
        assert "summary_count" in stats
        assert "indexed_paths" in stats
        assert "total_tokens" in stats

    @pytest.mark.asyncio
    async def test_persistence(self, tmp_path: Path):
        persist_path = tmp_path / "summaries.json"

        # Create and populate store
        store1 = DocumentSummaryStore(persist_path=persist_path)
        await store1.get_or_create("Persistent content", source_path="/test.md")

        # Create new store from same path
        store2 = DocumentSummaryStore(persist_path=persist_path)
        summary = store2.get_by_path("/test.md")

        assert summary is not None


# =============================================================================
# Chat Message Tests
# =============================================================================


class TestChatMessage:
    """Tests for ChatMessage."""

    def test_create_message(self):
        msg = ChatMessage(role="user", content="Hello")
        assert msg.role == "user"
        assert msg.content == "Hello"
        assert msg.timestamp is not None

    def test_to_dict(self):
        msg = ChatMessage(role="assistant", content="Response", metadata={"key": "value"})
        data = msg.to_dict()
        assert data["role"] == "assistant"
        assert data["content"] == "Response"
        assert data["metadata"]["key"] == "value"

    def test_from_dict(self):
        data = {
            "role": "user",
            "content": "Test",
            "timestamp": datetime.now(UTC).isoformat(),
            "metadata": {},
        }
        msg = ChatMessage.from_dict(data)
        assert msg.role == "user"
        assert msg.content == "Test"


# =============================================================================
# Chat Session Tests
# =============================================================================


class TestChatSession:
    """Tests for ChatSession."""

    def test_create_session(self):
        session = ChatSession(id="test-session")
        assert session.id == "test-session"
        assert len(session.messages) == 0

    def test_add_message(self):
        session = ChatSession(id="test")
        msg = session.add_message("user", "Hello")
        assert len(session.messages) == 1
        assert msg.content == "Hello"

    def test_total_tokens(self):
        session = ChatSession(id="test")
        session.add_message("user", "Hello world")  # ~2-3 tokens
        session.add_message("assistant", "Hi there")  # ~2 tokens
        assert session.total_tokens > 0

    def test_to_dict_and_back(self):
        session = ChatSession(id="test", tags=["important"])
        session.add_message("user", "Test message")

        data = session.to_dict()
        restored = ChatSession.from_dict(data)

        assert restored.id == session.id
        assert len(restored.messages) == 1
        assert restored.tags == ["important"]


# =============================================================================
# Chatlog Store Tests
# =============================================================================


class TestChatlogStore:
    """Tests for ChatlogStore."""

    def test_create_session(self):
        store = ChatlogStore()
        session = store.create_session(tags=["test"])
        assert session is not None
        assert "test" in session.tags

    def test_get_session(self):
        store = ChatlogStore()
        store.create_session(session_id="my-session")
        retrieved = store.get_session("my-session")
        assert retrieved is not None
        assert retrieved.id == "my-session"

    def test_add_message(self):
        store = ChatlogStore()
        session = store.create_session()
        msg = store.add_message(session.id, "user", "Hello")
        assert msg is not None
        assert msg.role == "user"

    @pytest.mark.asyncio
    async def test_summarize_session(self):
        store = ChatlogStore()
        session = store.create_session()
        store.add_message(session.id, "user", "What is the capital of France?")
        store.add_message(session.id, "assistant", "The capital of France is Paris.")

        summary = await store.summarize_session(session.id)
        assert summary is not None
        assert len(summary) > 0

    @pytest.mark.asyncio
    async def test_get_context_optimized_under_limit(self):
        store = ChatlogStore(max_context_tokens=10000)
        session = store.create_session()
        store.add_message(session.id, "user", "Short message")
        store.add_message(session.id, "assistant", "Short response")

        messages = await store.get_context_optimized(session.id)
        assert len(messages) == 2

    @pytest.mark.asyncio
    async def test_get_context_optimized_over_limit(self):
        store = ChatlogStore(max_context_tokens=100)
        session = store.create_session()

        # Add many messages to exceed limit
        for i in range(20):
            store.add_message(session.id, "user", f"Message {i} " * 10)
            store.add_message(session.id, "assistant", f"Response {i} " * 10)

        messages = await store.get_context_optimized(session.id)

        # Should have fewer messages (dropped/summarized middle)
        assert len(messages) < 40

    @pytest.mark.asyncio
    async def test_search_sessions(self):
        store = ChatlogStore()

        # Create sessions with different topics
        session1 = store.create_session(tags=["python"])
        store.add_message(session1.id, "user", "How do I use Python decorators?")

        session2 = store.create_session(tags=["javascript"])
        store.add_message(session2.id, "user", "How do I use JavaScript promises?")

        # Search for Python
        results = await store.search_sessions("Python decorators")
        assert len(results) > 0
        assert any(s.id == session1.id for s in results)

    @pytest.mark.asyncio
    async def test_search_with_tags(self):
        store = ChatlogStore()

        session1 = store.create_session(tags=["python"])
        store.add_message(session1.id, "user", "Python question")

        session2 = store.create_session(tags=["java"])
        store.add_message(session2.id, "user", "Python question")  # Same content

        # Search with tag filter
        results = await store.search_sessions("Python", tags=["python"])
        assert all("python" in s.tags for s in results)

    def test_delete_session(self):
        store = ChatlogStore()
        session = store.create_session()
        assert store.get_session(session.id) is not None

        store.delete_session(session.id)
        assert store.get_session(session.id) is None

    def test_max_sessions_eviction(self):
        store = ChatlogStore(max_sessions=5)

        # Create 6 sessions
        sessions = []
        for _ in range(6):
            sessions.append(store.create_session())

        # Should have evicted oldest
        assert len(store._sessions) == 5

    def test_stats(self):
        store = ChatlogStore()
        store.create_session()
        stats = store.stats
        assert stats["session_count"] == 1
        assert "total_messages" in stats
        assert "total_tokens" in stats

    @pytest.mark.asyncio
    async def test_persistence(self, tmp_path: Path):
        persist_path = tmp_path / "chatlogs.json"

        # Create and populate store
        store1 = ChatlogStore(persist_path=persist_path)
        session = store1.create_session(session_id="persistent")
        store1.add_message(session.id, "user", "Hello")

        # Create new store from same path
        store2 = ChatlogStore(persist_path=persist_path)
        loaded_session = store2.get_session("persistent")

        assert loaded_session is not None
        assert len(loaded_session.messages) == 1


# =============================================================================
# Global Store Tests
# =============================================================================


class TestGlobalStores:
    """Tests for global store instances."""

    def test_get_document_store(self):
        store1 = get_document_store()
        store2 = get_document_store()
        assert store1 is store2

    def test_get_chatlog_store(self):
        store1 = get_chatlog_store()
        store2 = get_chatlog_store()
        assert store1 is store2
