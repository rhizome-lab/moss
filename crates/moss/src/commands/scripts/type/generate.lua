-- Random value generator for type schemas
-- Usage: local generate = require("generate")
--        local value = generate(T.string)

local M = {}

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

-- Generators table - dispatched by schema.type
local generators = {}

--- Generate a random value matching a schema
--- @param schema table
--- @param opts table? { seed = number, max_depth = number, max_array_len = number }
--- @return any
local function generate(schema, opts)
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

    local generator = generators[schema.type]
    if not generator then
        error("unknown schema type for generation: " .. tostring(schema.type))
    end

    return generator(schema, opts)
end

generators.string = function(schema, opts)
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

generators.number = function(schema, opts)
    local min = schema.min or -1000
    local max = schema.max or 1000
    if schema.exclusive_min then min = min + 0.001 end
    if schema.exclusive_max then max = max - 0.001 end
    return rand_float(min, max)
end

generators.integer = function(schema, opts)
    local min = schema.min or -1000
    local max = schema.max or 1000
    if schema.exclusive_min then min = min + 1 end
    if schema.exclusive_max then max = max - 1 end
    return rand_int(math.floor(min), math.floor(max))
end

generators.boolean = function(schema, opts)
    return math.random() > 0.5
end

generators["nil"] = function(schema, opts)
    return nil
end

generators.any = function(schema, opts)
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

generators.struct = function(schema, opts)
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
            result[k] = generate(field_schema, child_opts)
        end
    end
    return result
end

generators.array = function(schema, opts)
    local result = {}
    local len = rand_int(0, opts.max_array_len)
    local child_opts = {
        max_depth = opts.max_depth,
        max_array_len = opts.max_array_len,
        _depth = opts._depth + 1,
    }
    for i = 1, len do
        table.insert(result, generate(schema.item, child_opts))
    end
    return result
end

generators.tuple = function(schema, opts)
    local result = {}
    local child_opts = {
        max_depth = opts.max_depth,
        max_array_len = opts.max_array_len,
        _depth = opts._depth + 1,
    }
    for i, item_schema in ipairs(schema.shape) do
        result[i] = generate(item_schema, child_opts)
    end
    return result
end

generators.dictionary = function(schema, opts)
    local result = {}
    local len = rand_int(0, opts.max_array_len)
    local child_opts = {
        max_depth = opts.max_depth,
        max_array_len = opts.max_array_len,
        _depth = opts._depth + 1,
    }
    for i = 1, len do
        local key = generate(schema.key, child_opts)
        local value = generate(schema.value, child_opts)
        result[key] = value
    end
    return result
end

generators.optional = function(schema, opts)
    -- 30% chance of nil
    if math.random() < 0.3 then
        return nil
    end
    return generate(schema.inner, opts)
end

generators.any_of = function(schema, opts)
    -- Pick a random type from the union
    local idx = rand_int(1, #schema.types)
    return generate(schema.types[idx], opts)
end

generators.all_of = function(schema, opts)
    -- For all_of, generate from the first type and hope it satisfies all
    -- (This is a limitation - proper implementation would need constraint solving)
    if #schema.types > 0 then
        return generate(schema.types[1], opts)
    end
    return nil
end

generators.literal = function(schema, opts)
    return schema.value
end

-- Return the generate function directly (module IS the function)
return generate
