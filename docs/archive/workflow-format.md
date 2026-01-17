# Lua Workflows

Moss uses Lua (LuaJIT) for workflow scripting. Lua was chosen over TOML because once you need conditionals, you're writing code anyway.

## Why Lua?

- **LuaJIT**: ~200KB, extremely fast, minimal overhead
- **Simple syntax**: `view("foo.rs")` is almost as simple as TOML's `view: foo.rs`
- **Real language**: Loops, conditionals, functions when you need them
- **Battle-tested**: Games, nginx, redis, neovim all use Lua for scripting

## Directory Structure

```
.moss/
  workflows/
    test.lua           # Custom workflow
    deploy.lua         # Another workflow
    moss.lua           # LuaCats type definitions (auto-generated)
```

Run with: `moss workflow run test`

## Available Functions

### Moss Commands

```lua
-- view(target) or view{target=..., depth=..., deps=..., context=...}
view("src/main.rs")
view{target="MyClass", depth=2, deps=true}

-- analyze{target=..., health=..., complexity=...}
analyze{health=true}
analyze{target="src/", complexity=true}

-- grep(pattern) or grep{pattern=..., path=..., type=...}
grep("TODO")
grep{pattern="fn.*new", type="rust"}

-- Simple commands
edit("src/foo.rs")
index()
lint()
```

### Helpers

```lua
-- Run shell command
local result = shell("cargo build")
print(result.output)
if result.success then ... end

-- Git helpers
if is_dirty() then
    shell("git add -A && git commit -m 'wip'")
end

-- Test runner
if tests_pass() then
    print("All tests pass!")
end

-- File operations
if file_exists("Cargo.toml") then
    local content = read_file("Cargo.toml")
end

-- Output
print("Hello", "world")  -- prints "Hello	world"
```

### Interactive (Coroutine-based)

```lua
-- Prompt user for text input
local name = prompt("Enter your name:")

-- Show menu, get selection
local choice = menu({"option1", "option2", "quit"})
```

### Shadow Git

```lua
-- Track file changes at hunk level
local snap = shadow.open()      -- Open/init shadow git, get HEAD
shadow.snapshot({"src/foo.rs"}) -- Snapshot current state of files
local hunks = shadow.hunks()    -- Get hunks since snapshot
shadow.restore(snap)            -- Restore to snapshot
```

## Drivers

### auto{} - LLM Agent Loop

Requires `--features llm` at build time.

```lua
auto{
    model = "claude-sonnet-4-20250514",
    prompt = "Fix all TODO comments in src/",
    max_steps = 10
}
```

The LLM can call moss commands via text format: `> view src/main.rs`

### manual{} - Interactive Menu

```lua
manual{
    actions = {
        build = function() return shell("cargo build") end,
        test = function() return shell("cargo test") end,
        check = function() return analyze{health=true} end,
    }
}
```

Presents a menu of actions, loops until user selects "quit".

## Example Workflow

```lua
-- .moss/workflows/validate.lua
-- Run validation and fix errors

print("Checking codebase health...")
local health = analyze{health=true}
print(health)

if is_dirty() then
    print("Uncommitted changes detected")
    local choice = menu({"commit", "stash", "continue"})
    if choice == "commit" then
        shell("git add -A && git commit -m 'wip'")
    elseif choice == "stash" then
        shell("git stash")
    end
end

print("Running tests...")
if tests_pass() then
    print("All tests pass!")
else
    print("Tests failed, launching auto-fix...")
    auto{
        prompt = "Fix the failing tests",
        max_steps = 5
    }
end
```

## Type Definitions

For IDE support, moss generates LuaCats type definitions:

```lua
-- .moss/workflows/moss.lua (auto-generated)
---@class ViewOpts
---@field target? string
---@field depth? integer
---@field deps? boolean
---@field context? boolean

---@param opts string|ViewOpts
---@return CommandResult
function view(opts) end
```

## Configuration

Workflow-related config in `.moss/config.toml`:

```toml
[workflow]
# Default workflow directory
directory = ".moss/workflows"
```
