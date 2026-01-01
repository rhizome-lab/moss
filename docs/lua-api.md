# Lua API Reference

Rust bindings exposed to Lua workflows. See `crates/moss/src/workflow/lua_runtime.rs`.

## Moss Commands

Wrappers around moss CLI commands, run as subprocesses.

| Function | Description |
|----------|-------------|
| `view(opts)` | View code structure. opts: `{target, deps, context, depth}` |
| `analyze(opts)` | Analyze code. opts: `{health, complexity, target}` |
| `grep(opts)` | Search code. opts: `{pattern, path, file_type}` |
| `edit(arg?)` | Edit files |
| `index(arg?)` | Manage index |
| `lint(arg?)` | Run linter |
| `plans(arg?)` | Manage plans |
| `sessions(arg?)` | View sessions |

## Helpers

| Function | Description |
|----------|-------------|
| `shell(cmd)` | Execute shell command. Returns `{output, success}` |
| `is_dirty()` | Check if git working tree has uncommitted changes |
| `tests_pass()` | Run `cargo test --quiet`, return success boolean |
| `file_exists(path)` | Check if path exists relative to project root |
| `read_file(path)` | Read file contents as string |
| `write_file(path, content)` | Write string to file |
| `print(...)` | Print values to stdout |

## Memory Store

Semantic memory with vector search (SQLite + embeddings).

| Function | Description |
|----------|-------------|
| `store(content, opts?)` | Store content. opts: `{context, weight, metadata}` |
| `recall(query, limit?)` | Search by similarity. Returns `{id, content, context, similarity}[]` |
| `forget(id)` | Delete entry by ID |

## Shadow Git

Lightweight snapshots for rollback without polluting git history.

| Function | Description |
|----------|-------------|
| `shadow.open()` | Initialize, returns current HEAD commit |
| `shadow.snapshot(files)` | Create snapshot of file list, returns snapshot ID |
| `shadow.hunks()` | Get current uncommitted hunks |
| `shadow.hunks_since(id)` | Get hunks since snapshot ID |
| `shadow.restore(id, files?)` | Restore snapshot (optionally only specific files) |
| `shadow.head()` | Get current HEAD commit |
| `shadow.list()` | List all snapshots as `{id, message}[]` |

Hunk structure: `{id, file, old_start, old_lines, new_start, new_lines, header, content, is_deletion, deletion_ratio}`

## Tree-sitter

| Function | Description |
|----------|-------------|
| `ts.parse(source, grammar)` | Parse source string with grammar name, returns tree userdata |

Tree userdata has `:root()` method returning a node. Nodes have: `:kind()`, `:text()`, `:start_row()`, `:end_row()`, `:child_count()`, `:child(i)`, `:children()`.

## Agent

| Function | Description |
|----------|-------------|
| `auto(config)` | Run autonomous agent loop. config: `{model, prompt, max_turns}` |

## Preloaded Modules

Available via `require()`:

- `cli` - CLI argument parsing
- `type` - Type definitions and schemas
- `type.describe` - Generate descriptions from types
- `type.validate` - Validate values against types
- `type.generate` - Generate test values from types
- `test` - Testing utilities
- `test.property` - Property-based testing

## Globals

| Name | Description |
|------|-------------|
| `_moss_root` | Project root path as string |
