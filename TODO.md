# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs. See `docs/dogfooding.md` for testing notes.

## Next Up

**1. View Primitive Polish**
- Barrel file hoisting: detect `export * from` and surface re-exported symbols
- Useless docstring detection: skip "Sets the user id" on `setUserId()`
- Fisheye for TypeScript (Python and Rust now supported)
- Selective import resolution (e.g., `--fisheye=moss.config` to expand only that import)

**2. Rust Module Cleanup**
- index.rs/symbols.rs: Keep separate (good layering: symbols=parsing, index=persistence)
- Many "dead code" warnings are serde false positives (daemon Request/Response types)

**3. TUI: View/Edit/Analyze Mode Refactor**
- Mode indicator (bottom right, near palette) OR tabbed content view
- Better integration of primitives into TUI workflow

**4. Reference Resolution**
- Full import graph with alias tracking
- Variable scoping analysis
- Type inference for method calls
- Cross-language tracking (Python ↔ Rust)

**5. Distribution**
- Auto-updates
- Portable single binary
- Pre-built binaries (GitHub Actions)

## Backlog

**Deferred:**
- Python edit separate targeting (LLM-based, intentionally different)
- Remaining docs: prior-art.md, hybrid-loops.md (lower priority)

**Call Graph:**
- Missing language support: Scala, Vue (no tree-sitter grammars yet)
- "(no ext)" files high count in some repos - add binary detection

**Session Analysis:**
- Correction pattern detection: flag "You're right", "Good point", "Ah yes", etc.
- Could be a `moss analyze-session` tool or part of telemetry
- Use detected corrections to identify friction points

**Editor Integration:**
- LSP refactor actions (rename symbol across files via language server)

**Memory System:**
- Layered memory for cross-session learning (see `docs/memory-system.md`)

**Agent TUI:**
- Terminal output sanitization: reset terminal state after nested command output

**Agent Research:**
- Conversational loop pattern (vs hierarchical)
- YOLO mode evaluation
- Diffusion-like parallel refactors
- Fine-tuned tiny models (100M RWKV)
- Analyze ampcode research notes (ampcode.com/research) for deeper patterns

## Notes

### Design Principles
See `docs/philosophy.md`. Key goals:
- **Generalize, Don't Multiply**: One flexible solution over N specialized ones
- **Three Primitives**: view, edit, analyze (composable, not specialized)
- Minimize LLM usage (structural tools first)
- Maximize useful work per token

### API Keys
See `.env.example` for ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY
