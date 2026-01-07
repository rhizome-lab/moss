# ---
# id = "rust/unnecessary-let"
# severity = "info"
# message = "Unnecessary let binding - consider using the value directly"
# languages = ["rust"]
# ---

; Detects: let x = y; where x is immutable and y is a simple identifier
; This may be intentional for clarity, so severity is info
((let_declaration
  pattern: (identifier) @_alias
  value: (identifier) @_value
  (#not-match? @_alias "^_")) @match)
