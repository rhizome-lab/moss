"""Context memory management for documents and chatlogs.

This module provides:
- Recursive document summarization with merkle hashing
- Chatlog summarization and management
- Context window optimization
- Retrieval for relevant past conversations
"""

from __future__ import annotations

import hashlib
import json
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

# =============================================================================
# Document Summarization with Merkle Hashes
# =============================================================================


@dataclass(frozen=True)
class ContentHash:
    """Content-addressable hash for documents."""

    hash: str
    content_type: str  # "file", "section", "paragraph"
    size_bytes: int

    @classmethod
    def from_content(cls, content: str, content_type: str = "file") -> ContentHash:
        """Create a hash from content."""
        content_bytes = content.encode("utf-8")
        hash_value = hashlib.sha256(content_bytes).hexdigest()[:16]
        return cls(hash=hash_value, content_type=content_type, size_bytes=len(content_bytes))


@dataclass
class DocumentSummary:
    """Summary of a document or section."""

    content_hash: ContentHash
    summary: str
    key_points: list[str]
    children: list[DocumentSummary] = field(default_factory=list)
    source_path: str | None = None
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    token_count: int = 0  # Approximate token count

    @property
    def merkle_hash(self) -> str:
        """Compute merkle hash including children."""
        parts = [self.content_hash.hash]
        for child in self.children:
            parts.append(child.merkle_hash)
        combined = ":".join(sorted(parts))
        return hashlib.sha256(combined.encode()).hexdigest()[:16]


class Summarizer(ABC):
    """Abstract base for text summarization."""

    @abstractmethod
    async def summarize(self, text: str, max_length: int = 200) -> str:
        """Summarize text to max_length tokens."""
        ...

    @abstractmethod
    async def extract_key_points(self, text: str, max_points: int = 5) -> list[str]:
        """Extract key points from text."""
        ...


class SimpleSummarizer(Summarizer):
    """Simple extractive summarizer.

    For production, replace with an LLM-based summarizer.
    """

    async def summarize(self, text: str, max_length: int = 200) -> str:
        """Extract first sentences up to max_length chars."""
        sentences = text.replace("\n", " ").split(". ")
        result = []
        current_length = 0

        for sentence in sentences:
            sentence = sentence.strip()
            if not sentence:
                continue
            if current_length + len(sentence) > max_length * 4:  # ~4 chars per token
                break
            result.append(sentence)
            current_length += len(sentence)

        return ". ".join(result) + "." if result else text[: max_length * 4]

    async def extract_key_points(self, text: str, max_points: int = 5) -> list[str]:
        """Extract sentences containing keywords."""
        keywords = ["important", "key", "note", "must", "should", "critical", "ensure"]
        sentences = text.replace("\n", " ").split(". ")

        points = []
        for sentence in sentences:
            sentence = sentence.strip()
            if not sentence:
                continue
            if any(kw in sentence.lower() for kw in keywords):
                points.append(sentence)
                if len(points) >= max_points:
                    break

        # If no keyword matches, take first N sentences
        if not points:
            points = [s.strip() for s in sentences[:max_points] if s.strip()]

        return points


