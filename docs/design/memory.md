# Memory System Design

## Philosophy

Memory is associations between contexts and knowledge. Not layers, not categories - just a graph you query.

## Core API

```lua
store(content, opts)   -- add knowledge
recall(query)          -- retrieve relevant knowledge
forget(query)          -- remove knowledge (optional)
```

That's the entire memory API.

### store(content, opts)

```lua
store("User prefers tabs", {
  context = "formatting",     -- what this relates to
  weight = 1.0,               -- importance (default 1.0)
  slot = "preferences",       -- hint for template placement
})

store("Last edit broke tests", {
  context = "auth.py",
  weight = 0.8,
})
```

### recall(query)

```lua
local results = recall("about auth.py")
-- Returns list of {content, weight, slot, ...} ordered by relevance

local prefs = recall({slot = "preferences"})
-- Can query by properties too
```

### Ordering

Recall returns results ordered by:
- Relevance to query (semantic similarity or pattern match)
- Weight (explicit importance)
- Recency (recently stored/accessed)
- Specificity (narrow context beats broad)

No manual ordering. The graph properties determine order.

## Sources

Sources are Lua modules that populate memory via `store()`. Not a special API - just code that runs and calls store.

### Builtin sources

```lua
-- .moss/sources/session.lua
-- Remembers current session automatically
moss.on("file_opened", function(file)
  store("Working on " .. file, {context = file, weight = 0.5})
end)
```

```lua
-- .moss/sources/git.lua
-- Learn from git history
for commit in git.log({limit = 100}) do
  if commit.message:match("fix") then
    store(commit.message, {
      context = commit.files,
      weight = 0.7,
    })
  end
end
```

```lua
-- .moss/sources/toml.lua
-- Read static knowledge from TOML
local data = toml.parse(".moss/memory.toml")
for _, item in ipairs(data.memory or {}) do
  store(item.content, {
    context = item.context,
    slot = item.slot,
    weight = item.weight or 1.0,
  })
end
```

### User-defined sources

```lua
-- .moss/sources/jira.lua
local issues = jira.fetch(config.project)
for _, issue in ipairs(issues) do
  store(issue.summary, {context = issue.key})
end
```

Just Lua. No plugin registration API needed.

## Templates

Templates are prompt construction - separate from memory.

### Simple (just strings)

```lua
local ctx = recall("about " .. file)
local prompt = "Context:\n" .. ctx .. "\n\nTask: " .. task
auto{prompt = prompt}
```

### With structure

```lua
local prompt = template(
  "<system>\n", recall({slot = "system"}), "\n</system>\n",
  "<context>\n", recall("about " .. file), "\n</context>\n",
  "<task>\n", task, "\n</task>"
)
```

`template()` is just a helper that concatenates, handling nil/empty values gracefully.

### TOML-configured template

If you want declarative template config, a source can read it:

```lua
-- .moss/sources/toml_template.lua
local cfg = toml.parse(".moss/config.toml")
if cfg.memory and cfg.memory.template then
  -- Store template structure for later use
  _G.prompt_template = cfg.memory.template
  _G.default_slot = cfg.memory.default_slot or "context"
end
```

```toml
# .moss/config.toml
[memory]
template = [
  "<system>\n",
  { slot = "system" },
  "\n</system>\n",
  "<context>\n",
  { slot = "context" },
  "\n</context>"
]
default_slot = "context"
```

But this is just convention, not core functionality.

## Slots

Slots are hints for template placement, stored as metadata on knowledge:

```lua
store("Be concise", {slot = "system"})
store("User prefers tabs", {slot = "preferences"})
```

Templates query by slot:

```lua
recall({slot = "system"})      -- get system prompt pieces
recall({slot = "preferences"}) -- get user preferences
```

Slots have no special meaning to the memory system. They're just queryable metadata.

## Persistence

Memory persists to `.moss/memory.db` (SQLite) or similar.

- `store()` writes to DB
- `recall()` queries DB
- Sources run at startup to populate

Session-scoped vs persistent is just a flag:

```lua
store("Currently editing auth.py", {
  context = "session",
  persist = false,  -- gone after session ends
})
```

## What This Replaces

The old "three-layer" design (automatic, triggered, on-demand) is replaced by:

| Old concept | New equivalent |
|-------------|----------------|
| Automatic layer | High-weight items in "system" slot, recalled at prompt start |
| Triggered layer | Items with specific context, recalled when that context is active |
| On-demand layer | Agent calls `recall()` explicitly |

No layers. Just one graph, one query operation, different query patterns.

## Non-Goals

- **No inheritance/patching**: Declare what you want, don't extend
- **No special TOML support**: TOML reading is a Lua source like any other
- **No template DSL**: Templates are Lua string construction
- **No ordering API**: Ordering emerges from graph properties
