# ---
# id = "ts/unnecessary-const"
# severity = "info"
# message = "Unnecessary const binding - consider using the value directly"
# languages = ["javascript", "typescript", "tsx", "jsx"]
# ---

; Detects: const x = y; where both are simple identifiers
((lexical_declaration
  kind: "const"
  (variable_declarator
    name: (identifier) @_alias
    value: (identifier) @_value)) @match)
