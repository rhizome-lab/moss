# Driver Architecture

Unified execution model where all task automation flows through pluggable drivers.

## Core Concept

A **Driver** decides what action to take next for a task. Different drivers implement different decision-making strategies:

- **UserDriver** - Waits for human input (interactive sessions)
- **LLMDriver** - Asks an LLM to decide (agent automation)
- **WorkflowDriver** - Follows predefined steps (sequential automation)
- **StateMachineDriver** - Follows state transitions (conditional automation)
- **Custom drivers** - Plugins register their own

## Protocol

```python
from typing import Protocol, Any
from moss.session import Task

class Action:
    """An action to execute."""
    tool: str
    parameters: dict[str, Any]

class Context:
    """Execution context for decision-making."""
    task: Task
    history: list[Action]  # Previous actions
    codebase: Any  # Access to code intelligence

class Driver(Protocol):
    """Protocol for task drivers."""

    name: str  # Driver identifier

    async def decide_next_step(
        self,
        task: Task,
        context: Context
    ) -> Action | None:
        """Decide what to do next.

        Returns:
            Action to execute, or None if task is complete.
        """
        ...

    async def on_action_complete(
        self,
        task: Task,
        action: Action,
        result: Any,
        error: str | None,
    ) -> None:
        """Called after an action completes. Update internal state."""
        ...
```

## Generic Execution Loop

```python
async def run_task(task: Task, driver: Driver) -> None:
    """Execute a task using the given driver."""
    context = Context(task=task, history=[], codebase=get_codebase())

    task.start()
    try:
        while True:
            action = await driver.decide_next_step(task, context)
            if action is None:
                break  # Driver says we're done

            result, error = await execute_action(action)
            context.history.append(action)

            await driver.on_action_complete(task, action, result, error)

        task.complete()
    except Exception as e:
        task.fail(str(e))
```

## Built-in Drivers

### UserDriver

Waits for user input via TUI or CLI.

```python
class UserDriver:
    name = "user"

    async def decide_next_step(self, task, context) -> Action | None:
        # Show prompt, wait for user command
        command = await self.prompt_user(task)
        if command is None or command == "done":
            return None
        return parse_command(command)
```

### LLMDriver

Asks an LLM to decide the next step.

```python
class LLMDriver:
    name = "llm"

    def __init__(self, model: str = "claude-sonnet"):
        self.model = model
        self.system_prompt = "..."

    async def decide_next_step(self, task, context) -> Action | None:
        prompt = self.format_prompt(task, context)
        response = await llm_complete(self.model, prompt)
        return parse_llm_response(response)
```

### WorkflowDriver

Follows predefined steps from a workflow definition.

```python
class WorkflowDriver:
    name = "workflow"

    def __init__(self, steps: list[WorkflowStep]):
        self.steps = steps
        self.current_index = 0

    async def decide_next_step(self, task, context) -> Action | None:
        if self.current_index >= len(self.steps):
            return None
        step = self.steps[self.current_index]
        self.current_index += 1
        return step.to_action()
```

### StateMachineDriver

Follows state transitions based on conditions.

```python
class StateMachineDriver:
    name = "state_machine"

    def __init__(self, states: dict[str, WorkflowState], initial: str):
        self.states = states
        self.current_state = initial

    async def decide_next_step(self, task, context) -> Action | None:
        state = self.states[self.current_state]
        if state.terminal:
            return None

        # Execute state action
        action = state.to_action()

        # Determine next state
        for transition in state.transitions:
            if await transition.condition(context):
                self.current_state = transition.next
                break

        return action
```

## Driver Registry

```python
class DriverRegistry:
    """Registry for driver plugins."""

    _drivers: dict[str, type[Driver]] = {}

    @classmethod
    def register(cls, driver_class: type[Driver]) -> None:
        cls._drivers[driver_class.name] = driver_class

    @classmethod
    def get(cls, name: str) -> type[Driver] | None:
        return cls._drivers.get(name)

    @classmethod
    def create(cls, name: str, **config) -> Driver:
        driver_class = cls._drivers[name]
        return driver_class(**config)

# Built-in registration
DriverRegistry.register(UserDriver)
DriverRegistry.register(LLMDriver)
DriverRegistry.register(WorkflowDriver)
DriverRegistry.register(StateMachineDriver)
```

## Plugin Drivers

External packages can register drivers via entry points:

```toml
# pyproject.toml
[project.entry-points."moss.drivers"]
my_driver = "my_package:MyCustomDriver"
```

Or programmatically:

```python
from moss.drivers import DriverRegistry

class MyCustomDriver:
    name = "custom"

    async def decide_next_step(self, task, context):
        # Custom decision logic
        ...

DriverRegistry.register(MyCustomDriver)
```

## Task Model Integration

The Task (Session) stores the driver name, not the driver instance:

```python
@dataclass
class Task:
    id: str
    driver: str = "user"  # Driver name, not enum
    driver_config: dict = field(default_factory=dict)  # Driver-specific config
    ...
```

When resuming a task:

```python
task = manager.get(task_id)
driver = DriverRegistry.create(task.driver, **task.driver_config)
await run_task(task, driver)
```

## Benefits

1. **Unified execution** - One loop for all automation types
2. **Pluggable** - Custom drivers without modifying core
3. **Composable** - Drivers can delegate to other drivers
4. **Resumable** - Driver state can be serialized with task
5. **Observable** - All drivers emit same events

## Migration Path

1. Keep existing `agent_loop`, `step_loop`, `state_machine_loop` working
2. Implement drivers that wrap existing loops
3. Gradually migrate callers to use driver API
4. Deprecate direct loop functions

## Open Questions

- Should drivers be async iterators instead of `decide_next_step`?
- How to handle driver state serialization for resumption?
- Should there be a `CompositeDriver` for chaining/fallback?
- How do drivers interact with shadow git branches?
