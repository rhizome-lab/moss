//! Comprehensive tests for syntax highlighting.
//!
//! Tests each highlight kind (keywords, numbers, strings, constants, comments,
//! types, function names, attributes) across multiple languages.

use crate::parsers::Parsers;
use crate::tree::{collect_highlight_spans, HighlightKind};

/// Helper to extract highlight spans from code
fn get_spans(code: &str, grammar: &str) -> Vec<(String, HighlightKind)> {
    let parsers = Parsers::new();
    let tree = parsers.parse_with_grammar(grammar, code).unwrap();
    let mut spans = Vec::new();
    collect_highlight_spans(tree.root_node(), &mut spans);
    spans
        .into_iter()
        .map(|s| (code[s.start..s.end].to_string(), s.kind))
        .collect()
}

/// Check that code contains a span with given text and kind
fn has_span(spans: &[(String, HighlightKind)], text: &str, kind: HighlightKind) -> bool {
    spans.iter().any(|(t, k)| t == text && *k == kind)
}

// ==================== Rust ====================

#[test]
fn test_highlight_rust_keywords() {
    let code = "pub fn foo() { let x = 1; if x > 0 { return x; } }";
    let spans = get_spans(code, "rust");
    assert!(has_span(&spans, "pub", HighlightKind::Keyword));
    assert!(has_span(&spans, "fn", HighlightKind::Keyword));
    assert!(has_span(&spans, "let", HighlightKind::Keyword));
    assert!(has_span(&spans, "if", HighlightKind::Keyword));
    assert!(has_span(&spans, "return", HighlightKind::Keyword));
}

#[test]
fn test_highlight_rust_numbers() {
    let code = "let x = 42; let y = 3.14; let z = 0xff;";
    let spans = get_spans(code, "rust");
    assert!(has_span(&spans, "42", HighlightKind::Number));
    assert!(has_span(&spans, "3.14", HighlightKind::Number));
    assert!(has_span(&spans, "0xff", HighlightKind::Number));
}

#[test]
fn test_highlight_rust_strings() {
    let code = r#"let s = "hello"; let r = r"raw";"#;
    let spans = get_spans(code, "rust");
    assert!(has_span(&spans, "\"hello\"", HighlightKind::String));
    assert!(has_span(&spans, "r\"raw\"", HighlightKind::String));
}

#[test]
fn test_highlight_rust_booleans() {
    let code = "let t = true; let f = false;";
    let spans = get_spans(code, "rust");
    assert!(has_span(&spans, "true", HighlightKind::Constant));
    assert!(has_span(&spans, "false", HighlightKind::Constant));
}

#[test]
fn test_highlight_rust_comments() {
    let code = "// line comment\n/* block */";
    let spans = get_spans(code, "rust");
    assert!(has_span(&spans, "// line comment", HighlightKind::Comment));
    assert!(has_span(&spans, "/* block */", HighlightKind::Comment));
}

#[test]
fn test_highlight_rust_types() {
    let code = "let x: String = String::new(); let y: Vec<i32> = vec![];";
    let spans = get_spans(code, "rust");
    assert!(has_span(&spans, "String", HighlightKind::Type));
    assert!(has_span(&spans, "Vec", HighlightKind::Type));
    assert!(has_span(&spans, "i32", HighlightKind::Type));
}

