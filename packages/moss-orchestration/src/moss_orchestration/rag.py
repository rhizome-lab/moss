"""RAG (Retrieval-Augmented Generation) interface for semantic code search.

This module provides a CLI-friendly interface for:
- `moss rag index <path>` - Build vector index of code/docs
- `moss rag search <query>` - Semantic search across indexed content
- `moss rag stats` - Show index statistics

Usage:
    from moss_orchestration.rag import RAGIndex

    # Create/load index
    rag = RAGIndex(project_root)

    # Index the codebase
    await rag.index()

    # Search
    results = await rag.search("function that parses config")
"""

from __future__ import annotations

import asyncio
import json
import logging
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


@dataclass
class SearchResult:
    """A search result with context."""

    file_path: str
    symbol_name: str | None
    symbol_kind: str | None
    line_start: int
    line_end: int
    score: float
    match_type: str
    snippet: str

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "file": self.file_path,
            "symbol": self.symbol_name,
            "kind": self.symbol_kind,
            "lines": f"{self.line_start}-{self.line_end}",
            "score": round(self.score, 3),
            "match_type": self.match_type,
            "snippet": self.snippet[:200] + "..." if len(self.snippet) > 200 else self.snippet,
        }

    def to_compact(self) -> str:
        """Format for LLM consumption."""
        symbol = self.symbol_name or self.file_path.split("/")[-1]
        kind = f" ({self.symbol_kind})" if self.symbol_kind else ""
        score = f"{self.score:.2f}"
        snippet_preview = self.snippet.strip().split("\n")[0][:60]
        return f"{symbol}{kind} [{score}] {self.file_path}:{self.line_start} - {snippet_preview}..."


@dataclass
class IndexStats:
    """Statistics about the RAG index."""

    total_documents: int = 0
    files_indexed: int = 0
    index_path: str = ""
    backend: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "total_documents": self.total_documents,
            "files_indexed": self.files_indexed,
            "index_path": self.index_path,
            "backend": self.backend,
        }

    def to_compact(self) -> str:
        """Format for LLM consumption."""
        return (
            f"RAG Index ({self.backend}): "
            f"{self.total_documents} chunks from {self.files_indexed} files"
        )


