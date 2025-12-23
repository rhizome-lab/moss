"""moss-context: Working memory for agents.

Domain-agnostic working memory management. No code knowledge, no LLM dependency.

Example:
    from moss_context import WorkingMemory, Item

    memory = WorkingMemory(budget=8000)
    memory.add(Item(id="doc:readme", content="...", relevance=0.9))
    memory.add(Item(id="file:main.py", content="...", relevance=0.7))

    # Check budget
    if not memory.fits("new content here"):
        memory.compact()  # Evict low-relevance items

    # Render for LLM
    prompt = memory.render()
"""

from dataclasses import dataclass, field
from datetime import datetime, UTC
from typing import Protocol, runtime_checkable
import hashlib


@runtime_checkable
class Summarizer(Protocol):
    """Protocol for summarization strategies.

    Implementations can use LLM, extractive methods, or custom logic.
    moss-context doesn't provide implementations - callers plug them in.
    """

    def summarize(self, items: list[str]) -> str:
        """Summarize multiple items into one."""
        ...


@runtime_checkable
class TokenCounter(Protocol):
    """Protocol for counting tokens.

    Default uses simple word-based estimation.
    Callers can plug in tiktoken or other tokenizers.
    """

    def count(self, text: str) -> int:
        """Count tokens in text."""
        ...


class SimpleTokenCounter:
    """Simple word-based token counter (~1.3 tokens per word)."""

    def count(self, text: str) -> int:
        words = len(text.split())
        return int(words * 1.3)


@dataclass
class Item:
    """An item in working memory."""

    id: str
    content: str
    relevance: float = 1.0
    tokens: int | None = None
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    accessed_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    access_count: int = 0
    metadata: dict = field(default_factory=dict)

    @property
    def content_hash(self) -> str:
        """Hash of content for deduplication."""
        return hashlib.sha256(self.content.encode()).hexdigest()[:16]

    def touch(self) -> None:
        """Mark as accessed."""
        self.accessed_at = datetime.now(UTC)
        self.access_count += 1


