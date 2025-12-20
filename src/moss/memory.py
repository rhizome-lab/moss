"""Memory Layer: Episodic store, semantic rules, and plugin system.

Layers:
- automatic: Always loaded into system prompt
- triggered: Pattern-activated, injected when relevant
- on_demand: Explicit recall via memory.recall()

Plugins are loaded from:
- Built-in plugins (preferences, episodic, semantic)
- .moss/memory/*.py (project-specific)
- ~/.config/moss/memory/*.py (user-level)
"""

from __future__ import annotations

import hashlib
import importlib.util
import logging
import sys
from abc import ABC, abstractmethod
from collections import OrderedDict
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum, auto
from pathlib import Path
from typing import TYPE_CHECKING, Any, Literal, Protocol, runtime_checkable

if TYPE_CHECKING:
    from collections.abc import Iterator, Sequence

logger = logging.getLogger(__name__)


# =============================================================================
# LRU Cache
# =============================================================================


class LRUCache[K, V]:
    """Thread-safe LRU cache with O(1) operations.

    Uses OrderedDict for efficient move-to-end on access.
    Evicts least recently used items when capacity is reached.
    """

    def __init__(self, capacity: int):
        if capacity < 1:
            raise ValueError("Capacity must be at least 1")
        self._capacity = capacity
        self._cache: OrderedDict[K, V] = OrderedDict()

    def get(self, key: K) -> V | None:
        """Get item and mark as recently used. Returns None if not found."""
        if key not in self._cache:
            return None
        self._cache.move_to_end(key)
        return self._cache[key]

    def put(self, key: K, value: V) -> K | None:
        """Add/update item. Returns evicted key if capacity exceeded, else None."""
        evicted: K | None = None
        if key in self._cache:
            self._cache.move_to_end(key)
        else:
            if len(self._cache) >= self._capacity:
                evicted, _ = self._cache.popitem(last=False)
        self._cache[key] = value
        return evicted

    def delete(self, key: K) -> bool:
        """Remove item. Returns True if found and removed."""
        if key in self._cache:
            del self._cache[key]
            return True
        return False

    def peek(self, key: K) -> V | None:
        """Get item without marking as recently used."""
        return self._cache.get(key)

    def __len__(self) -> int:
        return len(self._cache)

    def __contains__(self, key: K) -> bool:
        return key in self._cache

    def __iter__(self) -> Iterator[K]:
        """Iterate keys in LRU order (oldest first)."""
        return iter(self._cache)

    def keys(self) -> Iterator[K]:
        """Return keys in LRU order (oldest first)."""
        return iter(self._cache)

    def values(self) -> Iterator[V]:
        """Return values in LRU order (oldest first)."""
        return iter(self._cache.values())

    def items(self) -> Iterator[tuple[K, V]]:
        """Return (key, value) pairs in LRU order (oldest first)."""
        return iter(self._cache.items())

    @property
    def capacity(self) -> int:
        return self._capacity


# =============================================================================
# Plugin Protocol
# =============================================================================


@runtime_checkable
class MemoryPlugin(Protocol):
    """Protocol for memory sources.

    Plugins provide context for different layers:
    - automatic: Always included in system prompt
    - triggered: Injected when pattern matches current state
    - on_demand: Called explicitly via memory.recall()
    """

    @property
    def name(self) -> str:
        """Unique plugin identifier."""
        ...

    @property
    def layer(self) -> Literal["automatic", "triggered", "on_demand"]:
        """Which layer this plugin serves."""
        ...

    async def get_context(self, state: StateSnapshot) -> str | None:
        """Return context if relevant, None otherwise.

        For automatic: always returns context (or empty string)
        For triggered: returns context only if pattern matches
        For on_demand: called with query in state metadata
        """
        ...

    def configure(self, config: dict[str, Any]) -> None:
        """Apply configuration from .moss/config.toml."""
        ...


class Outcome(Enum):
    """Outcome of an action."""

    SUCCESS = auto()
    FAILURE = auto()
    PARTIAL = auto()  # Some goals achieved
    TIMEOUT = auto()
    CANCELLED = auto()


