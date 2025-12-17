# Moss Roadmap

## Phase 15: LLM Introspection Tooling

### CLI Enhancements
- [ ] Add `--json` output flag to all CLI commands
- [ ] `moss skeleton <path>` - Extract and display code skeleton
- [ ] `moss anchors <path>` - List all anchors (functions, classes, methods)
- [ ] `moss cfg <path> [function]` - Display control flow graph
- [ ] `moss deps <path>` - Show dependencies (imports/exports)
- [ ] `moss context <path>` - Combined view (skeleton + deps + summary)

### Architecture Note
> **Plugin Architecture**: Introspection commands (skeleton, anchors, cfg, deps) should
> eventually be refactored into a plugin system. This enables:
> - Non-code content analysis (documents, configs, data files)
> - Language-specific implementations as separate plugins
> - Third-party extensions for custom formats
>
> Current implementation is monolithic for rapid iteration. Refactor to plugins
> once the API stabilizes and we've validated usefulness through LLM evaluation.

### Query Interface
- [ ] `moss query` command with pattern matching
- [ ] Find functions by signature pattern
- [ ] Find classes by inheritance
- [ ] Search by complexity metrics (lines, branches, etc.)

### MCP Server
- [ ] Implement MCP server for direct tool access
- [ ] Expose skeleton extraction as MCP tool
- [ ] Expose anchor finding as MCP tool
- [ ] Expose CFG building as MCP tool
- [ ] Expose patch application as MCP tool

### LLM Evaluation
- [ ] Use Moss CLI to explore codebases
- [ ] Document what works well for LLM consumption
- [ ] Identify gaps and iterate

## Future Ideas
- Real-time file watching and incremental updates
- Language server protocol (LSP) integration
- Visual CFG rendering (graphviz/mermaid output)
- Semantic code search with embeddings
- Multi-file refactoring support
