# ---
# id = "hardcoded-secret"
# severity = "error"
# message = "Potential hardcoded secret - use environment variables or config"
# languages = ["rust"]
# allow = ["**/tests/**", "**/examples/**", "**/*.md"]
# ---

; Detects: let password = "..."; let api_key = "..."; etc
; High false positive rate expected - users should allowlist as needed
((let_declaration
  pattern: (identifier) @_name
  value: (string_literal) @_value
  (#match? @_name "(?i)password|secret|api.?key|token|credential")
  (#not-match? @_value "^\"\"$")) @match)
