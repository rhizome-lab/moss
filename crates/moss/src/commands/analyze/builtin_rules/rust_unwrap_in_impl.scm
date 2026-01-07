# ---
# id = "rust/unwrap-in-impl"
# severity = "info"
# message = ".unwrap() found - consider using ? or .expect() with context"
# languages = ["rust"]
# allow = ["**/tests/**", "**/test_*.rs", "**/*_test.rs", "**/*_tests.rs", "**/examples/**", "**/benches/**"]
# ---

; Detects: .unwrap() calls
((call_expression
  function: (field_expression
    field: (field_identifier) @_method)
  (#eq? @_method "unwrap")) @match)
