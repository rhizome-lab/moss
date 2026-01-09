# ---
# id = "typescript/tuple-return"
# severity = "info"
# message = "Function returns tuple - consider using an interface with named properties"
# languages = ["typescript", "tsx"]
# ---

; Detects functions returning tuple types like [A, B]
; Named interfaces are more self-documenting
(function_declaration
  return_type: (type_annotation (tuple_type)) @match)

(arrow_function
  return_type: (type_annotation (tuple_type)) @match)

(method_definition
  return_type: (type_annotation (tuple_type)) @match)
