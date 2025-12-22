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


class InheritedContext(ContextStrategy):
    """Wrapper that provides read access to parent, writes to own storage.

    Used for 'inherited' context mode:
    - Child sees parent's context (read)
    - Child writes to own storage (parent unchanged)
    - One-way visibility: parent never sees child's additions
    """

    def __init__(self, parent: ContextStrategy):
        self._parent = parent
        self._own_items: list[tuple[str, Any]] = []

    def add(self, key: str, value: Any) -> None:
        """Write to own storage only."""
        self._own_items.append((key, value))
        # Keep last 10 items
        if len(self._own_items) > 10:
            self._own_items.pop(0)

    def get_context(self) -> str:
        """Return parent context + own additions."""
        parent_ctx = self._parent.get_context()
        own_ctx = "\n".join(f"{k}: {v}" for k, v in self._own_items)
        if parent_ctx and own_ctx:
            return f"{parent_ctx}\n---\n{own_ctx}"
        return parent_ctx or own_ctx

    def child(self) -> ContextStrategy:
        """Create isolated child (inherited doesn't cascade)."""
        return FlatContext()


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
    retry: RetryStrategy = field(default_factory=NoRetry)
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
        retry: RetryStrategy | None = None,
        mode: str = "isolated",
    ) -> Generator[Scope]:
        """Create nested scope, optionally with different strategies.

        Args:
            context: Override context strategy (ignores mode if set)
            cache: Override cache strategy
            retry: Override retry strategy
            mode: Context mode if context not provided:
                - "isolated": Child gets fresh context via context.child()
                - "shared": Child uses same context object as parent
                - "inherited": Child sees parent context (read), writes to own
        """
        if context is not None:
            child_context = context
        elif mode == "shared":
            child_context = self.context  # Same object
        elif mode == "inherited":
            child_context = InheritedContext(self.context)
        else:  # isolated (default)
            child_context = self.context.child()

        child_scope = Scope(
            context=child_context,
            cache=cache or self.cache,
            retry=retry or self.retry,
            parent=self,
        )
        yield child_scope

    def run(self, action: str) -> str:
        """Execute an action in this scope with retry.

        1. Parse action (DWIM)
        2. Execute tool via Rust CLI (with retry on error)
        3. Cache result if large
        4. Update context
        """
        import time

        intent = parse_intent(action)

        # Execute with retry
        attempt = 0
        while True:
            result = execute_intent(intent)

            # Check for error
            is_error = result.startswith("[Error")

            if not is_error or not self.retry.should_retry(attempt, result):
                break

            # Wait and retry
            delay = self.retry.get_delay(attempt)
            if delay > 0:
                time.sleep(delay)
            attempt += 1

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
# Decision Model
# =============================================================================


@dataclass
class Decision:
    """Structured decision from LLM with inline chain-of-thought.

    The LLM outputs natural prose with commands interspersed:
        Let me check the main file first.
        view main.py
        Interesting, the function is on line 42.
        view utils.py

    This is parsed into actions (commands) and prose (reasoning).
    """

    raw: str  # Full LLM output
    actions: list[str] = field(default_factory=list)  # Extracted commands
    prose: list[str] = field(default_factory=list)  # Extracted reasoning
    parallel: bool = False  # Execute actions concurrently?
    done: bool = False  # Task complete?


# Command patterns for parsing
COMMAND_PATTERNS = {"view", "show", "edit", "fix", "analyze", "check", "done", "finished"}


def parse_decision(text: str) -> Decision:
    """Parse LLM output into structured Decision.

    Lines starting with command verbs are actions.
    Everything else is prose (inline chain-of-thought).
    """
    lines = text.strip().split("\n")
    actions: list[str] = []
    prose: list[str] = []
    done = False

    for line in lines:
        line = line.strip()
        if not line:
            continue

        # Check if line starts with a command verb
        first_word = line.split()[0].lower() if line.split() else ""

        if first_word in COMMAND_PATTERNS:
            if first_word in ("done", "finished"):
                done = True
            else:
                actions.append(line)
        else:
            prose.append(line)

    return Decision(
        raw=text,
        actions=actions,
        prose=prose,
        parallel=False,  # Default sequential
        done=done,
    )


