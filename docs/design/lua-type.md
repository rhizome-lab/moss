# Lua Type System Design

Declarative type schemas with validation and generation.

## Module Structure

```
type              -- Schema definitions (T.string, T.struct, etc.)
type.validate     -- Validation (check values against schemas)
type.generate     -- Generation (random values from schemas)
```

## Goals

1. Types as data (tables), not method chains
2. Composable via nesting
3. Clear error messages with field paths
4. Type coercion where sensible
5. Works with CLI library's parsed args

## Basic Usage

```lua
local T = require("type")
local validate = require("type.validate")

local schema = T.struct({
    name = T.string,
    count = { type = "number", min = 1, max = 100, default = 10 },
    verbose = { type = "boolean", default = false },
})

local result, err = validate.check(args, schema)
if err then
    print("Error: " .. err)
    os.exit(1)
end
```

### Primitive Types

```lua
T.string              -- { type = "string" }
T.number              -- { type = "number" }
T.integer             -- { type = "integer" }
T.boolean             -- { type = "boolean" }
T.any                 -- { type = "any" }
```

### Type with Constraints

```lua
-- Inline constraints
{ type = "string", min_len = 1, max_len = 100 }
{ type = "number", min = 0, max = 100 }
{ type = "integer", min = 1 }
{ type = "string", pattern = "^[a-z]+$" }
{ type = "string", one_of = { "red", "green", "blue" } }

-- Required/optional/default
{ type = "string", required = true }
{ type = "number", default = 10 }
```

### Composite Types

```lua
-- Struct (nested object)
{
    type = "struct",
    shape = {
        name = T.string,
        email = { type = "string", pattern = "@" },
    },
}

-- Array
{ type = "array", item = T.string }
{ type = "array", item = { type = "number", min = 0 } }

-- Optional wrapper
{ type = "optional", inner = T.string }
-- or shorthand:
T.optional(T.string)

-- Union
{ type = "any_of", types = { T.string, T.number } }
-- or shorthand:
T.any_of(T.string, T.number)

-- Literal value
{ type = "literal", value = "production" }
-- or shorthand:
T.literal("production")
```

### Shorthand Constructors

For common patterns:

```lua
T.optional(inner)           -- { type = "optional", inner = inner }
T.array(item)               -- { type = "array", item = item }
T.any_of(...)               -- { type = "any_of", types = {...} }
T.literal(value)            -- { type = "literal", value = value }
T.struct(shape)             -- { type = "struct", shape = shape }
```

### Built-in Validators

```lua
T.file_exists             -- { type = "string", file_exists = true }
T.dir_exists              -- { type = "string", dir_exists = true }
T.port                    -- { type = "integer", min = 1, max = 65535 }
T.positive                -- { type = "number", min = 0, exclusive_min = true }
T.non_empty_string        -- { type = "string", min_len = 1 }
```

### Custom Validation

```lua
{
    type = "string",
    check = function(v)
        if not v:match("^[a-z]+$") then
            return nil, "must be lowercase letters"
        end
        return v
    end,
}
```

### Full Example

```lua
local T = require("validate")

local schema = {
    name = { type = "string", required = true, min_len = 1 },
    port = T.port,
    mode = { type = "string", one_of = { "dev", "prod" }, default = "dev" },
    tags = { type = "array", item = T.non_empty_string, default = {} },
    config = {
        type = "struct",
        shape = {
            timeout = { type = "number", min = 0, default = 30 },
            retries = { type = "integer", min = 0, max = 10, default = 3 },
        },
    },
}

local result, err = T.check(args, schema)
```

### Error Messages

```
name: required field missing
port: must be between 1 and 65535, got 70000
mode: must be one of [dev, prod], got "test"
config.timeout: must be number, got string
tags[2]: must be non-empty string
```

## Implementation Notes

### Reference Implementation

See https://github.com/pterror/lua/blob/master/lib/type/check.lua

