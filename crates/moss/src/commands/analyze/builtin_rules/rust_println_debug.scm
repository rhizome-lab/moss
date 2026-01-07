# ---
# id = "rust/println-debug"
# severity = "info"
# message = "println!/print! found - consider using tracing or log crate"
# languages = ["rust"]
# allow = ["**/tests/**", "**/examples/**", "**/bin/**", "**/main.rs"]
# ---

((macro_invocation
  macro: (identifier) @_name
  (#any-of? @_name "println" "print" "eprintln" "eprint")) @match)