# =============================================================================
# LLM Strategy
# =============================================================================

AGENT_SYSTEM_PROMPT = """You are an agent that completes tasks using commands:
- view <path> - View file skeleton or symbol source
- edit <path> "task" - Make changes to a file
- analyze <path> - Analyze code health, complexity, etc.
- done - Signal task completion

You may think out loud between commands. Each command should be on its own line.
When the task is complete, say "done".

Example:
    Let me check the main file first.
    view main.py
    Now I'll look at the imports.
    view utils.py
"""


class LLMStrategy(ABC):
    """Base class for LLM strategies."""

    @abstractmethod
    def decide(self, task: str, context: str) -> Decision:
        """Given task and context, return structured decision."""
        ...


class NoLLM(LLMStrategy):
    """No LLM - returns fixed sequence for testing.

    Each item in actions can be:
    - A single command: "view main.py"
    - Multi-line with prose: "Let me check this.\\nview main.py"
    - "done" to signal completion
    """

    def __init__(self, actions: list[str] | None = None):
        self.actions = actions or ["done"]
        self._index = 0

    def decide(self, task: str, context: str) -> Decision:
        if self._index >= len(self.actions):
            return Decision(raw="done", actions=[], done=True)

        raw = self.actions[self._index]
        self._index += 1

        # Parse the raw text to extract actions and prose
        return parse_decision(raw)


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

    def decide(self, task: str, context: str) -> Decision:
        from moss.llm import complete

        prompt = f"Task: {task}\n\nContext:\n{context}\n\nWhat's next?"
        response = complete(
            prompt,
            system=self.system_prompt,
            provider=self.provider,
            model=self.model,
        )
        return parse_decision(response)


# =============================================================================
# Agent Loop
# =============================================================================


def agent_loop(
    task: str,
    context: ContextStrategy | None = None,
    cache: CacheStrategy | None = None,
    retry: RetryStrategy | None = None,
    llm: LLMStrategy | None = None,
    max_turns: int = 10,
) -> str:
    """Simple agent loop using composable primitives.

    This is what DWIMLoop's 1151 lines boils down to.

    Args:
        task: The task to complete
        context: Context strategy (default: FlatContext)
        cache: Cache strategy (default: InMemoryCache)
        retry: Retry strategy (default: NoRetry)
        llm: LLM strategy (default: NoLLM for safety)
        max_turns: Maximum iterations before stopping

    Returns:
        Final context string
    """
    scope = Scope(
        context=context or FlatContext(),
        cache=cache or InMemoryCache(),
        retry=retry or NoRetry(),
    )
    decider = llm or NoLLM()

    scope.context.add("task", task)

    for _turn in range(max_turns):
        ctx = scope.context.get_context()

        # Ask LLM for decision (may include multiple actions + prose)
        decision = decider.decide(task, ctx)

        # Store prose as reasoning/notes
        for thought in decision.prose:
            scope.context.add("note", thought)

        # Check if done
        if decision.done:
            break

        # Execute actions
        if decision.parallel and len(decision.actions) > 1:
            # Parallel execution using ThreadPoolExecutor
            from concurrent.futures import ThreadPoolExecutor, as_completed

            with ThreadPoolExecutor(max_workers=len(decision.actions)) as executor:
                futures = {
                    executor.submit(scope.run, action): action for action in decision.actions
                }
                for _future in as_completed(futures):
                    # Results already added to context by scope.run()
                    pass
        else:
            # Sequential execution
            for action in decision.actions:
                scope.run(action)

    return scope.context.get_context()


# =============================================================================
# Workflow Loader
# =============================================================================

