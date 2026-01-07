# ---
# id = "no-todo-comment"
# severity = "info"
# message = "TODO comment found"
# ---

; Matches line comments containing TODO
((line_comment) @match (#match? @match "TODO"))
