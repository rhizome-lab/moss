-- Validation library for declarative type schemas
-- Usage: local validate = require("validate")

local T = require("type")
local M = {}

-- Re-export type module for convenience
M.type = T

-- Checkers table - dispatched by schema.type
M.checkers = {}

--- Check a value against a schema
--- @param value any
--- @param schema table
--- @param path string? (for error messages)
--- @return any result, string? error
function M.check(value, schema, path)
    path = path or ""

    -- Handle nil + default
    if value == nil then
        if schema.default ~= nil then
            return schema.default, nil
        end
        if schema.required then
            return nil, path .. ": required field missing"
        end
        -- Optional (nil allowed)
        return nil, nil
    end

    -- Dispatch to type-specific checker
    local checker = M.checkers[schema.type]
    if not checker then
        return nil, path .. ": unknown schema type '" .. tostring(schema.type) .. "'"
    end

    return checker(value, schema, path)
end

-- Primitives

M.checkers.string = function(value, schema, path)
    -- Coercion
    if type(value) ~= "string" then
        if type(value) == "number" then
            value = tostring(value)
        else
            return nil, path .. ": expected string, got " .. type(value)
        end
    end

    -- Constraints
    if schema.min_len and #value < schema.min_len then
        return nil, path .. ": must be at least " .. schema.min_len .. " characters"
    end
    if schema.max_len and #value > schema.max_len then
        return nil, path .. ": must be at most " .. schema.max_len .. " characters"
    end
    if schema.pattern and not value:match(schema.pattern) then
        return nil, path .. ": must match pattern '" .. schema.pattern .. "'"
    end
    if schema.one_of then
        local found = false
        for _, v in ipairs(schema.one_of) do
            if v == value then found = true; break end
        end
        if not found then
            return nil, path .. ": must be one of [" .. table.concat(schema.one_of, ", ") .. "], got '" .. value .. "'"
        end
    end
    -- file_exists constraint
    if schema.file_exists then
        local f = io.open(value, "r")
        if not f then
            return nil, path .. ": file not found: " .. value
        end
        f:close()
    end
    -- dir_exists constraint
    if schema.dir_exists then
        local result = os.execute('test -d "' .. value .. '"')
        if not result then
            return nil, path .. ": directory not found: " .. value
        end
    end
    -- Custom check
    if schema.check then
        local result, err = schema.check(value)
        if err then return nil, path .. ": " .. err end
        value = result
    end

    return value, nil
end

M.checkers.number = function(value, schema, path)
    -- Coercion
    if type(value) ~= "number" then
        if type(value) == "string" then
            local n = tonumber(value)
            if n == nil then
                return nil, path .. ": expected number, got '" .. value .. "'"
            end
            value = n
        else
            return nil, path .. ": expected number, got " .. type(value)
        end
    end

    -- Constraints
    if schema.min then
        if schema.exclusive_min then
            if value <= schema.min then
                return nil, path .. ": must be greater than " .. schema.min
            end
        else
            if value < schema.min then
                return nil, path .. ": must be at least " .. schema.min
            end
        end
    end
    if schema.max then
        if schema.exclusive_max then
            if value >= schema.max then
                return nil, path .. ": must be less than " .. schema.max
            end
        else
            if value > schema.max then
                return nil, path .. ": must be at most " .. schema.max
            end
        end
    end
    if schema.check then
        local result, err = schema.check(value)
        if err then return nil, path .. ": " .. err end
        value = result
    end

    return value, nil
end

M.checkers.integer = function(value, schema, path)
    -- First check as number
    local result, err = M.checkers.number(value, schema, path)
    if err then return nil, err end

    -- Then check integer
    if result % 1 ~= 0 then
        return nil, path .. ": expected integer, got " .. result
    end

    return result, nil
end

M.checkers.boolean = function(value, schema, path)
    -- Coercion
    if type(value) ~= "boolean" then
        if type(value) == "string" then
            if value == "true" or value == "1" then
                value = true
            elseif value == "false" or value == "0" then
                value = false
            else
                return nil, path .. ": expected boolean, got '" .. value .. "'"
            end
        elseif type(value) == "number" then
            value = value ~= 0
        else
            return nil, path .. ": expected boolean, got " .. type(value)
        end
    end

    if schema.check then
        local result, err = schema.check(value)
        if err then return nil, path .. ": " .. err end
        value = result
    end

    return value, nil
