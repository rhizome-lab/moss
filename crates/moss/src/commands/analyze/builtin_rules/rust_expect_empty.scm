# ---
# id = "rust/expect-empty"
# severity = "warning"
# message = ".expect() with empty string - provide context message"
# languages = ["rust"]
# ---

; Detects: .expect("") with empty string literal
((call_expression
  function: (field_expression
    field: (field_identifier) @_method)
  arguments: (arguments
    (string_literal) @_msg)
  (#eq? @_method "expect")
  (#eq? @_msg "\"\"")) @match)
