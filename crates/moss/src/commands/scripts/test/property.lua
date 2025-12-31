-- Property-based testing using type.generate
-- Usage: local property = require("test.property")

local generate = require("type.generate")

local M = {}

--- Run a property test with generated values
--- @param schema table Type schema for generating test values
--- @param fn function Property function that should return true or throw
--- @param opts table? { iterations = number, seed = number }
--- @return boolean success, string? error
function M.check(schema, fn, opts)
    opts = opts or {}
    local iterations = opts.iterations or 100
    local seed = opts.seed

    for i = 1, iterations do
        local gen_opts = { seed = seed and (seed + i) or nil }
        local value = generate(schema, gen_opts)

        local ok, err = pcall(fn, value)
        if not ok then
            -- Format the failing value for debugging
            local value_str = M.format_value(value)
            return false, string.format(
                "Property failed on iteration %d with value: %s\nError: %s",
                i, value_str, tostring(err)
            )
        end

        -- If function returns false explicitly, that's also a failure
        if ok and err == false then
            local value_str = M.format_value(value)
            return false, string.format(
                "Property returned false on iteration %d with value: %s",
                i, value_str
            )
        end
    end

    return true, nil
end

--- Format a value for debugging output
--- @param value any
--- @param depth number?
--- @return string
function M.format_value(value, depth)
    depth = depth or 0
    if depth > 3 then return "..." end

    local t = type(value)
    if t == "string" then
        return string.format("%q", value)
    elseif t == "number" or t == "boolean" then
        return tostring(value)
    elseif t == "nil" then
        return "nil"
    elseif t == "table" then
        local parts = {}
        local is_array = #value > 0
        if is_array then
            for i, v in ipairs(value) do
                if i > 5 then
                    table.insert(parts, "...")
                    break
                end
                table.insert(parts, M.format_value(v, depth + 1))
            end
            return "[" .. table.concat(parts, ", ") .. "]"
        else
            local count = 0
            for k, v in pairs(value) do
                if count >= 5 then
                    table.insert(parts, "...")
                    break
                end
                table.insert(parts, string.format("%s=%s", tostring(k), M.format_value(v, depth + 1)))
                count = count + 1
            end
            return "{" .. table.concat(parts, ", ") .. "}"
        end
    else
        return "<" .. t .. ">"
    end
end

--- Helper to create a property test that integrates with test module
--- @param name string Test name
--- @param schema table Type schema
--- @param fn function Property function
--- @param opts table? Options
--- @return function Test function for use with test.test()
function M.prop(name, schema, fn, opts)
    return function()
        local ok, err = M.check(schema, fn, opts)
        if not ok then
            error(err, 2)
        end
    end
end

--- Run property and assert (throws on failure)
--- @param schema table
--- @param fn function
--- @param opts table?
function M.assert(schema, fn, opts)
    local ok, err = M.check(schema, fn, opts)
    if not ok then
        error(err, 2)
    end
end

return M
