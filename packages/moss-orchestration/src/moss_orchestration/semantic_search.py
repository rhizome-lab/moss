"""Semantic code search with hybrid TF-IDF and embedding routing.

This module provides:
- CodeIndexer: Index code symbols and files into a vector store
- SemanticSearch: Search API combining TF-IDF filtering with embedding ranking
- Hybrid routing for optimal accuracy/speed tradeoff

Usage:
    indexer = CodeIndexer(store)
    await indexer.index_file(Path("src/foo.py"))

    search = SemanticSearch(store)
    results = await search.search("function that parses JSON")
"""

from __future__ import annotations

import hashlib
import logging
import math
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from moss_intelligence.skeleton import Symbol

    from moss_orchestration.vector_store import SearchResult, VectorStore

logger = logging.getLogger(__name__)


# =============================================================================
# Data Types
# =============================================================================


@dataclass
class CodeChunk:
    """A chunk of code indexed for search."""

    id: str
    file_path: str
    symbol_name: str | None
    symbol_kind: str | None  # function, class, method, module
    content: str
    line_start: int
    line_end: int
    signature: str | None = None
    docstring: str | None = None

    def to_document(self) -> str:
        """Convert to searchable document text."""
        parts = []

        if self.symbol_name:
            parts.append(f"{self.symbol_kind or 'symbol'}: {self.symbol_name}")

        if self.signature:
            parts.append(f"signature: {self.signature}")

        if self.docstring:
            parts.append(f"description: {self.docstring}")

        # Add the actual code content
        parts.append(self.content)

        return "\n".join(parts)

    def to_metadata(self) -> dict[str, Any]:
        """Convert to metadata dictionary."""
        return {
            "file_path": self.file_path,
            "symbol_name": self.symbol_name,
            "symbol_kind": self.symbol_kind,
            "line_start": self.line_start,
            "line_end": self.line_end,
            "has_docstring": self.docstring is not None,
        }


@dataclass
class SearchHit:
    """A search result with context."""

    chunk: CodeChunk
    score: float
    match_type: str  # "exact", "semantic", "hybrid"


# =============================================================================
# TF-IDF Index for Fast Filtering
# =============================================================================


@dataclass
class TFIDFIndex:
    """In-memory TF-IDF index for fast initial filtering."""

    # Term -> document IDs containing the term
    term_docs: dict[str, set[str]] = field(default_factory=dict)
    # Document ID -> term frequencies
    doc_terms: dict[str, Counter[str]] = field(default_factory=dict)
    # Cached IDF values
    idf: dict[str, float] = field(default_factory=dict)

    def add(self, doc_id: str, text: str) -> None:
        """Add a document to the index."""
        terms = self._tokenize(text)
        self.doc_terms[doc_id] = Counter(terms)

        for term in set(terms):
            if term not in self.term_docs:
                self.term_docs[term] = set()
            self.term_docs[term].add(doc_id)

        # Invalidate IDF cache
        self.idf.clear()

    def remove(self, doc_id: str) -> None:
        """Remove a document from the index."""
        if doc_id not in self.doc_terms:
            return

        terms = self.doc_terms.pop(doc_id)
        for term in terms:
            if term in self.term_docs:
                self.term_docs[term].discard(doc_id)
                if not self.term_docs[term]:
                    del self.term_docs[term]

        self.idf.clear()

    def search(self, query: str, limit: int = 100) -> list[tuple[str, float]]:
        """Search for documents matching query terms.

        Returns list of (doc_id, score) tuples sorted by score descending.
        """
        terms = self._tokenize(query)
        if not terms:
            return []

        # Compute IDF if needed
        if not self.idf and self.doc_terms:
            n_docs = len(self.doc_terms)
            for term, docs in self.term_docs.items():
                # Add 1 to ensure non-zero IDF for rare terms
                self.idf[term] = math.log((n_docs + 1) / (1 + len(docs))) + 1.0

        # Score documents
        scores: dict[str, float] = {}
        for term in terms:
            if term not in self.term_docs:
                continue

            idf = self.idf.get(term, 1.0)
            for doc_id in self.term_docs[term]:
                tf = self.doc_terms[doc_id][term]
                scores[doc_id] = scores.get(doc_id, 0) + tf * idf

        # Sort and limit
        sorted_results = sorted(scores.items(), key=lambda x: x[1], reverse=True)
        return sorted_results[:limit]

    def _tokenize(self, text: str) -> list[str]:
        """Simple tokenization: lowercase, split on non-alphanumeric."""
        import re

        # Split camelCase and snake_case
        text = re.sub(r"([a-z])([A-Z])", r"\1 \2", text)
        text = text.replace("_", " ")

        # Extract alphanumeric tokens
        tokens = re.findall(r"[a-z0-9]+", text.lower())

        # Filter very short tokens
        return [t for t in tokens if len(t) > 1]


