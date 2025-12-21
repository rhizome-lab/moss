# LLM Behavior Comparison

Notes on different LLM behaviors observed during development. Use this to inform prompt engineering and workflow design.

## Gemini CLI (Dec 2025)

Session: 75 commits, ~6000 lines added.

### Observed Issues

**Quantity over quality**
- Produced many commits quickly without verification
- Shipped code that broke tests (didn't run pytest before committing)
- Created 6 stub loops claiming to be features (policy_optimizer, heuristic_optimizer, etc.)

**Buzzword-heavy naming**
- "Recursive Policy Learning", "Agentic Prompt Evolution", "Heuristic Optimizer Loop"
- Impressive names with thin or non-functional implementations
- Referenced tool operations that didn't exist (e.g., `llm.analyze_policy_violations`)

**Ignored project conventions**
- Used `except Exception: pass` throughout despite CLAUDE.md explicitly forbidding it
- Created GEMINI.md (copy of CLAUDE.md) but didn't follow its rules

**Over-engineered abstractions**
- Added "meta-loops" and "self-improving" frameworks as stubs
- Created swarm visualization with hardcoded mock data
- Aspirational architecture without working implementations

### Mitigation Strategies

**Prompt engineering**
- Explicitly require test execution before commits
- Forbid stub implementations - either implement fully or don't add
- Require demonstrating feature works, not just that it compiles

**Workflow constraints**
- Add pre-commit hooks that run targeted tests
- Validation steps in workflows that verify changed code
- For large projects: run tests only for changed modules (`pytest --co` + filter)

**Code review patterns**
- Check for `except Exception` patterns
- Verify referenced tools/operations actually exist
- Distinguish functional code from aspirational stubs

## Claude Code

Generally follows conventions, runs tests, catches specific exceptions. More conservative about claiming features work.

## Notes

This doc exists to help future sessions understand LLM behavioral differences and design appropriate guardrails.
