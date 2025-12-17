# Moss Roadmap

## Current: Phase 19 — Advanced Features

### Real-time Features
- [x] File watching for incremental updates
- [ ] LSP integration
- [ ] Live CFG rendering

## Backlog

- Visual CFG output (graphviz/mermaid)
- Multi-file refactoring support
- Configurable output verbosity
- Progress indicators for large scans

---

## Completed

See `docs/` for details on completed work:
- **Phase 19c**: Auto-fix System — safe/unsafe classification, preview/diff, conflict resolution, Shadow Git rollback
- **Phase 19b**: Embedding-based Search — hybrid TF-IDF + embedding routing, code indexer, CLI command
- **Phase 19a**: Non-Code Content Plugins — Markdown structure, JSON/YAML/TOML schema extraction
- **Phase 18**: Plugin Architecture — extensible view provider system, entry points discovery, multi-language support (tree-sitter)
- **Phase 17**: Introspection Improvements — symbol metrics, reverse deps, DWIM tuning, output improvements
- **Phase 15**: LLM Introspection Tooling (`docs/tools.md`, `docs/cli-architecture.md`)
- **Phase 16**: DWIM semantic routing (`docs/dwim-architecture.md`)
- **CI/CD**: Fixed in `.github/workflows/ci.yml`
