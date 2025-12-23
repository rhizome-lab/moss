"""Tests for semantic code search."""

from pathlib import Path

import pytest

from moss_orchestration.semantic_search import (
    CodeChunk,
    CodeIndexer,
    SearchHit,
    SemanticSearch,
    TFIDFIndex,
    create_search_system,
)
from moss_orchestration.vector_store import InMemoryVectorStore


class TestCodeChunk:
    """Tests for CodeChunk dataclass."""

    def test_to_document(self):
        chunk = CodeChunk(
            id="test:func:1",
            file_path="test.py",
            symbol_name="parse_json",
            symbol_kind="function",
            content="def parse_json(data): pass",
            line_start=1,
            line_end=1,
            signature="def parse_json(data: str) -> dict",
            docstring="Parse JSON data into a dictionary.",
        )

        doc = chunk.to_document()

        assert "function: parse_json" in doc
        assert "signature: def parse_json" in doc
        assert "description: Parse JSON data" in doc
        assert "def parse_json(data): pass" in doc

    def test_to_metadata(self):
        chunk = CodeChunk(
            id="test:func:1",
            file_path="test.py",
            symbol_name="parse_json",
            symbol_kind="function",
            content="code",
            line_start=10,
            line_end=20,
            docstring="Has a docstring",
        )

        meta = chunk.to_metadata()

        assert meta["file_path"] == "test.py"
        assert meta["symbol_name"] == "parse_json"
        assert meta["symbol_kind"] == "function"
        assert meta["line_start"] == 10
        assert meta["line_end"] == 20
        assert meta["has_docstring"] is True


class TestTFIDFIndex:
    """Tests for TF-IDF index."""

    @pytest.fixture
    def index(self) -> TFIDFIndex:
        return TFIDFIndex()

    def test_add_and_search(self, index: TFIDFIndex):
        index.add("doc1", "python function for parsing json")
        index.add("doc2", "javascript component for rendering")
        index.add("doc3", "python class for data processing")

        results = index.search("python parsing")

        assert len(results) >= 1
        # doc1 should be most relevant (has both "python" and "parsing")
        assert results[0][0] == "doc1"

    def test_search_empty_query(self, index: TFIDFIndex):
        index.add("doc1", "some content")
        results = index.search("")
        assert results == []

    def test_search_no_matches(self, index: TFIDFIndex):
        index.add("doc1", "python code")
        results = index.search("java")
        assert results == []

    def test_remove(self, index: TFIDFIndex):
        index.add("doc1", "python code")
        index.add("doc2", "python script")

        index.remove("doc1")

        results = index.search("python")
        assert len(results) == 1
        assert results[0][0] == "doc2"

    def test_tokenize_camel_case(self, index: TFIDFIndex):
        index.add("doc1", "parseJsonData")
        results = index.search("json")
        assert len(results) == 1

    def test_tokenize_snake_case(self, index: TFIDFIndex):
        index.add("doc1", "parse_json_data")
        results = index.search("json")
        assert len(results) == 1


class TestCodeIndexer:
    """Tests for CodeIndexer."""

    @pytest.fixture
    def store(self) -> InMemoryVectorStore:
        return InMemoryVectorStore()

    @pytest.fixture
    def indexer(self, store: InMemoryVectorStore) -> CodeIndexer:
        return CodeIndexer(store)

    async def test_index_python_file(self, indexer: CodeIndexer, tmp_path: Path):
        py_file = tmp_path / "example.py"
        py_file.write_text('''"""Module docstring."""

def parse_json(data: str) -> dict:
    """Parse JSON string to dict."""
    import json
    return json.loads(data)

class DataProcessor:
    """Process data."""

    def process(self, data):
        pass
''')

        count = await indexer.index_file(py_file)

        assert count >= 2  # At least function and class
        assert await indexer.store.count() >= 2

    async def test_index_markdown_file(self, indexer: CodeIndexer, tmp_path: Path):
        md_file = tmp_path / "README.md"
        md_file.write_text("""# Project Title

Introduction text.

## Installation

Install instructions.

## Usage

Usage examples.
""")

        count = await indexer.index_file(md_file)

        assert count >= 2  # Multiple sections

    async def test_skip_unchanged_file(self, indexer: CodeIndexer, tmp_path: Path):
        py_file = tmp_path / "test.py"
        py_file.write_text("def foo(): pass")

        count1 = await indexer.index_file(py_file)
        count2 = await indexer.index_file(py_file)

        assert count1 > 0
        assert count2 == 0  # Skipped, unchanged

    async def test_force_reindex(self, indexer: CodeIndexer, tmp_path: Path):
        py_file = tmp_path / "test.py"
        py_file.write_text("def foo(): pass")

        await indexer.index_file(py_file)
        count = await indexer.index_file(py_file, force=True)

        assert count > 0  # Re-indexed despite no changes

    async def test_remove_file(self, indexer: CodeIndexer, tmp_path: Path):
        py_file = tmp_path / "test.py"
        py_file.write_text("def foo(): pass")

        await indexer.index_file(py_file)
        initial_count = await indexer.store.count()

        result = await indexer.remove_file(py_file)

        assert result is True
        assert await indexer.store.count() < initial_count

    async def test_index_directory(self, indexer: CodeIndexer, tmp_path: Path):
        # Create test files
        (tmp_path / "src").mkdir()
        (tmp_path / "src" / "main.py").write_text("def main(): pass")
        (tmp_path / "src" / "utils.py").write_text("def helper(): pass")
        (tmp_path / "README.md").write_text("# Readme")

        count = await indexer.index_directory(tmp_path)

        assert count >= 3


