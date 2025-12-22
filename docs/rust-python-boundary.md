# Rust/Python Boundary

This document defines what functionality belongs in Rust vs Python, and why.

## The Core Principle

**Rust = Plumbing, Python = Interface**

- **Rust**: Fast, deterministic, syntax-aware operations on code
- **Python**: High-level orchestration, LLM integration, user interfaces

This follows "Separate Interface, Unify Plumbing" from philosophy.md.

## Decision Framework

Put it in **Rust** if:
- It's deterministic (same input → same output)
- It's performance-critical (needs to handle large codebases)
- It operates on syntax (tree-sitter, AST-level)
- It's a primitive operation other tools build on
- It benefits from parallelism (rayon)
- It's language-agnostic (works across 17+ languages)

Put it in **Python** if:
- It involves LLM calls (judgment, generation, ambiguity resolution)
- It's high-level orchestration (workflows, agents, state machines)
- It needs rapid iteration (prototyping new features)
- It's a user-facing interface (CLI wrapper, TUI, LSP, MCP)
- It integrates with Python ecosystem (pytest, mypy, chromadb)
- It's plugin/extension logic

## Current Division

### Rust (moss-cli, moss-core, moss-daemon)

| Category | Examples |
|----------|----------|
| Parsing | Tree-sitter for 17+ languages |
| Indexing | SQLite-backed symbol/call graph index |
| Search | Ripgrep integration, fuzzy matching |
| Structural ops | `view`, `edit --replace`, `analyze` core |
| Fast queries | `callers`, `callees`, `deps`, `complexity` |
| Daemon | Background indexing, file watching |

### Python (src/moss/)

| Category | Examples |
|----------|----------|
| LLM operations | edit (synthesis), agents, generation |
| Orchestration | workflows, state machines, execution primitives |
| User interfaces | CLI wrapper, TUI (Textual), LSP, MCP |
| Rich analysis | patterns, security, clones, test coverage |
| Git operations | shadow_git, atomic commits |
| Plugins | synthesis generators, view providers |
| Validation | pytest/mypy integration, validators |

## The Shim Pattern

Python calls Rust via `rust_shim.py`:

```
Python CLI (cli.py)
    ↓
rust_shim.passthrough() or rust_shim.call_rust()
    ↓
subprocess.run([rust_binary, args])
    ↓
JSON output → Python parses
```

**Rules:**
- Rust commands always support `--json` for machine consumption
- Python gracefully degrades if Rust binary unavailable
- Passthrough commands bypass Python entirely for speed

## Overlap Resolution

Some functionality exists in both. The rule: **Rust for speed, Python for richness**.

| Feature | Rust | Python | Use Rust when... | Use Python when... |
|---------|------|--------|------------------|-------------------|
| Skeleton | ✓ | ✓ | Large codebase, multi-language | Need Python-specific AST detail |
| Complexity | ✓ | ✓ | Batch analysis | Detailed breakdown needed |
| CFG | ✓ | ✓ | Quick visualization | Sophisticated analysis |
| Dependencies | ✓ | ✓ | Cross-language imports | Python import semantics |
| Edit | ✓ (structural) | ✓ (LLM) | Rename, move, delete | Complex refactors needing judgment |

**Eventual goal:** Reduce overlap. Python implementations become thin wrappers or are removed. Exception: Python edit stays for LLM-based synthesis.

## Adding New Features

### Checklist

1. **Is it deterministic?** → Rust
2. **Does it need LLM?** → Python
3. **Is it performance-critical?** → Rust
4. **Is it a one-off script?** → Python (prototype), then maybe Rust
5. **Does it extend existing Rust infra?** → Rust
6. **Does it extend existing Python infra?** → Python

### Migration Path

New features often start in Python (faster to iterate), then migrate to Rust if:
- They become performance bottlenecks
- They're used frequently enough to justify the effort
- They need to work on large codebases (>100k lines)

## Anti-patterns

**Don't:**
- Implement parsing/indexing in Python (use Rust or tree-sitter bindings)
- Call LLMs from Rust (Python handles all AI integration)
- Add Python wrappers that just call Rust (use passthrough instead)
- Duplicate Rust functionality in Python without clear reason

**Do:**
- Use `rust_shim.passthrough()` for Rust commands
- Add `--json` output to all Rust commands
- Keep Python edit for LLM-based operations
- Document why something is in Python if Rust equivalent exists
