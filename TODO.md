# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- [x] Fix remaining DWIM test failures (100% passing)
  - Skip first-word alias matching for NL markers (show/find/get)
  - Skip first-word-base typo matching for NL markers
  - Updated test expectations for structure → skeleton alias
- [ ] Add failure mode tests: Rust binary missing, invalid paths, malformed files
- [ ] Ensure all failure modes have informative error messages

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
- [x] Replace TF-IDF with embedding-based matching (fastembed/bge-small-en)
- [x] Simplify matching logic (tool-like vs NL detection)
- [x] Add `moss dwim --analyze` for embedding similarity debugging
- [x] Weighted example phrases per tool (ToolInfo.examples field)
- [x] Fix all DWIM tests (23/23 = 100% passing)
  - Skip first-word alias/tool matching for NL markers
  - Skip first-word-base typo matching for NL markers
- [ ] Natural language → tree operation mapping
- [ ] **Tool unification** (reduce fragmentation from MossAPI auto-registration)
  - Currently: 119 tools registered (only 8 match CLI commands)
  - Problem: MossAPI sub-tools (skeleton_extract, health_check_docs, etc.) compete with unified tools
  - Goal: Route to unified tools (skeleton, health, deps) not internal variants
  - High-similarity pairs to consider unifying:
    - `dependencies_analyze` ↔ `external_deps_analyze` (0.855)
    - `skeleton` ↔ `skeleton_extract/format/expand` (consolidate)
    - `health` ↔ `health_check/analyze_*` (consolidate)
  - Options: Tier system (cli vs mcp), filter MossAPI internals, or consolidate at API level

### Distribution & Installation
- [ ] Auto-updates: check for new versions, prompt user
- [ ] Portable installation: single binary or minimal deps
- [ ] Pre-built binaries for common platforms (GitHub Actions)

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