# ---
# id = "rust/unnecessary-type-alias"
# severity = "info"
# message = "Type alias to simple type - consider using the type directly"
# languages = ["rust"]
# ---

; Detects: type X = Y; where both are simple type identifiers
; May be intentional for re-exports or semantic clarity
(type_alias_declaration
  name: (type_identifier) @_alias
  type: (type_identifier) @_target) @match
