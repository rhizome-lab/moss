"""Composable Agent Loops: Define loops as data, execute with metrics.

Design principle: LLM calls are expensive. Structural tools are cheap.
Track them separately and optimize for fewer LLM calls.
"""

from __future__ import annotations

import time
from collections.abc import Callable
from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Any, Protocol, runtime_checkable


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
    on_error: ErrorAction = ErrorAction.ABORT
    goto_target: str | None = None  # For GOTO action
    max_retries: int = 3
    timeout_seconds: float | None = None

    def __post_init__(self) -> None:
        if self.on_error == ErrorAction.GOTO and not self.goto_target:
            raise ValueError("GOTO action requires goto_target")


@dataclass
class AgentLoop:
    """Composable agent loop definition.

    Loops are data - they describe what to do, not how to do it.
    The LoopRunner executes them.
    """

    name: str
    steps: list[LoopStep]
    entry: str | None = None  # Starting step (default: first)
    exit_conditions: list[str] = field(default_factory=list)

    # Resource limits
    max_iterations: int = 10
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
    """Protocol for executing tools."""

    async def execute(self, tool_name: str, input_data: Any) -> tuple[Any, int, int]:
        """Execute a tool and return (output, tokens_in, tokens_out).

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
            initial_input: Initial input for the first step

        Returns:
            LoopResult with final status and metrics
        """
        metrics = LoopMetrics()
        step_results: list[StepResult] = []
        step_outputs: dict[str, Any] = {}
        current_input = initial_input

        start_time = time.time()

        # Build step lookup
        step_map = {s.name: s for s in loop.steps}
        step_order = [s.name for s in loop.steps]

        current_step_name = loop.entry
        iteration = 0

        while iteration < loop.max_iterations:
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

            # Get input for this step
            if step.input_from and step.input_from in step_outputs:
                current_input = step_outputs[step.input_from]

            # Execute step with retries
            step_result = await self._execute_step(step, current_input, metrics)
            step_results.append(step_result)

            if step_result.status == StepStatus.SUCCESS:
                step_outputs[step.name] = step_result.output

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
            error=f"Max iterations ({loop.max_iterations}) reached",
        )

    async def _execute_step(
        self, step: LoopStep, input_data: Any, metrics: LoopMetrics
    ) -> StepResult:
        """Execute a single step with retry logic."""
        retries = 0
        last_error: str | None = None

        while retries <= step.max_retries:
            start = time.time()
            try:
                output, tokens_in, tokens_out = await self.executor.execute(step.tool, input_data)
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
    """Simple linear loop: understand → act → validate."""
    return AgentLoop(
        name=name,
        steps=[
            LoopStep("understand", "skeleton.format", step_type=StepType.TOOL),
            LoopStep("act", "patch.apply", input_from="understand", step_type=StepType.TOOL),
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


# ============================================================================
# MossAPI Tool Executor
# ============================================================================


class MossToolExecutor:
    """Execute tools via MossAPI.

    Maps tool names to MossAPI methods. Non-LLM tools return (0, 0) for tokens.
    """

    def __init__(self, root: Any = None):
        from pathlib import Path

        from moss.moss_api import MossAPI

        self.api = MossAPI(root or Path.cwd())
        self._tool_map = self._build_tool_map()

    def _build_tool_map(self) -> dict[str, Callable[..., Any]]:
        """Build mapping from tool names to API methods."""
        return {
            # Skeleton
            "skeleton.format": lambda input: self.api.skeleton.format(input["file_path"]),
            "skeleton.extract": lambda input: self.api.skeleton.extract(input["file_path"]),
            # Validation
            "validation.validate": lambda input: self.api.validation.validate(
                input.get("file_path", input)
            ),
            # Patch
            "patch.apply": lambda input: self.api.patch.apply(input["file_path"], input["patch"]),
            "patch.apply_with_fallback": lambda input: self.api.patch.apply_with_fallback(
                input["file_path"], input["patch"]
            ),
            # Anchors
            "anchor.find": lambda input: self.api.anchor.find(input["file_path"], input["name"]),
            "anchor.resolve": lambda input: self.api.anchor.resolve(
                input["file_path"], input["name"]
            ),
            # Dependencies
            "dependencies.format": lambda input: self.api.dependencies.format(input["file_path"]),
            # DWIM
            "dwim.analyze_intent": lambda input: self.api.dwim.analyze_intent(
                input if isinstance(input, str) else input["query"]
            ),
            # Health
            "health.check": lambda _: self.api.health.check(),
            "health.summarize": lambda _: self.api.health.summarize(),
            # Complexity
            "complexity.analyze": lambda input: self.api.complexity.analyze(
                input.get("pattern", "**/*.py") if isinstance(input, dict) else "**/*.py"
            ),
        }

    async def execute(self, tool_name: str, input_data: Any) -> tuple[Any, int, int]:
        """Execute a tool and return (output, tokens_in, tokens_out).

        All MossAPI tools are non-LLM, so tokens are always (0, 0).
        """
        if tool_name not in self._tool_map:
            raise ValueError(f"Unknown tool: {tool_name}")

        func = self._tool_map[tool_name]
        result = func(input_data)

        # Handle async results
        if hasattr(result, "__await__"):
            result = await result

        # All MossAPI tools are non-LLM
        return result, 0, 0


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