@dataclass(frozen=True)
class StateSnapshot:
    """Snapshot of system state at a point in time."""

    timestamp: datetime
    files: frozenset[str]  # Paths of relevant files
    context_hash: str  # Hash of context content
    error_count: int = 0
    metadata: tuple[tuple[str, Any], ...] = ()  # Immutable metadata

    @classmethod
    def create(
        cls,
        files: list[str] | set[str],
        context: str,
        error_count: int = 0,
        metadata: dict[str, Any] | None = None,
    ) -> StateSnapshot:
        """Create a state snapshot."""
        context_hash = hashlib.sha256(context.encode()).hexdigest()[:16]
        return cls(
            timestamp=datetime.now(UTC),
            files=frozenset(files),
            context_hash=context_hash,
            error_count=error_count,
            metadata=tuple((metadata or {}).items()),
        )


@dataclass(frozen=True)
class Action:
    """An action taken by an agent."""

    tool: str
    target: str | None = None
    parameters: tuple[tuple[str, Any], ...] = ()  # Immutable params
    description: str | None = None

    @classmethod
    def create(
        cls,
        tool: str,
        target: str | None = None,
        description: str | None = None,
        **parameters: Any,
    ) -> Action:
        """Create an action record."""
        return cls(
            tool=tool,
            target=target,
            parameters=tuple(parameters.items()),
            description=description,
        )


@dataclass(frozen=True)
class Episode:
    """A single (State, Action, Outcome) record."""

    id: str
    state: StateSnapshot
    action: Action
    outcome: Outcome
    result_state: StateSnapshot | None = None
    error_message: str | None = None
    duration_ms: int = 0
    tags: frozenset[str] = frozenset()

    @classmethod
    def create(
        cls,
        state: StateSnapshot,
        action: Action,
        outcome: Outcome,
        result_state: StateSnapshot | None = None,
        error_message: str | None = None,
        duration_ms: int = 0,
        tags: set[str] | None = None,
    ) -> Episode:
        """Create an episode with auto-generated ID."""
        id_content = f"{state.timestamp.isoformat()}-{action.tool}-{action.target}"
        episode_id = hashlib.sha256(id_content.encode()).hexdigest()[:12]
        return cls(
            id=episode_id,
            state=state,
            action=action,
            outcome=outcome,
            result_state=result_state,
            error_message=error_message,
            duration_ms=duration_ms,
            tags=frozenset(tags or set()),
        )


@dataclass
class SemanticRule:
    """A learned rule derived from episode patterns."""

    id: str
    pattern: str  # Human-readable pattern description
    action: str  # Recommended action or warning
    confidence: float  # 0.0 to 1.0
    supporting_episodes: list[str]  # Episode IDs that support this rule
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    last_matched: datetime | None = None
    match_count: int = 0

    def matches(self, context: str) -> bool:
        """Check if rule pattern matches the given context.

        Simple keyword matching - can be extended for more sophisticated matching.
        """
        pattern_lower = self.pattern.lower()
        context_lower = context.lower()
        # Check if all words in pattern appear in context
        pattern_words = pattern_lower.split()
        return all(word in context_lower for word in pattern_words)

    def record_match(self) -> None:
        """Record that this rule was matched."""
        self.last_matched = datetime.now(UTC)
        self.match_count += 1


class VectorIndex(ABC):
    """Abstract base for vector similarity search."""

    @abstractmethod
    async def index(self, id: str, text: str, metadata: dict[str, Any]) -> None:
        """Add an item to the index."""
        ...

    @abstractmethod
    async def search(
        self, query: str, limit: int = 10, filter: dict[str, Any] | None = None
    ) -> list[tuple[str, float]]:
        """Search for similar items. Returns (id, score) pairs."""
        ...

    @abstractmethod
    async def delete(self, id: str) -> bool:
        """Delete an item from the index."""
        ...


