# ---
# id = "ruby/binding-pry"
# severity = "warning"
# message = "binding.pry found - remove debug statement before committing"
# languages = ["ruby"]
# allow = ["**/tests/**", "**/test/**", "**/spec/**"]
# ---

((call
  receiver: (identifier) @_receiver
  method: (identifier) @_method
  (#eq? @_receiver "binding")
  (#any-of? @_method "pry" "irb")) @match)
