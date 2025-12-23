"""Driver architecture for unified task execution.

A Driver decides what action to take next for a task. Different drivers
implement different decision-making strategies:

- UserDriver: waits for human input (interactive sessions)
- LLMDriver: asks an LLM to decide (agent automation)
- WorkflowDriver: follows predefined steps (sequential automation)
- StateMachineDriver: follows state transitions (conditional automation)

See docs/driver-architecture.md for design details.
"""

from __future__ import annotations

from abc import abstractmethod
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, ClassVar, Protocol, runtime_checkable

if TYPE_CHECKING:
    from .session import Session


# =============================================================================
# Core Types
# =============================================================================


@dataclass
class Action:
    """An action to execute."""

    tool: str
    parameters: dict[str, Any] = field(default_factory=dict)

    def __str__(self) -> str:
        if self.parameters:
            params = ", ".join(f"{k}={v!r}" for k, v in self.parameters.items())
            return f"{self.tool}({params})"
        return self.tool


@dataclass
class ActionResult:
    """Result of an action execution."""

    success: bool
    output: Any = None
    error: str | None = None


@dataclass
class Context:
    """Execution context for decision-making."""

    task: Session
    history: list[tuple[Action, ActionResult]] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def last_result(self) -> ActionResult | None:
        """Get the result of the last action."""
        if self.history:
            return self.history[-1][1]
        return None

    def last_error(self) -> str | None:
        """Get the error from the last action, if any."""
        result = self.last_result()
        return result.error if result else None


# =============================================================================
# Driver Protocol
# =============================================================================


@runtime_checkable
class Driver(Protocol):
    """Protocol for task drivers.

    A driver decides what action to take next during task execution.
    It receives the current task and context, and returns either an
    Action to execute, or None to indicate the task is complete.
    """

    name: str

    @abstractmethod
    async def decide_next_step(
        self,
        task: Session,
        context: Context,
    ) -> Action | None:
        """Decide what to do next.

        Args:
            task: The current task/session
            context: Execution context with history

        Returns:
            Action to execute, or None if task is complete.
        """
        ...

    async def on_action_complete(
        self,
        task: Session,
        action: Action,
        result: ActionResult,
    ) -> None:
        """Called after an action completes.

        Override to update internal state based on action results.
        Default implementation does nothing.
        """
        pass


# =============================================================================
# Driver Registry
# =============================================================================


class DriverRegistry:
    """Registry for driver plugins.

    Supports multiple discovery mechanisms:
    1. Built-in drivers (registered at module load)
    2. Entry points: packages can register via 'moss.drivers' entry point
    3. Programmatic registration via register()
    """

    _drivers: ClassVar[dict[str, type]] = {}
    _discovered: ClassVar[bool] = False

    @classmethod
    def register(cls, driver_class: type) -> None:
        """Register a driver class."""
        name = getattr(driver_class, "name", driver_class.__name__)
        cls._drivers[name] = driver_class

    @classmethod
    def get(cls, name: str) -> type | None:
        """Get a driver class by name."""
        cls._ensure_discovered()
        return cls._drivers.get(name)

    @classmethod
    def create(cls, name: str, **config: Any) -> Driver:
        """Create a driver instance by name with configuration."""
        driver_class = cls.get(name)
        if driver_class is None:
            raise ValueError(f"Unknown driver: {name}")
        return driver_class(**config)

    @classmethod
    def list_drivers(cls) -> list[str]:
        """List all registered driver names."""
        cls._ensure_discovered()
        return list(cls._drivers.keys())

    @classmethod
    def _ensure_discovered(cls) -> None:
        """Discover drivers from entry points (lazy, once)."""
        if cls._discovered:
            return
        cls._discovered = True
        cls._discover_entry_points()

    @classmethod
    def _discover_entry_points(cls) -> None:
        """Discover drivers from installed packages via entry points."""
        try:
            from importlib.metadata import entry_points

            eps = entry_points()
            if hasattr(eps, "select"):
                driver_eps = eps.select(group="moss.drivers")
            else:
                driver_eps = eps.get("moss.drivers", [])

            for ep in driver_eps:
                try:
                    driver_class = ep.load()
                    cls.register(driver_class)
                except (ImportError, AttributeError, TypeError):
                    pass
        except ImportError:
            pass


# =============================================================================
# Generic Execution Loop
# =============================================================================


async def run_task(
    task: Session,
    driver: Driver,
    *,
    max_iterations: int = 1000,
    on_action: Any | None = None,
) -> Context:
    """Execute a task using the given driver.

    Args:
        task: The task/session to execute
        driver: The driver that decides actions
        max_iterations: Safety limit on iterations
        on_action: Optional callback(action, result) for each action

    Returns:
        The final context with full history
    """
    context = Context(task=task)

    task.start()
    try:
        for _ in range(max_iterations):
            action = await driver.decide_next_step(task, context)
            if action is None:
                break

            result = await execute_action(action, task)
            context.history.append((action, result))

            await driver.on_action_complete(task, action, result)

            if on_action:
                on_action(action, result)

        task.complete()
    except Exception as e:
        task.fail(str(e))
        raise

    return context


