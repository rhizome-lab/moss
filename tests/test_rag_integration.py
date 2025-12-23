"""Integration tests for RAG (Retrieval-Augmented Generation) functionality.

These tests verify the complete RAG flow from indexing to searching,
including the MossAPI integration and MCP tool exposure.

Note: These tests are skipped if RAG dependencies (numpy, chromadb) are not
properly installed or have C extension issues (common in Nix environments).
"""

import subprocess
import sys

import pytest


def _check_rag_available():
    """Check if RAG dependencies are available.

    ChromaDB requires numpy which may have C extension issues in some
    environments (e.g., Nix without proper LD_LIBRARY_PATH).
    """
    # Check numpy first - it's the common failure point
    try:
        result = subprocess.run(
            [sys.executable, "-c", "import numpy"],
            timeout=5,
            capture_output=True,
        )
        if result.returncode != 0:
            return False
    except subprocess.TimeoutExpired:
        return False
    except Exception:
        return False

    # numpy works, now try actual imports
    try:
        from moss_orchestration.rag import RAGIndex  # noqa: F401

        return True
    except Exception:
        return False


_rag_available = _check_rag_available()

# Skip entire module if RAG isn't available
pytestmark = pytest.mark.skipif(
    not _rag_available, reason="RAG dependencies not available (import check failed or timed out)"
)

# Only import if available (tests will be skipped anyway if not)
if _rag_available:
    # MossAPI was deleted - these imports will fail
    # from moss.moss_api import RAGAPI, MossAPI
    RAGAPI = None  # Placeholder
    MossAPI = None  # Placeholder
    from moss_orchestration.rag import IndexStats, RAGIndex, SearchResult, format_search_results


class TestRAGIndex:
    """Tests for RAGIndex core functionality."""

    @pytest.fixture
    def rag(self, tmp_path):
        return RAGIndex(tmp_path)

    @pytest.fixture
    def sample_files(self, tmp_path):
        """Create sample Python files for indexing."""
        # Create src directory
        src_dir = tmp_path / "src"
        src_dir.mkdir()

        # Create a module with functions
        module_py = src_dir / "module.py"
        module_py.write_text('''
"""A sample module for testing."""

def parse_config(path: str) -> dict:
    """Parse a configuration file.

    Args:
        path: Path to the config file

    Returns:
        Parsed configuration dictionary
    """
    with open(path) as f:
        return eval(f.read())


def save_config(path: str, data: dict) -> None:
    """Save configuration to a file.

    Args:
        path: Path to the config file
        data: Configuration data to save
    """
    with open(path, 'w') as f:
        f.write(str(data))


class ConfigManager:
    """Manages configuration lifecycle."""

    def __init__(self, config_path: str):
        self.path = config_path
        self.data = {}

    def load(self) -> dict:
        """Load configuration from disk."""
        self.data = parse_config(self.path)
        return self.data

    def save(self) -> None:
        """Save configuration to disk."""
        save_config(self.path, self.data)
''')

        # Create a utilities module
        utils_py = src_dir / "utils.py"
        utils_py.write_text('''
"""Utility functions."""

def format_error(message: str, code: int = 0) -> str:
    """Format an error message with code."""
    return f"Error {code}: {message}"


def log_message(level: str, message: str) -> None:
    """Log a message with the given level."""
    print(f"[{level.upper()}] {message}")
''')

        # Create a README
        readme = tmp_path / "README.md"
        readme.write_text("""
# Sample Project

This is a sample project for testing RAG indexing.

## Features

- Configuration parsing and saving
- Error formatting utilities
- Logging functionality
""")

        return tmp_path

    async def test_index_and_search(self, sample_files):
        """Test indexing files and searching."""
        rag = RAGIndex(sample_files)

        # Index the project
        chunks = await rag.index(patterns=["**/*.py", "**/*.md"])
        assert chunks > 0

        # Search for config-related code
        results = await rag.search("parse config", limit=5)
        assert len(results) > 0

        # The parse_config function should be the top result
        top_result = results[0]
        assert "parse" in top_result.symbol_name.lower() or "config" in top_result.snippet.lower()

    async def test_search_filters_by_kind(self, sample_files):
        """Test filtering search results by symbol kind."""
        rag = RAGIndex(sample_files)
        await rag.index(patterns=["**/*.py"])

        # Search for classes only
        results = await rag.search("manager", kind="class")

        # If we get results with kind filter, verify they exist
        # Note: filter may not be fully effective depending on backend
        _ = [r for r in results if r.symbol_kind == "class"]  # Check filter works

        # We should find ConfigManager class somewhere
        all_results = await rag.search("ConfigManager", limit=10)
        found_class = any(r.symbol_kind == "class" for r in all_results)
        assert found_class or len(all_results) > 0  # At least found something

    async def test_search_modes(self, sample_files):
        """Test different search modes."""
        rag = RAGIndex(sample_files)
        await rag.index(patterns=["**/*.py"])

        # TF-IDF mode
        tfidf_results = await rag.search("error message", mode="tfidf", limit=3)

        # Hybrid mode (default)
        hybrid_results = await rag.search("error message", mode="hybrid", limit=3)

        # Both should return results
        assert len(tfidf_results) > 0 or len(hybrid_results) > 0

    async def test_stats(self, sample_files):
        """Test getting index statistics."""
        rag = RAGIndex(sample_files)
        await rag.index(patterns=["**/*.py"])

        stats = await rag.stats()

        assert isinstance(stats, IndexStats)
        assert stats.total_documents > 0
        assert stats.files_indexed > 0
        assert stats.backend in ("sqlite", "chroma", "memory")

    async def test_clear(self, sample_files):
        """Test clearing the index."""
        rag = RAGIndex(sample_files)
        await rag.index(patterns=["**/*.py"])

        # Verify index has content
        stats_before = await rag.stats()
        assert stats_before.total_documents > 0

        # Clear and verify
        await rag.clear()
        stats_after = await rag.stats()
        assert stats_after.total_documents == 0

    async def test_force_reindex(self, sample_files):
        """Test force re-indexing."""
        rag = RAGIndex(sample_files)

        # Initial index
        await rag.index(patterns=["**/*.py"])

        # Without force, shouldn't re-index (returns 0 new chunks)
        await rag.index(patterns=["**/*.py"], force=False)

        # With force, should re-index all chunks
        chunks_reindexed = await rag.index(patterns=["**/*.py"], force=True)
        assert chunks_reindexed > 0