# Strategy registries
CONTEXT_STRATEGIES: dict[str, type[ContextStrategy]] = {
    "flat": FlatContext,
    "task_list": TaskListContext,
    "task_tree": TaskTreeContext,
}

CACHE_STRATEGIES: dict[str, type[CacheStrategy]] = {
    "none": NoCache,
    "in_memory": InMemoryCache,
}

RETRY_STRATEGIES: dict[str, type[RetryStrategy]] = {
    "none": NoRetry,
    "fixed": FixedRetry,
    "exponential": ExponentialRetry,
}

LLM_STRATEGIES: dict[str, type[LLMStrategy]] = {
    "none": NoLLM,
    "simple": SimpleLLM,
}


# =============================================================================
# Condition Plugins (for state machine transitions)
# =============================================================================


class ConditionPlugin(ABC):
    """Base class for transition condition plugins."""

    @abstractmethod
    def evaluate(self, context: str, result: str, param: str | None = None) -> bool:
        """Return True if condition is met."""
        ...


class HasErrorsCondition(ConditionPlugin):
    """True if result contains 'error' (case-insensitive)."""

    def evaluate(self, context: str, result: str, param: str | None = None) -> bool:
        return "error" in result.lower()


class SuccessCondition(ConditionPlugin):
    """True if result doesn't indicate an error."""

    def evaluate(self, context: str, result: str, param: str | None = None) -> bool:
        return not result.startswith("[Error")


class EmptyCondition(ConditionPlugin):
    """True if result is empty or whitespace."""

    def evaluate(self, context: str, result: str, param: str | None = None) -> bool:
        return not result.strip()


class ContainsCondition(ConditionPlugin):
    """True if result contains the parameter string."""

    def evaluate(self, context: str, result: str, param: str | None = None) -> bool:
        if param is None:
            return False
        return param in result


CONDITION_PLUGINS: dict[str, ConditionPlugin] = {
    "has_errors": HasErrorsCondition(),
    "success": SuccessCondition(),
    "empty": EmptyCondition(),
    "contains": ContainsCondition(),
}


def evaluate_condition(condition: str, context: str, result: str) -> bool:
    """Evaluate a condition string using plugins.

    Supports parametric conditions: "contains:TypeError"
    """
    if ":" in condition:
        name, param = condition.split(":", 1)
    else:
        name, param = condition, None

    plugin = CONDITION_PLUGINS.get(name)
    if plugin is None:
        return False

    return plugin.evaluate(context, result, param)


@dataclass
class StepResult:
    """Result from a compound step execution.

    Contains child results for parent access.
    """

    success: bool
    summary: str = ""
    child_results: list[str] = field(default_factory=list)


@dataclass
class WorkflowStep:
    """A predefined step in a workflow.

    Simple step: has action (command string to execute).
    Compound step: has steps (sub-steps that execute in a child Scope).

    Context modes for compound steps:
    - isolated: Child gets fresh context via context.child() (default)
    - shared: Child uses same context object as parent
    - inherited: Child sees parent context (read), writes to own

    Feedback control:
    - summarize: If True, generate a summary of child results for parent context
    """

    name: str
    action: str | None = None  # Command to execute, None for compound steps
    steps: list[WorkflowStep] | None = None  # Sub-steps for compound steps
    on_error: str = "fail"  # "fail", "skip", "retry"
    max_retries: int = 1
    context_mode: str = "isolated"  # "isolated", "shared", "inherited"
    summarize: bool = False  # Summarize child results for parent


@dataclass
class Transition:
    """A transition between states in a state machine workflow."""

    next: str  # Target state name
    condition: str | None = None  # Condition plugin name, None = always


