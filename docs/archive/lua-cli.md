# Lua CLI Library Design

Opinionated CLI parsing for moss scripts.

## Goals

1. Minimal boilerplate for common patterns
2. Auto-generated help text
3. Subcommand support (like @todo add/done/rm)
4. Type coercion (string → number/boolean)
5. Required vs optional args
6. Validation with clear errors

## API Sketch

### Declarative Style

Arrays for ordering, keys for clarity:

```lua
local cli = require("cli")

cli.run {
    name = "todo",
    description = "TODO list manager",

    commands = {
        { name = "list", description = "List items", default = true,
          run = function(args) print("Listing...") end },

        { name = "add", description = "Add a new item", args = { "text..." },
          run = function(args) print("Adding: " .. args.text) end },

        { name = "done", description = "Mark item as done", args = { "query" },
          run = function(args) print("Done: " .. args.query) end },
    },
}
```

### Simple Script (No Subcommands)

```lua
cli.run {
    name = "greet",
    description = "Greet someone",

    args = { "name" },
    options = {
        { name = "loud", short = "l", description = "Shout it" },
        { name = "times", short = "n", description = "Repeat count", default = 1 },
    },

    run = function(args)
        local msg = "Hello, " .. args.name
        if args.loud then msg = msg:upper() .. "!" end
        for i = 1, args.times do print(msg) end
    end,
}
```

### Subcommands

Nested commands (e.g., `moss @git remote add`):

```lua
cli.run {
    name = "git",
    description = "Git helper",

    commands = {
        { name = "status", description = "Show status",
          run = function(args) ... end },

        { name = "remote", description = "Manage remotes",
          commands = {
              { name = "list", description = "List remotes", default = true,
                run = function(args) ... end },

              { name = "add", description = "Add a remote",
                args = { "name", "url" },
                run = function(args)
                    print("Adding " .. args.name .. " -> " .. args.url)
                end },

              { name = "remove", description = "Remove a remote",
                args = { "name" },
                run = function(args) ... end },
          }},
    },
}
```

Usage:
```
moss @git remote add origin https://...
moss @git remote list
moss @git remote           # runs 'list' (default)
```

### Handlers

Handlers live with their command definition - everything in one place:

```lua
commands = {
    { name = "add", description = "Add an item",
      args = { "text..." },
      options = {
          { name = "priority", short = "p", description = "Set priority" },
      },
      run = function(args)
          -- args.text = collected positional args
          -- args.priority = option value or nil
          add_item(args.text, args.priority)
      end },
}
```

For complex handlers, define function above and reference:

```lua
local function handle_add(args)
    -- ... lots of logic ...
end

local function handle_remove(args)
    -- ... lots of logic ...
end

cli.run {
    name = "todo",
    commands = {
        { name = "add", description = "Add item", args = { "text..." }, run = handle_add },
        { name = "rm", description = "Remove item", args = { "query" }, run = handle_remove },
    },
}
```

### Syntax Reference

```lua
-- Positional args: array of strings (order matters)
args = { "file" }           -- required
args = { "file?" }          -- optional
args = { "files..." }       -- rest (collects remaining)
args = { "src", "dst?" }    -- src required, dst optional

-- Options: array of tables (order matters for help text)
options = {
    { name = "verbose", short = "v", description = "Verbose output" },
    { name = "output", short = "o", description = "Output file", default = "out.txt" },
    { name = "count", short = "n", description = "Repeat count", type = "number" },
}

-- Commands: array of tables (order matters for help text)
commands = {
    { name = "list", description = "List items", default = true, run = fn },
    { name = "add", description = "Add item", args = {...}, options = {...}, run = fn },
    { name = "remote", description = "Manage remotes", commands = {...} },  -- nested
}

-- A command can have both run + commands (run is fallback when no subcommand)
-- Prefer `default = true` on a subcommand if you want it visible in help
{ name = "remote", run = fallback_fn, commands = {...} }
```

### Generated Help

```
$ moss @todo --help
todo - TODO list manager

Usage: moss @todo <command> [options]

Commands:
  add <text>     Add a new item
  done <query>   Mark item as done
  list           List items (default)

Options:
  -h, --help     Show this help
  --version      Show version

$ moss @todo add --help
todo add - Add a new item

Usage: moss @todo add <text>

Arguments:
  text    Item text (can be multiple words)

Options:
  -h, --help    Show this help
```

