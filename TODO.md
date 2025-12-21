# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- [ ] **Exception Handler Cleanup**: Fix remaining ~130 generic `except Exception` in tui.py, heuristics.py, metrics.py, etc.
- [ ] **Incremental Test Runner**: Run tests only for changed files (pytest --co + git diff filtering)
- [ ] **GEMINI.md Prompt Engineering**: Add constraints to prevent stub code and require test verification
- [ ] **TUI Syntax Highlighting**: High-quality code highlighting in file previews
- [ ] **Local Model Constrained Inference**: Implement GBNF (GGML BNF) for structured output

## Recently Completed

- **Gemini Cleanup** (Dec 2025): Removed 6 stub loops, fixed 10 exception handlers, documented LLM differences
- **Memory & Resource Metrics** (Dec 2025): Show context and RAM usage (with breakdown) for every command
- **Adaptive and TUI enhancements** (Dec 2025):
  - Refactored core infrastructure (Sandbox, Workflows, RefCheck, Telemetry)
  - Implemented dynamic loop controls (Context, Budget, Depth, Models)
  - Modernized TUI with multi-mode navigation and Git dashboard
  - Strengthened safety with reliability guardrails and error localization

## Active Backlog

- Workflow argument passing improvement
- [ ] **Symbol Hover Info**: (TUI) Show signatures/docstrings on hover (Expanded)
- [ ] **Context Elision Heuristics**: (Core) Prune large files while preserving anchors (Expanded)
- [ ] **Shadow Git Branching**: (Git) Support for multiple concurrent experiment branches (Expanded)

**Large:**
- [ ] **Comprehensive Telemetry & Analysis**: (Partially Complete - see TelemetryAPI)
  - Track all token usage, patterns, and codebase access patterns by default
  - Store maximal metadata for every session
  - Built-in high-quality analysis tools (CLI & visual)
- [ ] Memory system - layered memory for cross-session learning (see `docs/memory-system.md`)

## Future Work

### Agent Research & Optimization
- [ ] **LLM Editing Performance Comparison**:
  - Investigate Gemini 3 Flash and Gemini 3 Pro issues with invalid code edits
  - Compare with Claude Code and Opus to identify architectural differences
- [ ] **YOLO Mode Evaluation**: Evaluate if a "YOLO mode" aligns with Moss architectural principles
- [ ] **Memory Usage Optimization**: Ensure Moss keeps RAM usage extremely low
- [ ] **Extensible Agent Modes**: Refactor TUI modes into a plugin-based system (Partially Complete)
- [ ] **'Diffusion-like' methods for large-scale refactors**:
  - Parallelize implementation of components based on high-level contracts
- [ ] **Small/Local Model Brute Force**: Fast models with higher iteration/voting counts
- [ ] **Fine-tuned Tiny Models**: Extreme optimization with models like 100M RWKV

### Codebase Tree Consolidation (see `docs/codebase-tree.md`)

**Phase 1: Python CLI delegates to Rust (remove Python implementations)**
- [ ] `skeleton` → delegate to Rust `view`
- [ ] `summarize` → delegate to Rust `view`
- [ ] `anchors` → delegate to Rust `search` with type filter
- [ ] `query` → delegate to Rust `search`
- [ ] `tree` → delegate to Rust `view` (directory-level)

**Phase 2: Unified tree model**
- [ ] Merge filesystem + AST into single tree data structure
- [ ] Consistent "context + node + children" view format

**Phase 3: DWIM integration**
- [ ] Natural language → tree operation mapping

### Reference Resolution (GitHub-level)
- [ ] Full import graph with alias tracking
- [ ] Variable scoping analysis
- [ ] Type inference for method calls
- [ ] Cross-language reference tracking (Python ↔ Rust) (Partially Complete)

## Notes

### Design Principles
See `docs/philosophy.md` for full tenets. Key goals:
- Minimize LLM usage (structural tools first)
- Maximize useful work per token
- Low barrier to entry, works on messy codebases
- **Heuristic Guardrails**: Mitigate LLM unreliability with verification loops and deterministic rules
- **Resource Efficiency**: High memory usage is a bug; favor streaming and lazy loading