@dataclass
class WorkflowState:
    """A state in a state machine workflow.

    Lifecycle hooks run in this order:
    1. on_entry (when entering state)
    2. action (main state logic) OR parallel states
    3. on_exit (before transitioning out)

    Parallel execution:
    If `parallel` is set, the state forks into parallel sub-states.
    All parallel states execute concurrently, then join back.
    The `join` field specifies which state to transition to after all complete.
    """

    name: str
    action: str | None = None  # Command to execute in this state
    transitions: list[Transition] = field(default_factory=list)
    terminal: bool = False  # End state?
    on_entry: str | None = None  # Run when entering state
    on_exit: str | None = None  # Run before leaving state
    parallel: list[str] | None = None  # State names to execute in parallel
    join: str | None = None  # State to transition to after parallel completion


@dataclass
class WorkflowConfig:
    """Parsed workflow configuration."""

    name: str
    description: str = ""
    max_turns: int = 20
    context: ContextStrategy | None = None
    cache: CacheStrategy | None = None
    retry: RetryStrategy | None = None
    llm: LLMStrategy | None = None
    steps: list[WorkflowStep] | None = None  # If set, run step-based instead of agentic
    states: list[WorkflowState] | None = None  # If set, run state machine
    initial_state: str | None = None  # Starting state for state machine


def load_workflow(path: str) -> WorkflowConfig:
    """Load workflow configuration from TOML file.

    Args:
        path: Path to TOML file

    Returns:
        WorkflowConfig with instantiated strategies
    """
    import tomllib
    from pathlib import Path

    with Path(path).open("rb") as f:
        data = tomllib.load(f)

    wf = data.get("workflow", {})

    # Parse limits
    limits = wf.get("limits", {})
    max_turns = limits.get("max_turns", 20)

    # Parse context strategy
    context = None
    if ctx_cfg := wf.get("context"):
        strategy_name = ctx_cfg.get("strategy", "flat")
        if strategy_cls := CONTEXT_STRATEGIES.get(strategy_name):
            context = strategy_cls()

    # Parse cache strategy
    cache = None
    if cache_cfg := wf.get("cache"):
        strategy_name = cache_cfg.get("strategy", "none")
        if strategy_cls := CACHE_STRATEGIES.get(strategy_name):
            cache = strategy_cls()

    # Parse retry strategy
    retry = None
    if retry_cfg := wf.get("retry"):
        strategy_name = retry_cfg.get("strategy", "none")
        if strategy_name == "fixed":
            retry = FixedRetry(
                max_attempts=retry_cfg.get("max_attempts", 3),
                delay=retry_cfg.get("delay", 1.0),
            )
        elif strategy_name == "exponential":
            retry = ExponentialRetry(
                max_attempts=retry_cfg.get("max_attempts", 5),
                base_delay=retry_cfg.get("base_delay", 1.0),
                max_delay=retry_cfg.get("max_delay", 60.0),
            )
        elif strategy_cls := RETRY_STRATEGIES.get(strategy_name):
            retry = strategy_cls()

    # Parse LLM strategy
    llm = None
    if llm_cfg := wf.get("llm"):
        strategy_name = llm_cfg.get("strategy", "none")
        if strategy_name == "simple":
            llm = SimpleLLM(
                provider=llm_cfg.get("provider"),
                model=llm_cfg.get("model"),
                system_prompt=llm_cfg.get("system_prompt", AGENT_SYSTEM_PROMPT),
            )
        elif strategy_name == "none":
            actions = llm_cfg.get("actions", ["done"])
            llm = NoLLM(actions=actions)

    # Parse steps (for step-based workflows)
    def parse_steps(steps_cfg: list[dict[str, Any]]) -> list[WorkflowStep]:
        """Parse step configs recursively."""
        result = []
        for step_cfg in steps_cfg:
            sub_steps = None
            if sub_cfg := step_cfg.get("steps"):
                sub_steps = parse_steps(sub_cfg)
            result.append(
                WorkflowStep(
                    name=step_cfg.get("name", "step"),
                    action=step_cfg.get("action"),
                    steps=sub_steps,
                    on_error=step_cfg.get("on_error", "fail"),
                    max_retries=step_cfg.get("max_retries", 1),
                    context_mode=step_cfg.get("context_mode", "isolated"),
                    summarize=step_cfg.get("summarize", False),
                )
            )
        return result

    steps = None
    if steps_cfg := wf.get("steps"):
        steps = parse_steps(steps_cfg)

    # Parse states (for state machine workflows)
    def parse_states(states_cfg: list[dict[str, Any]]) -> list[WorkflowState]:
        """Parse state configs with transitions."""
        result = []
        for state_cfg in states_cfg:
            transitions = []
            if trans_cfg := state_cfg.get("transitions"):
                for t in trans_cfg:
                    transitions.append(
                        Transition(
                            next=t.get("next", ""),
                            condition=t.get("condition"),
                        )
                    )
            result.append(
                WorkflowState(
                    name=state_cfg.get("name", "state"),
                    action=state_cfg.get("action"),
                    transitions=transitions,
                    terminal=state_cfg.get("terminal", False),
                    on_entry=state_cfg.get("on_entry"),
                    on_exit=state_cfg.get("on_exit"),
                    parallel=state_cfg.get("parallel"),
                    join=state_cfg.get("join"),
                )
            )
        return result

    states = None
    initial_state = wf.get("initial_state")
    if states_cfg := data.get("states"):
        states = parse_states(states_cfg)

    return WorkflowConfig(
        name=wf.get("name", "unnamed"),
        description=wf.get("description", ""),
        max_turns=max_turns,
        context=context,
        cache=cache,
        retry=retry,
        llm=llm,
        steps=steps,
        states=states,
        initial_state=initial_state,
    )


