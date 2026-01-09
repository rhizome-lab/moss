# ---
# id = "python/tuple-return"
# severity = "info"
# message = "Function returns tuple - consider using a dataclass or NamedTuple"
# languages = ["python"]
# ---

; Detects functions with return type annotation tuple[...]
; NamedTuple or dataclass provides named access and better IDE support
(function_definition
  return_type: (type
    (generic_type
      (identifier) @_tuple
      (#eq? @_tuple "tuple"))) @match)
