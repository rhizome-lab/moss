-- Type definitions for declarative schemas
-- Usage: local T = require("type")

local M = {}

-- Primitive type constants

M.string = { type = "string" }
M.number = { type = "number" }
M.integer = { type = "integer" }
M.boolean = { type = "boolean" }
M.any = { type = "any" }
M["nil"] = { type = "nil" }

-- Shorthand constructors (plain functions returning tables)

function M.struct(shape)
    return { type = "struct", shape = shape }
end

function M.array(item)
    return { type = "array", item = item }
end

function M.optional(inner)
    return { type = "optional", inner = inner }
end

function M.any_of(...)
    return { type = "any_of", types = { ... } }
end

function M.all_of(...)
    return { type = "all_of", types = { ... } }
end

function M.literal(value)
    return { type = "literal", value = value }
end

function M.tuple(shape)
    return { type = "tuple", shape = shape }
end

function M.dictionary(key, value)
    return { type = "dictionary", key = key, value = value }
end

-- Built-in type aliases

M.file_exists = {
    type = "string",
    file_exists = true,
}

M.dir_exists = {
    type = "string",
    dir_exists = true,
}

M.port = { type = "integer", min = 1, max = 65535 }
M.positive = { type = "number", min = 0, exclusive_min = true }
M.non_negative = { type = "number", min = 0 }
M.non_empty_string = { type = "string", min_len = 1 }

return M