class SimpleVectorIndex(VectorIndex):
    """Simple in-memory vector index using keyword matching.

    For production use, replace with a real vector database like
    Chroma, Pinecone, or Weaviate.

    Uses LRU caching when max_items is set to evict least recently accessed items.
    """

    def __init__(self, max_items: int | None = None):
        """Initialize the index.

        Args:
            max_items: Maximum items to store. If None, no limit (uses regular dict).
                      If set, uses LRU eviction when capacity is reached.
        """
        self._max_items = max_items
        if max_items is not None:
            self._lru: LRUCache[str, tuple[str, dict[str, Any]]] = LRUCache(max_items)
            self._items: dict[str, tuple[str, dict[str, Any]]] | None = None
        else:
            self._items = {}
            self._lru = None  # type: ignore[assignment]

    async def index(self, id: str, text: str, metadata: dict[str, Any]) -> None:
        value = (text.lower(), metadata)
        if self._lru is not None:
            self._lru.put(id, value)
        else:
            self._items[id] = value  # type: ignore[index]

    async def search(
        self, query: str, limit: int = 10, filter: dict[str, Any] | None = None
    ) -> list[tuple[str, float]]:
        query_words = set(query.lower().split())
        scores: list[tuple[str, float]] = []

        items_iter = self._lru.items() if self._lru is not None else self._items.items()  # type: ignore[union-attr]

        for id, (text, metadata) in items_iter:
            # Apply filter
            if filter:
                if not all(metadata.get(k) == v for k, v in filter.items()):
                    continue

            # Simple word overlap scoring
            text_words = set(text.split())
            overlap = len(query_words & text_words)
            if overlap > 0:
                score = overlap / max(len(query_words), len(text_words))
                scores.append((id, score))

        # Sort by score descending
        scores.sort(key=lambda x: -x[1])
        return scores[:limit]

    async def delete(self, id: str) -> bool:
        if self._lru is not None:
            return self._lru.delete(id)
        return self._items.pop(id, None) is not None  # type: ignore[union-attr]


