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

## User Override Example

Override the default terse system prompt with a project-specific version:

```bash
# Create project override directory
mkdir -p .moss/prompts

# Create custom terse prompt
cat > .moss/prompts/terse.txt << 'EOF'
You are working on a safety-critical system.
Be extremely precise. Double-check all changes.
Never make assumptions about undefined behavior.
When in doubt, ask for clarification.
EOF
```

This prompt will now be used instead of the built-in terse prompt for all LLM calls in this project.

Override the repair-engine prompt for stricter error handling:

```bash
cat > .moss/prompts/repair-engine.txt << 'EOF'
STRICT REPAIR MODE: Fix errors with zero tolerance for regressions.

Rules:
- Fix ONLY the exact error reported
- Do not touch any other code
- If the fix is ambiguous, leave a TODO comment instead
- Preserve all existing tests and behavior

Process:
1. Read the error message carefully
2. Locate the exact line causing the error
3. Apply the minimal fix
4. Verify the fix doesn't break related code
EOF
```

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

Completed:
1. ✅ Extract `REPAIR_ENGINE_PROMPT` to `src/moss/prompts/repair-engine.txt`
2. ✅ Add `load_prompt()` function that checks `.moss/prompts/` first
3. ✅ Update `agent_loop.py` to use `load_prompt("repair-engine")`
4. ✅ Extract `LLMConfig.system_prompt` to `src/moss/prompts/terse.txt`
5. ✅ Implement workflow loader with TOML parsing and @reference resolution

6. ✅ Add CLI commands (`moss workflow list/show/run`)

Remaining:
- Implement Python workflow protocol for complex logic

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
