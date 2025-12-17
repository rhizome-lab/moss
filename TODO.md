# Moss Roadmap

## Phase 15: LLM Introspection Tooling

### CLI Enhancements ✅
- [x] Add `--json` output flag to all CLI commands
- [x] `moss skeleton <path>` - Extract and display code skeleton
- [x] `moss anchors <path>` - List all anchors (functions, classes, methods)
- [x] `moss cfg <path> [function]` - Display control flow graph
- [x] `moss deps <path>` - Show dependencies (imports/exports)
- [x] `moss context <path>` - Combined view (skeleton + deps + summary)

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

## Phase 16: Plugin Architecture

> **Important**: This phase should only begin AFTER Phase 15 is complete and we've
> validated the current implementation through real-world LLM usage. Premature
> abstraction is worse than no abstraction.

### Plugin System Design
- [ ] Design plugin interface for view providers
- [ ] Implement plugin discovery and loading
- [ ] Create plugin registration and lifecycle management

### Content Type Plugins
- [ ] Refactor Python skeleton extraction as plugin
- [ ] Refactor CFG building as plugin
- [ ] Refactor dependency extraction as plugin
- [ ] Add support for non-code content (markdown, JSON, YAML, etc.)

### Language Plugins
- [ ] TypeScript/JavaScript plugin
- [ ] Go plugin
- [ ] Rust plugin

### Third-Party Extension Support
- [ ] Plugin API documentation
- [ ] Plugin development guide
- [ ] Example plugin implementation

## Future Ideas
- Real-time file watching and incremental updates
- Language server protocol (LSP) integration
- Visual CFG rendering (graphviz/mermaid output)
- Semantic code search with embeddings
- Multi-file refactoring support
