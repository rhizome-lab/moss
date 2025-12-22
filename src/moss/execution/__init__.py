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


class TaskTreeContext(ContextStrategy):
    """Hierarchical task tree - path from root to current.

    Tracks nested tasks like:
        Root: "fix all issues"
        └── Current: "fix type errors"
            └── Sub: "fix error in main.py"

    Context shows the path, giving LLM awareness of where it is
    in the overall task hierarchy.
    """

    def __init__(self, parent: TaskTreeContext | None = None):
        self._parent = parent
        self.task: str | None = None
        self.notes: list[str] = []  # Observations at this level
        self.children: list[TaskTreeContext] = []

    def add(self, key: str, value: Any) -> None:
        if key == "task":
            self.task = str(value)
        elif key == "note":
            self.notes.append(str(value))
        elif key == "result":
            # Keep last result as note
            self.notes.append(f"Result: {value}")
            if len(self.notes) > 3:
                self.notes.pop(0)

    def get_context(self) -> str:
        """Return path from root to current."""
        path = self._get_path()
        if not path:
            return ""
        lines = []
        for i, node in enumerate(path):
            indent = "  " * i
            prefix = "└── " if i > 0 else ""
            if node.task:
                lines.append(f"{indent}{prefix}{node.task}")
            for note in node.notes[-2:]:  # Last 2 notes per level
                lines.append(f"{indent}    • {note}")
        return "\n".join(lines)

    def _get_path(self) -> list[TaskTreeContext]:
        """Get path from root to self."""
        path = []
        node: TaskTreeContext | None = self
        while node is not None:
            path.append(node)
            node = node._parent
        return list(reversed(path))

    def child(self) -> ContextStrategy:
        """Create child node in tree."""
        child_ctx = TaskTreeContext(parent=self)
        self.children.append(child_ctx)
        return child_ctx


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

        1. Parse action (DWIM)
        2. Execute tool via Rust CLI
        3. Cache result if large
        4. Update context
        """
        intent = parse_intent(action)
        result = execute_intent(intent)

        # Cache if needed
        preview, _cache_id = self.cache.preview(result)

        # Update context
        self.context.add("result", preview)

        return preview


# =============================================================================
# Intent Parsing (simplified from dwim_loop.py)
# =============================================================================

# Verb aliases → canonical verb
VERBS = {
    # View/explore
    "view": "view",
    "show": "view",
    # Analyze
    "analyze": "analyze",
    "check": "analyze",
    # Edit
    "edit": "edit",
    "fix": "edit",
    # Done
    "done": "done",
    "finished": "done",
}


@dataclass
class Intent:
    """Parsed intent from LLM output."""

    verb: str  # Canonical verb (view, edit, analyze, done)
    target: str | None  # Path or symbol
    args: str | None  # Additional arguments
    raw: str  # Original text


def parse_intent(text: str) -> Intent:
    """Parse 'view foo.py' → Intent(verb='view', target='foo.py').

    ~20 lines vs dwim_loop.py's 50. Same functionality.
    """
    text = text.strip()
    if not text:
        return Intent(verb="", target=None, args=None, raw=text)

    parts = text.split(None, 2)
    first = parts[0].lower()

    verb = VERBS.get(first, "unknown")
    target = parts[1] if len(parts) > 1 else None
    args = parts[2] if len(parts) > 2 else None

    return Intent(verb=verb, target=target, args=args, raw=text)


def execute_intent(intent: Intent) -> str:
    """Execute an intent via Rust CLI.

    Returns result string (actual output, not just status).
    """
    if intent.verb == "done":
        return "done"

    if intent.verb == "unknown":
        return f"Unknown command: {intent.raw}"

    # Build CLI args: [subcommand, target, ...args]
    cli_args = [intent.verb]
    if intent.target:
        cli_args.append(intent.target)
    if intent.args:
        cli_args.extend(intent.args.split())

    # Call Rust CLI and capture output
    try:
        from moss.rust_shim import call_rust

        exit_code, output = call_rust(cli_args)
        if exit_code == 0:
            return output.strip() if output else "[OK]"
        else:
            return f"[Error] {output.strip()}" if output else f"[Error exit {exit_code}]"
    except Exception as e:
        return f"Error: {e}"


# =============================================================================
# Retry Strategies
# =============================================================================


class RetryStrategy(ABC):
    """Base class for retry strategies."""

    @abstractmethod
    def should_retry(self, attempt: int, error: str) -> bool:
        """Return True if should retry after this error."""
        ...

    @abstractmethod
    def get_delay(self, attempt: int) -> float:
        """Return delay in seconds before next retry."""
        ...


class NoRetry(RetryStrategy):
    """No retries - fail immediately."""

    def should_retry(self, attempt: int, error: str) -> bool:
        return False

    def get_delay(self, attempt: int) -> float:
        return 0.0


class FixedRetry(RetryStrategy):
    """Fixed delay retries."""

    def __init__(self, max_attempts: int = 3, delay: float = 1.0):
        self.max_attempts = max_attempts
        self.delay = delay

    def should_retry(self, attempt: int, error: str) -> bool:
        return attempt < self.max_attempts

    def get_delay(self, attempt: int) -> float:
        return self.delay


class ExponentialRetry(RetryStrategy):
    """Exponential backoff retries."""

    def __init__(
        self,
        max_attempts: int = 5,
        base_delay: float = 1.0,
        max_delay: float = 60.0,
    ):
        self.max_attempts = max_attempts
        self.base_delay = base_delay
        self.max_delay = max_delay

    def should_retry(self, attempt: int, error: str) -> bool:
        return attempt < self.max_attempts

    def get_delay(self, attempt: int) -> float:
        delay = self.base_delay * (2**attempt)
        return min(delay, self.max_delay)


# =============================================================================
# LLM Strategy
# =============================================================================

AGENT_SYSTEM_PROMPT = """You are an agent that completes tasks using three commands:
- view <path> - View file skeleton or symbol source
- edit <path> "task" - Make changes to a file
- analyze <path> - Analyze code health, complexity, etc.
- done - Signal task completion

