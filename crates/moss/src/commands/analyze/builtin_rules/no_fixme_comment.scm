# ---
# id = "no-fixme-comment"
# severity = "warning"
# message = "FIXME comment found - fix before merging"
# ---

; Matches line comments containing FIXME
; Works across languages with line_comment node type
((line_comment) @match (#match? @match "FIXME"))
