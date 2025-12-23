"""Tests for Vector Store abstraction."""

import subprocess
import sys

import pytest

from moss_orchestration.vector_store import (
    ChromaVectorStore,
    InMemoryVectorStore,
    SearchResult,
    SQLiteVectorStore,
    VectorStore,
    create_vector_store,
    document_hash,
)


def _check_chromadb_available() -> bool:
    """Check if ChromaDB can be imported without hanging.

    ChromaDB has native dependencies that may fail in some environments (e.g., Nix).
    We use a subprocess check with timeout to detect this reliably.
    """
    try:
        result = subprocess.run(
            [sys.executable, "-c", "import chromadb"],
            timeout=10,
            capture_output=True,
        )
        return result.returncode == 0
    except subprocess.TimeoutExpired:
        return False
    except Exception:
        return False


_chromadb_available = _check_chromadb_available()


class TestSearchResult:
    """Tests for SearchResult dataclass."""

    def test_create_result(self):
        result = SearchResult(
            id="doc1",
            score=0.95,
            metadata={"type": "code"},
            document="def hello(): pass",
        )

        assert result.id == "doc1"
        assert result.score == 0.95
        assert result.metadata["type"] == "code"
        assert result.document == "def hello(): pass"

    def test_default_values(self):
        result = SearchResult(id="doc1", score=0.5)

        assert result.metadata == {}
        assert result.document is None


class TestInMemoryVectorStore:
    """Tests for InMemoryVectorStore."""

    @pytest.fixture
    def store(self) -> InMemoryVectorStore:
        return InMemoryVectorStore()

    async def test_add_and_get(self, store: InMemoryVectorStore):
        await store.add("doc1", "Python function", {"type": "code"})

        result = await store.get("doc1")

        assert result is not None
        assert result.id == "doc1"
        assert result.document == "Python function"
        assert result.metadata["type"] == "code"

    async def test_get_nonexistent(self, store: InMemoryVectorStore):
        result = await store.get("nonexistent")
        assert result is None

    async def test_search(self, store: InMemoryVectorStore):
        await store.add("doc1", "Python parsing function", {"type": "code"})
        await store.add("doc2", "JavaScript component", {"type": "code"})
        await store.add("doc3", "Python testing framework", {"type": "test"})

        results = await store.search("Python function")

        assert len(results) >= 1
        # Python docs should score higher
        python_results = [r for r in results if "Python" in (r.document or "")]
        assert len(python_results) >= 1

    async def test_search_with_filter(self, store: InMemoryVectorStore):
        await store.add("doc1", "Python code", {"type": "code"})
        await store.add("doc2", "Python test", {"type": "test"})

        results = await store.search("Python", filter={"type": "test"})

        assert len(results) == 1
        assert results[0].id == "doc2"

    async def test_search_with_limit(self, store: InMemoryVectorStore):
        for i in range(10):
            await store.add(f"doc{i}", f"Document about Python {i}", {})

        results = await store.search("Python", limit=3)

        assert len(results) == 3

    async def test_delete(self, store: InMemoryVectorStore):
        await store.add("doc1", "Test document", {})

        assert await store.delete("doc1")
        assert await store.get("doc1") is None
        assert not await store.delete("doc1")  # Already deleted

    async def test_count(self, store: InMemoryVectorStore):
        assert await store.count() == 0

        await store.add("doc1", "First", {})
        await store.add("doc2", "Second", {})

        assert await store.count() == 2

    async def test_clear(self, store: InMemoryVectorStore):
        await store.add("doc1", "First", {})
        await store.add("doc2", "Second", {})

        await store.clear()

        assert await store.count() == 0

    async def test_add_batch(self, store: InMemoryVectorStore):
        await store.add_batch(
            ids=["doc1", "doc2", "doc3"],
            documents=["First doc", "Second doc", "Third doc"],
            metadatas=[{"n": 1}, {"n": 2}, {"n": 3}],
        )

        assert await store.count() == 3
        result = await store.get("doc2")
        assert result is not None
        assert result.metadata["n"] == 2

    async def test_protocol_compliance(self, store: InMemoryVectorStore):
        """Verify InMemoryVectorStore satisfies VectorStore protocol."""
        assert isinstance(store, VectorStore)


@pytest.mark.skipif(
    not _chromadb_available,
    reason="ChromaDB not available (import check failed or timed out)",
)
class TestChromaVectorStore:
    """Tests for ChromaVectorStore."""

    @pytest.fixture
    def store(self) -> ChromaVectorStore:
        # Use in-memory ChromaDB for testing
        return ChromaVectorStore(collection_name="test_collection")

    def test_lazy_initialization(self):
        """ChromaDB should not initialize until first use."""
        store = ChromaVectorStore(collection_name="lazy_test")
        assert store._client is None
        assert store._collection is None

    async def test_add_and_get(self, store: ChromaVectorStore):
        pytest.importorskip("chromadb")

        await store.add("doc1", "Python function", {"type": "code"})
        result = await store.get("doc1")

        assert result is not None
        assert result.id == "doc1"
        assert result.document == "Python function"

    async def test_search(self, store: ChromaVectorStore):
        pytest.importorskip("chromadb")

        await store.add("doc1", "Python machine learning", {"type": "code"})
        await store.add("doc2", "JavaScript frontend", {"type": "code"})

        results = await store.search("Python AI", limit=5)

        assert len(results) >= 1
        # Python doc should be most relevant
        assert results[0].id == "doc1"

    async def test_delete(self, store: ChromaVectorStore):
        pytest.importorskip("chromadb")

        await store.add("doc1", "Test document", {})
        assert await store.delete("doc1")
        assert await store.get("doc1") is None

    async def test_count(self, store: ChromaVectorStore):
        pytest.importorskip("chromadb")

        await store.add("doc1", "First", {})
        await store.add("doc2", "Second", {})

        assert await store.count() == 2

    async def test_protocol_compliance(self, store: ChromaVectorStore):
        """Verify ChromaVectorStore satisfies VectorStore protocol."""
        assert isinstance(store, VectorStore)