class WorkingMemory:
    """Working memory with token budgeting.

    Manages a collection of items within a token budget.
    Supports eviction strategies and optional summarization.
    """

    def __init__(
        self,
        budget: int = 8000,
        token_counter: TokenCounter | None = None,
        summarizer: Summarizer | None = None,
    ):
        """Initialize working memory.

        Args:
            budget: Maximum tokens to hold
            token_counter: Custom token counter (default: SimpleTokenCounter)
            summarizer: Optional summarizer for compaction
        """
        self.budget = budget
        self.token_counter = token_counter or SimpleTokenCounter()
        self.summarizer = summarizer
        self._items: dict[str, Item] = {}

    @property
    def items(self) -> list[Item]:
        """All items, sorted by relevance (highest first)."""
        return sorted(self._items.values(), key=lambda i: -i.relevance)

    @property
    def total_tokens(self) -> int:
        """Total tokens across all items."""
        return sum(self._get_tokens(item) for item in self._items.values())

    @property
    def available_tokens(self) -> int:
        """Tokens available within budget."""
        return max(0, self.budget - self.total_tokens)

    def _get_tokens(self, item: Item) -> int:
        """Get token count for item, computing if needed."""
        if item.tokens is None:
            item.tokens = self.token_counter.count(item.content)
        return item.tokens

    # === Core Operations ===

    def add(self, item: Item) -> bool:
        """Add item to memory.

        Args:
            item: Item to add

        Returns:
            True if added, False if would exceed budget
        """
        tokens = self._get_tokens(item)
        if tokens > self.available_tokens:
            return False
        self._items[item.id] = item
        return True

    def get(self, id: str) -> Item | None:
        """Get item by ID, marking as accessed."""
        item = self._items.get(id)
        if item:
            item.touch()
        return item

    def remove(self, id: str) -> bool:
        """Remove item by ID.

        Returns:
            True if removed, False if not found
        """
        if id in self._items:
            del self._items[id]
            return True
        return False

    def update(self, id: str, **changes) -> Item | None:
        """Update item fields.

        Args:
            id: Item ID
            **changes: Fields to update (content, relevance, metadata)

        Returns:
            Updated item, or None if not found
        """
        item = self._items.get(id)
        if not item:
            return None

        for key, value in changes.items():
            if hasattr(item, key):
                setattr(item, key, value)

        # Recompute tokens if content changed
        if "content" in changes:
            item.tokens = None

        return item

    # === Querying ===

    def fits(self, content: str) -> bool:
        """Check if content would fit in available budget."""
        tokens = self.token_counter.count(content)
        return tokens <= self.available_tokens

    def find(self, predicate: callable) -> list[Item]:
        """Find items matching predicate."""
        return [item for item in self._items.values() if predicate(item)]

    # === Eviction ===

    def compact(self, target_tokens: int | None = None) -> int:
        """Evict low-relevance items to free space.

        Args:
            target_tokens: Target available tokens (default: 25% of budget)

        Returns:
            Tokens freed
        """
        target = target_tokens or self.budget // 4
        freed = 0

        # Sort by relevance (lowest first) for eviction
        candidates = sorted(self._items.values(), key=lambda i: i.relevance)

        for item in candidates:
            if self.available_tokens >= target:
                break
            tokens = self._get_tokens(item)
            del self._items[item.id]
            freed += tokens

        return freed

    def evict_lru(self, count: int = 1) -> int:
        """Evict least recently used items.

        Args:
            count: Number of items to evict

        Returns:
            Tokens freed
        """
        freed = 0
        by_access = sorted(self._items.values(), key=lambda i: i.accessed_at)

        for item in by_access[:count]:
            tokens = self._get_tokens(item)
            del self._items[item.id]
            freed += tokens

        return freed

    # === Summarization ===

    def summarize_items(self, ids: list[str]) -> str | None:
        """Summarize multiple items into one.

        Requires a summarizer to be configured.

        Args:
            ids: Item IDs to summarize

        Returns:
            Summary text, or None if no summarizer
        """
        if not self.summarizer:
            return None

        contents = []
        for id in ids:
            item = self._items.get(id)
            if item:
                contents.append(item.content)

        if not contents:
            return None

        return self.summarizer.summarize(contents)

    def compact_by_summary(self, ids: list[str], new_id: str) -> Item | None:
        """Replace multiple items with their summary.

        Args:
            ids: Item IDs to summarize and remove
            new_id: ID for the summary item

        Returns:
            New summary item, or None if summarization failed
        """
        summary = self.summarize_items(ids)
        if not summary:
            return None

        # Calculate combined relevance (max of originals)
        relevance = max(
            (self._items[id].relevance for id in ids if id in self._items),
            default=0.5
        )

        # Remove originals
        for id in ids:
            self.remove(id)

        # Add summary
        item = Item(id=new_id, content=summary, relevance=relevance)
        self.add(item)
        return item

    # === Rendering ===

    def render(self, separator: str = "\n\n---\n\n") -> str:
        """Render all items as a single string.

        Args:
            separator: Separator between items

        Returns:
            Combined content string
        """
        parts = []
        for item in self.items:  # Already sorted by relevance
            parts.append(f"[{item.id}]\n{item.content}")
        return separator.join(parts)

    def render_within_budget(self, max_tokens: int | None = None) -> str:
        """Render items up to token limit.

        Args:
            max_tokens: Token limit (default: full budget)

        Returns:
            Combined content string
        """
        limit = max_tokens or self.budget
        parts = []
        tokens_used = 0

        for item in self.items:
            item_tokens = self._get_tokens(item)
            if tokens_used + item_tokens > limit:
                break
            parts.append(f"[{item.id}]\n{item.content}")
            tokens_used += item_tokens

        return "\n\n---\n\n".join(parts)

    # === Serialization ===

    def to_dict(self) -> dict:
        """Serialize to dictionary."""
        return {
            "budget": self.budget,
            "items": [
                {
                    "id": item.id,
                    "content": item.content,
                    "relevance": item.relevance,
                    "tokens": item.tokens,
                    "metadata": item.metadata,
                }
                for item in self._items.values()
            ],
        }

    @classmethod
    def from_dict(
        cls,
        data: dict,
        token_counter: TokenCounter | None = None,
        summarizer: Summarizer | None = None,
    ) -> "WorkingMemory":
        """Deserialize from dictionary."""
        memory = cls(
            budget=data["budget"],
            token_counter=token_counter,
            summarizer=summarizer,
        )
        for item_data in data.get("items", []):
            item = Item(
                id=item_data["id"],
                content=item_data["content"],
                relevance=item_data.get("relevance", 1.0),
                tokens=item_data.get("tokens"),
                metadata=item_data.get("metadata", {}),
            )
            memory._items[item.id] = item
        return memory


__all__ = [
    "WorkingMemory",
    "Item",
    "Summarizer",
    "TokenCounter",
    "SimpleTokenCounter",
]
