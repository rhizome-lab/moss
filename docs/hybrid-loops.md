# Hybrid Loops: Combining Multiple Tool Sources

Hybrid loops use `CompositeToolExecutor` to route tool calls to different backends based on prefix. This enables loops that combine local structural tools with external MCP servers and LLM calls.

Note: For declarative workflow definitions, see `moss workflow` which uses TOML files. This doc covers the low-level `AgentLoop` execution primitives.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    CompositeToolExecutor                     │
├─────────────────────────────────────────────────────────────┤
│  Prefix Routing:                                            │
│    "moss."  → MossToolExecutor (structural analysis)        │
│    "mcp."   → MCPToolExecutor (external MCP server)         │
│    "llm."   → LLMToolExecutor (LLM generation)              │
│    (no prefix) → default executor                           │
└─────────────────────────────────────────────────────────────┘
```

## Basic Usage

```python
from moss.agent_loop import (
    AgentLoop,
    AgentLoopRunner,
    CompositeToolExecutor,
    LLMToolExecutor,
    LoopStep,
    MossToolExecutor,
    StepType,
)

# Create individual executors
moss_executor = MossToolExecutor(root=project_root)
llm_executor = LLMToolExecutor(config=llm_config)

# Combine them with prefix routing
composite = CompositeToolExecutor(
    executors={
        "moss.": moss_executor,
        "llm.": llm_executor,
    },
    default=moss_executor,
)

# Define a loop using prefixed tool names
loop = AgentLoop(
    name="hybrid_analysis",
    steps=[
        LoopStep(name="view_file", tool="moss.skeleton.format"),
        LoopStep(name="analyze", tool="llm.analyze", input_from="view_file"),
    ],
    exit_conditions=["analyze.success"],
)

# Run
runner = AgentLoopRunner(composite)
result = await runner.run(loop, initial_input)
```

## With External MCP Server

```python
from moss.agent_loop import MCPServerConfig, MCPToolExecutor

# Configure MCP server
mcp_config = MCPServerConfig(
    command="npx",
    args=["@anthropic/mcp-server-filesystem"],
    cwd="/project",
)

# Create and connect MCP executor
mcp_executor = MCPToolExecutor(mcp_config)
await mcp_executor.connect()

# Add to composite
composite = CompositeToolExecutor(
    executors={
        "moss.": moss_executor,
        "mcp.": mcp_executor,
        "llm.": llm_executor,
    }
)

# Loop can now use MCP tools
loop = AgentLoop(
    name="with_mcp",
    steps=[
        LoopStep(name="read", tool="mcp.read_file"),
        LoopStep(name="analyze", tool="moss.skeleton.format", input_from="read"),
    ],
    ...
)

# Cleanup
await mcp_executor.disconnect()
```

## Prefix Stripping

The `CompositeToolExecutor` automatically strips the prefix before passing to the underlying executor:

| Loop Tool Name | Executor | Actual Tool Called |
|----------------|----------|-------------------|
| `moss.skeleton.format` | MossToolExecutor | `skeleton.format` | <!-- doc-check: ignore -->
| `mcp.read_file` | MCPToolExecutor | `read_file` |
| `llm.analyze` | LLMToolExecutor | `analyze` |

## Available MossAPI Tools

Tools available via `MossToolExecutor` (use with `moss.` prefix). These are internal Python wrappers that shell out to the Rust CLI. For direct CLI usage, prefer `moss view`, `moss edit`, `moss analyze`.

- `skeleton.format` - Extract file skeleton as text (wraps `moss view`)
- `skeleton.extract` - Extract skeleton as data structure
- `skeleton.expand` - Get full source of a symbol
- `validation.validate` - Run syntax and linting checks
- `patch.apply` - Apply a patch to a file
- `patch.apply_with_fallback` - Apply patch with text fallback
- `anchor.find` / `anchor.resolve` - Find code anchors
- `complexity.analyze` - Analyze code complexity

Note: The `skeleton.*` tools are historical from before the Rust rewrite. They exist for backwards compatibility with agent loops.

## Exit Conditions

Exit conditions use the format `{step_name}.success`. The loop exits successfully when the named step completes:

```python
loop = AgentLoop(
    name="example",
    steps=[...],
    exit_conditions=["final_step.success"],  # Exit when final_step succeeds
)
```

If no exit conditions are specified, the loop exits after all steps complete.

## Best Practices

1. **Use meaningful prefixes**: `moss.`, `mcp.`, `llm.` clearly indicate the tool source
2. **Set a default executor**: Handle unprefixed tools gracefully
3. **Connect MCP before running**: MCP executors require async connection
4. **Disconnect on cleanup**: Always disconnect MCP executors when done
5. **Use mock mode for testing**: Set `LLMConfig(mock=True)` for development

## Example: Full Hybrid Loop

See `examples/hybrid_loop.py` for a complete working example.
