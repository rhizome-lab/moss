# ---
# id = "js/console-log"
# severity = "info"
# message = "console.log/debug found - remove before committing"
# languages = ["javascript", "typescript", "tsx", "jsx"]
# allow = ["**/tests/**", "**/*.test.*", "**/*.spec.*"]
# ---

; Detects: console.log(), console.debug(), console.info()
((call_expression
  function: (member_expression
    object: (identifier) @_obj
    property: (property_identifier) @_prop)
  (#eq? @_obj "console")
  (#any-of? @_prop "log" "debug" "info")) @match)
