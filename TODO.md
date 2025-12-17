# Moss Roadmap

## Current: Phase 20 — Integration & Polish

### CLI Improvements
- [x] Wire output module into all CLI commands (consistent verbosity flags)
- [x] Add `--json` and `--quiet` global flags
- [x] Interactive REPL mode (`moss shell`)

### Performance
- [x] Caching layer for AST/CFG (avoid re-parsing unchanged files)
- [x] Parallel file analysis (multi-threaded processing)

### Configuration
- [x] Project config file (`moss.toml` or `.mossrc`)
- [x] Per-directory overrides

## Backlog

### Developer Experience
- [ ] Watch mode for tests (auto-run on file changes)
- [ ] Metrics dashboard (HTML report of codebase health)
- [ ] Custom analysis rules (user-defined patterns)

### Git Integration
- [ ] Diff analysis (analyze changes between commits)
- [ ] PR review helper (summarize changes, detect issues)
- [ ] Pre-commit hook integration

### Export & Integration
- [ ] SARIF output (for CI/CD integration)
- [ ] GitHub Actions integration
- [ ] VS Code extension

---

## Completed

See `docs/phase19-features.md` for detailed documentation.

### Phase 19: Advanced Features
- **19j**: Configurable Output Verbosity
- **19i**: Multi-file Refactoring
- **19h**: Progress Indicators
- **19g**: Live CFG Rendering
- **19f**: LSP Integration
- **19e**: Visual CFG Output
- **19c**: Auto-fix System
- **19b**: Embedding-based Search
- **19a**: Non-Code Content Plugins

### Earlier Phases
- **Phase 18**: Plugin Architecture
- **Phase 17**: Introspection Improvements
- **Phase 16**: DWIM Semantic Routing
- **Phase 15**: LLM Introspection Tooling