class EpisodicStore:
    """Store and retrieve episodes (State, Action, Outcome records).

    Uses LRU caching to evict least recently accessed episodes when at capacity.
    """

    def __init__(
        self,
        vector_index: VectorIndex | None = None,
        max_episodes: int = 10000,
    ):
        self._episodes: LRUCache[str, Episode] = LRUCache(max_episodes)
        self._by_outcome: dict[Outcome, list[str]] = {o: [] for o in Outcome}
        self._by_tool: dict[str, list[str]] = {}
        self._by_tag: dict[str, list[str]] = {}
        self._index = vector_index or SimpleVectorIndex()

    async def store(self, episode: Episode) -> str:
        """Store an episode and index it for retrieval.

        Uses LRU eviction - least recently accessed episode is removed when at capacity.
        """
        # Store in LRU cache - returns evicted key if any
        evicted_id = self._episodes.put(episode.id, episode)
        if evicted_id is not None:
            await self._cleanup_evicted(evicted_id)

        # Index by outcome
        self._by_outcome[episode.outcome].append(episode.id)

        # Index by tool
        if episode.action.tool not in self._by_tool:
            self._by_tool[episode.action.tool] = []
        self._by_tool[episode.action.tool].append(episode.id)

        # Index by tags
        for tag in episode.tags:
            if tag not in self._by_tag:
                self._by_tag[tag] = []
            self._by_tag[tag].append(episode.id)

        # Vector index for similarity search
        text = self._episode_to_text(episode)
        metadata = {
            "outcome": episode.outcome.name,
            "tool": episode.action.tool,
            "error_count": episode.state.error_count,
        }
        await self._index.index(episode.id, text, metadata)

        return episode.id

    async def get(self, id: str) -> Episode | None:
        """Get an episode by ID. Marks as recently used for LRU."""
        return self._episodes.get(id)

    async def _cleanup_evicted(self, evicted_id: str) -> None:
        """Clean up indices for an evicted episode.

        Called when LRU cache evicts an episode. We need to retrieve the episode
        from peek (since it was just evicted, we stored it before calling this).
        """
        # The episode was evicted, so we need to clean indices
        # We can't get it from cache anymore, so we look it up in indices
        for _outcome, ids in self._by_outcome.items():
            if evicted_id in ids:
                ids.remove(evicted_id)
                break

        for _tool, ids in self._by_tool.items():
            if evicted_id in ids:
                ids.remove(evicted_id)
                break

        for _tag, ids in list(self._by_tag.items()):
            if evicted_id in ids:
                ids.remove(evicted_id)

        await self._index.delete(evicted_id)

    async def delete(self, id: str) -> bool:
        """Delete an episode."""
        episode = self._episodes.peek(id)
        if episode is None:
            return False

        self._episodes.delete(id)

        # Remove from indices
        if id in self._by_outcome[episode.outcome]:
            self._by_outcome[episode.outcome].remove(id)
        if episode.action.tool in self._by_tool:
            if id in self._by_tool[episode.action.tool]:
                self._by_tool[episode.action.tool].remove(id)
        for tag in episode.tags:
            if tag in self._by_tag and id in self._by_tag[tag]:
                self._by_tag[tag].remove(id)

        await self._index.delete(id)
        return True

    async def find_similar(
        self,
        state: StateSnapshot,
        action: Action,
        limit: int = 5,
        outcome_filter: Outcome | None = None,
    ) -> list[Episode]:
        """Find episodes similar to the given state and action."""
        # Build query
        query_parts = [action.tool]
        if action.target:
            query_parts.append(action.target)
        query_parts.extend(state.files)
        query = " ".join(query_parts)

        # Build filter
        filter_dict: dict[str, Any] | None = None
        if outcome_filter:
            filter_dict = {"outcome": outcome_filter.name}

        # Search
        results = await self._index.search(query, limit=limit, filter=filter_dict)

        # Fetch episodes
        episodes = []
        for id, _score in results:
            episode = await self.get(id)
            if episode:
                episodes.append(episode)

        return episodes

    async def find_failures(self, tool: str | None = None, limit: int = 10) -> list[Episode]:
        """Find failure episodes, optionally filtered by tool."""
        failure_ids = self._by_outcome[Outcome.FAILURE]

        if tool:
            tool_ids = set(self._by_tool.get(tool, []))
            failure_ids = [id for id in failure_ids if id in tool_ids]

        episodes = []
        for id in failure_ids[-limit:]:  # Most recent
            episode = await self.get(id)
            if episode:
                episodes.append(episode)

        return episodes

    async def find_by_tag(self, tag: str, limit: int = 10) -> list[Episode]:
        """Find episodes with a specific tag."""
        tag_ids = self._by_tag.get(tag, [])
        episodes = []
        for id in tag_ids[-limit:]:
            episode = await self.get(id)
            if episode:
                episodes.append(episode)
        return episodes

    def _episode_to_text(self, episode: Episode) -> str:
        """Convert episode to searchable text."""
        parts = [
            episode.action.tool,
            episode.action.target or "",
            episode.action.description or "",
            " ".join(episode.state.files),
            episode.error_message or "",
            " ".join(episode.tags),
        ]
        return " ".join(parts)

    @property
    def count(self) -> int:
        """Get the number of stored episodes."""
        return len(self._episodes)

    def stats(self) -> dict[str, Any]:
        """Get statistics about stored episodes."""
        return {
            "total": self.count,
            "by_outcome": {o.name: len(ids) for o, ids in self._by_outcome.items()},
            "by_tool": {t: len(ids) for t, ids in self._by_tool.items()},
        }


