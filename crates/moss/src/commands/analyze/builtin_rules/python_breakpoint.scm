# ---
# id = "python/breakpoint"
# severity = "warning"
# message = "breakpoint() found - remove before committing"
# languages = ["python"]
# allow = ["**/tests/**"]
# ---

((call
  function: (identifier) @_name
  (#eq? @_name "breakpoint")) @match)
