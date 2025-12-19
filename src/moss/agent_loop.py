"""Composable Agent Loops: Define loops as data, execute with metrics.

Design principle: LLM calls are expensive. Structural tools are cheap.
Track them separately and optimize for fewer LLM calls.
"""

from __future__ import annotations

import time
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
    """

    input: Any = None
    steps: dict[str, Any] = field(default_factory=dict)
    last: Any = None

    def get(self, step_name: str, default: Any = None) -> Any:
        """Get a specific step's output."""
        return self.steps.get(step_name, default)

    def with_step(self, step_name: str, output: Any) -> LoopContext:
        """Return new context with step output added."""
        new_steps = dict(self.steps)
        new_steps[step_name] = output
        return LoopContext(input=self.input, steps=new_steps, last=output)


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

    Tools receive the full LoopContext and can access:
    - context.input: Original input (typically {file_path: ..., task: ...})
    - context.steps: Previous step outputs
    - context.get(step_name): Specific step output
    """

    def __init__(self, root: Any = None):
        from pathlib import Path

        from moss.moss_api import MossAPI

        self.api = MossAPI(root or Path.cwd())

    def _get_input(self, context: LoopContext, step: LoopStep) -> Any:
        """Extract the appropriate input for this step.

        Priority:
        1. If step.input_from is set, use that step's output
        2. Otherwise use context.input (original initial_input)
        """
        if step.input_from and step.input_from in context.steps:
            return context.steps[step.input_from]
        return context.input

    def _get_file_path(self, context: LoopContext, step: LoopStep) -> str:
        """Extract file_path from context - always available from initial input."""
        # Always get file_path from original input
        initial = context.input
        if isinstance(initial, dict) and "file_path" in initial:
            return initial["file_path"]
        if isinstance(initial, str):
            return initial
        raise ValueError(f"Cannot extract file_path from context.input: {initial}")

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

        elif tool_name == "validation.validate":
            file_path = self._get_file_path(context, step)
            result = self.api.validation.validate(file_path)

        elif tool_name == "patch.apply":
            file_path = self._get_file_path(context, step)
            if isinstance(input_data, dict):
                patch = input_data.get("patch", input_data)
            else:
                patch = input_data
            result = self.api.patch.apply(file_path, patch)

        elif tool_name == "patch.apply_with_fallback":
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
            file_path = self._get_file_path(context, step)
            name = input_data.get("name") if isinstance(input_data, dict) else input_data
            result = self.api.anchor.resolve(file_path, name)

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

        else:
            raise ValueError(f"Unknown tool: {tool_name}")

        # Handle async results
        if hasattr(result, "__await__"):
            result = await result

        # All MossAPI tools are non-LLM
        return result, 0, 0


# ============================================================================
# LLM Tool Executor
# ============================================================================


@dataclass
class LLMConfig:
    """Configuration for LLM calls via litellm.

    Uses litellm's unified interface for all providers. Model names follow
    litellm conventions (e.g., "gemini/gemini-2.0-flash", "gpt-4o").

    Attributes:
        model: Model name in litellm format (see litellm docs for all options)
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
    """

    model: str = "gemini/gemini-3-flash-preview"  # Default: latest flash model
    temperature: float = 0.0
    max_tokens: int | None = None  # Let the model determine output length
    system_prompt: str = (
        "Be terse. No preamble, no summary, no markdown formatting. "
        "Plain text only - no bold, no headers, no code blocks unless asked. "
        "For analysis: short bullet points, max 5 items, no code."
    )
    mock: bool = False  # Set True for testing without API calls


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

    Example:
        config = LLMConfig(model="gemini/gemini-2.0-flash")
        executor = LLMToolExecutor(config)

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
    ):
        """Initialize the executor.

        Args:
            config: LLM configuration (uses defaults if None)
            moss_executor: Executor for structural tools (created if None)
            root: Project root for MossToolExecutor
            load_env: Whether to load .env file (default: True)
        """
        # Load .env once per process
        if load_env and not LLMToolExecutor._dotenv_loaded:
            LLMToolExecutor._dotenv_loaded = _load_dotenv()

        self.config = config or LLMConfig()
        self.moss_executor = moss_executor or MossToolExecutor(root)

    async def execute(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[Any, int, int]:
        """Execute a tool and return (output, tokens_in, tokens_out).

        Routes to LLM for "llm.*" tools, otherwise uses MossToolExecutor.
        """
        if tool_name.startswith("llm."):
            return await self._execute_llm(tool_name, context, step)
        else:
            return await self.moss_executor.execute(tool_name, context, step)

    async def _execute_llm(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[Any, int, int]:
        """Execute an LLM-based tool.

        Uses google-genai for Gemini models. Falls back to litellm for others.
        """
        # Extract the specific LLM operation
        operation = tool_name.split(".", 1)[1] if "." in tool_name else tool_name

        # Build the prompt based on operation and full context
        prompt = self._build_prompt(operation, context, step)

        # Mock mode - return placeholder
        if self.config.mock:
            mock_response = f"[MOCK {operation}]: {str(context.last)[:100]}"
            tokens_in = len(prompt) // 4
            tokens_out = len(mock_response) // 4
            return mock_response, tokens_in, tokens_out

        # Route based on model prefix
        if self.config.model.startswith("gemini/"):
            return await self._call_gemini(prompt)
        else:
            return await self._call_litellm(prompt)

    async def _call_gemini(self, prompt: str) -> tuple[str, int, int]:
        """Call Gemini via google-genai SDK."""
        import asyncio

        try:
            from google import genai
        except ImportError as e:
            raise ImportError(
                "google-genai required for Gemini. Install with: pip install 'moss[gemini]'"
            ) from e

        def _sync_call() -> tuple[str, int, int]:
            client = genai.Client()  # Uses GOOGLE_API_KEY env var

            # Build config
            config: dict[str, Any] = {}
            if self.config.system_prompt:
                config["system_instruction"] = self.config.system_prompt
            if self.config.max_tokens:
                config["max_output_tokens"] = self.config.max_tokens
            if self.config.temperature > 0:
                config["temperature"] = self.config.temperature

            # Extract model name (remove "gemini/" prefix)
            model_name = self.config.model
            if model_name.startswith("gemini/"):
                model_name = model_name[7:]

            response = client.models.generate_content(
                model=model_name,
                contents=prompt,
                config=config if config else None,
            )

            text = response.text or ""
            tokens_in = 0
            tokens_out = 0
            if hasattr(response, "usage_metadata") and response.usage_metadata:
                tokens_in = getattr(response.usage_metadata, "prompt_token_count", 0) or 0
                tokens_out = getattr(response.usage_metadata, "candidates_token_count", 0) or 0

            return text, tokens_in, tokens_out

        return await asyncio.to_thread(_sync_call)

    async def _call_litellm(self, prompt: str) -> tuple[str, int, int]:
        """Call LLM via litellm (for non-Gemini models)."""
        import asyncio

        try:
            from litellm import completion
        except ImportError as e:
            raise ImportError(
                "litellm required for non-Gemini models. Install with: pip install 'moss[llm]'"
            ) from e

        def _sync_call() -> tuple[str, int, int]:
            messages: list[dict[str, str]] = []
            if self.config.system_prompt:
                messages.append({"role": "system", "content": self.config.system_prompt})
            messages.append({"role": "user", "content": prompt})

            kwargs: dict[str, Any] = {
                "model": self.config.model,
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

    def _build_prompt(self, operation: str, context: LoopContext, step: LoopStep) -> str:
        """Build a prompt for the given LLM operation.

        Has access to:
        - context.input: Original task/file info
        - context.steps: All previous step outputs
        - context.last: Most recent step output
        - step.input_from: Which step's output to focus on
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

        # Include original task context if available
        task_context = ""
        if isinstance(context.input, dict):
            task = context.input.get("task", "")
            file_path = context.input.get("file_path", "")
            if task:
                task_context = f"Task: {task}\n"
            if file_path:
                task_context += f"File: {file_path}\n"
        if task_context:
            task_context += "\n"

        prompts = {
            "generate": (
                f"{task_context}Generate code based on the following context:\n\n{focus_str}"
            ),
            "critique": (
                f"{task_context}Review and critique the following code. "
                f"Identify issues and suggest improvements:\n\n{focus_str}"
            ),
            "decide": (
                f"{task_context}Based on the following context, make a decision:\n\n{focus_str}"
            ),
            "needs_more_context": (
                f"{task_context}Given this code skeleton, do you need to see "
                f"the full implementation to make changes? "
                f"Answer YES or NO with brief explanation:\n\n{focus_str}"
            ),
            "add_docstrings": (
                f"Given this code skeleton, identify functions without docstrings. "
                f"For EACH function missing a docstring, output a line in this format:\n"
                f"FUNC:function_name|One-line description of what the function does\n\n"
                f"Only output functions that are MISSING docstrings. "
                f"Be concise - one short sentence per function.\n\n"
                f"Skeleton:\n{focus_str}"
            ),
            "analyze": (
                f"{task_context}Analyze the following and provide insights:\n\n{focus_str}"
            ),
        }

        default_prompt = f"{task_context}Process the following:\n\n{focus_str}"
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
