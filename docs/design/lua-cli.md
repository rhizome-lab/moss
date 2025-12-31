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

## Open Questions

1. **Chaining vs separate calls?**
   ```lua
   -- Chaining (shown above)
   app:command("add", {...}):arg("text", {...})

   -- Separate (more explicit)
   local add = app:command("add", {...})
   add:arg("text", {...})
   ```

2. **Global options inheritance?**
   Should `--verbose` on the app apply to all subcommands automatically?

3. **Nested subcommands?**
   `moss @git remote add` - probably overkill for scripts

4. **Aliases?**
   `app:command("remove", { aliases = {"rm", "delete"} })`