class SemanticStore:
    """Store and match semantic rules."""

    def __init__(self):
        self._rules: dict[str, SemanticRule] = {}
        self._by_pattern_word: dict[str, list[str]] = {}

    def add_rule(self, rule: SemanticRule) -> str:
        """Add a semantic rule."""
        self._rules[rule.id] = rule

        # Index by pattern words for fast lookup
        for word in rule.pattern.lower().split():
            if word not in self._by_pattern_word:
                self._by_pattern_word[word] = []
            self._by_pattern_word[word].append(rule.id)

        return rule.id

    def get_rule(self, id: str) -> SemanticRule | None:
        """Get a rule by ID."""
        return self._rules.get(id)

    def remove_rule(self, id: str) -> bool:
        """Remove a rule."""
        rule = self._rules.pop(id, None)
        if rule is None:
            return False

        for word in rule.pattern.lower().split():
            if word in self._by_pattern_word:
                if id in self._by_pattern_word[word]:
                    self._by_pattern_word[word].remove(id)

        return True

    def find_matching_rules(self, context: str, min_confidence: float = 0.5) -> list[SemanticRule]:
        """Find rules that match the given context."""
        context_words = set(context.lower().split())

        # Find candidate rules (those with overlapping pattern words)
        candidate_ids: set[str] = set()
        for word in context_words:
            if word in self._by_pattern_word:
                candidate_ids.update(self._by_pattern_word[word])

        # Check each candidate
        matching = []
        for id in candidate_ids:
            rule = self._rules.get(id)
            if rule and rule.confidence >= min_confidence and rule.matches(context):
                matching.append(rule)

        # Sort by confidence
        matching.sort(key=lambda r: -r.confidence)
        return matching

    @property
    def rules(self) -> list[SemanticRule]:
        """Get all rules."""
        return list(self._rules.values())


class PatternMatcher:
    """Analyze episodes to extract semantic rules."""

    def __init__(
        self,
        episodic_store: EpisodicStore,
        semantic_store: SemanticStore,
        min_occurrences: int = 3,
        min_confidence: float = 0.7,
    ):
        self.episodic_store = episodic_store
        self.semantic_store = semantic_store
        self.min_occurrences = min_occurrences
        self.min_confidence = min_confidence

    async def analyze_failures(self) -> list[SemanticRule]:
        """Analyze failure episodes to extract patterns.

        Returns newly created rules.
        """
        failures = await self.episodic_store.find_failures(limit=100)
        if len(failures) < self.min_occurrences:
            return []

        # Group failures by tool and target pattern
        patterns: dict[str, list[Episode]] = {}
        for ep in failures:
            # Create pattern key
            target = ep.action.target or "unknown"
            # Extract file extension or directory pattern
            if "." in target:
                pattern = f"{ep.action.tool}:*.{target.split('.')[-1]}"
            else:
                pattern = f"{ep.action.tool}:{target}"

            if pattern not in patterns:
                patterns[pattern] = []
            patterns[pattern].append(ep)

        # Create rules for patterns with enough occurrences
        new_rules: list[SemanticRule] = []
        for pattern, episodes in patterns.items():
            if len(episodes) < self.min_occurrences:
                continue

            # Check if rule already exists
            existing = self.semantic_store.find_matching_rules(pattern)
            if existing:
                continue

            # Calculate confidence based on consistency of error messages
            from collections import Counter  # Import here to avoid optional dependency issues

            error_messages = [ep.error_message for ep in episodes if ep.error_message]
            if error_messages:
                # Simple confidence: ratio of most common error
                counts = Counter(error_messages)
                most_common_count = counts.most_common(1)[0][1]
                confidence = most_common_count / len(episodes)
            else:
                confidence = 0.5

            if confidence < self.min_confidence:
                continue

            # Create rule
            rule_id = hashlib.sha256(pattern.encode()).hexdigest()[:12]
            action_desc = f"Warning: {pattern} has failed {len(episodes)} times"
            if error_messages:
                common_error = Counter(error_messages).most_common(1)[0][0]
                action_desc += f". Common error: {common_error[:100]}"

            rule = SemanticRule(
                id=rule_id,
                pattern=pattern,
                action=action_desc,
                confidence=confidence,
                supporting_episodes=[ep.id for ep in episodes],
            )
            self.semantic_store.add_rule(rule)
            new_rules.append(rule)

        return new_rules

    async def analyze_tool_sequences(self) -> list[SemanticRule]:
        """Analyze successful tool sequences to find patterns.

        Returns newly created rules.
        """
        # This is a simplified implementation
        # A full implementation would analyze sequences of tool calls
        # that lead to success vs failure
        return []