class DocumentSummaryStore:
    """Store for document summaries with merkle hash-based caching.

    Only re-summarizes documents when their content hash changes.
    """

    def __init__(
        self,
        summarizer: Summarizer | None = None,
        persist_path: Path | None = None,
    ):
        self._summarizer = summarizer or SimpleSummarizer()
        self._persist_path = persist_path
        self._summaries: dict[str, DocumentSummary] = {}  # merkle_hash -> summary
        self._path_index: dict[str, str] = {}  # source_path -> merkle_hash

        if persist_path and persist_path.exists():
            self._load()

    def _load(self) -> None:
        """Load persisted summaries."""
        if not self._persist_path or not self._persist_path.exists():
            return

        try:
            data = json.loads(self._persist_path.read_text())
            for entry in data.get("summaries", []):
                content_hash = ContentHash(
                    hash=entry["content_hash"]["hash"],
                    content_type=entry["content_hash"]["content_type"],
                    size_bytes=entry["content_hash"]["size_bytes"],
                )
                summary = DocumentSummary(
                    content_hash=content_hash,
                    summary=entry["summary"],
                    key_points=entry["key_points"],
                    source_path=entry.get("source_path"),
                    token_count=entry.get("token_count", 0),
                )
                self._summaries[summary.merkle_hash] = summary
                if summary.source_path:
                    self._path_index[summary.source_path] = summary.merkle_hash
        except (json.JSONDecodeError, KeyError):
            pass

    def _save(self) -> None:
        """Persist summaries to disk."""
        if not self._persist_path:
            return

        data = {
            "summaries": [
                {
                    "content_hash": {
                        "hash": s.content_hash.hash,
                        "content_type": s.content_hash.content_type,
                        "size_bytes": s.content_hash.size_bytes,
                    },
                    "summary": s.summary,
                    "key_points": s.key_points,
                    "source_path": s.source_path,
                    "token_count": s.token_count,
                }
                for s in self._summaries.values()
            ]
        }
        self._persist_path.parent.mkdir(parents=True, exist_ok=True)
        self._persist_path.write_text(json.dumps(data, indent=2))

    async def get_or_create(
        self,
        content: str,
        source_path: str | None = None,
        force_refresh: bool = False,
    ) -> DocumentSummary:
        """Get cached summary or create new one."""
        content_hash = ContentHash.from_content(content)

        # Check if we already have this summary
        if not force_refresh:
            for summary in self._summaries.values():
                if summary.content_hash.hash == content_hash.hash:
                    return summary

        # Create new summary
        summary_text = await self._summarizer.summarize(content)
        key_points = await self._summarizer.extract_key_points(content)

        summary = DocumentSummary(
            content_hash=content_hash,
            summary=summary_text,
            key_points=key_points,
            source_path=source_path,
            token_count=len(content) // 4,  # Approximate
        )

        self._summaries[summary.merkle_hash] = summary
        if source_path:
            self._path_index[source_path] = summary.merkle_hash

        self._save()
        return summary

    async def summarize_directory(
        self,
        directory: Path,
        patterns: list[str] | None = None,
    ) -> dict[str, DocumentSummary]:
        """Summarize all matching files in a directory."""
        patterns = patterns or ["**/*.md", "**/*.txt", "**/*.rst"]
        summaries: dict[str, DocumentSummary] = {}

        for pattern in patterns:
            for file_path in directory.glob(pattern):
                if file_path.is_file():
                    try:
                        content = file_path.read_text()
                        summary = await self.get_or_create(content, str(file_path))
                        summaries[str(file_path)] = summary
                    except (OSError, UnicodeDecodeError):
                        pass

        return summaries

    def get_by_path(self, path: str) -> DocumentSummary | None:
        """Get summary by source path."""
        merkle_hash = self._path_index.get(path)
        return self._summaries.get(merkle_hash) if merkle_hash else None

    def invalidate(self, path: str) -> bool:
        """Invalidate cached summary for a path."""
        merkle_hash = self._path_index.pop(path, None)
        if merkle_hash:
            self._summaries.pop(merkle_hash, None)
            self._save()
            return True
        return False

    @property
    def stats(self) -> dict[str, Any]:
        """Get store statistics."""
        return {
            "summary_count": len(self._summaries),
            "indexed_paths": len(self._path_index),
            "total_tokens": sum(s.token_count for s in self._summaries.values()),
        }


# =============================================================================
# Chatlog Management
# =============================================================================


