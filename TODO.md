# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- Session analysis: detect correction patterns
- Complete daemon integration

Test Status: 64 passing, 0 failing

## Backlog

**Language Support Refactor** (see `docs/language-support.md` for full design):

Phase 1 - Scaffold: ✅
- [x] Create `crates/moss-languages/` with Cargo.toml, feature flags
- [x] Define `Language` trait in `traits.rs`
- [x] Set up registry with `OnceLock` + `#[cfg(feature)]` gating

Phase 2 - Port existing languages: ✅
- [x] Port Python (most complex: docstrings, async, visibility)
- [x] Port Rust (impl blocks, doc comments, visibility modifiers)
- [x] Port JavaScript/TypeScript/TSX (shared extractor)
- [x] Port Go, Java, C, C++, Ruby, Scala, Vue
- [x] Port config formats: JSON, YAML, TOML, Markdown

Phase 3 - Integrate: ✅
- [x] Add trait infrastructure to `skeleton.rs` (extract_with_trait, convert_symbol)
- [x] Improve trait impls to match legacy behavior (Rust impl blocks, Go types, Java visibility)
- [x] Migrate languages to trait-based extraction:
  - Python, JavaScript, TypeScript, Rust, Go, Java, Ruby, C, C++
  - Scala, Markdown, JSON, YAML, TOML
  - Vue remains on legacy (needs script element parsing)
- [x] Add extract_imports/extract_exports to Language trait
- [x] Refactor `deps.rs` to use trait (Python, Rust, JS, Go migrated)
- [x] Refactor `complexity.rs` to use trait (complexity_nodes method)
- [x] Refactor `symbols.rs` to use trait
- [x] Refactor `anchors.rs` to use trait
- [x] Refactor `scopes.rs` to use trait (add scope_creating_kinds)
- [x] Refactor `edit.rs` to use trait (uses function_kinds/container_kinds)
- [x] Refactor `cfg.rs` to use trait (add control_flow_kinds)
- [x] Delete legacy code from symbols.rs, skeleton.rs, deps.rs (~2000 lines removed)
- [x] Refactor `index.rs` to use trait-based import extraction
- [x] Complete C++ language support (scope/control/complexity/nesting kinds)
- [x] Add `ImportResolver` trait for external package resolution (resolution.rs)
- [x] Migrate main.rs callers to use ImportResolver trait

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

**View Filtering:**
- Filter out tests from views (--no-tests or --exclude=tests)
- Filter by category: tests, config files, build files, etc.
- Inverse: show only specific categories (--only=tests)

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
- Now in commands/update.rs
- GITHUB_REPO constant → "pterror/moss"
- Custom SHA256 implementation (Sha256 struct)
- Expects GitHub release with SHA256SUMS.txt

## When Ready

**First Release:**
```bash
git tag v0.1.0
git push --tags
```
- Verify cross-platform builds in GitHub Actions
- Test `moss update` against real release
