# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- Global package index database: cache indexed external packages at `~/.cache/moss/python-X.Y.db`, `~/.cache/moss/go-X.Y.db`

Test Status: 2184 passing, 0 failing, 42 skipped (all optional deps)

## Backlog

**Language Support:**
- Fisheye for Java (package/class resolution)
- Fisheye for C/C++ (#include resolution)
- Call graph: Scala, Vue (no tree-sitter grammars yet)

**Analysis Features:**
- Session analysis: detect correction patterns ("You're right", "Good point")

**Integration:**
- Complete daemon integration (FileIndex API methods currently unused)
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