def _summarize_children(context: ContextStrategy) -> str:
    """Generate a summary of child results from a context.

    For TaskTreeContext, summarizes child nodes.
    For other contexts, returns a brief status.
    """
    if isinstance(context, TaskTreeContext):
        if not context.children:
            return "No child tasks"
        summaries = []
        for child in context.children:
            task = child.task or "unnamed"
            notes = child.notes[-1] if child.notes else "completed"
            summaries.append(f"- {task}: {notes}")
        return "\n".join(summaries)

    # Generic fallback
    ctx = context.get_context()
    lines = ctx.strip().split("\n")
    if len(lines) <= 3:
        return ctx
    return "\n".join(lines[-3:]) + f"\n... ({len(lines)} total items)"


def _run_steps(scope: Scope, steps: list[WorkflowStep]) -> StepResult:
    """Run steps in a scope, handling both simple and compound steps.

    Args:
        scope: The scope to run steps in
        steps: List of workflow steps to execute

    Returns:
        StepResult with success status, summary, and child results
    """
    child_results: list[str] = []

    for step in steps:
        scope.context.add("step", step.name)

        try:
            if step.steps:
                # Compound step: create child scope and run sub-steps
                with scope.child(mode=step.context_mode) as child_scope:
                    child_result = _run_steps(child_scope, step.steps)

                    if not child_result.success:
                        # Sub-step failed with on_error="fail"
                        if step.on_error == "skip":
                            scope.context.add("skipped", f"{step.name}: sub-step failed")
                            continue
                        elif step.on_error != "fail":
                            continue
                        else:
                            return StepResult(
                                success=False,
                                summary=f"Failed at {step.name}",
                                child_results=child_results,
                            )

                    # Collect child results
                    child_results.extend(child_result.child_results)

                    # Generate summary if requested
                    if step.summarize:
                        summary = _summarize_children(child_scope.context)
                        scope.context.add("child_summary", f"{step.name}:\n{summary}")
                        child_results.append(f"{step.name}: {summary}")

            elif step.action:
                # Simple step: execute action
                result = scope.run(step.action)
                scope.context.add("result", result)
                child_results.append(f"{step.name}: {result[:100]}")
            else:
                # Neither action nor steps - skip
                scope.context.add("skipped", f"{step.name}: no action or steps")
                continue

        except Exception as e:
            if step.on_error == "skip":
                scope.context.add("skipped", f"{step.name}: {e}")
                continue
            elif step.on_error == "retry" and step.max_retries > 1:
                # Simple retry
                success = False
                for _attempt in range(step.max_retries - 1):
                    try:
                        if step.action:
                            result = scope.run(step.action)
                            scope.context.add("result", result)
                            child_results.append(f"{step.name}: {result[:100]}")
                            success = True
                            break
                    except Exception:
                        continue
                if not success:
                    scope.context.add("error", f"{step.name}: {e}")
                    return StepResult(
                        success=False,
                        summary=f"Failed at {step.name}: {e}",
                        child_results=child_results,
                    )
            else:
                scope.context.add("error", f"{step.name}: {e}")
                return StepResult(
                    success=False,
                    summary=f"Failed at {step.name}: {e}",
                    child_results=child_results,
                )

    return StepResult(
        success=True,
        summary=f"Completed {len(steps)} steps",
        child_results=child_results,
    )


