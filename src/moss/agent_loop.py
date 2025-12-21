"""Composable Agent Loops: Define loops as data, execute with metrics.

Design principle: LLM calls are expensive. Structural tools are cheap.
Track them separately and optimize for fewer LLM calls.
"""

from __future__ import annotations

import json
import time
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum, auto
from pathlib import Path
from typing import Any, Protocol, runtime_checkable

# Lazy-loaded prompts.
# Loaded from src/moss/prompts/ (or user override in .moss/prompts/)
_repair_engine_prompt: str | None = None
_terse_prompt: str | None = None


def get_repair_engine_prompt() -> str:
    """Load repair engine prompt with caching."""
    global _repair_engine_prompt
    if _repair_engine_prompt is None:
        from moss.prompts import load_prompt

        _repair_engine_prompt = load_prompt("repair-engine")
    return _repair_engine_prompt


def get_terse_prompt() -> str:
    """Load terse system prompt with caching."""
    global _terse_prompt
    if _terse_prompt is None:
        from moss.prompts import load_prompt

        _terse_prompt = load_prompt("terse")
    return _terse_prompt


class StepType(Enum):
    """Type of step - affects how we count it."""

    TOOL = auto()  # Cheap, fast (skeleton, grep, validate)
    LLM = auto()  # Expensive, slow (generation, decisions)
    HYBRID = auto()  # Tool that may call LLM internally


class ErrorAction(Enum):
    """What to do when a step fails."""

    ABORT = auto()  # Stop the loop
    RETRY = auto()  # Retry the step (up to max_retries)
    SKIP = auto()  # Skip and continue to next step
    GOTO = auto()  # Jump to a specific step


@dataclass
class LoopStep:
    """Single step in an agent loop.

    Steps are composable units that can be tools or LLM calls.
    """

    name: str
    tool: str  # Tool name (e.g., "skeleton.format", "patch.apply")
    step_type: StepType = StepType.TOOL
    input_from: str | None = None  # Previous step to get input from
    parameters: dict[str, Any] = field(default_factory=dict)  # Static parameters
    on_error: ErrorAction = ErrorAction.ABORT
    goto_target: str | None = None  # For GOTO action
    max_retries: int = 3
    timeout_seconds: float | None = None

    def __post_init__(self) -> None:
        if self.on_error == ErrorAction.GOTO and not self.goto_target:
            raise ValueError("GOTO action requires goto_target")

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "name": self.name,
            "tool": self.tool,
            "step_type": self.step_type.name.lower(),
            "input_from": self.input_from,
            "parameters": self.parameters,
            "on_error": self.on_error.name.lower(),
            "goto_target": self.goto_target,
            "max_retries": self.max_retries,
            "timeout_seconds": self.timeout_seconds,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> LoopStep:
        """Create from dictionary."""
        step_type_str = data.get("step_type", "tool")
        on_error_str = data.get("on_error", "abort")
        return cls(
            name=data["name"],
            tool=data["tool"],
            step_type=StepType[step_type_str.upper()],
            input_from=data.get("input_from"),
            parameters=data.get("parameters", {}),
            on_error=ErrorAction[on_error_str.upper()],
            goto_target=data.get("goto_target"),
            max_retries=data.get("max_retries", 3),
            timeout_seconds=data.get("timeout_seconds"),
        )


@dataclass
class AgentLoop:
    """Composable agent loop definition.

    Loops are data - they describe what to do, not how to do it.
    The LoopRunner executes them.

    Note on max_steps: This counts total step executions, not complete
    passes through the loop. For a 3-step loop, max_steps=10 allows
    ~3 full passes before terminating.
    """

    name: str
    steps: list[LoopStep]
    entry: str | None = None  # Starting step (default: first)
    exit_conditions: list[str] = field(default_factory=list)

    # Resource limits
    max_steps: int = 10  # Total step executions (not loop passes)
    token_budget: int | None = None
    timeout_seconds: float | None = None

    def __post_init__(self) -> None:
        if not self.steps:
            raise ValueError("Loop must have at least one step")
        if self.entry is None:
            self.entry = self.steps[0].name

        # Validate all step names are unique
        names = [s.name for s in self.steps]
        if len(names) != len(set(names)):
            raise ValueError("Step names must be unique")

        # Validate entry and goto targets exist
        name_set = set(names)
        if self.entry not in name_set:
            raise ValueError(f"Entry step '{self.entry}' not found")
        for step in self.steps:
            if step.goto_target and step.goto_target not in name_set:
                raise ValueError(f"GOTO target '{step.goto_target}' not found")

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "name": self.name,
            "steps": [s.to_dict() for s in self.steps],
            "entry": self.entry,
            "exit_conditions": self.exit_conditions,
            "max_steps": self.max_steps,
            "token_budget": self.token_budget,
            "timeout_seconds": self.timeout_seconds,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> AgentLoop:
        """Create from dictionary."""
        return cls(
            name=data["name"],
            steps=[LoopStep.from_dict(s) for s in data["steps"]],
            entry=data.get("entry"),
            exit_conditions=data.get("exit_conditions", []),
            max_steps=data.get("max_steps", 10),
            token_budget=data.get("token_budget"),
            timeout_seconds=data.get("timeout_seconds"),
        )


@dataclass
class LoopMetrics:
    """Track what matters: LLM usage is the bottleneck.

    Primary goal: minimize llm_calls while maintaining success rate.
    """

    # LLM tracking (expensive!)
    llm_calls: int = 0
    llm_tokens_in: int = 0
    llm_tokens_out: int = 0

    # Tool tracking (cheap)
    tool_calls: int = 0

    # Time tracking
    wall_time_seconds: float = 0.0
    step_times: dict[str, float] = field(default_factory=dict)

    # Iteration tracking
    iterations: int = 0
    retries: int = 0

    def record_step(
        self,
        step_name: str,
        step_type: StepType,
        duration: float,
        tokens_in: int = 0,
        tokens_out: int = 0,
    ) -> None:
        """Record a step execution."""
        if step_type == StepType.LLM:
            self.llm_calls += 1
            self.llm_tokens_in += tokens_in
            self.llm_tokens_out += tokens_out
        elif step_type == StepType.TOOL:
            self.tool_calls += 1
        else:  # HYBRID
            self.tool_calls += 1
            if tokens_in or tokens_out:
                self.llm_calls += 1
                self.llm_tokens_in += tokens_in
                self.llm_tokens_out += tokens_out

        self.step_times[step_name] = self.step_times.get(step_name, 0) + duration

    def to_compact(self) -> str:
        """Format as compact summary."""
        lines = [
            f"LLM: {self.llm_calls} calls, {self.llm_tokens_in + self.llm_tokens_out} tokens",
            f"Tools: {self.tool_calls} calls",
            f"Time: {self.wall_time_seconds:.2f}s",
            f"Iterations: {self.iterations}, Retries: {self.retries}",
        ]
        return " | ".join(lines)


class StepStatus(Enum):
    """Status of a step execution."""

    SUCCESS = auto()
    FAILED = auto()
    SKIPPED = auto()
    TIMEOUT = auto()


@dataclass
class StepResult:
    """Result of executing a single step."""

    step_name: str
    status: StepStatus
    output: Any = None
    error: str | None = None
    duration_seconds: float = 0.0
    tokens_in: int = 0
    tokens_out: int = 0


@dataclass
class LoopContext:
    """Context passed through all loop steps.

    Provides access to initial input and all previous step outputs.
    This enables steps to access both the original request AND
    results from earlier steps.

    Attributes:
        input: The original initial_input passed to run()
        steps: Dict mapping step names to their outputs
        last: The most recent step's output (convenience)
        expanded_symbols: Symbols that have been fully viewed (not just skeleton)
    """

    input: Any = None
    steps: dict[str, Any] = field(default_factory=dict)
    last: Any = None
    expanded_symbols: set[str] = field(default_factory=set)

    def get(self, step_name: str, default: Any = None) -> Any:
        """Get a specific step's output."""
        return self.steps.get(step_name, default)

    def with_step(self, step_name: str, output: Any) -> LoopContext:
        """Return new context with step output added."""
        new_steps = dict(self.steps)
        new_steps[step_name] = output
        return LoopContext(
            input=self.input,
            steps=new_steps,
            last=output,
            expanded_symbols=set(self.expanded_symbols),
        )

    def with_expanded(self, symbol: str) -> LoopContext:
        """Return new context with symbol marked as expanded (Peek-First Policy)."""
        new_expanded = set(self.expanded_symbols)
        new_expanded.add(symbol)
        return LoopContext(
            input=self.input,
            steps=dict(self.steps),
            last=self.last,
            expanded_symbols=new_expanded,
        )

    def is_peeked(self, symbol: str) -> bool:
        """Check if a symbol has been fully viewed (expanded).

        Peek-First Policy: Symbols must be expanded before editing.
        This prevents hallucination of function bodies from skeleton-only views.
        """
        return symbol in self.expanded_symbols


class LoopStatus(Enum):
    """Final status of a loop execution."""

    SUCCESS = auto()
    FAILED = auto()
    TIMEOUT = auto()
    BUDGET_EXCEEDED = auto()
    MAX_ITERATIONS = auto()


@dataclass
class LoopResult:
    """Result of running an agent loop."""

    status: LoopStatus
    step_results: list[StepResult]
    metrics: LoopMetrics
    final_output: Any = None
    error: str | None = None

    @property
    def success(self) -> bool:
        return self.status == LoopStatus.SUCCESS

    def to_compact(self) -> str:
        """Format as compact summary."""
        status_str = "✓" if self.success else "✗"
        return f"{status_str} {self.status.name} | {self.metrics.to_compact()}"