@dataclass
class MemoryContext:
    """Context injected from memory into prompts."""

    relevant_episodes: list[Episode] = field(default_factory=list)
    matching_rules: list[SemanticRule] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    def to_text(self) -> str:
        """Convert memory context to text for prompt injection."""
        parts = []

        if self.warnings:
            parts.append("## Warnings from past experience")
            for w in self.warnings:
                parts.append(f"- {w}")
            parts.append("")

        if self.matching_rules:
            parts.append("## Relevant learned rules")
            for rule in self.matching_rules:
                parts.append(f"- [{rule.confidence:.0%}] {rule.action}")
            parts.append("")

        if self.relevant_episodes:
            parts.append("## Similar past episodes")
            for ep in self.relevant_episodes[:3]:  # Limit to 3
                outcome_str = "succeeded" if ep.outcome == Outcome.SUCCESS else "failed"
                parts.append(f"- {ep.action.tool} on {ep.action.target}: {outcome_str}")
                if ep.error_message:
                    parts.append(f"  Error: {ep.error_message[:100]}")
            parts.append("")

        return "\n".join(parts)


class MemoryManager:
    """Unified interface for memory operations."""

    def __init__(
        self,
        episodic_store: EpisodicStore | None = None,
        semantic_store: SemanticStore | None = None,
    ):
        self.episodic = episodic_store or EpisodicStore()
        self.semantic = semantic_store or SemanticStore()
        self._pattern_matcher = PatternMatcher(self.episodic, self.semantic)

    async def record_episode(
        self,
        state: StateSnapshot,
        action: Action,
        outcome: Outcome,
        result_state: StateSnapshot | None = None,
        error_message: str | None = None,
        duration_ms: int = 0,
        tags: set[str] | None = None,
    ) -> Episode:
        """Record an episode."""
        episode = Episode.create(
            state=state,
            action=action,
            outcome=outcome,
            result_state=result_state,
            error_message=error_message,
            duration_ms=duration_ms,
            tags=tags,
        )
        await self.episodic.store(episode)
        return episode

    async def get_context(
        self,
        state: StateSnapshot,
        action: Action,
    ) -> MemoryContext:
        """Get relevant memory context for a given state and action."""
        context = MemoryContext()

        # Find similar episodes
        similar = await self.episodic.find_similar(state, action, limit=5)
        context.relevant_episodes = similar

        # Check for failure patterns
        failures = [ep for ep in similar if ep.outcome == Outcome.FAILURE]
        if len(failures) >= 2:
            context.warnings.append(f"Similar actions have failed {len(failures)} times recently")

        # Find matching semantic rules
        search_text = f"{action.tool} {action.target or ''} {' '.join(state.files)}"
        rules = self.semantic.find_matching_rules(search_text)
        context.matching_rules = rules

        # Add rule warnings
        for rule in rules:
            rule.record_match()

        return context

    async def run_pattern_analysis(self) -> list[SemanticRule]:
        """Run pattern analysis to extract new rules."""
        return await self._pattern_matcher.analyze_failures()

    def add_rule(self, pattern: str, action: str, confidence: float = 0.8) -> str:
        """Manually add a semantic rule."""
        rule_id = hashlib.sha256(pattern.encode()).hexdigest()[:12]
        rule = SemanticRule(
            id=rule_id,
            pattern=pattern,
            action=action,
            confidence=confidence,
            supporting_episodes=[],
        )
        return self.semantic.add_rule(rule)

    async def recall(self, query: str, limit: int = 5) -> str:
        """Query memory for relevant past experiences.

        On-demand memory layer - agent explicitly asks for memories.

        Args:
            query: Natural language query (e.g., "What happened last time I modified auth.py?")
            limit: Maximum number of results to return

        Returns:
            Formatted string with relevant memories, or message if none found.
        """
        parts: list[str] = []

        # Search episodic memory directly using the query
        results = await self.episodic._index.search(query, limit=limit)
        episodes = []
        for ep_id, _score in results:
            ep = await self.episodic.get(ep_id)
            if ep:
                episodes.append(ep)

        if episodes:
            parts.append("Past episodes:")
            for ep in episodes:
                outcome_str = "succeeded" if ep.outcome == Outcome.SUCCESS else "failed"
                desc = ep.action.description or ep.action.tool
                line = f"- {desc}: {outcome_str}"
                if ep.error_message:
                    line += f" ({ep.error_message})"
                parts.append(line)

        # Search semantic rules
        rules = self.semantic.find_matching_rules(query)
        if rules:
            parts.append("")
            parts.append("Learned patterns:")
            for rule in rules[:limit]:
                parts.append(f"- {rule.action} (confidence: {rule.confidence:.0%})")

        if not parts:
            return "No relevant memories found."

        return "\n".join(parts)


