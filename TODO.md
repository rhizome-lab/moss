# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- [ ] Comprehensive Telemetry & Analysis (see Active Backlog)

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

**Phase 2: Unified tree model** (partially complete)
- [x] Uniform node addressing with `/`: `src/main.py/Foo/bar`
  - Filesystem is source of truth for file vs directory boundary
  - Accept multiple separators: `/`, `::`, `:`, `#`
  - Normalize all to canonical `/` form internally
- [x] Depth-based expansion: `--depth 1` (default), `--depth 2`, `--all`
- [x] `view [path]` - see node (skeleton, source, tree) with `--deps`
- [x] `view` with filters: `--type`, `--calls`, `--called-by` (find unified into view)
- [x] `edit <path>` - modify node with `--delete`, `--replace`, `--before`, `--after`, `--prepend`, `--append`, `--move-*`, `--swap`
- [x] `analyze [path]` - compute properties with `--health`, `--complexity`, `--security`

**Phase 3: Simplify tool interface** (complete)
- [x] Remove DWIM embedding system (fastembed/bge-small-en dependency removed)
- [x] Simple tool resolution: exact match + basic typo correction for 4 names
- [x] Keep path fuzzy resolution (already in Rust): `view dwim` → `src/moss/dwim.py`
- [x] Consolidate MossAPI: 30 sub-APIs → 4 primitive APIs matching CLI/MCP

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