def step_loop(
    steps: list[WorkflowStep],
    context: ContextStrategy | None = None,
    cache: CacheStrategy | None = None,
    retry: RetryStrategy | None = None,
    initial_context: dict[str, str] | None = None,
) -> str:
    """Run predefined steps in sequence.

    Supports nested steps: compound steps (with sub-steps) create child scopes.

    Args:
        steps: List of workflow steps to execute
        context: Context strategy (default: FlatContext)
        cache: Cache strategy (default: InMemoryCache)
        retry: Retry strategy (default: NoRetry)
        initial_context: Initial context values (e.g., {"file_path": "main.py"})

    Returns:
        Final context string
    """
    scope = Scope(
        context=context or FlatContext(),
        cache=cache or InMemoryCache(),
        retry=retry or NoRetry(),
    )

    # Add initial context
    if initial_context:
        for key, value in initial_context.items():
            scope.context.add(key, value)

    _run_steps(scope, steps)
    return scope.context.get_context()


def _execute_state(
    state: WorkflowState,
    scope: Scope,
    state_map: dict[str, WorkflowState],
    prev_state: WorkflowState | None,
) -> tuple[str, str]:
    """Execute a single state and return (result, next_state_name or empty).

    Returns:
        Tuple of (action result, next state name or "" if terminal/no transition)
    """
    scope.context.add("state", state.name)

    # Run on_entry hook (if entering new state)
    if state.on_entry and state != prev_state:
        scope.run(state.on_entry)

    if state.terminal:
        scope.context.add("result", "Terminal state reached")
        return "", ""

    # Execute state action
    result = ""
    if state.action:
        result = scope.run(state.action)

    # Find matching transition
    next_state_name = ""
    for t in state.transitions:
        if t.condition is None:
            next_state_name = t.next
            break
        if evaluate_condition(t.condition, scope.context.get_context(), result):
            next_state_name = t.next
            break

    # Run on_exit hook before transitioning
    if next_state_name and state.on_exit:
        scope.run(state.on_exit)

    return result, next_state_name


