# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Current Focus: CLI & Agent Experience

Dogfooding and CLI improvement are the same work stream. The goal is to make `moss agent` and the 3 primitives (view, edit, analyze) work reliably.

**Iterate:**
1. Run `moss agent "task description"` on real tasks
2. Log issues to `logs/dogfood-YYYY-MM-DD.md`
3. Fix issues, improve prompts/tooling
4. Repeat

**Known Issues:**
- [x] Agent path resolution - FIXED: now routes through Rust CLI with fuzzy resolution
- [ ] Agent stuck in retry loop (3x same intent) - needs fallback strategy
- [ ] Agent behavior inconsistent (some runs hit max turns for same query)

**Docs Alignment:**
- [x] `docs/dwim-architecture.md` - rewritten for 3 primitives
- [x] `docs/primitives-spec.md` - added `analyze` section
- [x] `docs/agentic-loop.md` - updated examples for view/edit/analyze
- [x] `docs/codebase-tree.md` - updated example to show current dwim.py structure
- [x] `docs/tools.md` - rewritten for 3 primitives + legacy migration
- [x] `docs/cli/commands.md` - added view/edit/analyze docs, deprecated health
- [x] `CLAUDE.md` - updated dogfooding section for 3 primitives
- [ ] Remaining: prior-art.md, hybrid-loops.md, etc. (lower priority)

**Unified Plumbing for 3 Primitives:**
- [x] Path resolution unified: `path_resolve::resolve_unified` used by view, edit, analyze
- [x] Add `--kind` filter to analyze (uses `--kind` to avoid `-t` conflict with `--threshold`)
- [x] Analyze uses unified resolution for symbol targeting (`analyze cli.py/func --complexity`)
- [ ] Python edit uses separate file/symbol targeting (LLM-based, intentionally different)

**CLI Cleanup:**
- [x] `dwim` CLI - REMOVED (module kept for alias resolution)
- [x] `loop` CLI - REMOVED along with predefined loops (simple, critic, etc.)
- [x] `patterns`, `git-hotspots` - NOT slow (6s, 2.5s), keeping both
- [x] `--compact` mode on patterns (added)
- [x] Large file detection in `analyze --health` (shows top 10 files >500 lines)

**Keys:** see `.env.example` for ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY

## Next Up

- [ ] Symbol-level token tracking in telemetry (extend file tokens to symbol paths)
- [ ] Agent stuck in retry loop fallback strategy (see Known Issues above)

## Backlog

**Telemetry** (see `docs/telemetry.md`):
- [x] `moss telemetry` CLI with aggregate analysis
- [x] HTML dashboard output
- [x] Plugin architecture for log formats (LogParser protocol)
- [x] File-level token tracking (`file_tokens` in SessionAnalysis)
- [x] Gemini CLI log parser
- [x] Real-time telemetry mode (`--watch`)
- [ ] Tokens per function/symbol (extend file tracking)

**Memory System** (see `docs/memory-system.md`):
- [ ] Layered memory for cross-session learning

## Future Work

### Agent Research
- [ ] Conversational loop pattern (vs hierarchical)
- [ ] YOLO mode evaluation
- [ ] Diffusion-like parallel refactors
- [ ] Fine-tuned tiny models (100M RWKV)

### Codebase Tree (see `docs/codebase-tree.md`)
Phase 1-3 complete. See changelog.

### Distribution
- [ ] Auto-updates
- [ ] Portable single binary
- [ ] Pre-built binaries (GitHub Actions)

### Reference Resolution
- [ ] Full import graph with alias tracking
- [ ] Variable scoping analysis
- [ ] Type inference for method calls
- [ ] Cross-language tracking (Python ↔ Rust)

## Notes

### Design Principles
See `docs/philosophy.md`. Key goals:
- **Generalize, Don't Multiply**: One flexible solution over N specialized ones
- **Three Primitives**: view, edit, analyze (composable, not specialized)
- Minimize LLM usage (structural tools first)
- Maximize useful work per token