async def execute_action(action: Action, task: Session) -> ActionResult:
    """Execute an action and return the result.

    This is the central action dispatcher. It routes actions to the
    appropriate tool implementation.
    """
    try:
        # Route to appropriate handler based on tool name
        if action.tool == "view":
            from moss_intelligence.core_api import ViewAPI

            api = ViewAPI(task.project_root)
            result = api.view(**action.parameters)
            return ActionResult(success=True, output=result)

        elif action.tool == "edit":
            from moss_intelligence.edit import EditContext, edit

            context = EditContext(
                project_root=task.project_root,
                target_file=action.parameters.get("target_file"),
                target_symbol=action.parameters.get("target_symbol"),
            )
            result = await edit(action.parameters.get("task", ""), context)
            return ActionResult(success=result.success, output=result, error=result.error)

        elif action.tool == "analyze":
            from moss_intelligence.core_api import AnalyzeAPI

            api = AnalyzeAPI(task.project_root)
            result = api.analyze(**action.parameters)
            return ActionResult(success=True, output=result)

        elif action.tool == "shell":
            import asyncio

            cmd = action.parameters.get("command", "")
            proc = await asyncio.create_subprocess_shell(
                cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                cwd=task.project_root,
            )
            stdout, stderr = await proc.communicate()
            success = proc.returncode == 0
            output = stdout.decode() if stdout else ""
            error = stderr.decode() if stderr and not success else None
            return ActionResult(success=success, output=output, error=error)

        elif action.tool == "done":
            # Special action to signal completion
            return ActionResult(success=True, output="Task marked complete")

        else:
            return ActionResult(
                success=False,
                error=f"Unknown tool: {action.tool}",
            )

    except Exception as e:
        return ActionResult(success=False, error=str(e))


# =============================================================================
# Built-in Drivers
# =============================================================================


class UserDriver:
    """Driver that waits for human input.

    Used for interactive sessions where the user provides commands
    through TUI or CLI.
    """

    name = "user"

    def __init__(self, prompt_callback: Any | None = None):
        """Initialize UserDriver.

        Args:
            prompt_callback: Async function that prompts user and returns command string.
                            If None, the driver will return None (completing the task).
        """
        self._prompt = prompt_callback

    async def decide_next_step(
        self,
        task: Session,
        context: Context,
    ) -> Action | None:
        if self._prompt is None:
            return None

        command = await self._prompt(task, context)
        if command is None or command.strip().lower() in ("done", "exit", "quit"):
            return None

        return self._parse_command(command)

    async def on_action_complete(
        self,
        task: Session,
        action: Action,
        result: ActionResult,
    ) -> None:
        pass

    def _parse_command(self, command: str) -> Action:
        """Parse a user command into an Action."""
        import shlex

        try:
            parts = shlex.split(command)
        except ValueError:
            parts = command.split()

        if not parts:
            return Action(tool="done")

        tool = parts[0].lower()
        params: dict[str, Any] = {}

        if tool in ("view", "v"):
            params["target"] = parts[1] if len(parts) > 1 else None
        elif tool in ("edit", "e"):
            params["target"] = parts[1] if len(parts) > 1 else None
            params["task"] = " ".join(parts[2:]) if len(parts) > 2 else None
        elif tool in ("analyze", "a"):
            params["target"] = parts[1] if len(parts) > 1 else "."
        elif tool == "shell":
            params["command"] = " ".join(parts[1:])
        else:
            # Unknown command, treat as done
            return Action(tool="done")

        return Action(tool=tool, parameters=params)


class LLMDriver:
    """Driver that asks an LLM to decide the next step.

    Used for autonomous agent execution where the LLM analyzes
    the current state and decides what to do next.
    """

    name = "llm"

    def __init__(
        self,
        model: str = "claude-sonnet-4-20250514",
        system_prompt: str | None = None,
        max_tokens: int = 4096,
    ):
        self.model = model
        self.system_prompt = system_prompt or self._default_system_prompt()
        self.max_tokens = max_tokens
        self._messages: list[dict] = []

    async def decide_next_step(
        self,
        task: Session,
        context: Context,
    ) -> Action | None:
        from moss_llm import complete

        # Build prompt from task and context
        user_prompt = self._format_prompt(task, context)
        self._messages.append({"role": "user", "content": user_prompt})

        response = await complete(
            model=self.model,
            messages=self._messages,
            system=self.system_prompt,
            max_tokens=self.max_tokens,
        )

        self._messages.append({"role": "assistant", "content": response})
        return self._parse_response(response)

    async def on_action_complete(
        self,
        task: Session,
        action: Action,
        result: ActionResult,
    ) -> None:
        # Add result to conversation for next iteration
        result_msg = f"Action completed: {action}\n"
        if result.success:
            result_msg += f"Output: {result.output}"
        else:
            result_msg += f"Error: {result.error}"
        self._messages.append({"role": "user", "content": result_msg})

    def _default_system_prompt(self) -> str:
        return """You are an AI assistant helping with code tasks.

Analyze the current state and decide the next action. Respond with a single action in this format:
TOOL: <tool_name>
PARAMS: <json_params>

Available tools:
- view: View file or symbol (params: {"target": "path/to/file"})
- edit: Edit code (params: {"target": "file", "task": "description"})
- analyze: Analyze code (params: {"target": "path"})
- shell: Run shell command (params: {"command": "..."})
- done: Complete the task (no params)

If the task is complete, respond with:
TOOL: done"""

    def _format_prompt(self, task: Session, context: Context) -> str:
        prompt = f"Task: {task.description or task.id}\n\n"

        if context.history:
            prompt += "Recent actions:\n"
            for action, result in context.history[-5:]:
                status = "OK" if result.success else f"ERROR: {result.error}"
                prompt += f"  - {action}: {status}\n"
            prompt += "\n"

        prompt += "What should be the next action?"
        return prompt

    def _parse_response(self, response: str) -> Action | None:
        import json
        import re

        # Parse TOOL: and PARAMS: format
        tool_match = re.search(r"TOOL:\s*(\w+)", response, re.IGNORECASE)
        if not tool_match:
            return Action(tool="done")

        tool = tool_match.group(1).lower()
        if tool == "done":
            return None

        params: dict[str, Any] = {}
        params_match = re.search(r"PARAMS:\s*(\{.*\})", response, re.IGNORECASE | re.DOTALL)
        if params_match:
            try:
                params = json.loads(params_match.group(1))
            except json.JSONDecodeError:
                pass

        return Action(tool=tool, parameters=params)


