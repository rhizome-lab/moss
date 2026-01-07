//! Embedded builtin rules for syntax-based linting.
//!
//! Rules are embedded at compile time and loaded as the lowest-priority source.
//! Users can override or disable them via ~/.config/moss/rules/ or .moss/rules/.

/// A builtin rule definition (id, content).
pub struct BuiltinRule {
    pub id: &'static str,
    pub content: &'static str,
}

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
    // Cross-language rules
    BuiltinRule {
        id: "no-fixme-comment",
        content: include_str!("no_fixme_comment.scm"),
    },
];
