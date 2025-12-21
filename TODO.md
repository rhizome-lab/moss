# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- [ ] Add failure mode tests: Rust binary missing, invalid paths, malformed files
- [ ] Ensure all failure modes have informative error messages

## Recently Completed

- **Session Dec 21 2025 (later)**:
  - Completed Rust delegation for anchors/tree commands
  - Simplified cmd_anchors and cmd_tree to just call Rust
  - Removed Python fallback (Rust is required for proper installation)
  - Phase 1 Rust delegation complete

- **Session Dec 21 2025 (latest)**:
  - Rust delegation for expand/callers/callees commands
  - Added rust_expand, rust_callers, rust_callees to rust_shim.py
  - Updated CLI to try Rust first with Python fallback

- **Session Dec 21 2025 (later)**:
  - Rust delegation for skeleton/summarize/search-tree/view
  - Fixed Rust search-tree extension patterns (`.py`, `.rs`)
  - Documented conversational loop pattern as future research
  - Verified hierarchical (not conversational) agent architecture

- **Session Dec 21 2025 (late)**:
  - Runtime Memory Bounds: streaming LLM responses, context eviction (max_context_steps)
  - Brute Force Voting: wired to LLMGenerator with majority/consensus/first_valid strategies
  - Session Log Comparison Tool: compare_sessions() for Claude vs Gemini CLI edit analysis

- **Session Dec 21 2025 (continued)**:
  - Claude vs Gemini CLI edit paradigm analysis (see `docs/edit-paradigm-comparison.md`)
  - Lazy imports for reduced baseline memory
  - Extensible TUI modes with plugin discovery
  - BruteForceConfig for small model voting

- **Session Dec 21 2025**:
  - Symbol hover info in TUI with signatures/docstrings
  - Context elision heuristics (anchor-preserving pruning)
  - Experiment branching for parallel shadow branches
  - GBNF grammar support for constrained local model inference
- **CLI & Workflow Improvements** (Dec 2025):
  - CLAUDE.md/GEMINI.md: Prefer CLI over MCP, added anti-stub constraints
  - Workflow `--arg` option for passing key=value arguments
  - Incremental test runner (`--incremental`) in watch command
  - TUI syntax highlighting for file previews (Python, Rust, JS, TS, Go, Ruby)
- **Exception Handler Cleanup** (Dec 2025): Fixed 150+ generic `except Exception` across 40+ files
- **Gemini Cleanup** (Dec 2025): Removed 6 stub loops, fixed 10 exception handlers
- **Memory & Resource Metrics** (Dec 2025): Show context and RAM usage for every command

## Active Backlog

**Large:**
- [ ] **Comprehensive Telemetry & Analysis**: (Partially Complete - see TelemetryAPI)
  - Track all token usage, patterns, and codebase access patterns by default
  - Store maximal metadata for every session
  - Built-in high-quality analysis tools (CLI & visual)
- [ ] Memory system - layered memory for cross-session learning (see `docs/memory-system.md`)

## Future Work

### Agent Research & Optimization
- [ ] **Conversational Loop Pattern**: Add optional conversation-accumulating mode to DWIMLoop
  - For evals: measure context utilization vs hierarchical approach
  - Session-wide RAG: retrieve from full session history, not just TaskTree
  - Compare: hierarchical (current) vs conversational vs hybrid approaches
- [ ] **YOLO Mode Evaluation**: Evaluate if a "YOLO mode" aligns with Moss architectural principles
- [ ] **'Diffusion-like' methods for large-scale refactors**:
  - Parallelize implementation of components based on high-level contracts
- [ ] **Fine-tuned Tiny Models**: Extreme optimization with models like 100M RWKV

### Codebase Tree Consolidation (see `docs/codebase-tree.md`)

**Phase 1: Python CLI delegates to Rust** (complete)
- [x] `skeleton` → Rust `skeleton`
- [x] `summarize` → Rust `summarize`
- [x] `expand` → Rust `expand`
- [x] `callers` → Rust `callers`
- [x] `callees` → Rust `callees`
- [x] `anchors` → Rust `anchors`
- [x] `tree` → Rust `tree`
- `query` - Python-only (rich filtering Rust lacks, no delegation needed)

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