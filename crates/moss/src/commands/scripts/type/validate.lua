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

return M