@dataclass
class RAGIndex:
    """RAG index for semantic search over a codebase.

    Stores the index in `.moss/rag/` within the project directory.
    Uses ChromaDB for persistent embedding storage when available,
    falls back to in-memory TF-IDF when ChromaDB is not installed.
    """

    root: Path
    _indexer: Any = field(default=None, repr=False)
    _search: Any = field(default=None, repr=False)
    _backend: str = field(default="", repr=False)
    _files_indexed: int = field(default=0, repr=False)

    def __post_init__(self) -> None:
        self.root = Path(self.root).resolve()

    @property
    def index_path(self) -> Path:
        """Path to the index storage."""
        return self.root / ".moss" / "rag"

    @property
    def metadata_path(self) -> Path:
        """Path to index metadata."""
        return self.index_path / "metadata.json"

    def _ensure_initialized(self) -> None:
        """Lazily initialize the search system."""
        if self._indexer is not None:
            return

        from moss_orchestration.semantic_search import CodeIndexer, SemanticSearch, TFIDFIndex

        # Ensure index directory exists
        self.index_path.mkdir(parents=True, exist_ok=True)

        # Priority: ChromaDB > SQLite > In-memory
        # ChromaDB: best embeddings but has binary deps
        # SQLite: persistent TF-IDF, no binary deps (works in Nix)
        # In-memory: fallback when nothing else works
        store = None

        # Try ChromaDB first (best quality)
        try:
            import chromadb  # noqa: F401 - check if available

            from moss_orchestration.vector_store import ChromaVectorStore

            store = ChromaVectorStore(
                collection_name="moss_rag",
                persist_directory=str(self.index_path / "chroma"),
            )
            self._backend = "chroma"
            logger.debug("Using ChromaDB backend for RAG index")
        except ImportError:
            pass

        # Fall back to SQLite (persistent, no binary deps)
        if store is None:
            try:
                from moss_orchestration.vector_store import SQLiteVectorStore

                store = SQLiteVectorStore(
                    db_path=str(self.index_path / "vectors.db"),
                )
                self._backend = "sqlite"
                logger.debug("Using SQLite backend for RAG index (persistent)")
            except (ImportError, OSError, ValueError) as e:
                logger.warning("SQLite backend failed: %s", e)

        # Last resort: in-memory (non-persistent)
        if store is None:
            from moss_orchestration.vector_store import InMemoryVectorStore

            store = InMemoryVectorStore()
            self._backend = "memory"
            logger.warning(
                "Using in-memory backend (index won't persist). "
                "SQLite should normally work - check for errors above."
            )

        tfidf = TFIDFIndex()
        self._indexer = CodeIndexer(store, tfidf)
        self._search = SemanticSearch(store, tfidf)

        # Load metadata
        self._load_metadata()

    def _load_metadata(self) -> None:
        """Load index metadata from disk."""
        if self.metadata_path.exists():
            try:
                data = json.loads(self.metadata_path.read_text())
                self._files_indexed = data.get("files_indexed", 0)
            except (OSError, json.JSONDecodeError):
                pass

    def _save_metadata(self) -> None:
        """Save index metadata to disk."""
        try:
            self.metadata_path.write_text(
                json.dumps(
                    {
                        "files_indexed": self._files_indexed,
                        "backend": self._backend,
                    },
                    indent=2,
                )
            )
        except OSError as e:
            logger.warning("Failed to save RAG metadata: %s", e)

    async def index(
        self,
        path: Path | None = None,
        patterns: list[str] | None = None,
        force: bool = False,
    ) -> int:
        """Index files for semantic search.

        Args:
            path: Directory to index (defaults to project root)
            patterns: Glob patterns to include (default: code and docs)
            force: Re-index even if content hasn't changed

        Returns:
            Number of chunks indexed
        """
        self._ensure_initialized()

        target = path or self.root
        patterns = patterns or [
            "**/*.py",
            "**/*.js",
            "**/*.ts",
            "**/*.md",
            "**/README*",
            "**/CLAUDE.md",
            "**/TODO.md",
        ]

        # Count files first
        files = []
        for pattern in patterns:
            for p in target.glob(pattern):
                if p.is_file() and not self._should_skip(p):
                    files.append(p)

        logger.info("Indexing %d files from %s", len(files), target)

        total_chunks = 0
        for file in files:
            try:
                chunks = await self._indexer.index_file(file, force=force)
                total_chunks += chunks
            except (OSError, UnicodeDecodeError, ValueError) as e:
                logger.warning("Failed to index %s: %s", file, e)

        self._files_indexed = len(files)
        self._save_metadata()

        logger.info("Indexed %d chunks from %d files", total_chunks, len(files))
        return total_chunks

    def _should_skip(self, path: Path) -> bool:
        """Check if a path should be skipped during indexing."""
        path_str = str(path)
        skip_patterns = [
            "/.git/",
            "/.venv/",
            "/venv/",
            "/node_modules/",
            "/__pycache__/",
            "/.moss/",
            "/dist/",
            "/build/",
            "/.mypy_cache/",
            "/.pytest_cache/",
        ]
        return any(pat in path_str for pat in skip_patterns)

    async def search(
        self,
        query: str,
        limit: int = 10,
        mode: str = "hybrid",
        kind: str | None = None,
    ) -> list[SearchResult]:
        """Search the index.

        Args:
            query: Natural language or code query
            limit: Maximum results
            mode: Search mode - "hybrid", "embedding", or "tfidf"
            kind: Filter by symbol kind (e.g., "function", "class", "module")

        Returns:
            List of SearchResult objects
        """
        self._ensure_initialized()

        filter_dict = None
        if kind:
            filter_dict = {"symbol_kind": kind}

        hits = await self._search.search(query, limit=limit, filter=filter_dict, mode=mode)

        results = []
        for hit in hits:
            results.append(
                SearchResult(
                    file_path=hit.chunk.file_path,
                    symbol_name=hit.chunk.symbol_name,
                    symbol_kind=hit.chunk.symbol_kind,
                    line_start=hit.chunk.line_start,
                    line_end=hit.chunk.line_end,
                    score=hit.score,
                    match_type=hit.match_type,
                    snippet=hit.chunk.content,
                )
            )

        return results

    async def stats(self) -> IndexStats:
        """Get index statistics."""
        self._ensure_initialized()

        doc_count = await self._indexer.store.count()

        return IndexStats(
            total_documents=doc_count,
            files_indexed=self._files_indexed,
            index_path=str(self.index_path),
            backend=self._backend,
        )

    async def clear(self) -> None:
        """Clear the index."""
        self._ensure_initialized()
        await self._indexer.store.clear()
        self._files_indexed = 0
        self._save_metadata()


def format_search_results(results: list[SearchResult]) -> str:
    """Format search results as markdown."""
    if not results:
        return "No results found."

    lines = [f"**Found {len(results)} results:**", ""]

    for i, result in enumerate(results, 1):
        symbol = result.symbol_name or result.file_path.split("/")[-1]
        kind = f" ({result.symbol_kind})" if result.symbol_kind else ""
        score = f"{result.score:.2f}"

        lines.append(f"**{i}. {symbol}**{kind} - score: {score}")
        lines.append(f"   `{result.file_path}:{result.line_start}-{result.line_end}`")

        # Show snippet preview
        snippet_lines = result.snippet.strip().split("\n")[:3]
        for line in snippet_lines:
            lines.append(f"   > {line[:80]}")
        if len(result.snippet.split("\n")) > 3:
            lines.append("   > ...")
        lines.append("")

    return "\n".join(lines)


async def index_project(
    root: Path | str,
    patterns: list[str] | None = None,
    force: bool = False,
) -> int:
    """Convenience function to index a project.

    Args:
        root: Project root directory
        patterns: Glob patterns to include
        force: Re-index everything

    Returns:
        Number of chunks indexed
    """
    rag = RAGIndex(Path(root))
    return await rag.index(patterns=patterns, force=force)


async def search_project(
    root: Path | str,
    query: str,
    limit: int = 10,
    mode: str = "hybrid",
) -> list[SearchResult]:
    """Convenience function to search a project.

    Args:
        root: Project root directory
        query: Search query
        limit: Maximum results
        mode: Search mode

    Returns:
        List of search results
    """
    rag = RAGIndex(Path(root))
    return await rag.search(query, limit=limit, mode=mode)


def run_index(
    root: Path | str,
    patterns: list[str] | None = None,
    force: bool = False,
) -> int:
    """Synchronous wrapper for index_project."""
    return asyncio.run(index_project(root, patterns, force))


def run_search(
    root: Path | str,
    query: str,
    limit: int = 10,
    mode: str = "hybrid",
) -> list[SearchResult]:
    """Synchronous wrapper for search_project."""
    return asyncio.run(search_project(root, query, limit, mode))