# =============================================================================
# Code Indexer
# =============================================================================


class CodeIndexer:
    """Index code symbols and files into a vector store."""

    def __init__(
        self,
        store: VectorStore,
        tfidf_index: TFIDFIndex | None = None,
    ) -> None:
        """Initialize the indexer.

        Args:
            store: Vector store for embeddings
            tfidf_index: Optional TF-IDF index for hybrid search
        """
        self.store = store
        self.tfidf = tfidf_index or TFIDFIndex()
        self._indexed_files: dict[str, str] = {}  # path -> content hash

    async def index_file(self, path: Path, force: bool = False) -> int:
        """Index a single file.

        Args:
            path: Path to the file
            force: Re-index even if content hasn't changed

        Returns:
            Number of chunks indexed
        """
        if not path.exists():
            return 0

        content = path.read_text()
        content_hash = self._hash(content)

        # Skip if unchanged
        if not force and self._indexed_files.get(str(path)) == content_hash:
            return 0

        # Remove old chunks for this file
        await self._remove_file_chunks(path)

        # Create chunks based on file type
        chunks = self._create_chunks(path, content)

        if not chunks:
            return 0

        # Add to vector store and TF-IDF index
        ids = [c.id for c in chunks]
        documents = [c.to_document() for c in chunks]
        metadatas = [c.to_metadata() for c in chunks]

        await self.store.add_batch(ids, documents, metadatas)

        for chunk_id, doc in zip(ids, documents, strict=True):
            self.tfidf.add(chunk_id, doc)

        self._indexed_files[str(path)] = content_hash

        logger.debug("Indexed %d chunks from %s", len(chunks), path)
        return len(chunks)

    async def index_directory(
        self,
        directory: Path,
        patterns: list[str] | None = None,
        exclude: list[str] | None = None,
    ) -> int:
        """Index all matching files in a directory.

        Args:
            directory: Root directory to scan
            patterns: Glob patterns to include (default: *.py, *.js, *.ts)
            exclude: Glob patterns to exclude

        Returns:
            Total number of chunks indexed
        """
        patterns = patterns or ["**/*.py", "**/*.js", "**/*.ts", "**/*.md", "**/*.json"]
        exclude = exclude or ["**/node_modules/**", "**/.git/**", "**/__pycache__/**"]

        total = 0
        for pattern in patterns:
            for path in directory.glob(pattern):
                # Check exclusions
                if any(path.match(ex) for ex in exclude):
                    continue
                if path.is_file():
                    total += await self.index_file(path)

        logger.info("Indexed %d chunks from %s", total, directory)
        return total

    async def remove_file(self, path: Path) -> bool:
        """Remove a file from the index.

        Args:
            path: Path to remove

        Returns:
            True if file was indexed
        """
        path_str = str(path)
        if path_str not in self._indexed_files:
            return False

        await self._remove_file_chunks(path)
        del self._indexed_files[path_str]
        return True

    async def _remove_file_chunks(self, path: Path) -> None:
        """Remove all chunks for a file."""
        # We use file path prefix in chunk IDs
        path_str = str(path)

        # Get all doc IDs for this file from TF-IDF index
        to_remove = [doc_id for doc_id in self.tfidf.doc_terms if doc_id.startswith(f"{path_str}:")]

        for doc_id in to_remove:
            self.tfidf.remove(doc_id)
            await self.store.delete(doc_id)

    def _create_chunks(self, path: Path, content: str) -> list[CodeChunk]:
        """Create indexable chunks from file content."""
        suffix = path.suffix.lower()

        if suffix == ".py":
            return self._chunk_python(path, content)
        elif suffix in (".js", ".ts", ".jsx", ".tsx"):
            return self._chunk_generic(path, content)
        elif suffix == ".md":
            return self._chunk_markdown(path, content)
        else:
            return self._chunk_generic(path, content)

    def _chunk_python(self, path: Path, content: str) -> list[CodeChunk]:
        """Create chunks from Python file using AST."""
        try:
            from moss_intelligence.skeleton import PythonSkeletonExtractor
        except ImportError:
            return self._chunk_generic(path, content)

        chunks = []
        path_str = str(path)

        try:
            extractor = PythonSkeletonExtractor(content, include_private=True)
            import ast

            tree = ast.parse(content)
            extractor.visit(tree)

            # Create chunks for each top-level symbol
            for symbol in extractor.symbols:
                chunks.extend(self._symbol_to_chunks(path_str, content, symbol))

            # If no symbols, create a module-level chunk
            if not chunks:
                chunks.append(
                    CodeChunk(
                        id=f"{path_str}:module",
                        file_path=path_str,
                        symbol_name=path.stem,
                        symbol_kind="module",
                        content=content[:2000],  # Truncate large files
                        line_start=1,
                        line_end=content.count("\n") + 1,
                    )
                )

        except SyntaxError:
            # Fall back to generic chunking
            return self._chunk_generic(path, content)

        return chunks

    def _symbol_to_chunks(
        self,
        path_str: str,
        content: str,
        symbol: Symbol,
        parent: str = "",
    ) -> list[CodeChunk]:
        """Convert a Symbol to CodeChunks."""
        chunks = []
        lines = content.splitlines()

        full_name = f"{parent}.{symbol.name}" if parent else symbol.name
        chunk_id = f"{path_str}:{full_name}:{symbol.lineno}"

        # Get the source for this symbol
        start = symbol.lineno - 1
        end = symbol.end_lineno or symbol.lineno
        symbol_lines = lines[start:end]
        symbol_content = "\n".join(symbol_lines)

        chunks.append(
            CodeChunk(
                id=chunk_id,
                file_path=path_str,
                symbol_name=full_name,
                symbol_kind=symbol.kind,
                content=symbol_content[:2000],  # Truncate
                line_start=symbol.lineno,
                line_end=symbol.end_lineno or symbol.lineno,
                signature=symbol.signature,
                docstring=symbol.docstring,
            )
        )

        # Recurse for nested symbols (methods in classes)
        for child in symbol.children:
            chunks.extend(self._symbol_to_chunks(path_str, content, child, full_name))

        return chunks

    def _chunk_markdown(self, path: Path, content: str) -> list[CodeChunk]:
        """Create chunks from Markdown by sections."""
        chunks = []
        path_str = str(path)
        lines = content.splitlines()

        current_section = None
        section_start = 0
        section_lines: list[str] = []

        for i, line in enumerate(lines):
            if line.startswith("#"):
                # Save previous section
                if section_lines:
                    chunks.append(
                        CodeChunk(
                            id=f"{path_str}:section:{section_start}",
                            file_path=path_str,
                            symbol_name=current_section,
                            symbol_kind="section",
                            content="\n".join(section_lines),
                            line_start=section_start + 1,
                            line_end=i,
                        )
                    )

                # Start new section
                current_section = line.lstrip("#").strip()
                section_start = i
                section_lines = [line]
            else:
                section_lines.append(line)

        # Save final section
        if section_lines:
            chunks.append(
                CodeChunk(
                    id=f"{path_str}:section:{section_start}",
                    file_path=path_str,
                    symbol_name=current_section,
                    symbol_kind="section",
                    content="\n".join(section_lines),
                    line_start=section_start + 1,
                    line_end=len(lines),
                )
            )

        return chunks or [self._whole_file_chunk(path, content)]

    def _chunk_generic(self, path: Path, content: str) -> list[CodeChunk]:
        """Generic chunking for files without specific parser."""
        return [self._whole_file_chunk(path, content)]

    def _whole_file_chunk(self, path: Path, content: str) -> CodeChunk:
        """Create a single chunk for the whole file."""
        return CodeChunk(
            id=f"{path!s}:file",
            file_path=str(path),
            symbol_name=path.name,
            symbol_kind="file",
            content=content[:4000],  # Truncate large files
            line_start=1,
            line_end=content.count("\n") + 1,
        )

    def _hash(self, content: str) -> str:
        """Generate hash for content deduplication."""
        return hashlib.sha256(content.encode()).hexdigest()[:16]


