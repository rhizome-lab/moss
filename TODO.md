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

**CLI Cleanup:**
- [ ] `dwim` - may no longer be necessary with 3 primitives
- [ ] `workflow` vs `loop` - redundant? consolidate?
- [ ] `patterns`, `git-hotspots` - slow, consider Rust port
- [ ] Missing `--compact` mode on `roadmap` and other commands
- [ ] Many commands need reconsidering: generalize, redesign, merge, or remove

**Keys:** see `.env.example` for ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY

## Next Up

- [ ] Tokens per symbol path in telemetry (leverage codebase tree)
- [ ] Real-time telemetry mode (`moss telemetry --watch`)
- [ ] Add Gemini CLI log parser to plugin system

## Backlog

**Telemetry** (see `docs/telemetry.md`):
- [x] `moss telemetry` CLI with aggregate analysis
- [x] HTML dashboard output
- [x] Plugin architecture for log formats (LogParser protocol)
- [ ] Tokens per function/file/module

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