Given the task and context, respond with exactly ONE command. No explanation.
When the task is complete, respond with "done".
"""


class LLMStrategy(ABC):
    """Base class for LLM strategies."""

    @abstractmethod
    def decide(self, task: str, context: str) -> str:
        """Given task and context, return next action."""
        ...


class NoLLM(LLMStrategy):
    """No LLM - returns fixed sequence for testing."""

    def __init__(self, actions: list[str] | None = None):
        self.actions = actions or ["done"]
        self._index = 0

    def decide(self, task: str, context: str) -> str:
        if self._index >= len(self.actions):
            return "done"
        action = self.actions[self._index]
        self._index += 1
        return action


class SimpleLLM(LLMStrategy):
    """LLM strategy using moss.llm.complete."""

    def __init__(
        self,
        provider: str | None = None,
        model: str | None = None,
        system_prompt: str = AGENT_SYSTEM_PROMPT,
    ):
        self.provider = provider
        self.model = model
        self.system_prompt = system_prompt

    def decide(self, task: str, context: str) -> str:
        from moss.llm import complete

        prompt = f"Task: {task}\n\nContext:\n{context}\n\nNext action:"
        return complete(
            prompt,
            system=self.system_prompt,
            provider=self.provider,
            model=self.model,
        ).strip()


# =============================================================================
# Agent Loop
# =============================================================================


def agent_loop(
    task: str,
    context: ContextStrategy | None = None,
    cache: CacheStrategy | None = None,
    llm: LLMStrategy | None = None,
    max_turns: int = 10,
) -> str:
    """Simple agent loop using composable primitives.

    This is what DWIMLoop's 1151 lines boils down to.

    Args:
        task: The task to complete
        context: Context strategy (default: FlatContext)
        cache: Cache strategy (default: InMemoryCache)
        llm: LLM strategy (default: NoLLM for safety)
        max_turns: Maximum iterations before stopping
    """
    scope = Scope(
        context=context or FlatContext(),
        cache=cache or InMemoryCache(),
    )
    decider = llm or NoLLM()

    scope.context.add("task", task)

    for _turn in range(max_turns):
        ctx = scope.context.get_context()

        # Ask LLM for next action
        action = decider.decide(task, ctx)

        if "done" in action.lower():
            break

        # Execute action
        scope.run(action)

    return scope.context.get_context()
