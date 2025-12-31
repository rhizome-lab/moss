//! Tests for the `type` Lua module.

use super::LuaRuntime;
use std::path::Path;

#[test]
fn primitives() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")
        assert(T.string.type == "string", "T.string")
        assert(T.number.type == "number", "T.number")
        assert(T.integer.type == "integer", "T.integer")
        assert(T.boolean.type == "boolean", "T.boolean")
        assert(T.any.type == "any", "T.any")
        assert(T["nil"].type == "nil", "T.nil")
        "#,
    );
    assert!(result.is_ok(), "type primitives failed: {:?}", result);
}

#[test]
fn constructors() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")

        local s = T.struct({ name = T.string })
        assert(s.type == "struct", "struct type")
        assert(s.shape.name.type == "string", "struct shape")

        local a = T.array(T.number)
        assert(a.type == "array", "array type")
        assert(a.item.type == "number", "array item")

        local o = T.optional(T.string)
        assert(o.type == "optional", "optional type")
        assert(o.inner.type == "string", "optional inner")

        local u = T.any_of(T.string, T.number)
        assert(u.type == "any_of", "any_of type")
        assert(#u.types == 2, "any_of types count")

        local l = T.literal("foo")
        assert(l.type == "literal", "literal type")
        assert(l.value == "foo", "literal value")

        local t = T.tuple({ T.string, T.number })
        assert(t.type == "tuple", "tuple type")
        assert(#t.shape == 2, "tuple shape count")

        local d = T.dictionary(T.string, T.number)
        assert(d.type == "dictionary", "dictionary type")
        assert(d.key.type == "string", "dictionary key")
        assert(d.value.type == "number", "dictionary value")

        local all = T.all_of(T.string, { type = "string", min_len = 1 })
        assert(all.type == "all_of", "all_of type")
        assert(#all.types == 2, "all_of types count")
        "#,
    );
    assert!(result.is_ok(), "type constructors failed: {:?}", result);
}

#[test]
fn aliases() {
    let runtime = LuaRuntime::new(Path::new(".")).unwrap();
    let result = runtime.run_string(
        r#"
        local T = require("type")

        assert(T.port.type == "integer", "port type")
        assert(T.port.min == 1, "port min")
        assert(T.port.max == 65535, "port max")

        assert(T.non_empty_string.type == "string", "non_empty_string type")
        assert(T.non_empty_string.min_len == 1, "non_empty_string min_len")

        assert(T.positive.type == "number", "positive type")
        assert(T.positive.min == 0, "positive min")
        assert(T.positive.exclusive_min == true, "positive exclusive_min")

        assert(T.non_negative.type == "number", "non_negative type")
        assert(T.non_negative.min == 0, "non_negative min")

        assert(T.file_exists.type == "string", "file_exists type")
        assert(T.file_exists.file_exists == true, "file_exists flag")

        assert(T.dir_exists.type == "string", "dir_exists type")
        assert(T.dir_exists.dir_exists == true, "dir_exists flag")
        "#,
    );
    assert!(result.is_ok(), "type aliases failed: {:?}", result);
}