def create_memory_manager() -> MemoryManager:
    """Create a memory manager with default stores."""
    return MemoryManager()


# =============================================================================
# Plugin Loading
# =============================================================================


def _load_plugin_module(path: Path) -> list[MemoryPlugin]:
    """Load memory plugins from a Python file.

    Scans the module for classes implementing MemoryPlugin protocol.

    Args:
        path: Path to Python file

    Returns:
        List of plugin instances found in the module
    """
    plugins: list[MemoryPlugin] = []

    try:
        # Create unique module name
        module_name = f"moss_memory_plugin_{path.stem}_{hash(str(path))}"

        spec = importlib.util.spec_from_file_location(module_name, path)
        if spec is None or spec.loader is None:
            logger.warning("Failed to create module spec for %s", path)
            return plugins

        module = importlib.util.module_from_spec(spec)
        sys.modules[module_name] = module

        try:
            spec.loader.exec_module(module)
        except Exception as e:
            logger.warning("Failed to load plugin module %s: %s", path, e)
            del sys.modules[module_name]
            return plugins

        # Find plugin classes
        for attr_name in dir(module):
            if attr_name.startswith("_"):
                continue
            attr = getattr(module, attr_name)
            if isinstance(attr, type) and attr is not MemoryPlugin:
                # Check if it implements the protocol
                if (
                    hasattr(attr, "name")
                    and hasattr(attr, "layer")
                    and hasattr(attr, "get_context")
                    and hasattr(attr, "configure")
                ):
                    try:
                        instance = attr()
                        if isinstance(instance, MemoryPlugin):
                            plugins.append(instance)
                            logger.debug("Loaded memory plugin: %s", instance.name)
                    except Exception as e:
                        logger.warning("Failed to instantiate plugin %s: %s", attr_name, e)

    except Exception as e:
        logger.warning("Error loading plugin from %s: %s", path, e)

    return plugins


def discover_plugins(project_dir: Path | None = None) -> list[MemoryPlugin]:
    """Discover memory plugins from filesystem.

    Scans:
    - <project>/.moss/memory/*.py
    - ~/.config/moss/memory/*.py

    Args:
        project_dir: Project directory (defaults to cwd)

    Returns:
        List of discovered plugin instances
    """
    plugins: list[MemoryPlugin] = []
    search_dirs: list[Path] = []

    # User-level plugins
    user_dir = Path.home() / ".config" / "moss" / "memory"
    if user_dir.exists():
        search_dirs.append(user_dir)

    # Project-level plugins (higher priority, loaded second)
    if project_dir is None:
        project_dir = Path.cwd()
    project_plugin_dir = project_dir / ".moss" / "memory"
    if project_plugin_dir.exists():
        search_dirs.append(project_plugin_dir)

    for search_dir in search_dirs:
        for py_file in sorted(search_dir.glob("*.py")):
            if py_file.name.startswith("_"):
                continue
            loaded = _load_plugin_module(py_file)
            plugins.extend(loaded)

    return plugins


