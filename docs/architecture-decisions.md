# Architecture Decisions

Key architectural decisions and their rationale.

## Language Choice: Pure Rust

**Decision**: Moss is implemented entirely in Rust.

### Why Rust?

- **Performance**: Parallel indexing with rayon for large codebases (100k+ files)
- **Tree-sitter native**: First-class tree-sitter integration
- **Single binary**: No runtime dependencies, easy distribution
- **Memory safety**: No GC pauses during indexing

### Crate Structure

```
crates/
├── moss/              # Core library + CLI
├── moss-languages/    # 98 language definitions
├── moss-packages/     # Package ecosystem support
├── moss-tools/        # MCP tool generation
├── moss-derive/       # Proc macros
├── moss-jsonschema/   # Schema generation
└── moss-openapi/      # OpenAPI generation
```

## Dynamic Grammar Loading

**Decision**: Load tree-sitter grammars from external `.so` files.

### Why?

- **Build time**: Bundling 98 grammars bloats compile time
- **Binary size**: Grammars add ~142MB uncompressed
- **User extensibility**: Users can add custom grammars

### Loading Order

1. `MOSS_GRAMMAR_PATH` environment variable
2. `~/.config/moss/grammars/`
3. Built-in fallback (if compiled with grammar features)

## Lua for Workflows

**Decision**: Use Lua (LuaJIT via mlua) for workflow scripting.

### Why Not TOML/YAML?

Once you need conditionals (`if is_dirty() then commit() end`), you're fighting the format. We tried TOML first and deleted ~1500 lines.

### Why Lua?

- ~200KB runtime, extremely fast
- Simple syntax: `view("foo.rs")` vs TOML's `view: foo.rs`
- Battle-tested: nginx, redis, neovim all use Lua
- Full language when needed: loops, functions, error handling

## Index-Optional Design

**Decision**: All commands work without the index (with graceful degradation).

### Fallback Behavior

| Feature | With Index | Without Index |
|---------|------------|---------------|
| Symbol search | SQLite query | Filesystem walk + parsing |
| Health metrics | Cached stats | Real-time file scan |
| Path resolution | Index lookup | Glob patterns |

### Configuration

```toml
[index]
enabled = true  # Set to false to disable indexing entirely
```

## Command Naming: `text-search` Not `grep`

**Decision**: Use `moss text-search` for text pattern matching instead of `moss grep`.

### Why Not `grep`?

1. **AI agent confusion**: LLMs like Claude (especially Opus 4.5) conflate `moss grep` with unix grep syntax. They constantly try `moss grep pattern file` (unix style) instead of `moss text-search pattern` (our style).

2. **Mental model conflict**: Unix grep has 50+ years of muscle memory. Our command uses ripgrep internally but has different semantics (no positional file args, `--only` instead of file patterns). Fighting the unix grep mental model wastes tokens and causes errors.

3. **Semantic expectations**: In the AI era, "search" and "find" imply semantic/vector search. `text-search` explicitly signals regex-based text matching.

### Why Not `search` or `find`?

Those names should be reserved for future semantic search features (embeddings, vector similarity). `text-search` is explicit about the mechanism.

### Config Section

The config section is `[text-search]` to match the command name.

## Local Model Memory Budget

For future local LLM/embedding integration:

| Model | Params | FP16 RAM |
|-------|--------|----------|
| all-MiniLM-L6-v2 | 33M | 65MB |
| distilbart-cnn | 139M | 280MB |
| T5-small | 60M | 120MB |
| T5-base | 220M | 440MB |

### Recommendations

1. Default to smallest viable model
2. Lazy loading (don't load until first use)
3. Graceful degradation (fall back to extractive methods if OOM)
4. Consider INT8 quantization (~4x memory reduction)

### Pre-summarization Tiers

1. **Zero-cost**: Title, headings, metadata extraction
2. **Extractive**: TextRank, TF-IDF (no NN needed)
3. **Small NN**: Embeddings, abstractive summary
4. **LLM**: Only when simpler methods insufficient
