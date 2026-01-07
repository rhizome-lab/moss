# ---
# id = "rust/todo-macro"
# severity = "warning"
# message = "todo!() macro found - implement before merging"
# languages = ["rust"]
# allow = ["**/tests/**", "**/*_test.rs", "**/test_*.rs"]
# ---

((macro_invocation
  macro: (identifier) @_name
  (#any-of? @_name "todo" "unimplemented")) @match)
