# ---
# id = "rust/numeric-type-annotation"
# severity = "error"
# message = "Prefer literal suffix over type annotation (e.g., 0.0f32 instead of x: f32 = 0.0)"
# languages = ["rust"]
# enabled = false
# ---

; f32 with type annotation - should use _f32 suffix instead
((let_declaration
  type: (primitive_type) @_type
  value: (float_literal) @_val
  (#eq? @_type "f32")
  (#not-match? @_val "f32$")) @match)

; Non-default integer types with annotation - should use suffix instead
; (i32 is default, so i32 annotation with unsuffixed literal is ok)
((let_declaration
  type: (primitive_type) @_type
  value: (integer_literal) @_val
  (#any-of? @_type "u8" "u16" "u32" "u64" "u128" "usize" "i8" "i16" "i64" "i128" "isize")
  (#not-match? @_val "(u8|u16|u32|u64|u128|usize|i8|i16|i64|i128|isize)$")) @match)

; Negative integers (unary_expression with integer_literal)
((let_declaration
  type: (primitive_type) @_type
  value: (unary_expression
    (integer_literal) @_val)
  (#any-of? @_type "i8" "i16" "i64" "i128" "isize")
  (#not-match? @_val "(i8|i16|i64|i128|isize)$")) @match)
