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

**Phase 2: Unified tree model** (see `docs/philosophy.md` - Unified Codebase Tree)
- [ ] Merge filesystem + AST into single tree data structure
- [ ] Uniform node addressing with `/`: `src/main.py/Foo/bar`
  - Filesystem is source of truth for file vs directory boundary
  - Also accept `::` syntax, normalize internally
- [ ] Depth-based expansion: `--depth 1` (default), `--depth 2`, `--all`
- [ ] Four primitives replacing 100+ tools:
  - `view [path]` - see node (skeleton, source, tree) with `--deps`, `--summary`
  - `find [query]` - search with composable filters `--type`, `--calls`, `--called-by`
  - `edit <path>` - modify node with `--insert`, `--replace`, `--delete`
  - `analyze [path]` - compute properties with `--health`, `--complexity`, `--security`

**Phase 3: DWIM integration**
- [x] Replace TF-IDF with embedding-based matching (fastembed/bge-small-en)
- [x] Simplify matching logic (tool-like vs NL detection)
- [x] Add `moss dwim --analyze` for embedding similarity debugging
- [x] Weighted example phrases per tool (ToolInfo.examples field)
- [x] Fix all DWIM tests
- [ ] Natural language → tree primitive mapping (view/find/edit/analyze)
- [ ] Consolidate MossAPI: 30 sub-APIs → 4 primitive APIs matching CLI/MCP

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