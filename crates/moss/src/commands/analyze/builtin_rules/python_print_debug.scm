# ---
# id = "python/print-debug"
# severity = "info"
# message = "print() found - consider using logging module"
# languages = ["python"]
# allow = ["**/tests/**", "**/test_*.py", "**/*_test.py", "**/examples/**", "**/__main__.py"]
# ---

((call
  function: (identifier) @_name
  (#eq? @_name "print")) @match)
