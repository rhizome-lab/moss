//! Tests for the `validate` Lua module.

use super::LuaRuntime;
use std::path::Path;

#[test]
fn primitives() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        local r, e = V.check("hello", T.string)
        assert(r == "hello" and e == nil, "string check")

        r, e = V.check(42, T.number)
        assert(r == 42 and e == nil, "number check")

        r, e = V.check(true, T.boolean)
        assert(r == true and e == nil, "boolean check")

        r, e = V.check(nil, T["nil"])
        assert(r == nil and e == nil, "nil check")

        r, e = V.check("anything", T.any)
        assert(r == "anything" and e == nil, "any check")
        "#,
    );
    assert!(result.is_ok(), "validate primitives failed: {:?}", result);
}

#[test]
fn coercion() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        -- String to number
        local r, e = V.check("123", T.number)
        assert(r == 123 and e == nil, "string to number: " .. tostring(e))

        -- String to integer
        r, e = V.check("42", T.integer)
        assert(r == 42 and e == nil, "string to integer")

        -- String to boolean
        r, e = V.check("true", T.boolean)
        assert(r == true and e == nil, "string 'true' to boolean")
        r, e = V.check("false", T.boolean)
        assert(r == false and e == nil, "string 'false' to boolean")
        r, e = V.check("1", T.boolean)
        assert(r == true and e == nil, "string '1' to boolean")
        r, e = V.check("0", T.boolean)
        assert(r == false and e == nil, "string '0' to boolean")

        -- Number to boolean
        r, e = V.check(1, T.boolean)
        assert(r == true and e == nil, "number 1 to boolean")
        r, e = V.check(0, T.boolean)
        assert(r == false and e == nil, "number 0 to boolean")

        -- Number to string
        r, e = V.check(42, T.string)
        assert(r == "42" and e == nil, "number to string")
        "#,
    );
    assert!(result.is_ok(), "validate coercion failed: {:?}", result);
}

#[test]
fn string_constraints() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local V = require("type.validate")

        -- min_len
        local r, e = V.check("ab", { type = "string", min_len = 3 })
        assert(e ~= nil, "min_len should fail")
        r, e = V.check("abc", { type = "string", min_len = 3 })
        assert(e == nil, "min_len should pass")

        -- max_len
        r, e = V.check("abcd", { type = "string", max_len = 3 })
        assert(e ~= nil, "max_len should fail")
        r, e = V.check("abc", { type = "string", max_len = 3 })
        assert(e == nil, "max_len should pass")

        -- pattern
        r, e = V.check("hello", { type = "string", pattern = "^[a-z]+$" })
        assert(e == nil, "pattern should pass")
        r, e = V.check("Hello", { type = "string", pattern = "^[a-z]+$" })
        assert(e ~= nil, "pattern should fail")

        -- one_of
        r, e = V.check("red", { type = "string", one_of = {"red", "green", "blue"} })
        assert(e == nil, "one_of should pass")
        r, e = V.check("yellow", { type = "string", one_of = {"red", "green", "blue"} })
        assert(e ~= nil, "one_of should fail")
        "#,
    );
    assert!(
        result.is_ok(),
        "validate string constraints failed: {:?}",
        result
    );
}

#[test]
fn number_constraints() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local V = require("type.validate")

        -- min/max
        local r, e = V.check(50, { type = "number", min = 0, max = 100 })
        assert(e == nil, "min/max should pass")
        r, e = V.check(-1, { type = "number", min = 0, max = 100 })
        assert(e ~= nil, "min should fail")
        r, e = V.check(101, { type = "number", min = 0, max = 100 })
        assert(e ~= nil, "max should fail")

        -- exclusive_min/exclusive_max
        r, e = V.check(0, { type = "number", min = 0, exclusive_min = true })
        assert(e ~= nil, "exclusive_min should fail at boundary")
        r, e = V.check(0.001, { type = "number", min = 0, exclusive_min = true })
        assert(e == nil, "exclusive_min should pass above boundary")
        r, e = V.check(100, { type = "number", max = 100, exclusive_max = true })
        assert(e ~= nil, "exclusive_max should fail at boundary")
        r, e = V.check(99.999, { type = "number", max = 100, exclusive_max = true })
        assert(e == nil, "exclusive_max should pass below boundary")

        -- integer check
        r, e = V.check(42, { type = "integer" })
        assert(e == nil, "integer should pass")
        r, e = V.check(42.5, { type = "integer" })
        assert(e ~= nil, "integer should fail for float")
        "#,
    );
    assert!(
        result.is_ok(),
        "validate number constraints failed: {:?}",
        result
    );
}