# =============================================================================
# Semantic Search
# =============================================================================


class SemanticSearch:
    """Hybrid semantic search combining TF-IDF and embeddings."""

    def __init__(
        self,
        store: VectorStore,
        tfidf_index: TFIDFIndex | None = None,
        hybrid_weight: float = 0.7,
    ) -> None:
        """Initialize semantic search.

        Args:
            store: Vector store with embeddings
            tfidf_index: Optional TF-IDF index for hybrid search
            hybrid_weight: Weight for embedding score (1-weight for TF-IDF)
        """
        self.store = store
        self.tfidf = tfidf_index or TFIDFIndex()
        self.hybrid_weight = hybrid_weight

    async def search(
        self,
        query: str,
        limit: int = 10,
        filter: dict[str, Any] | None = None,
        mode: str = "hybrid",
    ) -> list[SearchHit]:
        """Search for code matching the query.

        Args:
            query: Natural language or code query
            limit: Maximum results
            filter: Metadata filter (e.g., {"symbol_kind": "function"})
            mode: Search mode - "hybrid", "embedding", or "tfidf"

        Returns:
            List of SearchHit results
        """
        if mode == "tfidf":
            return await self._search_tfidf(query, limit)
        elif mode == "embedding":
            return await self._search_embedding(query, limit, filter)
        else:
            return await self._search_hybrid(query, limit, filter)

    async def _search_tfidf(self, query: str, limit: int) -> list[SearchHit]:
        """TF-IDF only search."""
        tfidf_results = self.tfidf.search(query, limit)

        hits = []
        for doc_id, score in tfidf_results:
            result = await self.store.get(doc_id)
            if result:
                chunk = self._result_to_chunk(doc_id, result)
                # Normalize score to 0-1
                max_score = tfidf_results[0][1] if tfidf_results else 1.0
                hits.append(SearchHit(chunk=chunk, score=score / max_score, match_type="tfidf"))

        return hits

    async def _search_embedding(
        self, query: str, limit: int, filter: dict[str, Any] | None
    ) -> list[SearchHit]:
        """Embedding only search."""
        results = await self.store.search(query, limit, filter)

        hits = []
        for result in results:
            chunk = self._result_to_chunk(result.id, result)
            hits.append(SearchHit(chunk=chunk, score=result.score, match_type="semantic"))

        return hits

    async def _search_hybrid(
        self, query: str, limit: int, filter: dict[str, Any] | None
    ) -> list[SearchHit]:
        """Hybrid TF-IDF + embedding search.

        Strategy:
        1. Use TF-IDF to get top-k candidates (fast, keyword-based)
        2. Re-rank candidates using embedding similarity
        3. Combine scores with configurable weighting
        """
        # Get more candidates from TF-IDF for re-ranking
        tfidf_results = self.tfidf.search(query, limit * 3)

        if not tfidf_results:
            # Fall back to pure embedding search
            return await self._search_embedding(query, limit, filter)

        # Normalize TF-IDF scores
        max_tfidf = tfidf_results[0][1] if tfidf_results else 1.0
        tfidf_scores = {doc_id: score / max_tfidf for doc_id, score in tfidf_results}

        # Get embedding scores for candidates
        embedding_results = await self.store.search(query, limit * 3, filter)
        embedding_scores = {r.id: r.score for r in embedding_results}

        # Combine scores
        all_ids = set(tfidf_scores.keys()) | set(embedding_scores.keys())
        combined: list[tuple[str, float]] = []

        for doc_id in all_ids:
            tfidf_score = tfidf_scores.get(doc_id, 0.0)
            embed_score = embedding_scores.get(doc_id, 0.0)
            combined_score = (
                self.hybrid_weight * embed_score + (1 - self.hybrid_weight) * tfidf_score
            )
            combined.append((doc_id, combined_score))

        # Sort and limit
        combined.sort(key=lambda x: x[1], reverse=True)
        combined = combined[:limit]

        # Build results
        hits = []
        for doc_id, score in combined:
            result = await self.store.get(doc_id)
            if result:
                chunk = self._result_to_chunk(doc_id, result)
                hits.append(SearchHit(chunk=chunk, score=score, match_type="hybrid"))

        return hits

    def _result_to_chunk(self, doc_id: str, result: SearchResult) -> CodeChunk:
        """Convert a search result back to a CodeChunk."""
        meta = result.metadata or {}
        return CodeChunk(
            id=doc_id,
            file_path=meta.get("file_path", ""),
            symbol_name=meta.get("symbol_name"),
            symbol_kind=meta.get("symbol_kind"),
            content=result.document or "",
            line_start=meta.get("line_start", 1),
            line_end=meta.get("line_end", 1),
        )


# =============================================================================
# Factory Functions
# =============================================================================


def create_search_system(
    backend: str = "memory",
    **kwargs: Any,
) -> tuple[CodeIndexer, SemanticSearch]:
    """Create a complete search system.

    Args:
        backend: Vector store backend ("memory" or "chroma")
        **kwargs: Backend-specific configuration

    Returns:
        Tuple of (CodeIndexer, SemanticSearch)

    Example:
        indexer, search = create_search_system("chroma", persist_directory=".moss/index")
        await indexer.index_directory(Path("src"))
        results = await search.search("parse JSON config")
    """
    from moss_orchestration.vector_store import create_vector_store

    store = create_vector_store(backend, **kwargs)
    tfidf = TFIDFIndex()

    indexer = CodeIndexer(store, tfidf)
    search = SemanticSearch(store, tfidf)

    return indexer, search