@dataclass
class WorkflowStep:
    """A step in a workflow."""

    tool: str
    parameters: dict[str, Any] = field(default_factory=dict)
    condition: str | None = None  # Optional condition expression

    def to_action(self) -> Action:
        return Action(tool=self.tool, parameters=self.parameters)


class WorkflowDriver:
    """Driver that follows predefined steps.

    Used for sequential automation where the steps are known ahead of time.
    """

    name = "workflow"

    def __init__(self, steps: list[WorkflowStep] | list[dict]):
        self.steps = [s if isinstance(s, WorkflowStep) else WorkflowStep(**s) for s in steps]
        self.current_index = 0

    async def decide_next_step(
        self,
        task: Session,
        context: Context,
    ) -> Action | None:
        while self.current_index < len(self.steps):
            step = self.steps[self.current_index]
            self.current_index += 1

            # Check condition if present
            if step.condition:
                if not self._evaluate_condition(step.condition, context):
                    continue

            return step.to_action()

        return None

    async def on_action_complete(
        self,
        task: Session,
        action: Action,
        result: ActionResult,
    ) -> None:
        pass

    def _evaluate_condition(self, condition: str, context: Context) -> bool:
        """Evaluate a simple condition expression."""
        # Simple condition evaluation: last_success, last_error, etc.
        if condition == "last_success":
            result = context.last_result()
            return result is not None and result.success
        elif condition == "last_error":
            return context.last_error() is not None
        elif condition.startswith("not "):
            return not self._evaluate_condition(condition[4:], context)
        return True


@dataclass
class StateTransition:
    """A transition between states."""

    next_state: str
    condition: str = "always"  # Condition expression


@dataclass
class WorkflowState:
    """A state in a state machine."""

    name: str
    action: Action | None = None
    transitions: list[StateTransition] = field(default_factory=list)
    terminal: bool = False


class StateMachineDriver:
    """Driver that follows state transitions.

    Used for conditional automation where the next action depends
    on the current state and conditions.
    """

    name = "state_machine"

    def __init__(
        self,
        states: dict[str, WorkflowState] | list[dict],
        initial: str,
    ):
        if isinstance(states, list):
            self.states = {s["name"]: WorkflowState(**s) for s in states}
        else:
            self.states = states
        self.current_state = initial

    async def decide_next_step(
        self,
        task: Session,
        context: Context,
    ) -> Action | None:
        state = self.states.get(self.current_state)
        if state is None or state.terminal:
            return None

        return state.action

    async def on_action_complete(
        self,
        task: Session,
        action: Action,
        result: ActionResult,
    ) -> None:
        # Determine next state based on transitions
        state = self.states.get(self.current_state)
        if state is None:
            return

        for transition in state.transitions:
            if self._evaluate_condition(transition.condition, result):
                self.current_state = transition.next_state
                break

    def _evaluate_condition(self, condition: str, result: ActionResult) -> bool:
        """Evaluate a transition condition."""
        if condition == "always":
            return True
        elif condition == "success":
            return result.success
        elif condition == "error":
            return not result.success
        return True


# =============================================================================
# Register Built-in Drivers
# =============================================================================

DriverRegistry.register(UserDriver)
DriverRegistry.register(LLMDriver)
DriverRegistry.register(WorkflowDriver)
DriverRegistry.register(StateMachineDriver)


__all__ = [
    "Action",
    "ActionResult",
    "Context",
    "Driver",
    "DriverRegistry",
    "LLMDriver",
    "StateMachineDriver",
    "StateTransition",
    "UserDriver",
    "WorkflowDriver",
    "WorkflowState",
    "WorkflowStep",
    "execute_action",
    "run_task",
]