#[test]
fn defaults_and_required() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local V = require("type.validate")

        -- Default value
        local r, e = V.check(nil, { type = "string", default = "default" })
        assert(r == "default" and e == nil, "default value")

        -- Required field
        r, e = V.check(nil, { type = "string", required = true })
        assert(e ~= nil, "required should fail")
        assert(e:match("required"), "error should mention required")

        -- Optional (nil allowed without default)
        r, e = V.check(nil, { type = "string" })
        assert(r == nil and e == nil, "optional nil should pass")
        "#,
    );
    assert!(
        result.is_ok(),
        "validate defaults/required failed: {:?}",
        result
    );
}

#[test]
fn struct_type() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        local schema = T.struct({
            name = { type = "string", required = true },
            age = { type = "number", default = 0 },
        })

        local r, e = V.check({ name = "Alice" }, schema)
        assert(e == nil, "struct check: " .. tostring(e))
        assert(r.name == "Alice", "struct name")
        assert(r.age == 0, "struct default age")

        -- Missing required field
        r, e = V.check({ age = 25 }, schema)
        assert(e ~= nil, "missing required should fail")
        assert(e:match("name"), "error should mention field name")

        -- Wrong type for table
        r, e = V.check("not a table", schema)
        assert(e ~= nil, "string should fail for struct")
        "#,
    );
    assert!(result.is_ok(), "validate struct failed: {:?}", result);
}

#[test]
fn array_type() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        local schema = T.array(T.number)

        local r, e = V.check({ 1, 2, 3 }, schema)
        assert(e == nil, "array check")
        assert(#r == 3, "array length")
        assert(r[1] == 1 and r[2] == 2 and r[3] == 3, "array values")

        -- With coercion
        r, e = V.check({ "1", "2", "3" }, schema)
        assert(e == nil, "array with coercion")
        assert(r[1] == 1, "coerced value")

        -- Invalid item
        r, e = V.check({ 1, "invalid", 3 }, schema)
        assert(e ~= nil, "invalid item should fail")
        assert(e:match("%[2%]"), "error should mention index")
        "#,
    );
    assert!(result.is_ok(), "validate array failed: {:?}", result);
}

#[test]
fn tuple_type() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        local schema = T.tuple({ T.string, T.number, T.boolean })

        local r, e = V.check({ "hello", 42, true }, schema)
        assert(e == nil, "tuple check: " .. tostring(e))
        assert(r[1] == "hello", "tuple first")
        assert(r[2] == 42, "tuple second")
        assert(r[3] == true, "tuple third")
        "#,
    );
    assert!(result.is_ok(), "validate tuple failed: {:?}", result);
}

#[test]
fn dictionary_type() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        local schema = T.dictionary(T.string, T.number)

        local r, e = V.check({ a = 1, b = 2 }, schema)
        assert(e == nil, "dictionary check: " .. tostring(e))
        assert(r.a == 1, "dictionary value a")
        assert(r.b == 2, "dictionary value b")
        "#,
    );
    assert!(result.is_ok(), "validate dictionary failed: {:?}", result);
}

#[test]
fn optional_type() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        local schema = T.optional(T.string)

        local r, e = V.check(nil, schema)
        assert(r == nil and e == nil, "optional nil")

        r, e = V.check("hello", schema)
        assert(r == "hello" and e == nil, "optional value")
        "#,
    );
    assert!(result.is_ok(), "validate optional failed: {:?}", result);
}

#[test]
fn any_of_type() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        local schema = T.any_of(T.string, T.number)

        local r, e = V.check("hello", schema)
        assert(e == nil, "any_of string")

        r, e = V.check(42, schema)
        assert(e == nil, "any_of number")

        r, e = V.check(true, schema)
        assert(e ~= nil, "any_of should fail for boolean")
        "#,
    );
    assert!(result.is_ok(), "validate any_of failed: {:?}", result);
}

#[test]
fn all_of_type() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        local schema = T.all_of(
            { type = "string" },
            { type = "string", min_len = 3 },
            { type = "string", max_len = 10 }
        )

        local r, e = V.check("hello", schema)
        assert(e == nil, "all_of should pass: " .. tostring(e))

        r, e = V.check("hi", schema)
        assert(e ~= nil, "all_of should fail min_len")

        r, e = V.check("hello world!", schema)
        assert(e ~= nil, "all_of should fail max_len")
        "#,
    );
    assert!(result.is_ok(), "validate all_of failed: {:?}", result);
}

#[test]
fn literal_type() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        local schema = T.literal("production")

        local r, e = V.check("production", schema)
        assert(e == nil and r == "production", "literal match")

        r, e = V.check("development", schema)
        assert(e ~= nil, "literal mismatch should fail")
        "#,
    );
    assert!(result.is_ok(), "validate literal failed: {:?}", result);
}

