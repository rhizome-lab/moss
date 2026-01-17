# Lua API Reference

Rust bindings exposed to Lua workflows. See `crates/moss/src/workflow/lua_runtime/`.

## Stability

APIs are marked with stability levels:

- **Stable** - Safe to depend on. Breaking changes require deprecation.
- **Experimental** - Works but interface may change between releases.
- **Internal** - Implementation details. Do not use in user scripts.

### What's Stable

| Category | Stable | Experimental | Internal |
|----------|--------|--------------|----------|
| Moss commands | `view`, `analyze`, `grep` | `edit`, `index`, `lint` | - |
| Helpers | `shell`, `read_file`, `write_file`, `file_exists`, `print` | `is_dirty`, `tests_pass` | - |
| Memory | `store`, `recall`, `forget` | - | - |
| Shadow | `shadow.*` | `shadow.worktree.*` | - |
| Tree-sitter | `ts.parse`, node methods | - | - |
| LLM | `llm.complete`, `llm.chat` | - | - |
| Agent | - | `agent.run`, `agent.classify_task` | submodules |
| Globals | `_moss_root` | `args`, `task` | - |

### Agent Submodules (Internal)

The `agent.*` submodules are implementation details:

- `agent.parser` - Command parsing, JSON encode/decode
- `agent.session` - Checkpoints, logs, session management
- `agent.context` - Context builders for LLM prompts
- `agent.risk` - Risk assessment and validators
- `agent.commands` - Batch edit execution
- `agent.roles` - Role prompts and state machine config

These are internal to `agent.lua` and may change without notice.

## Moss Commands (Stable)

Wrappers around moss CLI commands, run as subprocesses.

| Function | Stability | Description |
|----------|-----------|-------------|
| `view(opts)` | Stable | View code structure. opts: `{target, deps, context, depth}` |
| `analyze(opts)` | Stable | Analyze code. opts: `{health, complexity, target}` |
| `grep(opts)` | Stable | Search code. opts: `{pattern, path, file_type}` |
| `edit(arg?)` | Experimental | Edit files |
| `index(arg?)` | Experimental | Manage index |
| `lint(arg?)` | Experimental | Run linter |
| `plans(arg?)` | Experimental | Manage plans |
| `sessions(arg?)` | Experimental | View sessions |

## Helpers (Stable)

| Function | Stability | Description |
|----------|-----------|-------------|
| `shell(cmd)` | Stable | Execute shell command. Returns `{output, success}` |
| `file_exists(path)` | Stable | Check if path exists relative to project root |
| `read_file(path)` | Stable | Read file contents as string |
| `write_file(path, content)` | Stable | Write string to file |
| `print(...)` | Stable | Print values to stdout |
| `is_dirty()` | Experimental | Check if git working tree has uncommitted changes |
| `tests_pass()` | Experimental | Run `cargo test --quiet`, return success boolean |

## Memory Store (Stable)

Semantic memory with vector search (SQLite + embeddings).

| Function | Description |
|----------|-------------|
| `store(content, opts?)` | Store content. opts: `{context, weight, metadata}` |
| `recall(query, limit?)` | Search by similarity. Returns `{id, content, context, similarity}[]` |
| `forget(id)` | Delete entry by ID |

## Shadow Git (Stable)

Lightweight snapshots for rollback without polluting git history.

| Function | Stability | Description |
|----------|-----------|-------------|
| `shadow.open()` | Stable | Initialize, returns current HEAD commit |
| `shadow.snapshot(files)` | Stable | Create snapshot of file list, returns snapshot ID |
| `shadow.hunks()` | Stable | Get current uncommitted hunks |
| `shadow.hunks_since(id)` | Stable | Get hunks since snapshot ID |
| `shadow.restore(id, files?)` | Stable | Restore snapshot (optionally only specific files) |
| `shadow.head()` | Stable | Get current HEAD commit |
| `shadow.list()` | Stable | List all snapshots as `{id, message}[]` |

