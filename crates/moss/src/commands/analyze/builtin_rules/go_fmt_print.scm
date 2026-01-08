# ---
# id = "go/fmt-print"
# severity = "info"
# message = "fmt.Print found - consider using structured logging"
# languages = ["go"]
# allow = ["**/tests/**", "**/*_test.go", "**/examples/**", "**/cmd/**"]
# ---

((call_expression
  function: (selector_expression
    operand: (identifier) @_pkg
    field: (field_identifier) @_method)
  (#eq? @_pkg "fmt")
  (#any-of? @_method "Print" "Println" "Printf")) @match)
