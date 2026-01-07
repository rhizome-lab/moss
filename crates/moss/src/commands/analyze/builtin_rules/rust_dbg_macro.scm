# ---
# id = "rust/dbg-macro"
# severity = "warning"
# message = "dbg!() macro found - remove before committing"
# languages = ["rust"]
# allow = ["**/tests/**"]
# ---

((macro_invocation
  macro: (identifier) @_name
  (#eq? @_name "dbg")) @match)
