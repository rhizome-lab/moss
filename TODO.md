# Moss Roadmap

## Current: Phase 17 — Introspection Improvements

Based on LLM evaluation findings (see `docs/llm-evaluation.md`).

### Symbol Metrics
- [ ] Add `end_line` to Symbol for size calculation
- [ ] Add line count per function/method
- [ ] `--min-lines` / `--max-lines` filters for query command
- [ ] Cyclomatic complexity (optional)

### Reverse Dependencies
- [ ] "What imports this module?" query
- [ ] `moss deps --reverse <module>` command
- [ ] Internal dependency graph visualization

### DWIM Tuning
- [ ] Lower `SUGGEST_THRESHOLD` from 0.5 to 0.3
- [ ] Add more synonyms to tool descriptions
- [ ] Consider top-k results regardless of threshold

### Output Improvements
- [ ] CFG summary mode (node/edge counts only)
- [ ] `--group-by=file` for multi-file query results
- [ ] `--public-only` filter for exported symbols

## Phase 18: Plugin Architecture

> Begin after Phase 17 validates current implementation.

### Core
- [ ] Plugin interface for view providers
- [ ] Plugin discovery and loading
- [ ] Registration and lifecycle management

### Built-in Plugins
- [ ] Refactor Python skeleton as plugin
- [ ] Refactor CFG as plugin
- [ ] Refactor deps as plugin

### Language Support
- [ ] TypeScript/JavaScript
- [ ] Go
- [ ] Rust

### Non-Code Content
- [ ] Markdown structure
- [ ] JSON/YAML schema
- [ ] Config files

## Phase 19: Advanced Features

### Embedding-based Search
- [ ] Vector embeddings for semantic code search
- [ ] Integration with existing vector store
- [ ] Hybrid TF-IDF + embedding routing

### Auto-fix System
- [ ] Safe vs unsafe fix classification
- [ ] Preview/diff before applying
- [ ] Shadow Git integration for rollback
- [ ] Conflict resolution for overlapping fixes

### Real-time Features
- [ ] File watching for incremental updates
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
- **Phase 15**: LLM Introspection Tooling (`docs/tools.md`, `docs/cli-architecture.md`)
- **Phase 16**: DWIM semantic routing (`docs/dwim-architecture.md`)
- **CI/CD**: Fixed in `.github/workflows/ci.yml`