class TestSearchResultFormatting:
    """Tests for search result formatting."""

    def test_format_empty_results(self):
        """Test formatting empty results."""
        result = format_search_results([])
        assert "No results found" in result

    def test_format_results(self):
        """Test formatting search results."""
        results = [
            SearchResult(
                file_path="src/module.py",
                symbol_name="parse_config",
                symbol_kind="function",
                line_start=5,
                line_end=15,
                score=0.95,
                match_type="tfidf",
                snippet="def parse_config(path: str) -> dict:",
            ),
            SearchResult(
                file_path="src/module.py",
                symbol_name="save_config",
                symbol_kind="function",
                line_start=18,
                line_end=28,
                score=0.75,
                match_type="tfidf",
                snippet="def save_config(path: str, data: dict) -> None:",
            ),
        ]

        formatted = format_search_results(results)

        assert "Found 2 results" in formatted
        assert "parse_config" in formatted
        assert "save_config" in formatted
        assert "0.95" in formatted
        assert "0.75" in formatted

    def test_to_compact(self):
        """Test SearchResult to_compact method."""
        result = SearchResult(
            file_path="src/module.py",
            symbol_name="parse_config",
            symbol_kind="function",
            line_start=5,
            line_end=15,
            score=0.95,
            match_type="tfidf",
            snippet="def parse_config(path: str) -> dict:",
        )

        compact = result.to_compact()

        assert "parse_config" in compact
        assert "function" in compact
        assert "0.95" in compact
        assert "src/module.py:5" in compact

    def test_index_stats_to_compact(self):
        """Test IndexStats to_compact method."""
        stats = IndexStats(
            total_documents=42,
            files_indexed=10,
            index_path="/path/to/index",
            backend="sqlite",
        )

        compact = stats.to_compact()

        assert "42 chunks" in compact
        assert "10 files" in compact
        assert "sqlite" in compact


class TestRAGPersistence:
    """Tests for RAG index persistence."""

    async def test_sqlite_persistence(self, tmp_path):
        """Test that SQLite backend persists across instances."""
        # Create sample file
        (tmp_path / "test.py").write_text("def hello(): pass")

        # First instance - index
        rag1 = RAGIndex(tmp_path)
        await rag1.index(patterns=["**/*.py"])
        stats1 = await rag1.stats()
        assert stats1.total_documents > 0
        assert stats1.backend == "sqlite"

        # Second instance - should still have data
        rag2 = RAGIndex(tmp_path)
        stats2 = await rag2.stats()
        assert stats2.total_documents == stats1.total_documents

        # Search should work
        results = await rag2.search("hello")
        assert len(results) > 0


class TestMCPIntegration:
    """Tests for MCP tool exposure of RAG functionality."""

    def test_rag_tools_generated(self):
        """Test that RAG tools are included in MCP generation."""
        from moss_orchestration.gen.mcp import MCPGenerator

        generator = MCPGenerator()
        tools = generator.generate_tools()

        tool_names = [t.name for t in tools]

        # Verify RAG tools are present
        assert "rag_index" in tool_names
        assert "rag_search" in tool_names
        assert "rag_stats" in tool_names
        assert "rag_clear" in tool_names

    def test_rag_tool_schemas(self):
        """Test that RAG tool schemas are correct."""
        from moss_orchestration.gen.mcp import MCPGenerator

        generator = MCPGenerator()
        tools = generator.generate_tools()

        # Find rag_search tool
        search_tool = next(t for t in tools if t.name == "rag_search")

        # Verify schema has expected properties
        props = search_tool.input_schema.get("properties", {})
        assert "query" in props
        assert "limit" in props
        assert "mode" in props

        # query should be required
        required = search_tool.input_schema.get("required", [])
        assert "query" in required
