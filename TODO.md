# Moss Roadmap

## Current: Phase 19 — Advanced Features

### Real-time Features
- [x] File watching for incremental updates
- [x] LSP integration
- [x] Live CFG rendering

## Backlog

- Multi-file refactoring support
- Configurable output verbosity
- Progress indicators for large scans

---

## Completed

See `docs/` for details on completed work:
- **Phase 19g**: Live CFG Rendering — auto-refresh visualization, file watcher integration, modern UI
- **Phase 19f**: LSP Integration — pygls-based server, diagnostics, hover info, document symbols, go-to-definition
- **Phase 19e**: Visual CFG Output — Mermaid/Graphviz rendering, HTML visualization, CLI integration
- **Phase 19c**: Auto-fix System — safe/unsafe classification, preview/diff, conflict resolution, Shadow Git rollback
- **Phase 19b**: Embedding-based Search — hybrid TF-IDF + embedding routing, code indexer, CLI command
- **Phase 19a**: Non-Code Content Plugins — Markdown structure, JSON/YAML/TOML schema extraction
- **Phase 18**: Plugin Architecture — extensible view provider system, entry points discovery, multi-language support (tree-sitter)
- **Phase 17**: Introspection Improvements — symbol metrics, reverse deps, DWIM tuning, output improvements
- **Phase 15**: LLM Introspection Tooling (`docs/tools.md`, `docs/cli-architecture.md`)
- **Phase 16**: DWIM semantic routing (`docs/dwim-architecture.md`)
- **CI/CD**: Fixed in `.github/workflows/ci.yml`
