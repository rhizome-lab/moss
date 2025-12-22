"""Composable execution primitives.

This module provides the building blocks for workflows and agents:
- Scope: container for execution state
- Step: single unit of work
- Strategies: pluggable context, cache, retry behaviors

Design: docs/design/execution-primitives.md
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import Generator
from contextlib import contextmanager
from dataclasses import dataclass, field
from typing import Any

# =============================================================================
# Context Strategies
# =============================================================================


class ContextStrategy(ABC):
    """Base class for context management strategies."""

    @abstractmethod
    def add(self, key: str, value: Any) -> None:
        """Add item to context."""
        ...

    @abstractmethod
    def get_context(self) -> str:
        """Get context string for LLM prompt."""
        ...

    @abstractmethod
    def child(self) -> ContextStrategy:
        """Create child context (for nesting)."""
        ...


class FlatContext(ContextStrategy):
    """Simple flat context - just last N items."""

    def __init__(self, max_items: int = 10):
        self.max_items = max_items
        self.items: list[tuple[str, Any]] = []

    def add(self, key: str, value: Any) -> None:
        self.items.append((key, value))
        if len(self.items) > self.max_items:
            self.items.pop(0)

    def get_context(self) -> str:
        return "\n".join(f"{k}: {v}" for k, v in self.items)

    def child(self) -> ContextStrategy:
        # Flat context doesn't nest, just create new
        return FlatContext(self.max_items)


class TaskListContext(ContextStrategy):
    """Flat task list - pending and completed items."""

    def __init__(self):
        self.pending: list[str] = []
        self.completed: list[str] = []
        self.current: str | None = None

    def add(self, key: str, value: Any) -> None:
        if key == "task":
            self.pending.append(str(value))
        elif key == "done":
            if self.current:
                self.completed.append(self.current)
                self.current = None
        elif key == "start":
            if self.pending:
                self.current = self.pending.pop(0)

    def get_context(self) -> str:
        lines = []
        if self.current:
            lines.append(f"Current: {self.current}")
        if self.pending:
            lines.append(f"Pending: {', '.join(self.pending)}")
        if self.completed:
            lines.append(f"Done: {', '.join(self.completed[-3:])}")  # Last 3
        return "\n".join(lines)

    def child(self) -> ContextStrategy:
        # Task list creates independent child
        return TaskListContext()


# =============================================================================
# Cache Strategies
# =============================================================================


class CacheStrategy(ABC):
    """Base class for caching strategies."""

    @abstractmethod
    def store(self, content: str) -> str:
        """Store content, return ID."""
        ...

    @abstractmethod
    def retrieve(self, cache_id: str) -> str | None:
        """Retrieve content by ID."""
        ...

    @abstractmethod
    def preview(self, content: str, max_len: int = 500) -> tuple[str, str | None]:
        """Return (preview, cache_id or None if small enough)."""
        ...


class NoCache(CacheStrategy):
    """No caching - return content as-is."""

    def store(self, content: str) -> str:
        return "no-cache"

    def retrieve(self, cache_id: str) -> str | None:
        return None

    def preview(self, content: str, max_len: int = 500) -> tuple[str, str | None]:
        return content[:max_len], None


class InMemoryCache(CacheStrategy):
    """Simple in-memory cache."""

    def __init__(self):
        self._cache: dict[str, str] = {}
        self._counter = 0

    def store(self, content: str) -> str:
        self._counter += 1
        cache_id = f"cache_{self._counter}"
        self._cache[cache_id] = content
        return cache_id

    def retrieve(self, cache_id: str) -> str | None:
        return self._cache.get(cache_id)

    def preview(self, content: str, max_len: int = 500) -> tuple[str, str | None]:
        if len(content) <= max_len:
            return content, None
        cache_id = self.store(content)
        preview = content[:max_len] + f"\n... [truncated, full: {cache_id}]"
        return preview, cache_id


# =============================================================================
# Scope
# =============================================================================


@dataclass
class Scope:
    """Execution scope with pluggable strategies.

    Usage:
        with Scope(context=TaskListContext()) as scope:
            result = scope.run("view main.py")
            scope.context.add("done", "viewed main.py")
    """

    context: ContextStrategy = field(default_factory=FlatContext)
    cache: CacheStrategy = field(default_factory=NoCache)
    parent: Scope | None = None

    _current: Scope | None = field(default=None, init=False, repr=False)

    def __enter__(self) -> Scope:
        return self

    def __exit__(self, *args: Any) -> None:
        pass

    @contextmanager
    def child(
        self,
        context: ContextStrategy | None = None,
        cache: CacheStrategy | None = None,
    ) -> Generator[Scope]:
        """Create nested scope, optionally with different strategies."""
        child_scope = Scope(
            context=context or self.context.child(),
            cache=cache or self.cache,
            parent=self,
        )
        yield child_scope

    def run(self, action: str) -> str:
        """Execute an action in this scope.

        For now, just returns placeholder. Real implementation would:
        1. Parse action (DWIM)
        2. Execute tool
        3. Cache result if large
        4. Update context
        """
        # Placeholder - real impl would call tools
        result = f"[executed: {action}]"

        # Cache if needed
        preview, _cache_id = self.cache.preview(result)

        # Update context
        self.context.add("result", preview)

        return preview


# =============================================================================
# Convenience
# =============================================================================


def agent_loop(
    task: str,
    context: ContextStrategy | None = None,
    cache: CacheStrategy | None = None,
    max_turns: int = 10,
) -> str:
    """Simple agent loop using composable primitives.

    This is what DWIMLoop's 1151 lines boils down to.
    """
    scope = Scope(
        context=context or FlatContext(),
        cache=cache or InMemoryCache(),
    )

    scope.context.add("task", task)

    for turn in range(max_turns):
        # Get context for LLM (would pass to LLM)
        _ctx = scope.context.get_context()

        # Placeholder: would call LLM here
        # llm_output = call_llm(system_prompt, _ctx)
        llm_output = f"view turn_{turn}"  # Fake

        if "done" in llm_output.lower():
            break

        # Execute action
        scope.run(llm_output)

    return scope.context.get_context()
