"""Vector store abstraction for semantic search.

This module provides:
- VectorStore protocol defining the interface for vector stores
- InMemoryVectorStore: Simple in-memory implementation for testing
- ChromaVectorStore: Integration with ChromaDB for production use

Usage:
    # In-memory (testing/development)
    store = InMemoryVectorStore()

    # With ChromaDB (production)
    store = ChromaVectorStore(collection_name="moss_episodes")

    # Use the store
    await store.add("doc1", "Python function for parsing", {"type": "code"})
    results = await store.search("parsing function", limit=5)
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass, field
from typing import Any, Protocol, runtime_checkable


@dataclass
class SearchResult:
    """Result from a vector store search."""

    id: str
    score: float
    metadata: dict[str, Any] = field(default_factory=dict)
    document: str | None = None


@runtime_checkable
class VectorStore(Protocol):
    """Protocol for vector store implementations.

    Vector stores provide semantic search over documents using embeddings.
    Implementations may use different backends (in-memory, ChromaDB, Pinecone, etc.)
    """

    async def add(
        self,
        id: str,
        document: str,
        metadata: dict[str, Any] | None = None,
    ) -> None:
        """Add a document to the store.

        Args:
            id: Unique identifier for the document
            document: Text content to embed and store
            metadata: Optional metadata to associate with the document
        """
        ...

    async def add_batch(
        self,
        ids: list[str],
        documents: list[str],
        metadatas: list[dict[str, Any]] | None = None,
    ) -> None:
        """Add multiple documents in a batch.

        Args:
            ids: Unique identifiers for each document
            documents: Text content for each document
            metadatas: Optional metadata for each document
        """
        ...

    async def search(
        self,
        query: str,
        limit: int = 10,
        filter: dict[str, Any] | None = None,
    ) -> list[SearchResult]:
        """Search for similar documents.

        Args:
            query: Search query text
            limit: Maximum number of results
            filter: Optional metadata filter

        Returns:
            List of SearchResult objects ordered by similarity
        """
        ...

    async def get(self, id: str) -> SearchResult | None:
        """Get a document by ID.

        Args:
            id: Document identifier

        Returns:
            SearchResult if found, None otherwise
        """
        ...

    async def delete(self, id: str) -> bool:
        """Delete a document by ID.

        Args:
            id: Document identifier

        Returns:
            True if deleted, False if not found
        """
        ...

    async def count(self) -> int:
        """Get the total number of documents in the store."""
        ...

    async def clear(self) -> None:
        """Remove all documents from the store."""
        ...


class InMemoryVectorStore:
    """Simple in-memory vector store using keyword matching.

    This is a basic implementation suitable for testing and small datasets.
    It uses TF-IDF-like keyword matching rather than true embeddings.
    """

    def __init__(self) -> None:
        """Initialize the in-memory store."""
        self._documents: dict[str, tuple[str, dict[str, Any]]] = {}
        self._index: dict[str, set[str]] = {}  # word -> doc_ids

    async def add(
        self,
        id: str,
        document: str,
        metadata: dict[str, Any] | None = None,
    ) -> None:
        """Add a document to the store."""
        metadata = metadata or {}
        self._documents[id] = (document, metadata)

        # Index words
        words = set(document.lower().split())
        for word in words:
            if word not in self._index:
                self._index[word] = set()
            self._index[word].add(id)

    async def add_batch(
        self,
        ids: list[str],
        documents: list[str],
        metadatas: list[dict[str, Any]] | None = None,
    ) -> None:
        """Add multiple documents in a batch."""
        metadatas = metadatas or [{} for _ in ids]
        for id, doc, meta in zip(ids, documents, metadatas, strict=True):
            await self.add(id, doc, meta)

    async def search(
        self,
        query: str,
        limit: int = 10,
        filter: dict[str, Any] | None = None,
    ) -> list[SearchResult]:
        """Search for similar documents using keyword matching."""
        query_words = set(query.lower().split())

        # Score documents by word overlap
        scores: dict[str, float] = {}
        for word in query_words:
            for doc_id in self._index.get(word, set()):
                scores[doc_id] = scores.get(doc_id, 0) + 1

        # Normalize scores
        if scores:
            max_score = max(scores.values())
            scores = {k: v / max_score for k, v in scores.items()}

        # Apply filter
        if filter:
            filtered_scores = {}
            for doc_id, score in scores.items():
                doc, metadata = self._documents[doc_id]
                if all(metadata.get(k) == v for k, v in filter.items()):
                    filtered_scores[doc_id] = score
            scores = filtered_scores

        # Sort and limit
        sorted_ids = sorted(scores.keys(), key=lambda x: scores[x], reverse=True)[:limit]

        results = []
        for doc_id in sorted_ids:
            doc, metadata = self._documents[doc_id]
            results.append(
                SearchResult(
                    id=doc_id,
                    score=scores[doc_id],
                    metadata=metadata,
                    document=doc,
                )
            )

        return results

    async def get(self, id: str) -> SearchResult | None:
        """Get a document by ID."""
        if id not in self._documents:
            return None
        doc, metadata = self._documents[id]
        return SearchResult(id=id, score=1.0, metadata=metadata, document=doc)

    async def delete(self, id: str) -> bool:
        """Delete a document by ID."""
        if id not in self._documents:
            return False

        doc, _ = self._documents[id]
        del self._documents[id]

        # Remove from index
        words = set(doc.lower().split())
        for word in words:
            if word in self._index:
                self._index[word].discard(id)
                if not self._index[word]:
                    del self._index[word]

        return True

    async def count(self) -> int:
        """Get the total number of documents."""
        return len(self._documents)

    async def clear(self) -> None:
        """Remove all documents."""
        self._documents.clear()
        self._index.clear()


class ChromaVectorStore:
    """Vector store backed by ChromaDB.

    ChromaDB provides real embedding-based semantic search.
    Requires: pip install chromadb

    Usage:
        store = ChromaVectorStore(
            collection_name="moss_episodes",
            persist_directory=".moss/chroma",
        )
    """

    def __init__(
        self,
        collection_name: str = "moss",
        persist_directory: str | None = None,
        embedding_function: Any | None = None,
    ) -> None:
        """Initialize ChromaDB store.

        Args:
            collection_name: Name of the collection
            persist_directory: Directory to persist data (None for in-memory)
            embedding_function: Custom embedding function (uses default if None)
        """
        self._collection_name = collection_name
        self._persist_directory = persist_directory
        self._embedding_function = embedding_function
        self._client: Any = None
        self._collection: Any = None

    def _ensure_initialized(self) -> None:
        """Lazily initialize ChromaDB client and collection."""
        if self._client is not None:
            return

        try:
            import chromadb
            from chromadb.config import Settings
        except ImportError as e:
            raise ImportError("ChromaDB not installed. Install with: pip install chromadb") from e

        settings = Settings(anonymized_telemetry=False)

        if self._persist_directory:
            self._client = chromadb.PersistentClient(
                path=self._persist_directory,
                settings=settings,
            )
        else:
            self._client = chromadb.Client(settings)

        # Get or create collection
        kwargs: dict[str, Any] = {"name": self._collection_name}
        if self._embedding_function:
            kwargs["embedding_function"] = self._embedding_function

        self._collection = self._client.get_or_create_collection(**kwargs)

    async def add(
        self,
        id: str,
        document: str,
        metadata: dict[str, Any] | None = None,
    ) -> None:
        """Add a document to ChromaDB."""
        self._ensure_initialized()
        self._collection.upsert(
            ids=[id],
            documents=[document],
            metadatas=[metadata or {}],
        )

    async def add_batch(
        self,
        ids: list[str],
        documents: list[str],
        metadatas: list[dict[str, Any]] | None = None,
    ) -> None:
        """Add multiple documents to ChromaDB."""
        self._ensure_initialized()
        self._collection.upsert(
            ids=ids,
            documents=documents,
            metadatas=metadatas or [{} for _ in ids],
        )

    async def search(
        self,
        query: str,
        limit: int = 10,
        filter: dict[str, Any] | None = None,
    ) -> list[SearchResult]:
        """Search ChromaDB for similar documents."""
        self._ensure_initialized()

        kwargs: dict[str, Any] = {
            "query_texts": [query],
            "n_results": limit,
            "include": ["documents", "metadatas", "distances"],
        }
        if filter:
            kwargs["where"] = filter

        results = self._collection.query(**kwargs)

        search_results = []
        if results["ids"] and results["ids"][0]:
            ids = results["ids"][0]
            documents = results["documents"][0] if results["documents"] else [None] * len(ids)
            metadatas = results["metadatas"][0] if results["metadatas"] else [{}] * len(ids)
            distances = results["distances"][0] if results["distances"] else [0.0] * len(ids)

            for i, doc_id in enumerate(ids):
                # Convert distance to similarity score (1 - normalized_distance)
                score = max(0.0, 1.0 - distances[i])
                search_results.append(
                    SearchResult(
                        id=doc_id,
                        score=score,
                        metadata=metadatas[i] or {},
                        document=documents[i],
                    )
                )

        return search_results

    async def get(self, id: str) -> SearchResult | None:
        """Get a document from ChromaDB by ID."""
        self._ensure_initialized()
        results = self._collection.get(
            ids=[id],
            include=["documents", "metadatas"],
        )

        if not results["ids"]:
            return None

        return SearchResult(
            id=id,
            score=1.0,
            metadata=results["metadatas"][0] if results["metadatas"] else {},
            document=results["documents"][0] if results["documents"] else None,
        )

    async def delete(self, id: str) -> bool:
        """Delete a document from ChromaDB."""
        self._ensure_initialized()

        # Check if exists
        existing = await self.get(id)
        if existing is None:
            return False

        self._collection.delete(ids=[id])
        return True

    async def count(self) -> int:
        """Get the total number of documents."""
        self._ensure_initialized()
        return self._collection.count()

    async def clear(self) -> None:
        """Remove all documents."""
        self._ensure_initialized()
        # Delete and recreate collection
        self._client.delete_collection(self._collection_name)
        kwargs: dict[str, Any] = {"name": self._collection_name}
        if self._embedding_function:
            kwargs["embedding_function"] = self._embedding_function
        self._collection = self._client.create_collection(**kwargs)


class SQLiteVectorStore:
    """Vector store backed by SQLite for persistent TF-IDF search.

    This provides a lightweight persistent alternative to ChromaDB that works
    in Nix environments without binary dependencies. Uses TF-IDF for ranking.

    Usage:
        store = SQLiteVectorStore(db_path=".moss/rag/vectors.db")
    """

    def __init__(self, db_path: str) -> None:
        """Initialize SQLite store.

        Args:
            db_path: Path to SQLite database file
        """
        self._db_path = db_path
        self._conn: Any = None

    def _ensure_initialized(self) -> None:
        """Lazily initialize SQLite connection and tables."""
        if self._conn is not None:
            return

        import json
        import sqlite3
        from pathlib import Path

        # Ensure directory exists
        Path(self._db_path).parent.mkdir(parents=True, exist_ok=True)

        self._conn = sqlite3.connect(self._db_path)
        self._conn.row_factory = sqlite3.Row

        # Create tables
        self._conn.executescript("""
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                document TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE IF NOT EXISTS word_index (
                word TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                count INTEGER NOT NULL DEFAULT 1,
                PRIMARY KEY (word, doc_id),
                FOREIGN KEY (doc_id) REFERENCES documents(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_word ON word_index(word);
        """)
        self._conn.commit()

        # Store json module reference for later use
        self._json = json

    def _tokenize(self, text: str) -> dict[str, int]:
        """Tokenize text into word counts."""
        import re

        words = re.findall(r"\b\w+\b", text.lower())
        counts: dict[str, int] = {}
        for word in words:
            if len(word) >= 2:  # Skip single chars
                counts[word] = counts.get(word, 0) + 1
        return counts

    async def add(
        self,
        id: str,
        document: str,
        metadata: dict[str, Any] | None = None,
    ) -> None:
        """Add a document to SQLite."""
        self._ensure_initialized()

        metadata = metadata or {}
        metadata_json = self._json.dumps(metadata)

        # Delete existing if present
        self._conn.execute("DELETE FROM word_index WHERE doc_id = ?", (id,))
        self._conn.execute("DELETE FROM documents WHERE id = ?", (id,))

        # Insert document
        self._conn.execute(
            "INSERT INTO documents (id, document, metadata) VALUES (?, ?, ?)",
            (id, document, metadata_json),
        )

        # Index words
        word_counts = self._tokenize(document)
        for word, count in word_counts.items():
            self._conn.execute(
                "INSERT INTO word_index (word, doc_id, count) VALUES (?, ?, ?)",
                (word, id, count),
            )

        self._conn.commit()

    async def add_batch(
        self,
        ids: list[str],
        documents: list[str],
        metadatas: list[dict[str, Any]] | None = None,
    ) -> None:
        """Add multiple documents in a batch."""
        self._ensure_initialized()
        metadatas = metadatas or [{} for _ in ids]

        # Process in a transaction for efficiency
        for id, doc, meta in zip(ids, documents, metadatas, strict=True):
            metadata_json = self._json.dumps(meta)

            self._conn.execute("DELETE FROM word_index WHERE doc_id = ?", (id,))
            self._conn.execute("DELETE FROM documents WHERE id = ?", (id,))

            self._conn.execute(
                "INSERT INTO documents (id, document, metadata) VALUES (?, ?, ?)",
                (id, doc, metadata_json),
            )

            word_counts = self._tokenize(doc)
            for word, count in word_counts.items():
                self._conn.execute(
                    "INSERT INTO word_index (word, doc_id, count) VALUES (?, ?, ?)",
                    (word, id, count),
                )

        self._conn.commit()

    async def search(
        self,
        query: str,
        limit: int = 10,
        filter: dict[str, Any] | None = None,
    ) -> list[SearchResult]:
        """Search using TF-IDF-like scoring."""
        self._ensure_initialized()

        query_words = list(self._tokenize(query).keys())
        if not query_words:
            return []

        # Build query to find documents matching any query word
        placeholders = ",".join("?" * len(query_words))
        sql = f"""
            SELECT doc_id, SUM(count) as score
            FROM word_index
            WHERE word IN ({placeholders})
            GROUP BY doc_id
            ORDER BY score DESC
            LIMIT ?
        """

        cursor = self._conn.execute(sql, [*query_words, limit * 2])  # Fetch extra for filtering
        doc_scores = [(row["doc_id"], row["score"]) for row in cursor]

        if not doc_scores:
            return []

        # Normalize scores
        max_score = max(score for _, score in doc_scores)
        normalized = [(doc_id, score / max_score) for doc_id, score in doc_scores]

        # Fetch documents and apply filter
        results = []
        for doc_id, score in normalized:
            if len(results) >= limit:
                break

            row = self._conn.execute(
                "SELECT document, metadata FROM documents WHERE id = ?",
                (doc_id,),
            ).fetchone()

            if row:
                metadata = self._json.loads(row["metadata"])

                # Apply filter
                if filter:
                    if not all(metadata.get(k) == v for k, v in filter.items()):
                        continue

                results.append(
                    SearchResult(
                        id=doc_id,
                        score=score,
                        metadata=metadata,
                        document=row["document"],
                    )
                )

        return results

    async def get(self, id: str) -> SearchResult | None:
        """Get a document by ID."""
        self._ensure_initialized()

        row = self._conn.execute(
            "SELECT document, metadata FROM documents WHERE id = ?",
            (id,),
        ).fetchone()

        if not row:
            return None

        return SearchResult(
            id=id,
            score=1.0,
            metadata=self._json.loads(row["metadata"]),
            document=row["document"],
        )

    async def delete(self, id: str) -> bool:
        """Delete a document by ID."""
        self._ensure_initialized()

        # Check if exists
        existing = await self.get(id)
        if existing is None:
            return False

        self._conn.execute("DELETE FROM word_index WHERE doc_id = ?", (id,))
        self._conn.execute("DELETE FROM documents WHERE id = ?", (id,))
        self._conn.commit()
        return True

    async def count(self) -> int:
        """Get the total number of documents."""
        self._ensure_initialized()
        row = self._conn.execute("SELECT COUNT(*) as cnt FROM documents").fetchone()
        return row["cnt"] if row else 0

    async def clear(self) -> None:
        """Remove all documents."""
        self._ensure_initialized()
        self._conn.execute("DELETE FROM word_index")
        self._conn.execute("DELETE FROM documents")
        self._conn.commit()

    def close(self) -> None:
        """Close the database connection."""
        if self._conn:
            self._conn.close()
            self._conn = None


def create_vector_store(
    backend: str = "memory",
    **kwargs: Any,
) -> VectorStore:
    """Factory function to create a vector store.

    Args:
        backend: Store backend ("memory", "sqlite", or "chroma")
        **kwargs: Backend-specific configuration

    Returns:
        VectorStore instance

    Examples:
        # In-memory store
        store = create_vector_store("memory")

        # SQLite store (persistent, no binary deps)
        store = create_vector_store("sqlite", db_path=".moss/rag/vectors.db")

        # ChromaDB store (requires chromadb package)
        store = create_vector_store(
            "chroma",
            collection_name="my_collection",
            persist_directory=".moss/chroma",
        )
    """
    if backend == "memory":
        return InMemoryVectorStore()
    elif backend == "sqlite":
        return SQLiteVectorStore(**kwargs)
    elif backend == "chroma":
        return ChromaVectorStore(**kwargs)
    else:
        raise ValueError(f"Unknown backend: {backend}. Use 'memory', 'sqlite', or 'chroma'.")


def document_hash(content: str) -> str:
    """Generate a short hash for document deduplication.

    Args:
        content: Document content

    Returns:
        8-character hex hash
    """
    return hashlib.sha256(content.encode()).hexdigest()[:8]