class TestSemanticSearch:
    """Tests for SemanticSearch."""

    @pytest.fixture
    def store(self) -> InMemoryVectorStore:
        return InMemoryVectorStore()

    @pytest.fixture
    def tfidf(self) -> TFIDFIndex:
        return TFIDFIndex()

    @pytest.fixture
    def search(self, store: InMemoryVectorStore, tfidf: TFIDFIndex) -> SemanticSearch:
        return SemanticSearch(store, tfidf)

    async def test_search_tfidf_mode(
        self,
        store: InMemoryVectorStore,
        tfidf: TFIDFIndex,
        search: SemanticSearch,
    ):
        # Add documents
        await store.add("doc1", "python json parser", {"symbol_kind": "function"})
        tfidf.add("doc1", "python json parser")

        await store.add("doc2", "javascript renderer", {"symbol_kind": "function"})
        tfidf.add("doc2", "javascript renderer")

        results = await search.search("python parser", mode="tfidf")

        assert len(results) >= 1
        assert results[0].chunk.id == "doc1"
        assert results[0].match_type == "tfidf"

    async def test_search_embedding_mode(
        self,
        store: InMemoryVectorStore,
        search: SemanticSearch,
    ):
        await store.add("doc1", "python json parser", {"symbol_kind": "function"})
        await store.add("doc2", "javascript renderer", {"symbol_kind": "function"})

        results = await search.search("python", mode="embedding")

        assert len(results) >= 1
        assert results[0].match_type == "semantic"

    async def test_search_hybrid_mode(
        self,
        store: InMemoryVectorStore,
        tfidf: TFIDFIndex,
        search: SemanticSearch,
    ):
        await store.add("doc1", "python json parser", {"symbol_kind": "function"})
        tfidf.add("doc1", "python json parser")

        await store.add("doc2", "javascript renderer", {"symbol_kind": "function"})
        tfidf.add("doc2", "javascript renderer")

        results = await search.search("python parser", mode="hybrid")

        assert len(results) >= 1
        assert results[0].match_type == "hybrid"

    async def test_search_with_filter(
        self,
        store: InMemoryVectorStore,
        search: SemanticSearch,
    ):
        await store.add("doc1", "python function", {"symbol_kind": "function"})
        await store.add("doc2", "python class", {"symbol_kind": "class"})

        results = await search.search("python", filter={"symbol_kind": "class"}, mode="embedding")

        assert len(results) == 1
        assert results[0].chunk.id == "doc2"


class TestSearchHit:
    """Tests for SearchHit dataclass."""

    def test_create_hit(self):
        chunk = CodeChunk(
            id="test:1",
            file_path="test.py",
            symbol_name="foo",
            symbol_kind="function",
            content="code",
            line_start=1,
            line_end=10,
        )

        hit = SearchHit(chunk=chunk, score=0.95, match_type="hybrid")

        assert hit.chunk.symbol_name == "foo"
        assert hit.score == 0.95
        assert hit.match_type == "hybrid"


class TestCreateSearchSystem:
    """Tests for factory function."""

    def test_create_memory_backend(self):
        indexer, search = create_search_system("memory")

        assert indexer is not None
        assert search is not None
        assert indexer.tfidf is search.tfidf  # Shared TF-IDF index

    def test_create_chroma_backend(self):
        indexer, search = create_search_system("chroma", collection_name="test_search")

        assert indexer is not None
        assert search is not None


class TestIntegration:
    """Integration tests for full search workflow."""

    async def test_index_and_search(self, tmp_path: Path):
        indexer, search = create_search_system("memory")

        # Create test file
        py_file = tmp_path / "parser.py"
        py_file.write_text('''
def parse_json(data: str) -> dict:
    """Parse a JSON string into a dictionary."""
    import json
    return json.loads(data)

def parse_xml(data: str):
    """Parse XML data."""
    pass

class ConfigLoader:
    """Load configuration files."""

    def load_yaml(self, path):
        """Load YAML config."""
        pass
''')

        # Index
        await indexer.index_file(py_file)

        # Search
        results = await search.search("parse JSON data")

        assert len(results) >= 1
        # parse_json should be in top results
        names = [r.chunk.symbol_name for r in results if r.chunk.symbol_name]
        assert any("parse_json" in (name or "") for name in names)

    async def test_index_update_on_change(self, tmp_path: Path):
        indexer, search = create_search_system("memory")

        py_file = tmp_path / "module.py"
        py_file.write_text("def old_function(): pass")

        await indexer.index_file(py_file)
        results1 = await search.search("old function")
        assert len(results1) >= 1  # Should find old_function

        # Update file
        py_file.write_text("def new_function(): pass")
        await indexer.index_file(py_file)

        results2 = await search.search("new function")

        # Old content should not match well, new should
        assert len(results2) >= 1