@dataclass
class ChatMessage:
    """A single message in a chat log."""

    role: str  # "user", "assistant", "system"
    content: str
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        return {
            "role": self.role,
            "content": self.content,
            "timestamp": self.timestamp.isoformat(),
            "metadata": self.metadata,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChatMessage:
        """Create from dictionary."""
        return cls(
            role=data["role"],
            content=data["content"],
            timestamp=datetime.fromisoformat(data["timestamp"]),
            metadata=data.get("metadata", {}),
        )


@dataclass
class ChatSession:
    """A conversation session between user and agent."""

    id: str
    messages: list[ChatMessage] = field(default_factory=list)
    summary: str | None = None
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    updated_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    tags: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def add_message(self, role: str, content: str, **metadata: Any) -> ChatMessage:
        """Add a message to the session."""
        message = ChatMessage(role=role, content=content, metadata=metadata)
        self.messages.append(message)
        self.updated_at = datetime.now(UTC)
        return message

    @property
    def total_tokens(self) -> int:
        """Approximate total token count."""
        return sum(len(m.content) // 4 for m in self.messages)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        return {
            "id": self.id,
            "messages": [m.to_dict() for m in self.messages],
            "summary": self.summary,
            "created_at": self.created_at.isoformat(),
            "updated_at": self.updated_at.isoformat(),
            "tags": self.tags,
            "metadata": self.metadata,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChatSession:
        """Create from dictionary."""
        session = cls(
            id=data["id"],
            summary=data.get("summary"),
            created_at=datetime.fromisoformat(data["created_at"]),
            updated_at=datetime.fromisoformat(data["updated_at"]),
            tags=data.get("tags", []),
            metadata=data.get("metadata", {}),
        )
        session.messages = [ChatMessage.from_dict(m) for m in data.get("messages", [])]
        return session


class ChatlogStore:
    """Store and manage chat sessions.

    Features:
    - Persist full chat history as JSON
    - Summarize sessions for context retention
    - Retrieve relevant past conversations
    - Drop stale content from active context
    """

    def __init__(
        self,
        persist_path: Path | None = None,
        summarizer: Summarizer | None = None,
        max_sessions: int = 1000,
        max_context_tokens: int = 8000,
    ):
        self._persist_path = persist_path
        self._summarizer = summarizer or SimpleSummarizer()
        self._max_sessions = max_sessions
        self._max_context_tokens = max_context_tokens
        self._sessions: dict[str, ChatSession] = {}

        if persist_path and persist_path.exists():
            self._load()

    def _load(self) -> None:
        """Load persisted sessions."""
        if not self._persist_path or not self._persist_path.exists():
            return

        try:
            data = json.loads(self._persist_path.read_text())
            for session_data in data.get("sessions", []):
                session = ChatSession.from_dict(session_data)
                self._sessions[session.id] = session
        except (json.JSONDecodeError, KeyError):
            pass

    def _save(self) -> None:
        """Persist sessions to disk."""
        if not self._persist_path:
            return

        data = {"sessions": [s.to_dict() for s in self._sessions.values()]}
        self._persist_path.parent.mkdir(parents=True, exist_ok=True)
        self._persist_path.write_text(json.dumps(data, indent=2))

    def create_session(
        self,
        session_id: str | None = None,
        tags: list[str] | None = None,
        **metadata: Any,
    ) -> ChatSession:
        """Create a new chat session."""
        if session_id is None:
            session_id = hashlib.sha256(datetime.now(UTC).isoformat().encode()).hexdigest()[:12]

        # Evict oldest if at capacity
        if len(self._sessions) >= self._max_sessions:
            oldest = min(self._sessions.values(), key=lambda s: s.updated_at)
            del self._sessions[oldest.id]

        session = ChatSession(id=session_id, tags=tags or [], metadata=metadata)
        self._sessions[session_id] = session
        self._save()
        return session

    def get_session(self, session_id: str) -> ChatSession | None:
        """Get a session by ID."""
        return self._sessions.get(session_id)

    def add_message(
        self,
        session_id: str,
        role: str,
        content: str,
        **metadata: Any,
    ) -> ChatMessage | None:
        """Add a message to a session."""
        session = self._sessions.get(session_id)
        if session is None:
            return None

        message = session.add_message(role, content, **metadata)
        self._save()
        return message

    async def summarize_session(
        self,
        session_id: str,
        force_refresh: bool = False,
    ) -> str | None:
        """Summarize a session for context retention."""
        session = self._sessions.get(session_id)
        if session is None:
            return None

        if session.summary and not force_refresh:
            return session.summary

        # Combine all messages
        full_text = "\n".join(f"{m.role}: {m.content}" for m in session.messages)

        session.summary = await self._summarizer.summarize(full_text, max_length=500)
        self._save()
        return session.summary

    async def get_context_optimized(
        self,
        session_id: str,
        max_tokens: int | None = None,
    ) -> list[ChatMessage]:
        """Get messages optimized for context window.

        Keeps recent messages, drops middle content, summarizes old.
        """
        max_tokens = max_tokens or self._max_context_tokens
        session = self._sessions.get(session_id)
        if session is None:
            return []

        # Keep all if under limit
        if session.total_tokens <= max_tokens:
            return session.messages.copy()

        # Strategy: Keep first message (task), last N messages, summarize middle
        result: list[ChatMessage] = []
        current_tokens = 0

        # Always include first message (usually contains the task)
        if session.messages:
            first = session.messages[0]
            result.append(first)
            current_tokens += len(first.content) // 4

        # Add recent messages from the end
        recent: list[ChatMessage] = []
        for message in reversed(session.messages[1:]):
            msg_tokens = len(message.content) // 4
            if current_tokens + msg_tokens > max_tokens * 0.8:  # Leave room for summary
                break
            recent.append(message)
            current_tokens += msg_tokens

        recent.reverse()

        # If we dropped messages, add a summary
        dropped_count = len(session.messages) - len(result) - len(recent)
        if dropped_count > 0:
            # Summarize dropped messages
            dropped_messages = (
                session.messages[1 : -len(recent)] if recent else session.messages[1:]
            )
            if dropped_messages:
                dropped_text = "\n".join(f"{m.role}: {m.content}" for m in dropped_messages)
                summary = await self._summarizer.summarize(dropped_text, max_length=200)
                result.append(
                    ChatMessage(
                        role="system",
                        content=f"[Summary of {dropped_count} earlier messages: {summary}]",
                    )
                )

        result.extend(recent)
        return result

    async def search_sessions(
        self,
        query: str,
        limit: int = 10,
        tags: list[str] | None = None,
    ) -> list[ChatSession]:
        """Search for relevant past sessions."""
        results: list[tuple[ChatSession, float]] = []

        query_words = set(query.lower().split())

        for session in self._sessions.values():
            # Filter by tags
            if tags and not any(t in session.tags for t in tags):
                continue

            # Score by content match
            score = 0.0

            # Check summary
            if session.summary:
                summary_words = set(session.summary.lower().split())
                overlap = len(query_words & summary_words)
                score += overlap * 2

            # Check messages
            for message in session.messages[:10]:  # Only check first 10 messages
                msg_words = set(message.content.lower().split())
                overlap = len(query_words & msg_words)
                score += overlap

            if score > 0:
                results.append((session, score))

        # Sort by score descending
        results.sort(key=lambda x: -x[1])
        return [s for s, _ in results[:limit]]

    def delete_session(self, session_id: str) -> bool:
        """Delete a session."""
        if session_id in self._sessions:
            del self._sessions[session_id]
            self._save()
            return True
        return False

    @property
    def stats(self) -> dict[str, Any]:
        """Get store statistics."""
        total_messages = sum(len(s.messages) for s in self._sessions.values())
        total_tokens = sum(s.total_tokens for s in self._sessions.values())
        return {
            "session_count": len(self._sessions),
            "total_messages": total_messages,
            "total_tokens": total_tokens,
            "sessions_with_summary": sum(1 for s in self._sessions.values() if s.summary),
        }


# =============================================================================
# Factory Functions
# =============================================================================


_doc_store: DocumentSummaryStore | None = None
_chatlog_store: ChatlogStore | None = None


def get_document_store(persist_path: Path | None = None) -> DocumentSummaryStore:
    """Get or create global document summary store."""
    global _doc_store
    if _doc_store is None:
        _doc_store = DocumentSummaryStore(persist_path=persist_path)
    return _doc_store


def get_chatlog_store(persist_path: Path | None = None) -> ChatlogStore:
    """Get or create global chatlog store."""
    global _chatlog_store
    if _chatlog_store is None:
        _chatlog_store = ChatlogStore(persist_path=persist_path)
    return _chatlog_store


def reset_stores() -> None:
    """Reset global stores (for testing)."""
    global _doc_store, _chatlog_store
    _doc_store = None
    _chatlog_store = None
