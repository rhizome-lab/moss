# moss script

Run Lua scripts with moss bindings.

## Usage

```bash
moss script <PATH>
moss <PATH>           # Direct invocation for .lua files
moss @<script-name>   # Run from .moss/scripts/
```

## Examples

```bash
# Run a script
moss script analyze.lua

# Direct invocation
moss ./my-script.lua

# Named script from .moss/scripts/
moss @todo list
moss @cleanup
```

## Script Location

Scripts are searched in:
1. Direct path (if provided)
2. `.moss/scripts/` directory
3. `~/.moss/scripts/` (global)

## Lua Bindings

Scripts have access to:

```lua
-- File operations
moss.read(path)
moss.write(path, content)
moss.glob(pattern)
moss.grep(pattern, path)

-- Tree-sitter
moss.parse(path)
moss.skeleton(path)

-- Subprocess
moss.exec(cmd, args)

-- Output
moss.print(...)
moss.json(value)
```

See `docs/scripting.md` for full API.