### Type Coercion

```lua
app:option("count", {
    short = "n",
    type = "number",  -- auto-converts, errors if not a number
    default = 10,
})

app:flag("dry-run", {
    type = "boolean",  -- default for flags
})

app:option("tags", {
    type = "list",  -- comma-separated → table
})
```

### Validation

```lua
app:option("level", {
    type = "number",
    validate = function(v)
        if v < 1 or v > 5 then
            return nil, "must be between 1 and 5"
        end
        return v
    end,
})

app:arg("file", {
    validate = function(v)
        if not file_exists(v) then
            return nil, "file not found: " .. v
        end
        return v
    end,
})
```

### Error Handling

```
$ moss @myapp --count abc
Error: --count: expected number, got 'abc'

$ moss @myapp --level 10
Error: --level: must be between 1 and 5

$ moss @myapp
Error: missing required argument: file

Run 'moss @myapp --help' for usage.
```

## Implementation Notes

### Where to Put It

Option A: Builtin Lua module (like `ts`, `view`, `edit`)
- Pro: Always available, no require path issues
- Con: More code in moss binary

Option B: Bundled .lua file loaded via require
- Pro: Pure Lua, easy to modify
- Con: Need to handle module loading

Recommendation: **Option A** - builtin module. CLI parsing is fundamental enough to warrant builtin status.

### Template Integration

New template: `moss script new foo --template cli`

```lua
local cli = require("cli")

local app = cli.app {
    name = "{name}",
    description = "Description of {name}",
}

app:command("list", {
    description = "List items",
    default = true,
    run = function(args)
        print("TODO: implement list")
    end,
})

app:command("add", {
    description = "Add an item",
    run = function(args)
        print("TODO: implement add: " .. args.text)
    end,
})
:arg("text", { description = "Item text", rest = true })

app:run()
```

## Implemented Features

All open questions have been resolved and implemented:

### Global Options Inheritance

App-level options are inherited by all subcommands:

```lua
cli.run {
    name = "app",
    options = {
        { name = "verbose", short = "v", description = "Verbose output" },
    },
    commands = {
        { name = "build", run = function(args)
            if args.verbose then print("Building...") end
        end },
    },
}
```

Usage: `moss @app --verbose build` or `moss @app build --verbose`

### Command Aliases

Commands can have multiple names:

```lua
{ name = "remove", aliases = {"rm", "delete"}, ... }
```

### Type Coercion

Options can specify type for automatic conversion:

```lua
options = {
    { name = "count", type = "number" },    -- string → number
    { name = "port", type = "integer" },    -- string → integer (must be whole)
    { name = "force", type = "boolean" },   -- "true"/"1" → true, "false"/"0" → false
}
```

### Environment Variable Fallbacks

Options can specify an environment variable to use as default:

```lua
{ name = "port", type = "integer", env = "PORT" }
```

### Required Options

Options can be marked as required (only enforced in strict mode):

```lua
{ name = "output", short = "o", required = true }
```

### Mutually Exclusive Options

Options can conflict with each other (only enforced in strict mode):

```lua
options = {
    { name = "json", conflicts_with = "text" },
    { name = "text", conflicts_with = "json" },
}
```

## Config Flags

These are opt-in behaviors, disabled by default:

```lua
cli.run {
    name = "app",
    bundling = true,   -- enable -abc → -a -b -c
    negatable = true,  -- enable --no-* for all flags
    strict = true,     -- enable validation (required args/options, conflicts)
    ...
}
```

### Short Option Bundling (`bundling = true`)

When enabled, combined short flags expand:
- `-abc` → `-a -b -c`
- `-vvv` → `-v -v -v`

Only works for flags (options without values).

### Negatable Flags (`negatable = true`)

When enabled globally, all flags can be negated with `--no-` prefix:
- `--verbose` → sets `verbose = true`
- `--no-verbose` → sets `verbose = false`

Individual options can also opt-in/out:
```lua
{ name = "color", negatable = true }   -- allows --no-color even without global flag
{ name = "force", negatable = false }  -- prevents --no-force even with global flag
```

### Strict Mode (`strict = true`)

Enables validation errors for:
- Missing required positional arguments
- Missing required options
- Mutually exclusive option conflicts

Without strict mode, these issues are silently ignored (useful for lenient parsing).
