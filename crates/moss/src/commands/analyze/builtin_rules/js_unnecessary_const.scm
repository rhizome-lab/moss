# ---
# id = "js/unnecessary-const"
# severity = "info"
# message = "Unnecessary const binding - consider using the value directly"
# languages = ["javascript", "typescript", "tsx", "jsx"]
# ---

; Detects: const x = y; where both are simple identifiers
; Excludes: undefined, Infinity, NaN (global constants)
((lexical_declaration
  kind: "const"
  (variable_declarator
    name: (identifier) @_alias
    value: (identifier) @_value))
  (#not-any-of? @_value "undefined" "Infinity" "NaN")) @match