#[test]
fn test_highlight_rust_function_def() {
    let code = "fn my_function() {}";
    let spans = get_spans(code, "rust");
    assert!(has_span(&spans, "my_function", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_rust_function_call() {
    let code = "foo(); bar::baz();";
    let spans = get_spans(code, "rust");
    assert!(has_span(&spans, "foo", HighlightKind::FunctionName));
    assert!(has_span(&spans, "baz", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_rust_method_call() {
    let code = "x.method(); a.b.c();";
    let spans = get_spans(code, "rust");
    assert!(has_span(&spans, "method", HighlightKind::FunctionName));
    assert!(has_span(&spans, "c", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_rust_attributes() {
    let code = "#[derive(Debug)]\nstruct Foo;";
    let spans = get_spans(code, "rust");
    assert!(has_span(
        &spans,
        "#[derive(Debug)]",
        HighlightKind::Attribute
    ));
}

// ==================== Python ====================

#[test]
fn test_highlight_python_keywords() {
    let code = "def foo(): pass\nif True: return\nfor x in []: break";
    let spans = get_spans(code, "python");
    assert!(has_span(&spans, "def", HighlightKind::Keyword));
    assert!(has_span(&spans, "pass", HighlightKind::Keyword));
    assert!(has_span(&spans, "if", HighlightKind::Keyword));
    assert!(has_span(&spans, "return", HighlightKind::Keyword));
    assert!(has_span(&spans, "for", HighlightKind::Keyword));
    assert!(has_span(&spans, "in", HighlightKind::Keyword));
    assert!(has_span(&spans, "break", HighlightKind::Keyword));
}

#[test]
fn test_highlight_python_numbers() {
    let code = "x = 42\ny = 3.14\nz = 1e10";
    let spans = get_spans(code, "python");
    assert!(has_span(&spans, "42", HighlightKind::Number));
    assert!(has_span(&spans, "3.14", HighlightKind::Number));
    assert!(has_span(&spans, "1e10", HighlightKind::Number));
}

#[test]
fn test_highlight_python_strings() {
    let code = r#"s = "hello"
r = 'world'"#;
    let spans = get_spans(code, "python");
    // Python uses string_content for the inner text
    assert!(spans
        .iter()
        .any(|(t, k)| t.contains("hello") && *k == HighlightKind::String));
}

#[test]
fn test_highlight_python_constants() {
    let code = "t = True\nf = False\nn = None";
    let spans = get_spans(code, "python");
    assert!(has_span(&spans, "True", HighlightKind::Constant));
    assert!(has_span(&spans, "False", HighlightKind::Constant));
    assert!(has_span(&spans, "None", HighlightKind::Constant));
}

#[test]
fn test_highlight_python_comments() {
    let code = "# this is a comment\nx = 1";
    let spans = get_spans(code, "python");
    assert!(has_span(
        &spans,
        "# this is a comment",
        HighlightKind::Comment
    ));
}

#[test]
fn test_highlight_python_function_def() {
    let code = "def my_func(): pass";
    let spans = get_spans(code, "python");
    assert!(has_span(&spans, "my_func", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_python_function_call() {
    let code = "print(foo())";
    let spans = get_spans(code, "python");
    assert!(has_span(&spans, "print", HighlightKind::FunctionName));
    assert!(has_span(&spans, "foo", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_python_method_call() {
    let code = "x.method().chain()";
    let spans = get_spans(code, "python");
    assert!(has_span(&spans, "method", HighlightKind::FunctionName));
    assert!(has_span(&spans, "chain", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_python_decorator() {
    let code = "@decorator\ndef foo(): pass";
    let spans = get_spans(code, "python");
    assert!(has_span(&spans, "@decorator", HighlightKind::Attribute));
}

// ==================== JavaScript ====================

#[test]
fn test_highlight_js_keywords() {
    let code = "function foo() { let x = 1; const y = 2; if (x) return; }";
    let spans = get_spans(code, "javascript");
    assert!(has_span(&spans, "function", HighlightKind::Keyword));
    assert!(has_span(&spans, "let", HighlightKind::Keyword));
    assert!(has_span(&spans, "const", HighlightKind::Keyword));
    assert!(has_span(&spans, "if", HighlightKind::Keyword));
    assert!(has_span(&spans, "return", HighlightKind::Keyword));
}

#[test]
fn test_highlight_js_numbers() {
    let code = "let x = 42; let y = 3.14; let z = 0xff;";
    let spans = get_spans(code, "javascript");
    assert!(has_span(&spans, "42", HighlightKind::Number));
    assert!(has_span(&spans, "3.14", HighlightKind::Number));
    assert!(has_span(&spans, "0xff", HighlightKind::Number));
}

#[test]
fn test_highlight_js_strings() {
    let code = r#"let s = "hello"; let t = 'world';"#;
    let spans = get_spans(code, "javascript");
    assert!(has_span(&spans, "\"hello\"", HighlightKind::String));
    assert!(has_span(&spans, "'world'", HighlightKind::String));
}

#[test]
fn test_highlight_js_constants() {
    let code = "let t = true; let f = false; let n = null; let u = undefined;";
    let spans = get_spans(code, "javascript");
    assert!(has_span(&spans, "true", HighlightKind::Constant));
    assert!(has_span(&spans, "false", HighlightKind::Constant));
    assert!(has_span(&spans, "null", HighlightKind::Constant));
    assert!(has_span(&spans, "undefined", HighlightKind::Constant));
}

#[test]
fn test_highlight_js_comments() {
    let code = "// line\n/* block */";
    let spans = get_spans(code, "javascript");
    assert!(has_span(&spans, "// line", HighlightKind::Comment));
    assert!(has_span(&spans, "/* block */", HighlightKind::Comment));
}

#[test]
fn test_highlight_js_function_def() {
    let code = "function myFunc() {}";
    let spans = get_spans(code, "javascript");
    assert!(has_span(&spans, "myFunc", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_js_function_call() {
    let code = "foo(); bar();";
    let spans = get_spans(code, "javascript");
    assert!(has_span(&spans, "foo", HighlightKind::FunctionName));
    assert!(has_span(&spans, "bar", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_js_method_call() {
    let code = "obj.method().chain();";
    let spans = get_spans(code, "javascript");
    assert!(has_span(&spans, "method", HighlightKind::FunctionName));
    assert!(has_span(&spans, "chain", HighlightKind::FunctionName));
}

// ==================== Go ====================

#[test]
fn test_highlight_go_keywords() {
    let code = "package main\nfunc foo() { if true { return } for {} defer f() }";
    let spans = get_spans(code, "go");
    assert!(has_span(&spans, "package", HighlightKind::Keyword));
    assert!(has_span(&spans, "func", HighlightKind::Keyword));
    assert!(has_span(&spans, "if", HighlightKind::Keyword));
    assert!(has_span(&spans, "return", HighlightKind::Keyword));
    assert!(has_span(&spans, "for", HighlightKind::Keyword));
    assert!(has_span(&spans, "defer", HighlightKind::Keyword));
}

#[test]
fn test_highlight_go_numbers() {
    let code = "x := 42\ny := 3.14";
    let spans = get_spans(code, "go");
    assert!(has_span(&spans, "42", HighlightKind::Number));
    assert!(has_span(&spans, "3.14", HighlightKind::Number));
}

#[test]
fn test_highlight_go_strings() {
    let code = r#"s := "hello"
r := `raw`"#;
    let spans = get_spans(code, "go");
    assert!(has_span(&spans, "\"hello\"", HighlightKind::String));
    assert!(has_span(&spans, "`raw`", HighlightKind::String));
}

#[test]
fn test_highlight_go_constants() {
    let code = "t := true\nf := false\nn := nil";
    let spans = get_spans(code, "go");
    assert!(has_span(&spans, "true", HighlightKind::Constant));
    assert!(has_span(&spans, "false", HighlightKind::Constant));
    assert!(has_span(&spans, "nil", HighlightKind::Constant));
}

#[test]
fn test_highlight_go_comments() {
    let code = "// line\n/* block */";
    let spans = get_spans(code, "go");
    assert!(has_span(&spans, "// line", HighlightKind::Comment));
    assert!(has_span(&spans, "/* block */", HighlightKind::Comment));
}

#[test]
fn test_highlight_go_function_def() {
    let code = "func myFunc() {}";
    let spans = get_spans(code, "go");
    assert!(has_span(&spans, "myFunc", HighlightKind::FunctionName));
}

// ==================== Lua ====================

#[test]
fn test_highlight_lua_keywords() {
    let code = "local function foo() if true then return end for i = 1, 10 do end end";
    let spans = get_spans(code, "lua");
    assert!(has_span(&spans, "local", HighlightKind::Keyword));
    assert!(has_span(&spans, "function", HighlightKind::Keyword));
    assert!(has_span(&spans, "if", HighlightKind::Keyword));
    assert!(has_span(&spans, "then", HighlightKind::Keyword));
    assert!(has_span(&spans, "return", HighlightKind::Keyword));
    assert!(has_span(&spans, "end", HighlightKind::Keyword));
    assert!(has_span(&spans, "for", HighlightKind::Keyword));
    assert!(has_span(&spans, "do", HighlightKind::Keyword));
}

#[test]
fn test_highlight_lua_numbers() {
    let code = "x = 42\ny = 3.14";
    let spans = get_spans(code, "lua");
    assert!(has_span(&spans, "42", HighlightKind::Number));
    assert!(has_span(&spans, "3.14", HighlightKind::Number));
}

#[test]
fn test_highlight_lua_strings() {
    let code = r#"s = "hello"
t = 'world'"#;
    let spans = get_spans(code, "lua");
    // Lua strings include quotes
    assert!(spans
        .iter()
        .any(|(t, k)| t.contains("hello") && *k == HighlightKind::String));
}

#[test]
fn test_highlight_lua_constants() {
    let code = "t = true\nf = false\nn = nil";
    let spans = get_spans(code, "lua");
    assert!(has_span(&spans, "true", HighlightKind::Constant));
    assert!(has_span(&spans, "false", HighlightKind::Constant));
    assert!(has_span(&spans, "nil", HighlightKind::Constant));
}

#[test]
fn test_highlight_lua_comments() {
    let code = "-- line comment\nx = 1";
    let spans = get_spans(code, "lua");
    assert!(has_span(&spans, "-- line comment", HighlightKind::Comment));
}

#[test]
fn test_highlight_lua_function_def() {
    let code = "local function my_func() end";
    let spans = get_spans(code, "lua");
    assert!(has_span(&spans, "my_func", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_lua_function_call() {
    let code = "print(foo())";
    let spans = get_spans(code, "lua");
    assert!(has_span(&spans, "print", HighlightKind::FunctionName));
    assert!(has_span(&spans, "foo", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_lua_method_call() {
    let code = "x:method()\na.b.c()";
    let spans = get_spans(code, "lua");
    assert!(has_span(&spans, "method", HighlightKind::FunctionName));
    assert!(has_span(&spans, "c", HighlightKind::FunctionName));
    // Ensure receiver is NOT highlighted as function
    assert!(!has_span(&spans, "x", HighlightKind::FunctionName));
    assert!(!has_span(&spans, "a", HighlightKind::FunctionName));
    assert!(!has_span(&spans, "b", HighlightKind::FunctionName));
}

#[test]
fn test_highlight_lua_chained_calls() {
    let code = "a.b.c().d.e:f()";
    let spans = get_spans(code, "lua");
    // c and f should be highlighted (they're the called functions)
    assert!(has_span(&spans, "c", HighlightKind::FunctionName));
    assert!(has_span(&spans, "f", HighlightKind::FunctionName));
    // a, b, d, e should NOT be highlighted
    assert!(!has_span(&spans, "a", HighlightKind::FunctionName));
    assert!(!has_span(&spans, "b", HighlightKind::FunctionName));
    assert!(!has_span(&spans, "d", HighlightKind::FunctionName));
    assert!(!has_span(&spans, "e", HighlightKind::FunctionName));
}

// ==================== TypeScript ====================

#[test]
fn test_highlight_ts_types() {
    let code = "let x: string; let y: number; interface Foo {}";
    let spans = get_spans(code, "typescript");
    assert!(has_span(&spans, "interface", HighlightKind::Keyword));
}
