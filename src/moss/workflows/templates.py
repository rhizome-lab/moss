"""Workflow templates for 'moss workflow new'."""

AGENTIC_WORKFLOW = """# Agentic workflow with LLM decision loop
[workflow]
name = "{name}"
description = "Agentic workflow for autonomous task execution"
version = "1.0"

[workflow.limits]
max_turns = 20
timeout_seconds = 300

[workflow.context]
strategy = "task_tree"

[workflow.cache]
strategy = "in_memory"
preview_length = 500

[workflow.retry]
strategy = "exponential"
max_attempts = 3
base_delay = 1.0

[workflow.llm]
strategy = "simple"
model = "gemini/gemini-2.0-flash"
system_prompt = \"\"\"Commands:
- view [path] - tree, file skeleton, or symbol (e.g. view cli.py/func)
- edit <file> "task" - LLM edit (-s SYMBOL to target function/class)
- analyze [--health|--complexity|--security] [-t N]
- done

Think between commands. One command per line.
\"\"\"
allow_parallel = true
"""

STEP_WORKFLOW = """# Step-based workflow with predefined actions
[workflow]
name = "{name}"
description = "Step-based workflow for validation tasks"
version = "1.0"

[workflow.limits]
max_turns = 10

[workflow.context]
strategy = "flat"

[workflow.retry]
strategy = "exponential"
max_attempts = 2

[[steps]]
name = "check-health"
action = "analyze --health"
on_error = "skip"

[[steps]]
name = "check-complexity"
action = "analyze --complexity -t 8"
on_error = "skip"
"""

TEMPLATES = {
    "agentic": AGENTIC_WORKFLOW,
    "step": STEP_WORKFLOW,
}
