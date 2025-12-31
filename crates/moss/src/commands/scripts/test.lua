-- Test framework with busted-style assertions
-- Usage: local test = require("test")

local M = {}

-- Track test results
M.passed = 0
M.failed = 0
M.errors = {}

--- Reset test state
function M.reset()
    M.passed = 0
    M.failed = 0
    M.errors = {}
end

--- Report test results
--- @return boolean all_passed
function M.report()
    local total = M.passed + M.failed
    if M.failed == 0 then
        print(string.format("All %d tests passed", total))
        return true
    else
        print(string.format("%d/%d tests passed", M.passed, total))
        for _, err in ipairs(M.errors) do
            print("  FAIL: " .. err)
        end
        return false
    end
end

--- Run a test function and track results
--- @param name string
--- @param fn function
function M.test(name, fn)
    local ok, err = pcall(fn)
    if ok then
        M.passed = M.passed + 1
    else
        M.failed = M.failed + 1
        table.insert(M.errors, name .. ": " .. tostring(err))
    end
end

-- Assertion helpers

local assert_mt = {}

--- Check equality
function assert_mt.equals(a, b, msg)
    if a ~= b then
        error(msg or string.format("expected %s, got %s", tostring(b), tostring(a)), 2)
    end
end

--- Check deep equality for tables
function assert_mt.same(a, b, msg)
    local function deep_equal(x, y)
        if type(x) ~= type(y) then return false end
        if type(x) ~= "table" then return x == y end
        for k, v in pairs(x) do
            if not deep_equal(v, y[k]) then return false end
        end
        for k, v in pairs(y) do
            if not deep_equal(v, x[k]) then return false end
        end
        return true
    end
    if not deep_equal(a, b) then
        error(msg or "tables are not deeply equal", 2)
    end
end

--- Check truthy
function assert_mt.is_true(v, msg)
    if not v then
        error(msg or "expected truthy value", 2)
    end
end

--- Check falsy
function assert_mt.is_false(v, msg)
    if v then
        error(msg or "expected falsy value", 2)
    end
end

--- Check nil
function assert_mt.is_nil(v, msg)
    if v ~= nil then
        error(msg or string.format("expected nil, got %s", tostring(v)), 2)
    end
end

--- Check not nil
function assert_mt.is_not_nil(v, msg)
    if v == nil then
        error(msg or "expected non-nil value", 2)
    end
end

--- Check type
function assert_mt.is_type(v, expected_type, msg)
    local actual = type(v)
    if actual ~= expected_type then
        error(msg or string.format("expected type %s, got %s", expected_type, actual), 2)
    end
end

--- Check string contains
function assert_mt.contains(str, substr, msg)
    if type(str) ~= "string" or not str:find(substr, 1, true) then
        error(msg or string.format("expected string to contain '%s'", substr), 2)
    end
end

--- Check string matches pattern
function assert_mt.matches(str, pattern, msg)
    if type(str) ~= "string" or not str:match(pattern) then
        error(msg or string.format("expected string to match '%s'", pattern), 2)
    end
end

--- Check value is in table
function assert_mt.is_in(value, tbl, msg)
    for _, v in ipairs(tbl) do
        if v == value then return end
    end
    error(msg or string.format("expected %s to be in table", tostring(value)), 2)
end

--- Check function throws error
function assert_mt.throws(fn, pattern, msg)
    local ok, err = pcall(fn)
    if ok then
        error(msg or "expected function to throw", 2)
    end
    if pattern and not tostring(err):match(pattern) then
        error(msg or string.format("expected error matching '%s', got '%s'", pattern, tostring(err)), 2)
    end
end

--- Check function does not throw
function assert_mt.does_not_throw(fn, msg)
    local ok, err = pcall(fn)
    if not ok then
        error(msg or string.format("expected no error, got '%s'", tostring(err)), 2)
    end
end

--- Check approximately equal (for floats)
function assert_mt.near(a, b, tolerance, msg)
    tolerance = tolerance or 0.0001
    if math.abs(a - b) > tolerance then
        error(msg or string.format("expected %s to be near %s (tolerance %s)", a, b, tolerance), 2)
    end
end

--- Check greater than
function assert_mt.gt(a, b, msg)
    if not (a > b) then
        error(msg or string.format("expected %s > %s", tostring(a), tostring(b)), 2)
    end
end

--- Check greater than or equal
function assert_mt.gte(a, b, msg)
    if not (a >= b) then
        error(msg or string.format("expected %s >= %s", tostring(a), tostring(b)), 2)
    end
end

--- Check less than
function assert_mt.lt(a, b, msg)
    if not (a < b) then
        error(msg or string.format("expected %s < %s", tostring(a), tostring(b)), 2)
    end
end

--- Check less than or equal
function assert_mt.lte(a, b, msg)
    if not (a <= b) then
        error(msg or string.format("expected %s <= %s", tostring(a), tostring(b)), 2)
    end
end

M.assert = assert_mt

return M