Hunk structure: `{id, file, old_start, old_lines, new_start, new_lines, header, content, is_deletion, deletion_ratio}`

### Shadow Worktree (Experimental)

Isolated editing environment for validation before applying changes.

| Function | Description |
|----------|-------------|
| `shadow.worktree.open()` | Create/open worktree, returns path |
| `shadow.worktree.sync()` | Reset worktree to HEAD |
| `shadow.worktree.edit(path, content)` | Edit file in worktree |
| `shadow.worktree.read(path)` | Read file from worktree |
| `shadow.worktree.validate(cmd)` | Run validation command. Returns `{success, stdout, stderr, exit_code}` |
| `shadow.worktree.diff()` | Get diff of changes |
| `shadow.worktree.apply()` | Apply changes to real repo, returns file list |
| `shadow.worktree.reset()` | Discard changes |
| `shadow.worktree.modified()` | List modified file paths |
| `shadow.worktree.enable()` | Route `edit()` through shadow worktree |
| `shadow.worktree.disable()` | Stop routing through shadow |
| `shadow.worktree.enabled()` | Check if shadow edit mode is active |

## Tree-sitter (Stable)

| Function | Description |
|----------|-------------|
| `ts.parse(source, grammar)` | Parse source string with grammar name, returns tree userdata |

Tree methods:
- `tree:root()` - Get root node

Node methods:
- `:kind()` - Node type as string
- `:text()` - Source text for this node
- `:start_row()`, `:end_row()` - Line numbers (1-indexed)
- `:child_count()` - Number of children
- `:child(i)` - Get child by index (1-indexed)
- `:children()` - All children as table
- `:named_children()` - Named children only
- `:child_by_field(name)` - Get child by field name
- `:is_named()` - Whether node is named vs anonymous
- `:parent()` - Parent node or nil
- `:next_sibling()`, `:prev_sibling()` - Sibling navigation

## LLM (Stable)

Requires `--features llm` build.

| Function | Description |
|----------|-------------|
| `llm.complete(provider?, model?, system?, prompt)` | Single-turn completion |
| `llm.chat(provider, model, system?, prompt, history)` | Multi-turn chat with history |

Arguments:
- `provider`: "anthropic", "openai", "gemini", "openrouter", etc.
- `model`: Model name, or nil for provider default
- `system`: System prompt, or nil
- `prompt`: User prompt (required)
- `history`: Table of `{role, content}` messages (for chat)

Example:
```lua
-- Single completion
local response = llm.complete(nil, nil, "You are helpful.", "What is 2+2?")

-- Multi-turn chat
local history = {
    {role = "user", content = "Hello"},
    {role = "assistant", content = "Hi there!"}
}
local response = llm.chat("anthropic", nil, nil, "How are you?", history)
```

## Agent (Experimental)

| Function | Description |
|----------|-------------|
| `auto(config)` | Run autonomous agent loop. config: `{model, prompt, max_turns}` |

## Preloaded Modules (Stable)

Available via `require()`:

| Module | Stability | Description |
|--------|-----------|-------------|
| `cli` | Stable | CLI argument parsing |
| `type` | Stable | Type definitions and schemas |
| `type.describe` | Stable | Generate descriptions from types |
| `type.validate` | Stable | Validate values against types |
| `type.generate` | Stable | Generate test values from types |
| `test` | Stable | Testing utilities |
| `test.property` | Stable | Property-based testing |
| `agent` | Internal | Agent implementation (use CLI instead) |
| `agent.*` | Internal | Agent submodules (parser, session, context, risk, commands, roles) |

## Globals

| Name | Stability | Description |
|------|-----------|-------------|
| `_moss_root` | Stable | Project root path as string |
| `args` | Experimental | Command-line arguments table (1-indexed) |
| `task` | Experimental | Task description from `--task` flag |
