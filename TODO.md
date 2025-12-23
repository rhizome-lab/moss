# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs. See `docs/dogfooding.md` for testing notes.

## Next Up

- Push v0.1.0 release: `git push && git push --tags` (tag created, needs push)
- Test Coverage Heuristics: autodetect missing tests
- TUI: ScopesAPI for public/private symbol stats
- Call Graph: wire FunctionComplexity.short_name() into output

Test Status: 2110 passing, 0 failing, 42 skipped (all optional deps)

**Deferred:**
- Driver integration improvements
- Call graph language support

## Implementation Notes

**Self-update (`moss update`):**
- GITHUB_REPO constant in main.rs:4004 is set to "pterror/moss"
- Uses custom SHA256 implementation (no external crypto dep) in main.rs:4220-4310
- Expects GitHub release with SHA256SUMS.txt containing checksums
- Binary replacement creates temp file then renames (atomic on Unix)

## Backlog

**File Boundaries Don't Exist:**
- See `docs/file-boundaries.md` for design
- Phase 1: DONE - `expand_import_context()` + ViewOptions.expand_imports
- Phase 2: Available modules summary (show what's importable BEFORE writing)
- Phase 3: Transitive context with depth limit

**Test Coverage Heuristics:**
- Autodetect missing tests (like coverage but cheaper, no execution needed)
- Heuristically detect test file naming patterns per-repo (test_*.py, *_test.go, etc.)
- Handle in-file tests (Rust's `#[cfg(test)]` modules)
- Report: files without corresponding tests based on observed pattern
- Could be `moss analyze --test-coverage` or separate `moss test-gaps` command

**TUI as Library Interface:**
- Consider ScopesAPI for public/private symbol stats (or add to SkeletonAPI)

**Reference Resolution (partial):**
- Cross-language tracking (Python ↔ Rust) - see `docs/rust-python-boundary.md` for design

**Deferred:**
- Python edit separate targeting (LLM-based, intentionally different)
- Remaining docs: prior-art.md, hybrid-loops.md (lower priority)

**Fisheye for Other Languages:**
- Go (import resolution from go.mod)
- Java (package/class resolution)
- C/C++ (#include resolution)
- Ruby (require resolution)

**Call Graph:**
- Missing language support: Scala, Vue (no tree-sitter grammars yet)
- "(no ext)" files high count in some repos - add binary detection
- Wire FunctionComplexity.short_name() into complexity output
- Complete daemon integration (FileIndex API methods currently unused)

**Session Analysis:**
- Correction pattern detection: flag "You're right", "Good point", "Ah yes", etc.
- Could be a `moss analyze-session` tool or part of telemetry
- Use detected corrections to identify friction points

**Test Analysis:**
- Extract pytest markers: `@pytest.mark.skip`, `@pytest.mark.xfail`, `@pytest.mark.skipif`, `@pytest.mark.parametrize`
- Summarize test health: skip reasons, xfail counts, conditional skips
- Could integrate with `moss analyze --tests` or separate `moss test-health` command

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

## When Ready

**First Release**
- Create first GitHub release to test distribution pipeline:
  ```bash
  git tag v0.1.0
  git push --tags
  ```
- Verify cross-platform builds succeed in GitHub Actions
- Test `moss update` against the real release