end

M.checkers["nil"] = function(value, _, path)
    if value ~= nil then
        return nil, path .. ": expected nil, got " .. type(value)
    end
    return nil, nil
end

M.checkers.any = function(value, schema, path)
    if schema.check then
        local result, err = schema.check(value)
        if err then return nil, path .. ": " .. err end
        value = result
    end
    return value, nil
end

-- Composite types

M.checkers.struct = function(value, schema, path)
    if type(value) ~= "table" then
        return nil, path .. ": expected table, got " .. type(value)
    end

    local result = {}
    for k, field_schema in pairs(schema.shape) do
        local field_path = path == "" and k or (path .. "." .. k)
        local field_value, err = M.check(value[k], field_schema, field_path)
        if err then return nil, err end
        result[k] = field_value
    end

    return result, nil
end

M.checkers.array = function(value, schema, path)
    if type(value) ~= "table" then
        return nil, path .. ": expected array, got " .. type(value)
    end

    local result = {}
    local item_schema = schema.item
    for i, item in ipairs(value) do
        local item_path = path .. "[" .. i .. "]"
        local item_value, err = M.check(item, item_schema, item_path)
        if err then return nil, err end
        result[i] = item_value
    end

    return result, nil
end

M.checkers.tuple = function(value, schema, path)
    if type(value) ~= "table" then
        return nil, path .. ": expected tuple, got " .. type(value)
    end

    local result = {}
    for i, item_schema in ipairs(schema.shape) do
        local item_path = path .. "[" .. i .. "]"
        local item_value, err = M.check(value[i], item_schema, item_path)
        if err then return nil, err end
        result[i] = item_value
    end

    return result, nil
end

M.checkers.dictionary = function(value, schema, path)
    if type(value) ~= "table" then
        return nil, path .. ": expected dictionary, got " .. type(value)
    end

    local result = {}
    for k, v in pairs(value) do
        local key_result, err = M.check(k, schema.key, path .. ".<key>")
        if err then return nil, err end
        local val_result, err2 = M.check(v, schema.value, path .. "[" .. tostring(k) .. "]")
        if err2 then return nil, err2 end
        result[key_result] = val_result
    end

    return result, nil
end

M.checkers.optional = function(value, schema, path)
    if value == nil then
        return nil, nil
    end
    return M.check(value, schema.inner, path)
end

M.checkers.any_of = function(value, schema, path)
    for _, type_schema in ipairs(schema.types) do
        local result, err = M.check(value, type_schema, path)
        if not err then
            return result, nil
        end
    end
    return nil, path .. ": did not match any type"
end

M.checkers.all_of = function(value, schema, path)
    local result = value
    for _, type_schema in ipairs(schema.types) do
        local r, err = M.check(result, type_schema, path)
        if err then return nil, err end
        result = r
    end
    return result, nil
end

M.checkers.literal = function(value, schema, path)
    if value ~= schema.value then
        return nil, path .. ": expected '" .. tostring(schema.value) .. "', got '" .. tostring(value) .. "'"
    end
    return value, nil
end

-- ============================================================
-- Generators - produce random values from schemas
-- ============================================================

M.generators = {}

-- Random number in range
local function rand_int(min, max)
    return math.random(min, max)
end

local function rand_float(min, max)
    return min + math.random() * (max - min)
end

-- Random string of given length
local function rand_string(len)
    local chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
    local result = {}
    for i = 1, len do
        local idx = rand_int(1, #chars)
        table.insert(result, chars:sub(idx, idx))
    end
    return table.concat(result)
end

--- Generate a random value matching a schema
--- @param schema table
--- @param opts table? { seed = number, max_depth = number, max_array_len = number }
--- @return any
function M.generate(schema, opts)
    opts = opts or {}
    if opts.seed then
        math.randomseed(opts.seed)
    end
    opts.max_depth = opts.max_depth or 5
    opts.max_array_len = opts.max_array_len or 10
    opts._depth = opts._depth or 0

    if opts._depth > opts.max_depth then
        return nil
    end

    local generator = M.generators[schema.type]
    if not generator then
        error("unknown schema type for generation: " .. tostring(schema.type))
    end

    return generator(schema, opts)
end

M.generators.string = function(schema, opts)
    -- Handle one_of constraint
    if schema.one_of then
        return schema.one_of[rand_int(1, #schema.one_of)]
    end

    -- Handle pattern (limited support - just generate random string)
    local min_len = schema.min_len or 1
    local max_len = schema.max_len or 20
    local len = rand_int(min_len, max_len)
    return rand_string(len)
end

M.generators.number = function(schema, opts)
    local min = schema.min or -1000
    local max = schema.max or 1000
    if schema.exclusive_min then min = min + 0.001 end
    if schema.exclusive_max then max = max - 0.001 end
    return rand_float(min, max)
end

M.generators.integer = function(schema, opts)
    local min = schema.min or -1000
    local max = schema.max or 1000
    if schema.exclusive_min then min = min + 1 end
    if schema.exclusive_max then max = max - 1 end
    return rand_int(math.floor(min), math.floor(max))
end

M.generators.boolean = function(schema, opts)
    return math.random() > 0.5
end

M.generators["nil"] = function(schema, opts)
    return nil
end

M.generators.any = function(schema, opts)
    -- Generate one of: string, number, boolean
    local choice = rand_int(1, 3)
    if choice == 1 then
        return rand_string(rand_int(1, 10))
    elseif choice == 2 then
        return rand_float(-100, 100)
    else
        return math.random() > 0.5
    end
end

M.generators.struct = function(schema, opts)
    local result = {}
    local child_opts = {
        max_depth = opts.max_depth,
        max_array_len = opts.max_array_len,
        _depth = opts._depth + 1,
    }
    for k, field_schema in pairs(schema.shape) do
        -- Skip optional fields randomly
        if not field_schema.required and math.random() > 0.7 then
            if field_schema.default ~= nil then
                result[k] = field_schema.default
            end
        else
            result[k] = M.generate(field_schema, child_opts)
        end
    end
    return result
end

M.generators.array = function(schema, opts)
    local result = {}
    local len = rand_int(0, opts.max_array_len)
    local child_opts = {
        max_depth = opts.max_depth,
        max_array_len = opts.max_array_len,
        _depth = opts._depth + 1,
    }
    for i = 1, len do
        table.insert(result, M.generate(schema.item, child_opts))
    end
    return result
end

M.generators.tuple = function(schema, opts)
    local result = {}
    local child_opts = {
        max_depth = opts.max_depth,
        max_array_len = opts.max_array_len,
        _depth = opts._depth + 1,
    }
    for i, item_schema in ipairs(schema.shape) do
        result[i] = M.generate(item_schema, child_opts)
    end
    return result
end

M.generators.dictionary = function(schema, opts)
    local result = {}
    local len = rand_int(0, opts.max_array_len)
    local child_opts = {
        max_depth = opts.max_depth,
        max_array_len = opts.max_array_len,
        _depth = opts._depth + 1,
    }
    for i = 1, len do
        local key = M.generate(schema.key, child_opts)
        local value = M.generate(schema.value, child_opts)
        result[key] = value
    end
    return result
end

M.generators.optional = function(schema, opts)
    -- 30% chance of nil
    if math.random() < 0.3 then
        return nil
    end
    return M.generate(schema.inner, opts)
end

M.generators.any_of = function(schema, opts)
    -- Pick a random type from the union
    local idx = rand_int(1, #schema.types)
    return M.generate(schema.types[idx], opts)
end

M.generators.all_of = function(schema, opts)
    -- For all_of, generate from the first type and hope it satisfies all
    -- (This is a limitation - proper implementation would need constraint solving)
    if #schema.types > 0 then
        return M.generate(schema.types[1], opts)
    end
    return nil
end

M.generators.literal = function(schema, opts)
    return schema.value
end

return M
