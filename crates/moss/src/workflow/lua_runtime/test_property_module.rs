//! Tests for the `test.property` Lua module.

use super::LuaRuntime;
use std::path::Path;

#[test]
fn property_check_passes() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local property = require("test.property")

        -- Property that always holds
        local ok, err = property.check(T.integer, function(n)
            return type(n) == "number"
        end, { iterations = 50 })

        assert(ok, "property should pass: " .. tostring(err))
        "#,
    );
    assert!(result.is_ok(), "property_check_passes failed: {:?}", result);
}

#[test]
fn property_check_fails() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local property = require("test.property")

        -- Property that fails for negative numbers
        local ok, err = property.check(
            { type = "integer", min = -100, max = 100 },
            function(n)
                if n < 0 then error("negative!") end
            end,
            { iterations = 100, seed = 42 }
        )

        assert(not ok, "property should fail")
        assert(err:find("negative"), "error should mention negative: " .. err)
        "#,
    );
    assert!(result.is_ok(), "property_check_fails failed: {:?}", result);
}

#[test]
fn property_with_struct() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local property = require("test.property")

        local user_schema = T.struct({
            name = { type = "string", min_len = 1, required = true },
            age = { type = "integer", min = 0, max = 150, required = true },
        })

        local ok, err = property.check(user_schema, function(user)
            assert(type(user.name) == "string", "name should be string")
            assert(type(user.age) == "number", "age should be number")
            assert(user.age >= 0 and user.age <= 150, "age in range")
        end, { iterations = 20 })

        assert(ok, "struct property should pass: " .. tostring(err))
        "#,
    );
    assert!(result.is_ok(), "property_with_struct failed: {:?}", result);
}

#[test]
fn property_assert() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local property = require("test.property")

        -- assert throws on failure
        local ok = pcall(function()
            property.assert({ type = "integer", min = 1, max = 10 }, function(n)
                if n > 5 then error("too big") end
            end, { iterations = 50, seed = 42 })
        end)

        assert(not ok, "assert should throw on property failure")
        "#,
    );
    assert!(result.is_ok(), "property_assert failed: {:?}", result);
}

#[test]
fn property_format_value() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local property = require("test.property")

        assert(property.format_value("hello") == '"hello"', "string format")
        assert(property.format_value(42) == "42", "number format")
        assert(property.format_value(true) == "true", "boolean format")
        assert(property.format_value(nil) == "nil", "nil format")

        local arr = property.format_value({1, 2, 3})
        assert(arr:find("%[") and arr:find("1"), "array format: " .. arr)

        local obj = property.format_value({a = 1})
        assert(obj:find("{") and obj:find("a="), "object format: " .. obj)
        "#,
    );
    assert!(result.is_ok(), "property_format_value failed: {:?}", result);
}

#[test]
fn property_with_test_module() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local test = require("test")
        local property = require("test.property")

        test.reset()

        -- Use prop helper to create test function
        test.test("integers are numbers", property.prop(
            "integers are numbers",
            T.integer,
            function(n) return type(n) == "number" end,
            { iterations = 20 }
        ))

        assert(test.passed == 1, "expected 1 passed")
        "#,
    );
    assert!(
        result.is_ok(),
        "property_with_test_module failed: {:?}",
        result
    );
}
