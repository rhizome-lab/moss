# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- Session analysis: detect correction patterns ("You're right", "Good point")
- Complete daemon integration (FileIndex API methods currently unused)

Test Status: 79 passing, 0 failing

## Backlog

**Language Support Refactor** (see `docs/language-support.md` for full design):

Phase 1 - Scaffold: ✅
- [x] Create `crates/moss-languages/` with Cargo.toml, feature flags
- [x] Define `LanguageSupport` trait in `traits.rs`
- [x] Set up registry with `OnceLock` + `#[cfg(feature)]` gating

Phase 2 - Port existing languages: ✅
- [x] Port Python (most complex: docstrings, async, visibility)
- [x] Port Rust (impl blocks, doc comments, visibility modifiers)
- [x] Port JavaScript/TypeScript/TSX (shared extractor)
- [x] Port Go, Java, C, C++, Ruby, Scala, Vue
- [x] Port config formats: JSON, YAML, TOML, Markdown

Phase 3 - Integrate (in progress):
- [x] Add trait infrastructure to `skeleton.rs` (extract_with_trait, convert_symbol)
- [x] Improve trait impls to match legacy behavior (Rust impl blocks, Go types, Java visibility)
- [x] Migrate languages to trait-based extraction:
  - Python, JavaScript, TypeScript, Rust, Go, Java, Ruby, C, C++
  - Scala, Markdown, JSON, YAML, TOML
  - Vue remains on legacy (needs script element parsing)
- [x] Add extract_imports/extract_exports to LanguageSupport trait
- [x] Refactor `deps.rs` to use trait (Python, Rust, JS, Go migrated)
- [ ] Refactor `complexity.rs`, `scopes.rs`, `symbols.rs`
- [ ] Refactor `anchors.rs`, `edit.rs`, `cfg.rs`
- [ ] Delete old language-specific code from moss-cli

Phase 4 - Expand:
- [ ] Kotlin, Swift, Dart (mobile)
- [ ] C#, F# (.NET)
- [ ] PHP, Elixir, Erlang (backends)
- [ ] Zig, Lua (systems/gamedev)
- [ ] SQL, GraphQL (data)
- [ ] Dockerfile, HCL (infra)
- [ ] Svelte, SCSS (frontend)

**Integration:**
- LSP refactor actions (rename symbol across files)
- Cross-language reference tracking (Python ↔ Rust)

**Agent Research:**
- Conversational loop pattern (vs hierarchical)
- YOLO mode evaluation
- Diffusion-like parallel refactors

## Deferred

- Driver integration improvements
- Python edit separate targeting (LLM-based)
- Remaining docs: prior-art.md, hybrid-loops.md
- Memory system: layered cross-session learning
- Agent TUI: terminal state reset after nested commands

## Implementation Notes

**Self-update (`moss update`):**
- GITHUB_REPO constant in main.rs:4004 → "pterror/moss"
- Custom SHA256 implementation (main.rs:4220-4310)
- Expects GitHub release with SHA256SUMS.txt

## When Ready

**First Release:**
```bash
git tag v0.1.0
git push --tags
```
- Verify cross-platform builds in GitHub Actions
- Test `moss update` against real release