def state_machine_loop(
    states: list[WorkflowState],
    initial: str,
    context: ContextStrategy | None = None,
    cache: CacheStrategy | None = None,
    retry: RetryStrategy | None = None,
    max_transitions: int = 50,
    initial_context: dict[str, str] | None = None,
) -> str:
    """Execute state machine until terminal state or limit.

    Supports parallel state execution: if a state has `parallel` set,
    all listed states execute concurrently via ThreadPoolExecutor,
    then transition to the `join` state.

    Args:
        states: List of workflow states
        initial: Name of initial state
        context: Context strategy (default: FlatContext)
        cache: Cache strategy (default: InMemoryCache)
        retry: Retry strategy (default: NoRetry)
        max_transitions: Maximum state transitions before stopping
        initial_context: Initial context values

    Returns:
        Final context string
    """
    from concurrent.futures import ThreadPoolExecutor, as_completed

    scope = Scope(
        context=context or FlatContext(),
        cache=cache or InMemoryCache(),
        retry=retry or NoRetry(),
    )

    # Add initial context
    if initial_context:
        for key, value in initial_context.items():
            scope.context.add(key, value)

    # Build state lookup
    state_map = {s.name: s for s in states}

    if initial not in state_map:
        scope.context.add("error", f"Initial state '{initial}' not found")
        return scope.context.get_context()

    current = state_map[initial]
    prev_state: WorkflowState | None = None

    for _ in range(max_transitions):
        # Check for parallel execution
        if current.parallel:
            # Validate parallel states exist
            missing = [s for s in current.parallel if s not in state_map]
            if missing:
                scope.context.add("error", f"Parallel states not found: {missing}")
                break

            if not current.join:
                scope.context.add("error", f"State '{current.name}' has parallel but no join")
                break

            if current.join not in state_map:
                scope.context.add("error", f"Join state '{current.join}' not found")
                break

            scope.context.add("state", f"{current.name} (forking)")

            # Run on_entry for the fork state
            if current.on_entry and current != prev_state:
                scope.run(current.on_entry)

            # Execute parallel states concurrently
            parallel_states = [state_map[s] for s in current.parallel]

            def run_parallel_state(pstate: WorkflowState) -> tuple[str, str, str]:
                """Run a state in parallel, return (name, result, next)."""
                with scope.child() as child_scope:
                    result, next_name = _execute_state(pstate, child_scope, state_map, None)
                    return pstate.name, result, next_name

            results: list[tuple[str, str, str]] = []
            with ThreadPoolExecutor(max_workers=len(parallel_states)) as executor:
                futures = {executor.submit(run_parallel_state, ps): ps for ps in parallel_states}
                for future in as_completed(futures):
                    results.append(future.result())

            # Record parallel results
            for name, result, _ in results:
                scope.context.add("parallel_result", f"{name}: {result[:100]}")

            # Run on_exit for the fork state
            if current.on_exit:
                scope.run(current.on_exit)

            prev_state = current
            current = state_map[current.join]
            continue

        # Normal sequential execution
        result, next_state_name = _execute_state(current, scope, state_map, prev_state)

        if current.terminal:
            break

        if not next_state_name:
            scope.context.add("error", f"No valid transition from state '{current.name}'")
            break

        if next_state_name not in state_map:
            scope.context.add("error", f"Target state '{next_state_name}' not found")
            break

        prev_state = current
        current = state_map[next_state_name]

    return scope.context.get_context()


def run_workflow(path: str, task: str = "", initial_context: dict[str, str] | None = None) -> str:
    """Load and run a workflow from TOML.

    Automatically detects workflow type (priority: states > steps > llm):
    - If workflow has states: runs state_machine_loop
    - If workflow has steps: runs step_loop
    - If workflow has llm: runs agent_loop

    Args:
        path: Path to workflow TOML file
        task: Task description (for agentic workflows)
        initial_context: Initial context (for step-based/state machine workflows)

    Returns:
        Final context string
    """
    config = load_workflow(path)

    # State machine workflow
    if config.states and config.initial_state:
        return state_machine_loop(
            states=config.states,
            initial=config.initial_state,
            context=config.context,
            cache=config.cache,
            retry=config.retry,
            max_transitions=config.max_turns,
            initial_context=initial_context,
        )

    # Step-based workflow
    if config.steps:
        return step_loop(
            steps=config.steps,
            context=config.context,
            cache=config.cache,
            retry=config.retry,
            initial_context=initial_context,
        )

    # Agentic workflow
    return agent_loop(
        task=task,
        context=config.context,
        cache=config.cache,
        retry=config.retry,
        llm=config.llm,
        max_turns=config.max_turns,
    )
