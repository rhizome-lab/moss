//! Tests for the `test` Lua module.

use super::LuaRuntime;
use std::path::Path;

#[test]
fn assert_equals() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local test = require("test")
        local a = test.assert

        a.equals(1, 1)
        a.equals("foo", "foo")
        a.equals(true, true)

        local ok, err = pcall(function() a.equals(1, 2) end)
        assert(not ok, "should fail on mismatch")
        "#,
    );
    assert!(result.is_ok(), "assert_equals failed: {:?}", result);
}

#[test]
fn assert_same() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local test = require("test")
        local a = test.assert

        a.same({a = 1, b = 2}, {a = 1, b = 2})
        a.same({1, 2, 3}, {1, 2, 3})
        a.same({nested = {x = 1}}, {nested = {x = 1}})

        local ok = pcall(function() a.same({a = 1}, {a = 2}) end)
        assert(not ok, "should fail on different tables")
        "#,
    );
    assert!(result.is_ok(), "assert_same failed: {:?}", result);
}

#[test]
fn assert_truthy_falsy() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local test = require("test")
        local a = test.assert

        a.is_true(true)
        a.is_true(1)
        a.is_true("yes")

        a.is_false(false)
        a.is_false(nil)

        a.is_nil(nil)
        a.is_not_nil(1)
        a.is_not_nil(false)
        "#,
    );
    assert!(result.is_ok(), "assert_truthy_falsy failed: {:?}", result);
}

#[test]
fn assert_types() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local test = require("test")
        local a = test.assert

        a.is_type("hello", "string")
        a.is_type(42, "number")
        a.is_type({}, "table")
        a.is_type(function() end, "function")

        local ok = pcall(function() a.is_type("hello", "number") end)
        assert(not ok, "should fail on type mismatch")
        "#,
    );
    assert!(result.is_ok(), "assert_types failed: {:?}", result);
}

#[test]
fn assert_strings() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local test = require("test")
        local a = test.assert

        a.contains("hello world", "world")
        a.matches("hello123", "%d+")

        local ok = pcall(function() a.contains("hello", "xyz") end)
        assert(not ok, "should fail when substring not found")
        "#,
    );
    assert!(result.is_ok(), "assert_strings failed: {:?}", result);
}

#[test]
fn assert_comparisons() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local test = require("test")
        local a = test.assert

        a.gt(5, 3)
        a.gte(5, 5)
        a.lt(3, 5)
        a.lte(5, 5)
        a.near(3.14159, 3.14160, 0.001)

        local ok = pcall(function() a.gt(3, 5) end)
        assert(not ok, "should fail when not greater")
        "#,
    );
    assert!(result.is_ok(), "assert_comparisons failed: {:?}", result);
}

#[test]
fn assert_throws() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local test = require("test")
        local a = test.assert

        a.throws(function() error("boom") end)
        a.throws(function() error("specific error") end, "specific")
        a.does_not_throw(function() return 42 end)

        local ok = pcall(function() a.throws(function() end) end)
        assert(not ok, "should fail when function doesn't throw")
        "#,
    );
    assert!(result.is_ok(), "assert_throws failed: {:?}", result);
}

#[test]
fn test_runner() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local test = require("test")

        test.reset()

        test.test("passing test", function()
            test.assert.equals(1 + 1, 2)
        end)

        test.test("another passing test", function()
            test.assert.is_true(true)
        end)

        assert(test.passed == 2, "expected 2 passed")
        assert(test.failed == 0, "expected 0 failed")
        "#,
    );
    assert!(result.is_ok(), "test_runner failed: {:?}", result);
}

#[test]
fn test_runner_with_failures() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local test = require("test")

        test.reset()

        test.test("passing", function()
            test.assert.equals(1, 1)
        end)

        test.test("failing", function()
            test.assert.equals(1, 2)
        end)

        assert(test.passed == 1, "expected 1 passed")
        assert(test.failed == 1, "expected 1 failed")
        assert(#test.errors == 1, "expected 1 error")
        "#,
    );
    assert!(
        result.is_ok(),
        "test_runner_with_failures failed: {:?}",
        result
    );
}
