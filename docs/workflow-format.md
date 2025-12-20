# Workflow Externalization Format

Data-driven workflow definitions. TOML is the default plugin; Python workflows remain for complex logic.

## Design Goals

- External prompts: load from files, no code changes to update wording
- Composable loops: define once, reuse across agents
- User customization: `.moss/workflows/` for project-specific overrides
- Plugin architecture: TOML loader is just one implementation

## Directory Structure

```
.moss/
  workflows/
    repair.toml          # Override built-in repair workflow
    custom-review.toml   # User-defined workflow
  prompts/
    repair-engine.txt    # Override built-in prompt
```

Built-in workflows ship in `src/moss/workflows/` and are loaded as defaults.

## TOML Schema

### Prompt Files

Plain text files (`.txt`) or TOML with metadata:

```toml
# prompts/repair-engine.toml
[meta]
name = "repair-engine"
version = "1.0"
description = "Focused prompt for fixing compilation errors"

[prompt]
text = """
REPAIR MODE: Previous changes caused errors. Fix them precisely.

Rules:
- Focus ONLY on fixing the reported errors
- Do not refactor or improve unrelated code
- Preserve the original intent of the code
- If a fix is unclear, make the minimal safe change

For each error:
1. Identify the root cause from the error message and location
2. Apply the smallest fix that resolves it
3. If the error has a suggestion, prefer that fix
"""
```

Or simply `repair-engine.txt` with just the prompt text (no metadata).

### Workflow Files

```toml
# workflows/validate-fix.toml
[workflow]
name = "validate-fix"
description = "Validate changes and fix errors in a loop"
version = "1.0"

[workflow.limits]
max_steps = 10
token_budget = 50000
timeout_seconds = 300

[workflow.llm]
model = "gemini/gemini-3-flash-preview"
temperature = 0.0
system_prompt = "@prompts/terse"  # Reference to prompt file

[[workflow.steps]]
name = "validate"
tool = "validator.run"
type = "tool"
on_error = "skip"  # Continue to fix step even if validation fails

[[workflow.steps]]
name = "analyze"
tool = "llm.analyze"
type = "llm"
input_from = "validate"
prompt = "@prompts/repair-engine"  # Injected when errors present

[[workflow.steps]]
name = "fix"
tool = "patch.apply"
type = "tool"
input_from = "analyze"
on_error = { action = "goto", target = "validate" }
max_retries = 3
```

### Agent Definitions

Agents compose workflows with tool sets:

```toml
# agents/repair-agent.toml
[agent]
name = "repair-agent"
description = "Fixes compilation errors using structural tools"

workflow = "@workflows/validate-fix"

[agent.tools]
enabled = ["skeleton", "grep", "patch", "validator"]
disabled = []

[agent.context]
# What context to inject
include_diagnostics = true
include_memory = true
peek_first = true  # Enforce expand-before-edit
```

## Reference Syntax

`@path/to/resource` references are resolved at load time:
- `@prompts/name` → loads from prompts directory
- `@workflows/name` → loads workflow definition
- Searches: `.moss/` first, then `src/moss/` for built-ins

## Loader API

```python
from moss.workflows import load_workflow, load_prompt

# Load with user overrides
workflow = load_workflow("validate-fix")  # Checks .moss/ then built-ins
prompt = load_prompt("repair-engine")

# Explicit paths
workflow = load_workflow(Path(".moss/workflows/custom.toml"))
```

## Migration Path

1. Extract `REPAIR_ENGINE_PROMPT` to `src/moss/prompts/repair-engine.txt`
2. Add `load_prompt()` function that checks `.moss/prompts/` first
3. Update `agent_loop.py` to use `load_prompt("repair-engine")`
4. Later: extract full workflow definitions

## Complex Workflows

For logic that can't be expressed in TOML (conditionals, dynamic steps), use Python:

```python
# workflows/complex.py
from moss.workflows import Workflow, Step

class ComplexWorkflow(Workflow):
    def build_steps(self, context):
        steps = [Step("validate", "validator.run")]
        if context.has_tests:
            steps.append(Step("test", "pytest.run"))
        return steps
```

Python workflows implement the same `Workflow` protocol and are loaded via entry points.