#[test]
fn error_paths() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local V = require("type.validate")

        -- Nested struct error path
        local schema = T.struct({
            config = T.struct({
                port = T.port,
            }),
        })
        local _, e = V.check({ config = { port = 0 } }, schema)
        assert(e ~= nil, "should error")
        assert(e:match("config%.port"), "error should contain path 'config.port', got: " .. e)

        -- Array error path
        schema = T.array(T.struct({ name = { type = "string", required = true } }))
        _, e = V.check({ { name = "a" }, { name = "b" }, {} }, schema)
        assert(e ~= nil, "should error on missing name")
        assert(e:match("%[3%]"), "error should mention index [3], got: " .. e)
        "#,
    );
    assert!(result.is_ok(), "validate error paths failed: {:?}", result);
}

#[test]
fn custom_check_function() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local V = require("type.validate")

        local schema = {
            type = "string",
            check = function(v)
                if not v:match("^[a-z]+$") then
                    return nil, "must be lowercase letters only"
                end
                return v:upper()  -- Transform the value
            end,
        }

        local r, e = V.check("hello", schema)
        assert(e == nil, "custom check should pass")
        assert(r == "HELLO", "custom check should transform")

        r, e = V.check("Hello", schema)
        assert(e ~= nil, "custom check should fail")
        assert(e:match("lowercase"), "error should mention constraint")
        "#,
    );
    assert!(result.is_ok(), "validate custom check failed: {:?}", result);
}

#[test]
fn type_reexport() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local V = require("type.validate")

        -- V.type should re-export the type module
        assert(V.type ~= nil, "V.type should exist")
        assert(V.type.string.type == "string", "V.type.string should work")
        assert(V.type.port.type == "integer", "V.type.port should work")
        "#,
    );
    assert!(
        result.is_ok(),
        "validate type reexport failed: {:?}",
        result
    );
}

// ========== Generator tests ==========

#[test]
fn generate_primitives() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local generate = require("type.generate")

        math.randomseed(42)

        local s = generate(T.string)
        assert(type(s) == "string", "generated string should be string")

        local n = generate(T.number)
        assert(type(n) == "number", "generated number should be number")

        local i = generate(T.integer)
        assert(type(i) == "number" and i % 1 == 0, "generated integer should be whole")

        local b = generate(T.boolean)
        assert(type(b) == "boolean", "generated boolean should be boolean")
        "#,
    );
    assert!(result.is_ok(), "generate primitives failed: {:?}", result);
}

#[test]
fn generate_with_constraints() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local generate = require("type.generate")
        math.randomseed(42)

        -- String with length constraints
        for i = 1, 10 do
            local s = generate({ type = "string", min_len = 5, max_len = 10 })
            assert(#s >= 5 and #s <= 10, "string length should be 5-10")
        end

        -- Number with range
        for i = 1, 10 do
            local n = generate({ type = "number", min = 0, max = 100 })
            assert(n >= 0 and n <= 100, "number should be 0-100")
        end

        -- one_of constraint
        for i = 1, 10 do
            local s = generate({ type = "string", one_of = { "red", "green", "blue" } })
            assert(s == "red" or s == "green" or s == "blue", "should be one of choices")
        end
        "#,
    );
    assert!(result.is_ok(), "generate constraints failed: {:?}", result);
}

#[test]
fn generate_composite_types() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local generate = require("type.generate")
        math.randomseed(42)

        -- Struct (use required fields for deterministic test)
        local s = generate(T.struct({
            name = { type = "string", required = true },
            age = { type = "integer", required = true }
        }))
        assert(type(s) == "table" and type(s.name) == "string", "struct generation")

        -- Array
        local a = generate(T.array(T.number), { max_array_len = 5 })
        assert(type(a) == "table", "array generation")

        -- Tuple
        local t = generate(T.tuple({ T.string, T.number }))
        assert(type(t[1]) == "string" and type(t[2]) == "number", "tuple generation")

        -- Literal
        assert(generate(T.literal("fixed")) == "fixed", "literal generation")
        "#,
    );
    assert!(result.is_ok(), "generate composite failed: {:?}", result);
}

#[test]
fn generate_validates() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        local validate = require("type.validate")
        local generate = require("type.generate")
        math.randomseed(42)

        local schema = T.struct({
            name = { type = "string", min_len = 1 },
            port = T.port,
            tags = T.array({ type = "string", one_of = { "a", "b", "c" } }),
        })

        for i = 1, 5 do
            local generated = generate(schema, { max_array_len = 3 })
            local _, err = validate.check(generated, schema)
            assert(err == nil, "generated value should validate: " .. tostring(err))
        end
        "#,
    );
    assert!(result.is_ok(), "generate validates failed: {:?}", result);
}