class MemoryLayer:
    """Unified memory layer that aggregates plugins.

    Provides:
    - get_automatic(): Context from automatic-layer plugins
    - check_triggers(state): Warnings from triggered-layer plugins
    - recall(query): Results from on-demand-layer plugins
    """

    def __init__(
        self,
        plugins: Sequence[MemoryPlugin] | None = None,
        memory_manager: MemoryManager | None = None,
    ):
        """Initialize the memory layer.

        Args:
            plugins: Pre-loaded plugins (if None, discovery is run)
            memory_manager: MemoryManager for built-in episodic/semantic stores
        """
        self._plugins: list[MemoryPlugin] = list(plugins) if plugins else []
        self._by_layer: dict[str, list[MemoryPlugin]] = {
            "automatic": [],
            "triggered": [],
            "on_demand": [],
        }
        self._manager = memory_manager or MemoryManager()
        self._config: dict[str, dict[str, Any]] = {}

        for plugin in self._plugins:
            self._by_layer[plugin.layer].append(plugin)

    @classmethod
    def default(cls, project_dir: Path | None = None) -> MemoryLayer:
        """Create a MemoryLayer with discovered plugins and defaults."""
        plugins = discover_plugins(project_dir)
        return cls(plugins=plugins)

    def add_plugin(self, plugin: MemoryPlugin) -> None:
        """Add a plugin to the layer."""
        self._plugins.append(plugin)
        self._by_layer[plugin.layer].append(plugin)
        if plugin.name in self._config:
            plugin.configure(self._config[plugin.name])

    def configure(self, config: dict[str, dict[str, Any]]) -> None:
        """Apply configuration to all plugins.

        Config format:
        {
            "plugin_name": {"key": "value", ...},
            ...
        }
        """
        self._config = config
        for plugin in self._plugins:
            if plugin.name in config:
                plugin.configure(config[plugin.name])

    async def get_automatic(self, state: StateSnapshot | None = None) -> str:
        """Get context from automatic-layer plugins.

        Returns combined context from all automatic plugins.
        """
        if state is None:
            state = StateSnapshot.create(files=[], context="")

        parts: list[str] = []
        for plugin in self._by_layer["automatic"]:
            try:
                ctx = await plugin.get_context(state)
                if ctx:
                    parts.append(ctx)
            except Exception as e:
                logger.warning("Plugin %s failed in get_automatic: %s", plugin.name, e)

        return "\n\n".join(parts)

    async def check_triggers(self, state: StateSnapshot) -> list[str]:
        """Check triggered-layer plugins for warnings.

        Returns list of warning strings from plugins whose patterns match.
        """
        warnings: list[str] = []
        for plugin in self._by_layer["triggered"]:
            try:
                ctx = await plugin.get_context(state)
                if ctx:
                    warnings.append(ctx)
            except Exception as e:
                logger.warning("Plugin %s failed in check_triggers: %s", plugin.name, e)

        return warnings

    async def recall(self, query: str, limit: int = 5) -> str:
        """Query on-demand plugins plus built-in memory.

        Args:
            query: Natural language query
            limit: Maximum results per source

        Returns:
            Combined results from all sources
        """
        parts: list[str] = []

        # Query built-in memory manager first
        manager_result = await self._manager.recall(query, limit=limit)
        if manager_result and manager_result != "No relevant memories found.":
            parts.append(manager_result)

        # Query on-demand plugins
        state = StateSnapshot.create(files=[], context=query, metadata={"query": query})
        for plugin in self._by_layer["on_demand"]:
            try:
                ctx = await plugin.get_context(state)
                if ctx:
                    parts.append(ctx)
            except Exception as e:
                logger.warning("Plugin %s failed in recall: %s", plugin.name, e)

        if not parts:
            return "No relevant memories found."

        return "\n\n".join(parts)

    @property
    def manager(self) -> MemoryManager:
        """Get the underlying MemoryManager for direct access."""
        return self._manager

    @property
    def plugins(self) -> list[MemoryPlugin]:
        """Get all registered plugins."""
        return list(self._plugins)


__all__ = [
    "Action",
    "Episode",
    "EpisodicStore",
    "LRUCache",
    "MemoryContext",
    "MemoryLayer",
    "MemoryManager",
    "MemoryPlugin",
    "Outcome",
    "PatternMatcher",
    "SemanticRule",
    "SemanticStore",
    "SimpleVectorIndex",
    "StateSnapshot",
    "VectorIndex",
    "create_memory_manager",
    "discover_plugins",
]
