# ---
# id = "go/many-returns"
# severity = "info"
# message = "Function has 3+ return values - consider using a struct"
# languages = ["go"]
# ---

; Detects functions with 3 or more return values
; More than (value, error) usually warrants a result struct
(function_declaration
  result: (parameter_list
    (parameter_declaration)
    (parameter_declaration)
    (parameter_declaration)) @match)
