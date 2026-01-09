# ---
# id = "rust/tuple-return"
# severity = "info"
# message = "Function returns tuple - consider using a struct with named fields"
# languages = ["rust"]
# ---

; Detects functions returning tuple types like (A, B)
; Named structs are more self-documenting and refactor-friendly
(function_item
  return_type: (tuple_type) @match)