@runtime_checkable
class ToolExecutor(Protocol):
    """Protocol for executing tools.

    Tools receive a LoopContext containing:
    - context.input: Original initial_input from run()
    - context.steps: Dict of previous step outputs
    - context.last: Most recent step output
    - context.get(name): Get specific step output
    """

    async def execute(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[Any, int, int]:
        """Execute a tool and return (output, tokens_in, tokens_out).

        Args:
            tool_name: The tool to execute (e.g., "skeleton.format")
            context: Full loop context with input and previous outputs
            step: The step definition (for input_from, etc.)

        Returns:
            Tuple of (output, tokens_in, tokens_out).
            For non-LLM tools, tokens should be (0, 0).
        """
        ...


class AgentLoopRunner:
    """Executes agent loops, tracking metrics.

    Separates LLM calls from tool calls for optimization.
    """

    def __init__(self, executor: ToolExecutor):
        self.executor = executor

    async def run(self, loop: AgentLoop, initial_input: Any = None) -> LoopResult:
        """Execute a loop with metrics tracking.

        Args:
            loop: The loop definition to execute
            initial_input: Initial input for the first step. Can be any value;
                          it will be wrapped in a LoopContext for step access.

        Returns:
            LoopResult with final status and metrics

        Note:
            Steps receive a LoopContext with:
            - context.input: The original initial_input
            - context.steps: Dict of all previous step outputs
            - context.last: Most recent step output
            - context.get(step_name): Get specific step output
        """
        metrics = LoopMetrics()
        step_results: list[StepResult] = []
        context = LoopContext(input=initial_input)

        start_time = time.time()

        # Build step lookup
        step_map = {s.name: s for s in loop.steps}
        step_order = [s.name for s in loop.steps]

        current_step_name = loop.entry
        iteration = 0

        while iteration < loop.max_steps:
            iteration += 1
            metrics.iterations = iteration

            step = step_map.get(current_step_name)
            if not step:
                return LoopResult(
                    status=LoopStatus.FAILED,
                    step_results=step_results,
                    metrics=metrics,
                    error=f"Step '{current_step_name}' not found",
                )

            # Execute step with the full context
            step_result = await self._execute_step(step, context, metrics)
            step_results.append(step_result)

            if step_result.status == StepStatus.SUCCESS:
                context = context.with_step(step.name, step_result.output)

                # Peek-First Policy: Track expanded symbols
                output = step_result.output
                if isinstance(output, dict) and "_expanded_symbol" in output:
                    context = context.with_expanded(output["_expanded_symbol"])

                # Check exit conditions
                for condition in loop.exit_conditions:
                    if condition == f"{step.name}.success":
                        metrics.wall_time_seconds = time.time() - start_time
                        return LoopResult(
                            status=LoopStatus.SUCCESS,
                            step_results=step_results,
                            metrics=metrics,
                            final_output=step_result.output,
                        )

                # Move to next step
                current_idx = step_order.index(current_step_name)
                if current_idx + 1 < len(step_order):
                    current_step_name = step_order[current_idx + 1]
                else:
                    # Reached end of steps - success if no exit conditions
                    if not loop.exit_conditions:
                        metrics.wall_time_seconds = time.time() - start_time
                        return LoopResult(
                            status=LoopStatus.SUCCESS,
                            step_results=step_results,
                            metrics=metrics,
                            final_output=step_result.output,
                        )
                    # Otherwise restart from entry
                    current_step_name = loop.entry

            else:
                # Handle error based on action
                if step.on_error == ErrorAction.ABORT:
                    metrics.wall_time_seconds = time.time() - start_time
                    return LoopResult(
                        status=LoopStatus.FAILED,
                        step_results=step_results,
                        metrics=metrics,
                        error=step_result.error,
                    )
                elif step.on_error == ErrorAction.SKIP:
                    current_idx = step_order.index(current_step_name)
                    if current_idx + 1 < len(step_order):
                        current_step_name = step_order[current_idx + 1]
                    else:
                        current_step_name = loop.entry
                elif step.on_error == ErrorAction.GOTO:
                    current_step_name = step.goto_target
                # RETRY is handled in _execute_step

            # Check timeout
            if loop.timeout_seconds:
                elapsed = time.time() - start_time
                if elapsed > loop.timeout_seconds:
                    metrics.wall_time_seconds = elapsed
                    return LoopResult(
                        status=LoopStatus.TIMEOUT,
                        step_results=step_results,
                        metrics=metrics,
                        error=f"Timeout after {elapsed:.1f}s",
                    )

            # Check token budget
            if loop.token_budget:
                total_tokens = metrics.llm_tokens_in + metrics.llm_tokens_out
                if total_tokens > loop.token_budget:
                    metrics.wall_time_seconds = time.time() - start_time
                    return LoopResult(
                        status=LoopStatus.BUDGET_EXCEEDED,
                        step_results=step_results,
                        metrics=metrics,
                        error=f"Token budget exceeded: {total_tokens} > {loop.token_budget}",
                    )

        # Max iterations reached
        metrics.wall_time_seconds = time.time() - start_time
        return LoopResult(
            status=LoopStatus.MAX_ITERATIONS,
            step_results=step_results,
            metrics=metrics,
            error=f"Max steps ({loop.max_steps}) reached",
        )

    async def _execute_step(
        self, step: LoopStep, context: LoopContext, metrics: LoopMetrics
    ) -> StepResult:
        """Execute a single step with retry logic."""
        retries = 0
        last_error: str | None = None

        while retries <= step.max_retries:
            start = time.time()
            try:
                output, tokens_in, tokens_out = await self.executor.execute(
                    step.tool, context, step
                )
                duration = time.time() - start

                metrics.record_step(step.name, step.step_type, duration, tokens_in, tokens_out)

                return StepResult(
                    step_name=step.name,
                    status=StepStatus.SUCCESS,
                    output=output,
                    duration_seconds=duration,
                    tokens_in=tokens_in,
                    tokens_out=tokens_out,
                )

            except TimeoutError:
                return StepResult(
                    step_name=step.name,
                    status=StepStatus.TIMEOUT,
                    error="Step timed out",
                    duration_seconds=time.time() - start,
                )

            except Exception as e:
                last_error = str(e)
                retries += 1
                metrics.retries += 1

                if step.on_error != ErrorAction.RETRY or retries > step.max_retries:
                    break

        return StepResult(
            step_name=step.name,
            status=StepStatus.FAILED,
            error=last_error,
        )


# ============================================================================
# Pre-built loop templates
# ============================================================================


def simple_loop(name: str = "simple") -> AgentLoop:
    """Simple linear loop: understand → act → validate.

    NOTE: This loop template requires an LLM executor to work properly.
    The 'act' step needs to generate a patch from the skeleton context.
    With MossToolExecutor alone, the data flow breaks because skeleton
    output (string) doesn't match patch.apply input (dict with file_path, patch).

    For tool-only analysis, use a custom loop without input chaining:
        AgentLoop(steps=[
            LoopStep('skeleton', 'skeleton.format'),
            LoopStep('deps', 'dependencies.format'),
        ], max_steps=10)
    """
    return AgentLoop(
        name=name,
        steps=[
            LoopStep("understand", "skeleton.format", step_type=StepType.TOOL),
            LoopStep("act", "patch.apply", input_from="understand", step_type=StepType.LLM),
            LoopStep(
                "validate",
                "validation.validate",
                input_from="act",
                step_type=StepType.TOOL,
                on_error=ErrorAction.RETRY,
            ),
        ],
        exit_conditions=["validate.success"],
    )


def critic_loop(name: str = "critic") -> AgentLoop:
    """Two-pass loop: draft → review → revise → validate."""
    return AgentLoop(
        name=name,
        steps=[
            LoopStep("draft", "patch.apply", step_type=StepType.TOOL),
            LoopStep("review", "llm.critique", input_from="draft", step_type=StepType.LLM),
            LoopStep(
                "revise",
                "patch.apply",
                input_from="review",
                step_type=StepType.TOOL,
                on_error=ErrorAction.SKIP,
            ),
            LoopStep(
                "validate",
                "validation.validate",
                input_from="revise",
                step_type=StepType.TOOL,
            ),
        ],
        exit_conditions=["validate.success"],
    )


def enhanced_critic_loop(name: str = "enhanced_critic") -> AgentLoop:
    """Robust critic loop with dedicated mistake detection."""
    return AgentLoop(
        name=name,
        steps=[
            LoopStep("draft", "llm.generate", step_type=StepType.LLM),
            LoopStep("detect", "llm.critique", input_from="draft", step_type=StepType.LLM),
            LoopStep("fix", "llm.generate", input_from="detect", step_type=StepType.LLM),
            LoopStep("apply", "patch.apply", input_from="fix", step_type=StepType.TOOL),
            LoopStep(
                "validate", "validation.validate", input_from="apply", step_type=StepType.TOOL
            ),
        ],
        exit_conditions=["validate.success"],
    )


def analysis_loop(name: str = "analysis") -> AgentLoop:
    """Simple analysis loop: skeleton → LLM analyze → done.

    This is the simplest E2E loop that uses real LLM calls.
    Good for testing the infrastructure works.
    """
    return AgentLoop(
        name=name,
        steps=[
            LoopStep("skeleton", "skeleton.format", step_type=StepType.TOOL),
            LoopStep(
                "analyze",
                "llm.analyze",
                input_from="skeleton",
                step_type=StepType.LLM,
            ),
        ],
        exit_conditions=["analyze.success"],
    )


def docstring_loop(name: str = "docstring") -> AgentLoop:
    """Docstring generation loop: skeleton → LLM identify missing → done.

    Identifies functions missing docstrings. The output can be used
    to generate patches in a follow-up step.
    """
    return AgentLoop(
        name=name,
        steps=[
            LoopStep("skeleton", "skeleton.format", step_type=StepType.TOOL),
            LoopStep(
                "identify",
                "llm.add_docstrings",
                input_from="skeleton",
                step_type=StepType.LLM,
            ),
        ],
        exit_conditions=["identify.success"],
    )


def docstring_full_loop(name: str = "docstring_full") -> AgentLoop:
    """Full docstring loop: skeleton → LLM identify → parse → done.

    Returns parsed docstring entries ready for patching.
    Each entry has 'function' and 'docstring' keys.

    Example output:
        [
            {"function": "my_func", "docstring": "Does something useful."},
            {"function": "other_func", "docstring": "Does something else."},
        ]
    """
    return AgentLoop(
        name=name,
        steps=[
            LoopStep("skeleton", "skeleton.format", step_type=StepType.TOOL),
            LoopStep(
                "identify",
                "llm.add_docstrings",
                input_from="skeleton",
                step_type=StepType.LLM,
            ),
            LoopStep(
                "parse",
                "parse.docstrings",
                input_from="identify",
                step_type=StepType.TOOL,
            ),
        ],
        exit_conditions=["parse.success"],
    )


def docstring_apply_loop(name: str = "docstring_apply") -> AgentLoop:
    """Full docstring workflow: skeleton → LLM identify → parse → apply patches.

    This loop:
    1. Gets file skeleton (skeleton.format)
    2. LLM identifies functions needing docstrings (llm.add_docstrings)
    3. Parses LLM output into structured data (parse.docstrings)
    4. Applies docstrings to the file (patch.docstrings)

    Input: {file_path: str}
    Output: {applied: list, skipped: list, errors: list}
    """
    return AgentLoop(
        name=name,
        steps=[
            LoopStep("skeleton", "skeleton.format", step_type=StepType.TOOL),
            LoopStep(
                "identify",
                "llm.add_docstrings",
                input_from="skeleton",
                step_type=StepType.LLM,
            ),
            LoopStep(
                "parse",
                "parse.docstrings",
                input_from="identify",
                step_type=StepType.TOOL,
            ),
            LoopStep(
                "apply",
                "patch.docstrings",
                input_from="parse",
                step_type=StepType.TOOL,
            ),
        ],
        exit_conditions=["apply.success"],
    )


def incremental_loop(name: str = "incremental") -> AgentLoop:
    """Incremental context loading: skeleton → targeted → full (if needed)."""
    return AgentLoop(
        name=name,
        steps=[
            LoopStep("skeleton", "skeleton.format", step_type=StepType.TOOL),
            LoopStep(
                "decide",
                "llm.needs_more_context",
                input_from="skeleton",
                step_type=StepType.LLM,
            ),
            LoopStep(
                "targeted",
                "anchor.resolve",
                input_from="decide",
                step_type=StepType.TOOL,
                on_error=ErrorAction.SKIP,
            ),
            LoopStep(
                "act",
                "patch.apply",
                input_from="targeted",
                step_type=StepType.TOOL,
            ),
            LoopStep(
                "validate",
                "validation.validate",
                input_from="act",
                step_type=StepType.TOOL,
            ),
        ],
        exit_conditions=["validate.success"],
    )


def loop_critic_loop(name: str = "loop_critic") -> AgentLoop:
    """Meta-loop that critiques and improves loop definitions.

    Takes a loop definition (YAML/JSON) as input and produces
    suggestions for improvement. This is recursive self-improvement:
    a loop that improves other loops.

    Steps:
    1. Analyze the loop structure
    2. Identify potential issues (missing error handling, inefficiencies)
    3. Suggest improvements
    """
    return AgentLoop(
        name=name,
        steps=[
            LoopStep(
                "analyze",
                "llm.analyze_loop",
                step_type=StepType.LLM,
                config={
                    "prompt": (
                        "Analyze this loop definition. Identify:\n"
                        "- Missing error handling (steps without on_error)\n"
                        "- Potential infinite loops (cycles without exit conditions)\n"
                        "- Inefficient step ordering\n"
                        "- Missing validation steps\n"
                        "- Unclear step purposes\n"
                        "Output as structured analysis."
                    )
                },
            ),
            LoopStep(
                "suggest",
                "llm.suggest_improvements",
                input_from="analyze",
                step_type=StepType.LLM,
                config={
                    "prompt": (
                        "Based on this analysis, suggest specific improvements.\n"
                        "Format as a list of changes with before/after examples.\n"
                        "Prioritize: safety > efficiency > clarity."
                    )
                },
            ),
        ],
        exit_conditions=["suggest.success"],
    )


def loop_optimizer_loop(name: str = "loop_optimizer") -> AgentLoop:
    """Meta-loop that optimizes loop definitions for token efficiency.

    Analyzes a loop and produces an optimized version that:
    - Reduces LLM calls where possible
    - Combines steps that can be merged
    - Adds caching hints
    - Removes redundant validation

    This loop outputs a modified loop definition.
    """
    return AgentLoop(
        name=name,
        steps=[
            LoopStep(
                "measure",
                "llm.estimate_tokens",
                step_type=StepType.LLM,
                config={
                    "prompt": (
                        "Estimate token usage for each step in this loop.\n"
                        "Consider: input size, output size, prompt overhead.\n"
                        "Output as JSON: {step_name: estimated_tokens}"
                    )
                },
            ),
            LoopStep(
                "identify_waste",
                "llm.find_redundancy",
                input_from="measure",
                step_type=StepType.LLM,
                config={
                    "prompt": (
                        "Identify token waste:\n"
                        "- Redundant LLM calls (could be tool calls)\n"
                        "- Steps that always produce same output\n"
                        "- Validation that duplicates earlier checks\n"
                        "- Overly verbose prompts"
                    )
                },
            ),
            LoopStep(
                "optimize",
                "llm.rewrite_loop",
                input_from="identify_waste",
                step_type=StepType.LLM,
                config={
                    "prompt": (
                        "Rewrite the loop definition to reduce token usage.\n"
                        "Apply optimizations identified above.\n"
                        "Output the complete optimized loop as YAML."
                    )
                },
            ),
        ],
        exit_conditions=["optimize.success"],
    )


def telemetry_optimizer_loop(name: str = "telemetry_optimizer") -> AgentLoop:
    """Meta-loop that optimizes agent performance based on real session telemetry."""
    return AgentLoop(
        name=name,
        steps=[
            LoopStep("fetch_data", "telemetry.analyze_all_sessions", step_type=StepType.TOOL),
            LoopStep(
                "analyze",
                "llm.analyze_telemetry",
                input_from="fetch_data",
                step_type=StepType.LLM,
            ),
            LoopStep(
                "propose",
                "llm.propose_optimizations",
                input_from="analyze",
                step_type=StepType.LLM,
            ),
        ],
        exit_conditions=["propose.success"],
    )


def self_improving_docstring_loop(name: str = "self_improve_docstring") -> AgentLoop:
    """Docstring loop that learns from its own performance.

    Combines docstring generation with self-critique:
    1. Generate docstrings
    2. Validate quality
    3. If quality low, analyze what went wrong
    4. Retry with improved prompt (learning from mistakes)

    This demonstrates recursive improvement within a single run.
    """
    return AgentLoop(
        name=name,
        steps=[
            # Initial generation
            LoopStep("skeleton", "skeleton.format", step_type=StepType.TOOL),
            LoopStep(
                "generate",
                "llm.add_docstrings",
                input_from="skeleton",
                step_type=StepType.LLM,
            ),
            # Self-critique
            LoopStep(
                "critique",
                "llm.critique_docstrings",
                input_from="generate",
                step_type=StepType.LLM,
                config={
                    "prompt": (
                        "Critique these docstrings:\n"
                        "- Are they accurate?\n"
                        "- Do they explain the 'why', not just 'what'?\n"
                        "- Are args/returns documented?\n"
                        "- Score 1-10 and list issues."
                    )
                },
            ),
            # Conditional improvement (retry if score < 7)
            LoopStep(
                "improve",
                "llm.improve_docstrings",
                input_from="critique",
                step_type=StepType.LLM,
                on_error=ErrorAction.SKIP,
                config={
                    "prompt": (
                        "Improve the docstrings based on the critique.\n"
                        "Address each issue listed.\n"
                        "Output the improved docstrings."
                    ),
                    "condition": "critique.score < 7",  # Only run if quality low
                },
            ),
        ],
        exit_conditions=["improve.success", "critique.score >= 7"],
        max_steps=10,
    )


# ============================================================================
# MossAPI Tool Executor
# ============================================================================


class MossToolExecutor:
    """Execute tools via MossAPI.

    Maps tool names to MossAPI methods. Non-LLM tools return (0, 0) for tokens.

    Tools receive the full LoopContext and can access:
    - context.input: Original input (typically {file_path: ..., task: ...})
    - context.steps: Previous step outputs
    - context.get(step_name): Specific step output

    Peek-First Policy:
        When enforce_peek_first=True, patch operations will fail if the target
        symbol was only seen via skeleton, not expanded. This prevents
        hallucination of function bodies.
    """

    def __init__(self, root: Any = None, enforce_peek_first: bool = False):
        from pathlib import Path

        from moss.moss_api import MossAPI

        self.api = MossAPI(root or Path.cwd())
        self.enforce_peek_first = enforce_peek_first

    def _get_input(self, context: LoopContext, step: LoopStep) -> Any:
        """Extract the appropriate input for this step.

        Priority:
        1. If step.input_from is set, use that step's output
        2. If step.parameters is set, use that
        3. Otherwise use context.input (original initial_input)
        """
        if step.input_from and step.input_from in context.steps:
            return context.steps[step.input_from]
        if step.parameters:
            return step.parameters
        return context.input

    def _get_file_path(self, context: LoopContext, step: LoopStep) -> str:
        """Extract file_path from context or step parameters."""
        # 1. Check parameters
        if step.parameters and "file_path" in step.parameters:
            return step.parameters["file_path"]

        # 2. Check input from previous step
        if step.input_from and step.input_from in context.steps:
            inp = context.steps[step.input_from]
            if isinstance(inp, dict) and "file_path" in inp:
                return inp["file_path"]
            if isinstance(inp, str):
                # Heuristic: assume string input might be a path if no other context
                return inp

        # 3. Check original input
        initial = context.input
        if isinstance(initial, dict) and "file_path" in initial:
            return initial["file_path"]
        if isinstance(initial, str):
            return initial

        raise ValueError(f"Cannot extract file_path from context or parameters. Input: {initial}")

    def _parse_docstring_output(self, llm_output: str) -> list[dict[str, str]]:
        """Parse LLM output in FUNC:name|docstring format.

        Returns list of dicts with 'function' and 'docstring' keys.
        """
        results = []
        for line in llm_output.strip().split("\n"):
            line = line.strip()
            if not line or not line.startswith("FUNC:"):
                continue
            # Format: FUNC:function_name|One-line description
            rest = line[5:]  # Remove "FUNC:" prefix
            if "|" not in rest:
                continue
            name, docstring = rest.split("|", 1)
            name = name.strip()
            if not name:  # Skip empty function names
                continue
            results.append(
                {
                    "function": name,
                    "docstring": docstring.strip(),
                }
            )
        return results

    def _apply_docstrings(self, file_path: str, docstrings: list[dict[str, str]]) -> dict[str, Any]:
        """Apply docstrings to functions in a file.

        Args:
            file_path: Path to the Python file
            docstrings: List of dicts with 'function' and 'docstring' keys

        Returns:
            Dict with 'applied', 'skipped', and 'errors' lists
        """
        from pathlib import Path as P

        path = P(file_path) if not isinstance(file_path, P) else file_path
        if not path.is_absolute():
            path = self.root / path

        if not path.exists():
            return {"applied": [], "skipped": [], "errors": [f"File not found: {path}"]}

        source = path.read_text()
        lines = source.splitlines(keepends=True)

        applied = []
        skipped = []
        errors = []

        # Find function definitions and their locations
        import ast

        try:
            tree = ast.parse(source)
        except SyntaxError as e:
            return {"applied": [], "skipped": [], "errors": [f"Syntax error: {e}"]}

        # Build a map of function names to their line numbers
        func_lines: dict[str, int] = {}
        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                func_lines[node.name] = node.lineno

        # Process docstrings in reverse order to avoid line number shifts
        insertions: list[tuple[int, str, str]] = []  # (line_num, func_name, docstring)

        for entry in docstrings:
            func_name = entry.get("function", "")
            docstring = entry.get("docstring", "")

            if not func_name or not docstring:
                skipped.append(func_name or "(empty)")
                continue

            if func_name not in func_lines:
                errors.append(f"Function not found: {func_name}")
                continue

            line_num = func_lines[func_name]
            insertions.append((line_num, func_name, docstring))

        # Sort by line number in reverse order
        insertions.sort(key=lambda x: x[0], reverse=True)

        # Apply insertions
        for line_num, func_name, docstring in insertions:
            # Find the function's first line and get its indentation
            func_line = lines[line_num - 1] if line_num <= len(lines) else ""
            base_indent = len(func_line) - len(func_line.lstrip())

            # The docstring should be indented one level more than the function
            body_indent = " " * (base_indent + 4)

            # Format the docstring
            docstring_text = f'{body_indent}"""{docstring}"""\n'

            # Find where to insert (after the function signature)
            # This is the line after the def line(s)
            insert_line = line_num
            # Handle multi-line function signatures
            while insert_line <= len(lines):
                if lines[insert_line - 1].rstrip().endswith(":"):
                    break
                insert_line += 1

            # Insert after the colon line
            lines.insert(insert_line, docstring_text)
            applied.append(func_name)

        # Write the modified file
        if applied:
            path.write_text("".join(lines))

        return {"applied": applied, "skipped": skipped, "errors": errors}

    def _parse_patch_output(self, llm_output: str) -> dict[str, Any]:
        """Parse LLM output into Patch object.

        Expected format (simple):
            ANCHOR: function_name
            TYPE: replace
            CONTENT:
            def function_name():
                new implementation

        Returns dict with 'patch' key containing the Patch object, or 'error' if parsing fails.
        """
        from moss.anchors import Anchor
        from moss.patches import Patch, PatchType

        lines = llm_output.strip().split("\n")
        anchor_name: str | None = None
        patch_type_str: str = "replace"
        content_lines: list[str] = []
        in_content = False

        for line in lines:
            line_stripped = line.strip()
            upper = line_stripped.upper()

            if upper.startswith("ANCHOR:"):
                anchor_name = line_stripped[7:].strip()
                in_content = False
            elif upper.startswith("TYPE:"):
                patch_type_str = line_stripped[5:].strip().lower()
                in_content = False
            elif upper.startswith("CONTENT:"):
                in_content = True
                # Check if there's content on the same line
                rest = line_stripped[8:].strip()
                if rest:
                    content_lines.append(rest)
            elif in_content:
                content_lines.append(line)

        if not anchor_name:
            return {"error": "Missing ANCHOR: line in patch output"}

        # Map type string to PatchType
        type_map = {
            "replace": PatchType.REPLACE,
            "insert_before": PatchType.INSERT_BEFORE,
            "insert_after": PatchType.INSERT_AFTER,
            "delete": PatchType.DELETE,
        }
        patch_type = type_map.get(patch_type_str, PatchType.REPLACE)

        content = "\n".join(content_lines)
        anchor = Anchor(name=anchor_name)
        patch = Patch(anchor=anchor, patch_type=patch_type, content=content)

        return {"patch": patch}

    async def execute(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[Any, int, int]:
        """Execute a tool and return (output, tokens_in, tokens_out).

        All MossAPI tools are non-LLM, so tokens are always (0, 0).
        """
        input_data = self._get_input(context, step)

        # Route to appropriate tool
        if tool_name == "skeleton.format":
            file_path = self._get_file_path(context, step)
            result = self.api.skeleton.format(file_path)

        elif tool_name == "skeleton.extract":
            file_path = self._get_file_path(context, step)
            result = self.api.skeleton.extract(file_path)

        elif tool_name == "skeleton.expand":
            # Peek-First Policy: Track expanded symbols
            file_path = self._get_file_path(context, step)
            symbol_name = input_data.get("symbol") if isinstance(input_data, dict) else input_data
            result = self.api.skeleton.expand(file_path, symbol_name)
            # Mark symbol as expanded (return value includes tracking info)
            if result is not None:
                result = {"content": result, "_expanded_symbol": f"{file_path}:{symbol_name}"}

        elif tool_name in ("validation.validate", "validator.run"):
            file_path = self._get_file_path(context, step)
            result = self.api.validation.validate(file_path)

        elif tool_name == "patch.apply":
            # Peek-First Policy: Warn if no symbols were expanded before editing
            if self.enforce_peek_first and not context.expanded_symbols:
                raise ValueError(
                    "Peek-First Policy: Must expand at least one symbol before applying patches. "
                    "Use skeleton.expand or anchor.resolve to view full implementations first."
                )
            file_path = self._get_file_path(context, step)
            if isinstance(input_data, dict):
                patch = input_data.get("patch", input_data)
            else:
                patch = input_data
            result = self.api.patch.apply(file_path, patch)

        elif tool_name == "patch.apply_with_fallback":
            # Peek-First Policy: Same check as patch.apply
            if self.enforce_peek_first and not context.expanded_symbols:
                raise ValueError(
                    "Peek-First Policy: Must expand at least one symbol before applying patches. "
                    "Use skeleton.expand or anchor.resolve to view full implementations first."
                )
            file_path = self._get_file_path(context, step)
            if isinstance(input_data, dict):
                patch = input_data.get("patch", input_data)
            else:
                patch = input_data
            result = self.api.patch.apply_with_fallback(file_path, patch)

        elif tool_name == "anchor.find":
            file_path = self._get_file_path(context, step)
            name = input_data.get("name") if isinstance(input_data, dict) else input_data
            result = self.api.anchor.find(file_path, name)

        elif tool_name == "anchor.resolve":
            # Peek-First Policy: anchor.resolve shows full code, counts as expand
            file_path = self._get_file_path(context, step)
            name = input_data.get("name") if isinstance(input_data, dict) else input_data
            result = self.api.anchor.resolve(file_path, name)
            if result is not None:
                result = {"content": result, "_expanded_symbol": f"{file_path}:{name}"}

        elif tool_name == "dependencies.format":
            file_path = self._get_file_path(context, step)
            result = self.api.dependencies.format(file_path)

        elif tool_name == "dwim.analyze_intent":
            if isinstance(input_data, str):
                query = input_data
            else:
                query = input_data.get("query", str(input_data))
            result = self.api.dwim.analyze_intent(query)

        elif tool_name == "health.check":
            result = self.api.health.check()

        elif tool_name == "health.summarize":
            result = self.api.health.summarize()

        elif tool_name == "complexity.analyze":
            pattern = "**/*.py"
            if isinstance(input_data, dict):
                pattern = input_data.get("pattern", pattern)
            result = self.api.complexity.analyze(pattern)

        elif tool_name == "parse.docstrings":
            # Parse LLM output in FUNC:name|docstring format
            if not isinstance(input_data, str):
                input_data = str(input_data)
            result = self._parse_docstring_output(input_data)

        elif tool_name == "patch.docstrings":
            # Apply parsed docstrings to functions
            file_path = self._get_file_path(context, step)
            if not isinstance(input_data, list):
                raise ValueError(f"patch.docstrings expects list, got {type(input_data)}")
            result = self._apply_docstrings(file_path, input_data)

        elif tool_name == "parse.patch":
            # Parse LLM output into Patch object (ANCHOR/TYPE/CONTENT format)
            if not isinstance(input_data, str):
                input_data = str(input_data)
            result = self._parse_patch_output(input_data)

        else:
            raise ValueError(f"Unknown tool: {tool_name}")

        # Handle async results
        if hasattr(result, "__await__"):
            result = await result

        # All MossAPI tools are non-LLM
        return result, 0, 0


# ============================================================================
# MCP Client Executor
# ============================================================================


@dataclass
class MCPServerConfig:
    """Configuration for connecting to an MCP server.

    Supports stdio transport (subprocess) for now.
    """

    command: str  # e.g., "uv", "npx", "python"
    args: list[str] = field(default_factory=list)  # e.g., ["run", "moss-mcp"]
    cwd: str | None = None
    env: dict[str, str] | None = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "command": self.command,
            "args": self.args,
            "cwd": self.cwd,
            "env": self.env,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MCPServerConfig:
        """Create from dictionary."""
        return cls(
            command=data["command"],
            args=data.get("args", []),
            cwd=data.get("cwd"),
            env=data.get("env"),
        )


class MCPToolExecutor(ToolExecutor):
    """Execute tools via MCP client connections.

    This executor connects to external MCP servers and calls their tools.
    Useful for integrating external capabilities (filesystem, git, browser, etc.)
    into moss loops.

    Example:
        config = MCPServerConfig(command="npx", args=["@anthropic/mcp-server-filesystem"])
        executor = MCPToolExecutor(config)
        result, _, _ = await executor.execute("read_file", context, step)
    """

    def __init__(self, config: MCPServerConfig):
        self.config = config
        self._session: Any | None = None
        self._read_stream: Any | None = None
        self._write_stream: Any | None = None
        self._tools: dict[str, Any] = {}

    async def connect(self) -> None:
        """Connect to the MCP server and initialize session."""
        from mcp.client.session import ClientSession
        from mcp.client.stdio import StdioServerParameters, stdio_client

        server_params = StdioServerParameters(
            command=self.config.command,
            args=self.config.args,
            cwd=self.config.cwd,
            env=self.config.env,
        )

        # Store the context managers for cleanup
        self._stdio_cm = stdio_client(server_params)
        self._read_stream, self._write_stream = await self._stdio_cm.__aenter__()

        self._session_cm = ClientSession(self._read_stream, self._write_stream)
        self._session = await self._session_cm.__aenter__()

        # Initialize and cache tools
        await self._session.initialize()
        tools_result = await self._session.list_tools()
        self._tools = {t.name: t for t in tools_result.tools}

        # Auto-register tools into DWIM for natural language routing
        self._register_dwim_tools()

    def _register_dwim_tools(self) -> None:
        """Register MCP tools into DWIM registry for natural language routing."""
        from moss.dwim import register_mcp_tool

        for tool in self._tools.values():
            # Extract description and schema from MCP tool
            description = getattr(tool, "description", "") or tool.name
            input_schema = getattr(tool, "inputSchema", None)
            register_mcp_tool(
                name=tool.name,
                description=description,
                prefix="mcp",
                input_schema=input_schema,
            )

    def _unregister_dwim_tools(self) -> None:
        """Unregister MCP tools from DWIM registry."""
        from moss.dwim import unregister_mcp_tools

        unregister_mcp_tools(prefix="mcp")

    async def disconnect(self) -> None:
        """Disconnect from the MCP server."""
        # Unregister DWIM tools before disconnecting
        self._unregister_dwim_tools()

        if self._session_cm:
            await self._session_cm.__aexit__(None, None, None)
        if self._stdio_cm:
            await self._stdio_cm.__aexit__(None, None, None)
        self._session = None
        self._tools = {}

    def list_tools(self) -> list[str]:
        """Return list of available tool names."""
        return list(self._tools.keys())

    async def execute(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[Any, int, int]:
        """Execute an MCP tool and return (output, tokens_in, tokens_out).

        MCP tools don't use tokens directly.
        """
        if not self._session:
            await self.connect()

        # Get input data from context
        if step.input_from and step.input_from in context.steps:
            input_data = context.steps[step.input_from]
        else:
            input_data = context.input

        # Convert input to arguments dict
        if isinstance(input_data, dict):
            arguments = input_data
        elif isinstance(input_data, str):
            arguments = {"input": input_data}
        else:
            arguments = {"data": input_data}

        # Call the tool
        result = await self._session.call_tool(tool_name, arguments)

        # Extract content from result
        if hasattr(result, "content") and result.content:
            # MCP returns content as a list of content blocks
            output = []
            for block in result.content:
                if hasattr(block, "text"):
                    output.append(block.text)
                elif hasattr(block, "data"):
                    output.append(block.data)
            output = "\n".join(str(o) for o in output) if output else None
        else:
            output = result

        return output, 0, 0

    async def __aenter__(self) -> MCPToolExecutor:
        await self.connect()
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        await self.disconnect()


class CompositeToolExecutor(ToolExecutor):
    """Route tools to different executors based on prefix.

    Enables hybrid loops that use both local tools (MossAPI) and
    external tools (MCP servers, LLM).

    Example:
        executor = CompositeToolExecutor({
            "moss.": MossToolExecutor(root=Path(".")),
            "mcp.": MCPToolExecutor(config),
            "llm.": LLMToolExecutor(llm_config),
        })

        # Routes to MossToolExecutor
        await executor.execute("moss.skeleton.format", context, step)

        # Routes to MCPToolExecutor
        await executor.execute("mcp.read_file", context, step)

    The first matching prefix wins. If no prefix matches, raises ValueError.
    """

    def __init__(self, executors: dict[str, ToolExecutor], default: ToolExecutor | None = None):
        """Initialize with prefix-to-executor mapping.

        Args:
            executors: Dict mapping prefixes to executors (e.g., {"moss.": moss_exec})
            default: Optional fallback executor for tools with no matching prefix
        """
        self.executors = executors
        self.default = default

    def _get_executor(self, tool_name: str) -> tuple[ToolExecutor, str]:
        """Find the executor and stripped tool name for a tool.

        Returns:
            Tuple of (executor, stripped_tool_name)

        Raises:
            ValueError: If no matching executor found
        """
        for prefix, executor in self.executors.items():
            if tool_name.startswith(prefix):
                return executor, tool_name[len(prefix) :]
        if self.default:
            return self.default, tool_name
        raise ValueError(
            f"No executor found for tool: {tool_name}. "
            f"Available prefixes: {list(self.executors.keys())}"
        )

    async def execute(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[Any, int, int]:
        """Route to appropriate executor and execute."""
        executor, stripped_name = self._get_executor(tool_name)

        # Create a modified step with the stripped tool name
        modified_step = LoopStep(
            name=step.name,
            tool=stripped_name,
            step_type=step.step_type,
            input_from=step.input_from,
            on_error=step.on_error,
            goto_target=step.goto_target,
            max_retries=step.max_retries,
        )

        return await executor.execute(stripped_name, context, modified_step)


# ============================================================================
# LLM Tool Executor
# ============================================================================


@dataclass
class LLMConfig:
    """Configuration for LLM calls via litellm.

    Uses litellm's unified interface for all providers. Model names follow
    litellm conventions (e.g., "gemini/gemini-2.0-flash", "gpt-4o").

    Attributes:
        model: Primary model name (used if models is empty)
        models: List of models for rotation (reduces single-model bias)
        rotation: Rotation strategy ("round_robin", "random", or None to disable)
        temperature: Sampling temperature (0.0 = deterministic)
        max_tokens: Maximum tokens in response
        system_prompt: Optional system prompt (prepended to all requests)
        mock: If True, return placeholder responses without API calls

    Common model names:
        - "gemini/gemini-3-flash-preview" (latest flash, fast)
        - "gemini/gemini-3-pro" (powerful reasoning)
        - "gpt-4o" (OpenAI)
        - "claude-sonnet-4-20250514" (Anthropic)
        - "ollama/llama3" (local via Ollama)

    API keys are read from environment variables automatically by litellm:
        - GOOGLE_API_KEY for Gemini
        - OPENAI_API_KEY for OpenAI
        - ANTHROPIC_API_KEY for Anthropic

    Example with rotation:
        config = LLMConfig(
            models=["gemini/gemini-3-flash-preview", "gpt-4o"],
            rotation="round_robin"
        )
    """

    model: str = "gemini/gemini-3-flash-preview"  # Default: latest flash model
    models: list[str] = field(default_factory=list)  # For rotation
    rotation: str | None = None  # "round_robin", "random", or None
    temperature: float = 0.0
    max_tokens: int | None = None  # Let the model determine output length
    system_prompt: str | None = None  # None = load from prompts/terse.txt
    mock: bool = False  # Set True for testing without API calls

    def get_system_prompt(self) -> str:
        """Get system prompt, loading from file if not explicitly set."""
        if self.system_prompt is not None:
            return self.system_prompt
        return get_terse_prompt()

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "model": self.model,
            "models": self.models,
            "rotation": self.rotation,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "system_prompt": self.get_system_prompt(),
            "mock": self.mock,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> LLMConfig:
        """Create from dictionary."""
        return cls(
            model=data.get("model", "gemini/gemini-3-flash-preview"),
            models=data.get("models", []),
            rotation=data.get("rotation"),
            temperature=data.get("temperature", 0.0),
            max_tokens=data.get("max_tokens"),
            system_prompt=data.get("system_prompt"),  # None = use default from file
            mock=data.get("mock", False),
        )


def _load_dotenv() -> bool:
    """Load environment variables from .env file if python-dotenv is available.

    Searches for .env in current directory and parent directories.

    Returns:
        True if .env was loaded, False otherwise
    """
    try:
        from dotenv import load_dotenv

        return load_dotenv()
    except ImportError:
        return False


class LLMToolExecutor:
    """Execute tools including LLM-based ones with token tracking.

    Routes tool calls to either MossAPI (structural tools) or LLM
    (generation/reasoning tools). Uses litellm for unified LLM access.

    LLM tools are prefixed with "llm." and include:
    - llm.generate: Generate code/text from prompt
    - llm.critique: Review and critique code
    - llm.decide: Make a decision based on context

    Environment variables are loaded from .env file automatically.

    Memory Integration:
        When a MemoryManager is provided, the executor will:
        - Inject automatic memory context into system prompts
        - Check triggered memory before tool execution
        - Record episodes for future learning

    Example:
        config = LLMConfig(model="gemini/gemini-2.0-flash")
        memory = create_memory_manager()
        executor = LLMToolExecutor(config, memory=memory)

        runner = AgentLoopRunner(executor)
        result = await runner.run(critic_loop(), initial_input)

        print(f"Tokens: {result.metrics.llm_tokens_in + result.metrics.llm_tokens_out}")
    """

    _dotenv_loaded: bool = False

    def __init__(
        self,
        config: LLMConfig | None = None,
        moss_executor: MossToolExecutor | None = None,
        root: Any = None,
        load_env: bool = True,
        memory: Any = None,  # MemoryManager, typed as Any to avoid circular import
        cache_large_outputs: bool = True,
        large_output_threshold: int = 4000,
    ):
        """Initialize the executor.

        Args:
            config: LLM configuration (uses defaults if None)
            moss_executor: Executor for structural tools (created if None)
            root: Project root for MossToolExecutor
            load_env: Whether to load .env file (default: True)
            memory: Optional MemoryManager for cross-session learning
            cache_large_outputs: Whether to cache large outputs (default: True)
            large_output_threshold: Character threshold for caching (default: 4000)
        """
        # Load .env once per process
        if load_env and not LLMToolExecutor._dotenv_loaded:
            LLMToolExecutor._dotenv_loaded = _load_dotenv()

        self.config = config or LLMConfig()
        self.moss_executor = moss_executor or MossToolExecutor(root)
        self.memory = memory
        self._call_count = 0  # For round-robin rotation

        # Ephemeral output caching
        self.cache_large_outputs = cache_large_outputs
        self.large_output_threshold = large_output_threshold
        self._ephemeral_cache: Any = None  # Lazy-loaded

    def _get_ephemeral_cache(self) -> Any:
        """Get the ephemeral cache instance (lazy-loaded)."""
        if self._ephemeral_cache is None:
            from moss.cache import get_ephemeral_cache

            self._ephemeral_cache = get_ephemeral_cache()
        return self._ephemeral_cache

    def _maybe_cache_output(self, result: tuple[Any, int, int]) -> tuple[Any, int, int]:
        """Cache large outputs and return preview with cache ID.

        If the output is large (>threshold characters), stores the full content
        in the ephemeral cache and returns a preview with a cache ID.
        Agent can use "cache.get <id>" to retrieve the full content.

        Args:
            result: Tuple of (output, tokens_in, tokens_out)

        Returns:
            Original tuple if small, or (preview_with_id, tokens_in, tokens_out) if cached
        """
        if not self.cache_large_outputs:
            return result

        output, tokens_in, tokens_out = result

        # Only cache string outputs
        if not isinstance(output, str):
            # Try converting dicts/lists to string for size check
            if isinstance(output, (dict, list)):
                output_str = str(output)
            else:
                return result
        else:
            output_str = output

        # Check size
        if len(output_str) <= self.large_output_threshold:
            return result

        # Cache the full output
        cache = self._get_ephemeral_cache()
        cache_id = cache.store(output_str)
        preview = cache.generate_preview(output_str, max_chars=2000)

        # Return preview with cache ID
        cached_output = (
            f"{preview}\n\n"
            f"[Cache ID: {cache_id}] Use 'cache.get {cache_id}' to retrieve full content."
        )
        return (cached_output, tokens_in, tokens_out)

    def _get_model(self) -> str:
        """Get the model to use for the next LLM call.

        Implements rotation if configured:
        - round_robin: Cycles through models in order
        - random: Picks randomly from the pool
        - None: Uses the primary model
        """
        import random as rand

        # If no rotation pool, use primary model
        if not self.config.models or not self.config.rotation:
            return self.config.model

        if self.config.rotation == "round_robin":
            model = self.config.models[self._call_count % len(self.config.models)]
            self._call_count += 1
            return model
        elif self.config.rotation == "random":
            return rand.choice(self.config.models)
        else:
            return self.config.model

    async def execute(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[Any, int, int]:
        """Execute a tool and return (output, tokens_in, tokens_out).

        Routes to LLM for "llm.*" tools, otherwise uses MossToolExecutor.
        When memory is enabled, checks triggered memory and records episodes.
        """
        import time

        start_time = time.time()
        error_message: str | None = None

        # Check triggered memory before execution (for warnings/context)
        memory_warnings = await self._check_triggered_memory(tool_name, context, step)
        if memory_warnings:
            # Log warnings but continue execution
            # In a full implementation, these could be injected into the prompt
            pass

        try:
            if tool_name.startswith("llm.") or tool_name == "agent.step":
                result = await self._execute_llm(tool_name, context, step)
            elif tool_name.startswith("memory."):
                result = await self._execute_memory(tool_name, context, step)
            elif tool_name.startswith("cache."):
                result = await self._execute_cache(tool_name, context, step)
            else:
                result = await self.moss_executor.execute(tool_name, context, step)
        except Exception as e:
            error_message = str(e)
            # Record failure episode
            duration = int((time.time() - start_time) * 1000)
            await self._record_episode(
                tool_name, context, step, success=False, error=error_message, duration_ms=duration
            )
            raise

        # Record success episode
        duration = int((time.time() - start_time) * 1000)
        await self._record_episode(
            tool_name, context, step, success=True, error=None, duration_ms=duration
        )

        # Cache large outputs (not for cache.get operations to avoid recursion)
        if not tool_name.startswith("cache."):
            result = self._maybe_cache_output(result)

        return result

    async def _check_triggered_memory(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> list[str]:
        """Check triggered memory for warnings about this operation."""
        if not self.memory:
            return []

        try:
            from moss.memory import Action, StateSnapshot

            # Extract files from context
            files: list[str] = []
            if isinstance(context.input, dict):
                file_path = context.input.get("file_path")
                if file_path:
                    files.append(str(file_path))

            ctx_str = str(context.last)[:500] if context.last else ""
            state = StateSnapshot.create(files=files, context=ctx_str)
            target = files[0] if files else None
            action = Action.create(tool=tool_name, target=target, description=step.name)
            memory_ctx = await self.memory.get_context(state, action)
            return memory_ctx.warnings
        except Exception:
            return []

    async def _record_episode(
        self,
        tool_name: str,
        context: LoopContext,
        step: LoopStep,
        success: bool,
        error: str | None,
        duration_ms: int,
    ) -> None:
        """Record an episode for future memory retrieval."""
        if not self.memory:
            return

        try:
            from moss.memory import Action, Outcome, StateSnapshot

            # Extract files from context
            files: list[str] = []
            if isinstance(context.input, dict):
                file_path = context.input.get("file_path")
                if file_path:
                    files.append(str(file_path))

            ctx_str = str(context.last)[:500] if context.last else ""
            state = StateSnapshot.create(files=files, context=ctx_str)
            target = files[0] if files else None
            action = Action.create(tool=tool_name, target=target, description=step.name)
            outcome = Outcome.SUCCESS if success else Outcome.FAILURE

            await self.memory.record_episode(
                state=state,
                action=action,
                outcome=outcome,
                error_message=error,
                duration_ms=duration_ms,
            )
        except Exception:
            # Don't let memory errors break execution
            pass

    async def _execute_llm(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[Any, int, int]:
        """Execute an LLM-based tool via litellm.

        litellm provides unified access to all providers (Gemini, OpenAI, Anthropic, etc.)
        """
        # Extract the specific LLM operation
        operation = tool_name.split(".", 1)[1] if "." in tool_name else tool_name

        # Build the prompt based on operation and full context
        prompt = self._build_prompt(operation, context, step)

        # Extract repair context if validation errors are present
        repair_context = self._extract_repair_context(context)

        # Mock mode - return placeholder
        if self.config.mock:
            mock_response = f"[MOCK {operation}]: {str(context.last)[:100]}"
            tokens_in = len(prompt) // 4
            tokens_out = len(mock_response) // 4
            return mock_response, tokens_in, tokens_out

        return await self._call_litellm(prompt, repair_context)

    async def _execute_memory(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[Any, int, int]:
        """Execute a memory operation.

        Memory tools are non-LLM, so tokens are always (0, 0).

        Supported operations:
        - memory.recall: Query memory for relevant past experiences
        """
        operation = tool_name.split(".", 1)[1] if "." in tool_name else tool_name

        if not self.memory:
            return "Memory not configured.", 0, 0

        if operation == "recall":
            # Extract query from step input or context
            query = ""
            if step.input_from and context.get(step.input_from):
                query = str(context.get(step.input_from))
            elif context.last:
                query = str(context.last)
            else:
                query = step.name

            result = await self.memory.recall(query)
            return result, 0, 0
        else:
            return f"Unknown memory operation: {operation}", 0, 0

    async def _execute_cache(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[Any, int, int]:
        """Execute a cache operation.

        Cache tools are non-LLM, so tokens are always (0, 0).

        Supported operations:
        - cache.get <id>: Retrieve cached content by ID
        - cache.stats: Get cache statistics
        """
        operation = tool_name.split(".", 1)[1] if "." in tool_name else tool_name

        if operation == "get":
            # Extract cache ID from step input or context
            cache_id = ""
            if step.input_from and context.get(step.input_from):
                cache_id = str(context.get(step.input_from))
            elif context.last:
                cache_id = str(context.last)
            else:
                cache_id = step.name

            cache = self._get_ephemeral_cache()
            content = cache.get_content(cache_id.strip())
            if content is None:
                return f"Cache entry not found or expired: {cache_id}", 0, 0
            return content, 0, 0

        elif operation == "stats":
            cache = self._get_ephemeral_cache()
            stats = cache.stats()
            return str(stats), 0, 0

        else:
            return f"Unknown cache operation: {operation}", 0, 0

    async def _get_memory_context(self) -> str:
        """Get automatic memory context to inject into system prompt."""
        if not self.memory:
            return ""

        try:
            # Import here to avoid circular imports
            from moss.memory import Action, StateSnapshot

            # Create a generic state snapshot for automatic context
            state = StateSnapshot.create(files=[], context="automatic")
            action = Action.create(tool="llm", description="LLM call")
            memory_ctx = await self.memory.get_context(state, action)
            return memory_ctx.to_text()
        except Exception:
            # Don't let memory errors break LLM calls
            return ""

    def _extract_repair_context(self, context: LoopContext) -> str:
        """Extract repair context from validation errors in loop context.

        Checks for ValidationResult or DiagnosticSet in step outputs.
        Returns formatted error summary for the Syntax Repair Engine.
        """
        errors: list[str] = []

        # Check all step outputs for validation/diagnostic results
        for _, output in context.steps.items():
            if output is None:
                continue

            # Handle ValidationResult (from validators.py)
            if hasattr(output, "success") and hasattr(output, "issues"):
                if not output.success:
                    for issue in getattr(output, "issues", []):
                        if hasattr(issue, "severity"):
                            sev = issue.severity
                            # Check if it's an error severity
                            if hasattr(sev, "name") and sev.name == "ERROR":
                                errors.append(str(issue))
                            elif hasattr(sev, "value") and sev.value == 1:  # ERROR = 1
                                errors.append(str(issue))

            # Handle DiagnosticSet (from diagnostics.py)
            elif hasattr(output, "diagnostics") and hasattr(output, "error_count"):
                if output.error_count > 0:
                    for diag in getattr(output, "errors", []):
                        if hasattr(diag, "to_compact"):
                            errors.append(diag.to_compact())
                        else:
                            errors.append(str(diag))

            # Handle dict-based results (serialized validation)
            elif isinstance(output, dict):
                if output.get("success") is False:
                    for issue in output.get("issues", []):
                        if isinstance(issue, dict):
                            msg = issue.get("message", "")
                            loc = issue.get("file", "")
                            if issue.get("line"):
                                loc += f":{issue['line']}"
                            errors.append(f"{loc}: {msg}" if loc else msg)
                        else:
                            errors.append(str(issue))

        if not errors:
            return ""

        # Format errors for the repair engine
        error_list = "\n".join(f"- {e}" for e in errors[:10])  # Limit to 10
        if len(errors) > 10:
            error_list += f"\n... and {len(errors) - 10} more errors"

        return f"Errors to fix:\n{error_list}"

    async def _call_litellm(self, prompt: str, repair_context: str = "") -> tuple[str, int, int]:
        """Call LLM via litellm (unified interface for all providers)."""
        import asyncio

        try:
            from litellm import completion
        except ImportError as e:
            raise ImportError(
                "litellm required for LLM calls. Install with: pip install 'moss[llm]'"
            ) from e

        # Get model (may rotate if configured)
        model = self._get_model()

        # Get memory context (async, so do before sync call)
        memory_context = await self._get_memory_context()

        def _sync_call() -> tuple[str, int, int]:
            messages: list[dict[str, str]] = []

            # Build system prompt with memory context and repair engine
            system_prompt = self.config.get_system_prompt()
            if memory_context:
                system_prompt = f"{system_prompt}\n\n{memory_context}"
            if repair_context:
                # Inject Syntax Repair Engine when errors are present
                repair_prompt = get_repair_engine_prompt()
                system_prompt = f"{system_prompt}\n\n{repair_prompt}\n\n{repair_context}"

            if system_prompt:
                messages.append({"role": "system", "content": system_prompt})
            messages.append({"role": "user", "content": prompt})

            kwargs: dict[str, Any] = {
                "model": model,
                "messages": messages,
                "temperature": self.config.temperature,
            }
            if self.config.max_tokens is not None:
                kwargs["max_tokens"] = self.config.max_tokens

            response = completion(**kwargs)

            text = response.choices[0].message.content or ""
            tokens_in = response.usage.prompt_tokens if response.usage else 0
            tokens_out = response.usage.completion_tokens if response.usage else 0

            return text, tokens_in, tokens_out

        return await asyncio.to_thread(_sync_call)

    def _build_structured_context(self, context: LoopContext, step: LoopStep) -> str:
        """Build structured context summary following Goose's approach.

        Sections (inspired by Goose's summarize_oneshot.md):
        - User Intent: What the user wants to accomplish
        - Technical Context: Files, code structures, dependencies
        - Current Work: What step we're on, what's been done
        - Pending: What still needs to happen

        This structured format helps LLMs maintain coherent understanding
        across multi-step loops.
        """
        sections = []

        # User Intent - extract from original input
        if isinstance(context.input, dict):
            task = context.input.get("task", "")
            file_path = context.input.get("file_path", "")
            if task:
                sections.append(f"User Intent: {task}")
            if file_path:
                sections.append(f"Target: {file_path}")
        elif isinstance(context.input, str) and context.input:
            sections.append(f"User Intent: {context.input}")

        # Technical Context - summarize completed steps
        if context.steps:
            completed = []
            for name, output in context.steps.items():
                # Summarize each step's output concisely
                if isinstance(output, str):
                    # Truncate long outputs
                    summary = output[:200] + "..." if len(output) > 200 else output
                    # Single line
                    summary = summary.replace("\n", " ")[:100]
                elif isinstance(output, dict):
                    summary = ", ".join(f"{k}={v}" for k, v in list(output.items())[:3])
                elif isinstance(output, list):
                    summary = f"[{len(output)} items]"
                else:
                    summary = str(output)[:50]
                completed.append(f"  {name}: {summary}")
            if completed:
                sections.append("Completed Steps:\n" + "\n".join(completed))

        # Current Work - what step we're executing
        sections.append(f"Current Step: {step.name} (tool: {step.tool})")
        if step.input_from:
            sections.append(f"Input From: {step.input_from}")

        return "\n".join(sections)

    def _build_prompt(self, operation: str, context: LoopContext, step: LoopStep) -> str:
        """Build a prompt for the given LLM operation.

        Has access to:
        - context.input: Original task/file info
        - context.steps: All previous step outputs
        - context.last: Most recent step output
        - step.input_from: Which step's output to focus on

        Uses structured context sections for complex multi-step loops.
        """
        # Get the primary input (what this step should focus on)
        if step.input_from and step.input_from in context.steps:
            focus_input = context.steps[step.input_from]
        else:
            focus_input = context.last or context.input

        # Format the focus input
        if isinstance(focus_input, dict):
            focus_str = "\n".join(f"{k}: {v}" for k, v in focus_input.items())
        else:
            focus_str = str(focus_input)

        # Build structured context for complex operations
        structured_context = self._build_structured_context(context, step)

        # Operation-specific prompts
        # Simple operations get minimal context, complex ones get structured
        prompts = {
            "generate": (
                f"{structured_context}\n\nGenerate code based on the following:\n{focus_str}"
            ),
            "critique": (
                f"{structured_context}\n\n"
                f"Review and critique. Identify issues and suggest improvements:\n{focus_str}"
            ),
            "decide": (f"{structured_context}\n\nMake a decision based on:\n{focus_str}"),
            "needs_more_context": (
                f"Given this skeleton, do you need full implementation to make changes? "
                f"Answer YES or NO with brief explanation:\n\n{focus_str}"
            ),
            "add_docstrings": (
                f"Identify functions without docstrings. "
                f"For EACH missing docstring, output:\n"
                f"FUNC:function_name|One-line description\n\n"
                f"Only functions MISSING docstrings. One sentence each.\n\n"
                f"Skeleton:\n{focus_str}"
            ),
            "analyze": (f"{structured_context}\n\nAnalyze and provide insights:\n{focus_str}"),
            # Meta-loop operations for loop improvement
            "analyze_loop": (
                f"Analyze this loop definition. Identify:\n"
                f"- Missing error handling\n"
                f"- Potential infinite loops\n"
                f"- Inefficient step ordering\n"
                f"- Missing validation\n\n"
                f"Loop:\n{focus_str}"
            ),
            "suggest_improvements": (
                f"{structured_context}\n\n"
                f"Based on analysis, suggest specific improvements.\n"
                f"Format as before/after changes. Prioritize: safety > efficiency > clarity.\n\n"
                f"Analysis:\n{focus_str}"
            ),
            "estimate_tokens": (
                f"Estimate token usage for each step. Consider input/output size.\n"
                f"Output JSON: {{step_name: estimated_tokens}}\n\n"
                f"Loop:\n{focus_str}"
            ),
            "find_redundancy": (
                f"Identify token waste:\n"
                f"- Redundant LLM calls\n"
                f"- Steps producing constant output\n"
                f"- Duplicate validation\n\n"
                f"Token estimates:\n{focus_str}"
            ),
            "rewrite_loop": (
                f"Rewrite loop to reduce token usage. Apply identified optimizations.\n"
                f"Output complete optimized loop as YAML.\n\n"
                f"Current loop + waste analysis:\n{focus_str}"
            ),
            "critique_docstrings": (
                f"Critique these docstrings:\n"
                f"- Accurate?\n"
                f"- Explain 'why' not just 'what'?\n"
                f"- Args/returns documented?\n"
                f"Score 1-10, list issues.\n\n"
                f"Docstrings:\n{focus_str}"
            ),
            "improve_docstrings": (
                f"Improve docstrings based on critique. Address each issue.\n"
                f"Output improved docstrings.\n\n"
                f"Critique:\n{focus_str}"
            ),
            "detect_mistakes": (
                f"{structured_context}\n\n"
                f"Critically analyze the PREVIOUS turn for mistakes.\n"
                f"Identify:\n"
                f"- Logical errors or incorrect assumptions\n"
                f"- Hallucinated code or file content\n"
                f"- Safety violations or risky patterns\n"
                f"- Inefficient or redundant actions\n\n"
                f"Previous action & result:\n{focus_str}\n\n"
                f"Output a bulleted list of concerns, or 'No mistakes detected' "
                f"if it looks correct."
            ),
            "analyze_telemetry": (
                f"{structured_context}\n\n"
                f"Analyze the following telemetry data from agent sessions.\n"
                f"Identify patterns of:\n"
                f"- High token waste (large inputs/outputs that could be elided)\n"
                f"- Frequent failures or retries in specific steps\n"
                f"- Redundant tool calls\n"
                f"- Suboptimal model choices for specific tasks\n\n"
                f"Telemetry Data:\n{focus_str}\n\n"
                f"Output a structured analysis of bottlenecks and inefficiencies."
            ),
            "propose_optimizations": (
                f"{structured_context}\n\n"
                f"Based on the telemetry analysis, propose specific optimizations.\n"
                f"Consider:\n"
                f"- Merging or splitting steps\n"
                f"- Changing tool parameters (e.g. better filters)\n"
                f"- Switching models for specific operations\n"
                f"- Adding new heuristics or validators to catch early mistakes\n\n"
                f"Analysis:\n{focus_str}\n\n"
                f"Output a prioritized list of actionable optimizations."
            ),
        }

        default_prompt = f"{structured_context}\n\nProcess:\n{focus_str}"
        return prompts.get(operation, default_prompt)


async def run_simple_loop(file_path: str, patch_spec: dict[str, Any]) -> LoopResult:
    """Convenience function to run a simple edit loop.

    Args:
        file_path: Path to the file to edit
        patch_spec: Patch specification (anchor_name, content, etc.)

    Returns:
        LoopResult with metrics
    """
    executor = MossToolExecutor()
    runner = AgentLoopRunner(executor)
    loop = simple_loop()

    initial_input = {"file_path": file_path, "patch": patch_spec}
    return await runner.run(loop, initial_input)


# ============================================================================
# Benchmarking
# ============================================================================


@dataclass
class BenchmarkTask:
    """A single task for benchmarking."""

    name: str
    input_data: Any
    expected_success: bool = True


@dataclass
class BenchmarkResult:
    """Result of benchmarking a loop on multiple tasks."""

    loop_name: str
    tasks_run: int
    successes: int
    failures: int

    # Aggregate metrics (primary: LLM calls)
    total_llm_calls: int
    total_llm_tokens: int
    total_tool_calls: int
    total_time_seconds: float

    # Per-task results
    task_results: list[tuple[str, LoopResult]]

    @property
    def success_rate(self) -> float:
        return self.successes / self.tasks_run if self.tasks_run > 0 else 0.0

    @property
    def avg_llm_calls(self) -> float:
        return self.total_llm_calls / self.tasks_run if self.tasks_run > 0 else 0.0

    @property
    def avg_tool_calls(self) -> float:
        return self.total_tool_calls / self.tasks_run if self.tasks_run > 0 else 0.0

    def to_compact(self) -> str:
        """Format as compact summary."""
        return (
            f"{self.loop_name}: {self.success_rate:.0%} success "
            f"| LLM: {self.avg_llm_calls:.1f} calls/task "
            f"| Tools: {self.avg_tool_calls:.1f} calls/task "
            f"| Time: {self.total_time_seconds:.2f}s total"
        )

    def to_markdown(self) -> str:
        """Format as markdown report."""
        lines = [
            f"# Benchmark: {self.loop_name}",
            "",
            "## Summary",
            f"- Tasks: {self.tasks_run} ({self.successes} success, {self.failures} failed)",
            f"- Success rate: {self.success_rate:.1%}",
            "",
            "## LLM Usage (minimize this!)",
            f"- Total LLM calls: {self.total_llm_calls}",
            f"- Avg LLM calls/task: {self.avg_llm_calls:.2f}",
            f"- Total LLM tokens: {self.total_llm_tokens}",
            "",
            "## Tool Usage (cheap, prefer this)",
            f"- Total tool calls: {self.total_tool_calls}",
            f"- Avg tool calls/task: {self.avg_tool_calls:.2f}",
            "",
            "## Time",
            f"- Total: {self.total_time_seconds:.2f}s",
            f"- Avg per task: {self.total_time_seconds / self.tasks_run:.2f}s"
            if self.tasks_run > 0
            else "",
            "",
            "## Per-Task Results",
        ]

        for task_name, result in self.task_results:
            status = "✓" if result.success else "✗"
            lines.append(f"- {status} {task_name}: {result.metrics.to_compact()}")

        return "\n".join(lines)


class LoopBenchmark:
    """Benchmark loops against standard tasks.

    Primary metric: LLM calls per task (minimize).
    Secondary metrics: success rate, tool calls, time.
    """

    def __init__(self, executor: ToolExecutor | None = None):
        self.executor = executor or MossToolExecutor()

    async def run(self, loop: AgentLoop, tasks: list[BenchmarkTask]) -> BenchmarkResult:
        """Run a loop on multiple tasks, collecting metrics."""
        runner = AgentLoopRunner(self.executor)
        task_results: list[tuple[str, LoopResult]] = []

        total_llm_calls = 0
        total_llm_tokens = 0
        total_tool_calls = 0
        total_time = 0.0
        successes = 0
        failures = 0

        for task in tasks:
            result = await runner.run(loop, task.input_data)
            task_results.append((task.name, result))

            # Aggregate metrics
            total_llm_calls += result.metrics.llm_calls
            total_llm_tokens += result.metrics.llm_tokens_in + result.metrics.llm_tokens_out
            total_tool_calls += result.metrics.tool_calls
            total_time += result.metrics.wall_time_seconds

            if result.success:
                successes += 1
            else:
                failures += 1

        return BenchmarkResult(
            loop_name=loop.name,
            tasks_run=len(tasks),
            successes=successes,
            failures=failures,
            total_llm_calls=total_llm_calls,
            total_llm_tokens=total_llm_tokens,
            total_tool_calls=total_tool_calls,
            total_time_seconds=total_time,
            task_results=task_results,
        )

    async def compare(
        self, loops: list[AgentLoop], tasks: list[BenchmarkTask]
    ) -> list[BenchmarkResult]:
        """Compare multiple loops on the same tasks."""
        results = []
        for loop in loops:
            result = await self.run(loop, tasks)
            results.append(result)
        return results


def print_comparison(results: list[BenchmarkResult]) -> str:
    """Format comparison of multiple loop benchmarks."""
    lines = ["# Loop Comparison", ""]

    # Sort by LLM calls (primary metric)
    sorted_results = sorted(results, key=lambda r: r.avg_llm_calls)

    lines.append("| Loop | Success | Avg LLM Calls | Avg Tool Calls | Time |")
    lines.append("|------|---------|---------------|----------------|------|")

    for r in sorted_results:
        lines.append(
            f"| {r.loop_name} | {r.success_rate:.0%} | "
            f"{r.avg_llm_calls:.2f} | {r.avg_tool_calls:.2f} | "
            f"{r.total_time_seconds:.2f}s |"
        )

    lines.append("")
    lines.append(f"Winner (fewest LLM calls): **{sorted_results[0].loop_name}**")

    return "\n".join(lines)


@dataclass
class MultiModelBenchmarkResult:
    """Comparison of multiple models on standard tasks."""

    loop_name: str
    model_results: dict[str, BenchmarkResult]
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))

    def to_markdown(self) -> str:
        """Format comparison as markdown table."""
        lines = [f"# Model Benchmark: {self.loop_name}", ""]
        lines.append("| Model | Success | Avg LLM Calls | Avg Tokens | Time/Task |")
        lines.append("|-------|---------|---------------|------------|-----------|")

        # Sort models by success rate then LLM calls
        sorted_models = sorted(
            self.model_results.items(),
            key=lambda x: (x[1].success_rate, -x[1].avg_llm_calls),
            reverse=True,
        )

        for model, res in sorted_models:
            avg_tokens = res.total_llm_tokens / res.tasks_run if res.tasks_run > 0 else 0
            avg_time = res.total_time_seconds / res.tasks_run if res.tasks_run > 0 else 0
            lines.append(
                f"| {model} | {res.success_rate:.0%} | "
                f"{res.avg_llm_calls:.2f} | {avg_tokens:.0f} | {avg_time:.2f}s |"
            )

        return "\n".join(lines)


class AutomatedBenchmark:
    """Automates benchmarking across multiple models."""

    async def run_comparison(
        self,
        loop: AgentLoop,
        models: list[str],
        tasks: list[BenchmarkTask],
    ) -> MultiModelBenchmarkResult:
        """Run the benchmark for each model and aggregate results."""
        from moss.agent_loop import LLMConfig, LLMToolExecutor

        model_results = {}

        for model in models:
            # Create specialized executor for this model
            llm_config = LLMConfig(model=model)
            executor = LLMToolExecutor(config=llm_config)

            benchmark = LoopBenchmark(executor)
            model_results[model] = await benchmark.run(loop, tasks)

        return MultiModelBenchmarkResult(loop_name=loop.name, model_results=model_results)


# ============================================================================
# Loop Serialization (YAML/JSON)
# ============================================================================


def dump_loop_json(loop: AgentLoop, path: str | Path | None = None) -> str:
    """Serialize a loop to JSON.

    Args:
        loop: The loop to serialize
        path: Optional file path to write to

    Returns:
        JSON string representation
    """
    data = loop.to_dict()
    json_str = json.dumps(data, indent=2)
    if path:
        Path(path).write_text(json_str)
    return json_str


def load_loop_json(source: str | Path) -> AgentLoop:
    """Load a loop from JSON.

    Args:
        source: JSON string or path to JSON file

    Returns:
        AgentLoop instance
    """
    # If it's a Path object, read from file
    if isinstance(source, Path):
        data = json.loads(source.read_text())
    # If it's a string that looks like a path (no newlines, exists)
    elif "\n" not in source and len(source) < 500:
        path = Path(source)
        if path.exists():
            data = json.loads(path.read_text())
        else:
            data = json.loads(source)
    else:
        data = json.loads(source)
    return AgentLoop.from_dict(data)


def dump_loop_yaml(loop: AgentLoop, path: str | Path | None = None) -> str:
    """Serialize a loop to YAML.

    Requires PyYAML (optional dependency).

    Args:
        loop: The loop to serialize
        path: Optional file path to write to

    Returns:
        YAML string representation
    """
    try:
        import yaml
    except ImportError as e:
        raise ImportError("PyYAML required for YAML serialization: pip install pyyaml") from e

    data = loop.to_dict()
    yaml_str = yaml.dump(data, default_flow_style=False, sort_keys=False)
    if path:
        Path(path).write_text(yaml_str)
    return yaml_str


def load_loop_yaml(source: str | Path) -> AgentLoop:
    """Load a loop from YAML.

    Requires PyYAML (optional dependency).

    Args:
        source: YAML string or path to YAML file

    Returns:
        AgentLoop instance
    """
    try:
        import yaml
    except ImportError as e:
        raise ImportError("PyYAML required for YAML serialization: pip install pyyaml") from e

    # If it's a Path object, read from file
    if isinstance(source, Path):
        data = yaml.safe_load(source.read_text())
    # If it's a string that looks like a path (no newlines, exists)
    elif "\n" not in source and len(source) < 500:
        path = Path(source)
        if path.exists():
            data = yaml.safe_load(path.read_text())
        else:
            data = yaml.safe_load(source)
    else:
        data = yaml.safe_load(source)
    return AgentLoop.from_dict(data)
