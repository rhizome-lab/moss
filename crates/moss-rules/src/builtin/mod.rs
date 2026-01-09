//! Embedded builtin rules for syntax-based linting.
//!
//! Rules are embedded at compile time and loaded as the lowest-priority source.
//! Users can override or disable them via ~/.config/moss/rules/ or .moss/rules/.

use crate::BuiltinRule;

/// All embedded builtin rules.
pub const BUILTIN_RULES: &[BuiltinRule] = &[
    // Rust rules
    BuiltinRule {
        id: "rust/todo-macro",
        content: include_str!("rust_todo_macro.scm"),
    },
    BuiltinRule {
        id: "rust/println-debug",
        content: include_str!("rust_println_debug.scm"),
    },
    BuiltinRule {
        id: "rust/dbg-macro",
        content: include_str!("rust_dbg_macro.scm"),
    },
    BuiltinRule {
        id: "rust/expect-empty",
        content: include_str!("rust_expect_empty.scm"),
    },
    BuiltinRule {
        id: "rust/unwrap-in-impl",
        content: include_str!("rust_unwrap_in_impl.scm"),
    },
    BuiltinRule {
        id: "rust/unnecessary-let",
        content: include_str!("rust_unnecessary_let.scm"),
    },
    BuiltinRule {
        id: "rust/unnecessary-type-alias",
        content: include_str!("rust_unnecessary_type_alias.scm"),
    },
    BuiltinRule {
        id: "rust/chained-if-let",
        content: include_str!("rust_chained_if_let.scm"),
    },
    BuiltinRule {
        id: "rust/numeric-type-annotation",
        content: include_str!("rust_numeric_type_annotation.scm"),
    },
    BuiltinRule {
        id: "rust/tuple-return",
        content: include_str!("rust_tuple_return.scm"),
    },
    BuiltinRule {
        id: "hardcoded-secret",
        content: include_str!("hardcoded_secret.scm"),
    },
    // JavaScript/TypeScript rules
    BuiltinRule {
        id: "js/console-log",
        content: include_str!("js_console_log.scm"),
    },
    BuiltinRule {
        id: "js/unnecessary-const",
        content: include_str!("js_unnecessary_const.scm"),
    },
    BuiltinRule {
        id: "typescript/tuple-return",
        content: include_str!("typescript_tuple_return.scm"),
    },
    // Python rules
    BuiltinRule {
        id: "python/print-debug",
        content: include_str!("python_print_debug.scm"),
    },
    BuiltinRule {
        id: "python/breakpoint",
        content: include_str!("python_breakpoint.scm"),
    },
    BuiltinRule {
        id: "python/tuple-return",
        content: include_str!("python_tuple_return.scm"),
    },
    // Go rules
    BuiltinRule {
        id: "go/fmt-print",
        content: include_str!("go_fmt_print.scm"),
    },
    BuiltinRule {
        id: "go/many-returns",
        content: include_str!("go_many_returns.scm"),
    },
    // Ruby rules
    BuiltinRule {
        id: "ruby/binding-pry",
        content: include_str!("ruby_binding_pry.scm"),
    },
    // Cross-language rules
    BuiltinRule {
        id: "no-todo-comment",
        content: include_str!("no_todo_comment.scm"),
    },
    BuiltinRule {
        id: "no-fixme-comment",
        content: include_str!("no_fixme_comment.scm"),
    },
];