class TestSQLiteVectorStore:
    """Tests for SQLiteVectorStore."""

    @pytest.fixture
    def store(self, tmp_path) -> SQLiteVectorStore:
        db_path = tmp_path / "test.db"
        return SQLiteVectorStore(db_path=str(db_path))

    async def test_add_and_get(self, store: SQLiteVectorStore):
        await store.add("doc1", "Python function for parsing", {"type": "code"})

        result = await store.get("doc1")

        assert result is not None
        assert result.id == "doc1"
        assert result.document == "Python function for parsing"
        assert result.metadata["type"] == "code"

    async def test_get_nonexistent(self, store: SQLiteVectorStore):
        result = await store.get("nonexistent")
        assert result is None

    async def test_search(self, store: SQLiteVectorStore):
        await store.add("doc1", "Python function for parsing config", {"type": "code"})
        await store.add("doc2", "JavaScript module for API calls", {"type": "code"})

        results = await store.search("parsing config", limit=5)

        assert len(results) >= 1
        # Python doc should be most relevant
        assert results[0].id == "doc1"

    async def test_search_empty_query(self, store: SQLiteVectorStore):
        await store.add("doc1", "Some content", {})
        results = await store.search("", limit=5)
        assert len(results) == 0

    async def test_search_with_filter(self, store: SQLiteVectorStore):
        await store.add("doc1", "Python code", {"type": "code"})
        await store.add("doc2", "Python docs", {"type": "doc"})

        results = await store.search("Python", filter={"type": "doc"})

        assert len(results) == 1
        assert results[0].id == "doc2"

    async def test_delete(self, store: SQLiteVectorStore):
        await store.add("doc1", "Test document", {})

        assert await store.delete("doc1")
        assert await store.get("doc1") is None
        assert not await store.delete("doc1")  # Already deleted

    async def test_count(self, store: SQLiteVectorStore):
        assert await store.count() == 0

        await store.add("doc1", "First", {})
        await store.add("doc2", "Second", {})

        assert await store.count() == 2

    async def test_clear(self, store: SQLiteVectorStore):
        await store.add("doc1", "First", {})
        await store.add("doc2", "Second", {})

        await store.clear()

        assert await store.count() == 0

    async def test_add_batch(self, store: SQLiteVectorStore):
        await store.add_batch(
            ids=["doc1", "doc2"],
            documents=["First document", "Second document"],
            metadatas=[{"type": "a"}, {"type": "b"}],
        )

        assert await store.count() == 2
        doc1 = await store.get("doc1")
        assert doc1 is not None
        assert doc1.metadata["type"] == "a"

    async def test_protocol_compliance(self, store: SQLiteVectorStore):
        """Verify SQLiteVectorStore satisfies VectorStore protocol."""
        assert isinstance(store, VectorStore)

    async def test_persistence(self, tmp_path):
        """Verify data persists across store instances."""
        db_path = tmp_path / "persist_test.db"

        # Create first store and add data
        store1 = SQLiteVectorStore(db_path=str(db_path))
        await store1.add("doc1", "Persistent data", {"version": 1})
        store1.close()

        # Create second store and verify data
        store2 = SQLiteVectorStore(db_path=str(db_path))
        result = await store2.get("doc1")

        assert result is not None
        assert result.document == "Persistent data"
        store2.close()


class TestCreateVectorStore:
    """Tests for create_vector_store factory."""

    def test_create_memory_store(self):
        store = create_vector_store("memory")
        assert isinstance(store, InMemoryVectorStore)

    def test_create_sqlite_store(self, tmp_path):
        db_path = tmp_path / "factory_test.db"
        store = create_vector_store("sqlite", db_path=str(db_path))
        assert isinstance(store, SQLiteVectorStore)

    @pytest.mark.skipif(not _chromadb_available, reason="ChromaDB not available")
    def test_create_chroma_store(self):
        store = create_vector_store("chroma", collection_name="test")
        assert isinstance(store, ChromaVectorStore)

    def test_unknown_backend(self):
        with pytest.raises(ValueError, match="Unknown backend"):
            create_vector_store("unknown")


class TestDocumentHash:
    """Tests for document_hash function."""

    def test_generates_hash(self):
        hash1 = document_hash("Hello world")
        assert len(hash1) == 8
        assert hash1.isalnum()

    def test_deterministic(self):
        hash1 = document_hash("Same content")
        hash2 = document_hash("Same content")
        assert hash1 == hash2

    def test_different_content(self):
        hash1 = document_hash("Content A")
        hash2 = document_hash("Content B")
        assert hash1 != hash2
