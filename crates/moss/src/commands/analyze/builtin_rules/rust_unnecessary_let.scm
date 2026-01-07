# ---
# id = "rust/unnecessary-let"
# severity = "info"
# message = "Unnecessary let binding - consider using the value directly"
# languages = ["rust"]
# ---

; Detects: let x = y; where both are simple identifiers
; Excludes: underscore-prefixed names, None (Option variant)
; Note: Also matches `let mut` - tree-sitter can't easily exclude sibling nodes
; This may be intentional for clarity, so severity is info
(let_declaration
  pattern: (identifier) @_alias
  value: (identifier) @_value
  (#not-match? @_alias "^_")
  (#not-eq? @_value "None")) @match