Dispatcher pattern:
```lua
mod.checkers = {}
mod.check = function(schema, x)
    return mod.checkers[schema.type](schema, x)
end
mod.checkers.string = function(_, x) return type(x) == "string" end
mod.checkers.struct = function(s, x)
    for k, s2 in pairs(s.shape) do
        if not mod.check(s2, x[k]) then return false end
    end
    return true
end
```

Our version extends this with:
- Return `(result, err)` not just boolean
- Constraint checking (min, max, pattern)
- Coercion (string → number)
- Default values
- Error paths ("config.timeout: must be number")

### No Normalization Needed

`T.string` is already `{ type = "string" }` - no runtime normalization.
Tables without `type` field are errors.

### Coercion

- String "123" → number 123 (for number/integer types)
- String "true"/"false" → boolean
- Coercion happens before constraint checking

### Required vs Optional

- Fields are **optional by default** (nil allowed)
- Use `required = true` for mandatory fields
- `default = value` provides fallback for nil

## Integration with CLI

```lua
local cli = require("cli")
local T = require("validate")

cli.run {
    name = "deploy",
    commands = {
        { name = "run", args = { "env", "port?" },
          run = function(args)
              local validated, err = T.check(args, {
                  env = { type = "string", one_of = { "dev", "staging", "prod" }, required = true },
                  port = T.port,
              })
              if err then
                  print("Error: " .. err)
                  os.exit(1)
              end
              deploy(validated.env, validated.port or 8080)
          end },
    },
}
```

## Design Decisions

1. **No implicit struct inference.** `{ name = T.string }` does NOT auto-become struct.
   Explicit > implicit. Use `T.struct({ name = T.string })` or `{ type = "struct", shape = ... }`.

2. **Shorthand constructors are plain functions.** `T.array(item)` just returns
   `{ type = "array", item = item }`. No magic, no metatables.

## Random Value Generation (`type.generate`)

The `generate` function produces random values matching a schema.
Useful for property-based testing, fuzzing, and mock data generation.

```lua
local T = require("type")
local generate = require("type.generate")

-- Generate a random string
local s = generate(T.string)  -- e.g., "kJ7xQm2"

-- Generate with constraints
local n = generate({ type = "integer", min = 1, max = 10 })  -- e.g., 7

-- Generate complex structures
local user = generate(T.struct({
    name = T.string,
    age = { type = "integer", min = 0, max = 120 },
    active = T.boolean,
}))
-- e.g., { name = "abc", age = 45, active = true }
```

### Options

```lua
generate(schema, {
    seed = 12345,        -- for reproducible output
    max_depth = 5,       -- limit nesting (default: 5)
    max_array_len = 10,  -- limit array size (default: 10)
})
```

### Supported Types

| Type | Behavior |
|------|----------|
| `string` | Random alphanumeric, respects `min_len`/`max_len`/`one_of` |
| `number` | Random float in `[min, max]` range (default: -1000 to 1000) |
| `integer` | Random int in `[min, max]` range |
| `boolean` | 50% true/false |
| `nil` | Always nil |
| `any` | Random string, number, or boolean |
| `struct` | Recursively generates each field (optional fields 30% nil) |
| `array` | Random length, generates each item |
| `tuple` | Generates each positional item |
| `dictionary` | Random key-value pairs |
| `optional` | 30% nil, 70% inner value |
| `any_of` | Picks random type from union |
| `all_of` | Generates from first type (best effort) |
| `literal` | Returns the literal value |

### Property Testing Pattern

```lua
local T = require("type")
local validate = require("type.validate")
local generate = require("type.generate")

local user_schema = T.struct({
    name = { type = "string", min_len = 1, required = true },
    age = { type = "integer", min = 0, max = 150 },
})

-- Generate many random values and verify they validate
for i = 1, 100 do
    local user = generate(user_schema, { seed = i })
    local result, err = validate.check(user, user_schema)
    assert(err == nil, "generated value should validate: " .. tostring(err))
end
```

## Open Questions

1. **Transform/coerce hook?**
   `{ type = "string", transform = string.lower }` to normalize values?

2. **LuaLS type annotations?**
   TBD. Could add later if valuable for IDE support.
