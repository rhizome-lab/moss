# Event System

Moss uses an async event bus for communication between components.

## Event Types

```python
from moss.events import EventType

EventType.USER_MESSAGE      # User input received
EventType.PLAN_GENERATED    # Planning complete
EventType.TOOL_CALL         # Tool invocation
EventType.VALIDATION_FAILED # Validation error
EventType.SHADOW_COMMIT     # Git checkpoint created
```

## Publishing Events

```python
from moss.events import Event, EventBus

bus = EventBus()

# Publish an event
await bus.publish(Event(
    type=EventType.TOOL_CALL,
    payload={"tool": "read", "path": "/src/main.py"}
))
```

## Subscribing to Events

```python
# Subscribe to specific event type
@bus.subscribe(EventType.TOOL_CALL)
async def on_tool_call(event: Event):
    print(f"Tool called: {event.payload}")

# Subscribe to all events
@bus.subscribe()
async def on_any_event(event: Event):
    print(f"Event: {event.type}")
```

## Synthesis Events

The synthesis framework emits its own events:

```python
SynthesisEventType.STRATEGY_SELECTED   # Strategy chosen
SynthesisEventType.DECOMPOSITION_COMPLETE  # Subproblems created
SynthesisEventType.SUBPROBLEM_START    # Starting subproblem
SynthesisEventType.SUBPROBLEM_COMPLETE # Subproblem solved
SynthesisEventType.VALIDATION_START    # Validation beginning
SynthesisEventType.VALIDATION_COMPLETE # Validation done
SynthesisEventType.COMPOSITION_START   # Composing solutions
```

## Event Payloads

Events carry typed payloads:

```python
# Tool call event
{
    "tool": "edit",
    "path": "/src/app.py",
    "operation": "replace",
    "line": 42
}

# Validation failed event
{
    "validator": "pytest",
    "errors": ["AssertionError in test_foo"],
    "file": "/tests/test_app.py"
}
```

## Event Persistence

Events can be persisted for replay and debugging:

```python
from moss.events import EventStore

store = EventStore(path="events.jsonl")
await store.append(event)

# Replay events
async for event in store.replay():
    await bus.publish(event)
```
