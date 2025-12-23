# Restructuring Plan

Based on the layered architecture from `docs/langgraph-evaluation.md`.

## Architecture Principle: No Monolith

**Sub-packages are the source of truth.** Implementation lives in `packages/`:
- `moss-intelligence` - code understanding (skeleton, complexity, security, deps)
- `moss-context` - working memory (domain-agnostic)
- `moss-orchestration` - agent loops, sessions, drivers
- `moss-llm` - LLM adapters (litellm-based)
- `moss-mcp`, `moss-lsp`, `moss-tui`, `moss-acp` - frontends

**The `moss` package is a meta-package only:**
- Convenience re-exports for external users: `from moss import Intelligence`
- CLI entry point
- **Never used internally** - all internal imports go to sub-packages

**Migration direction:**
- Move implementation FROM `src/moss/` TO `packages/moss-*/`
- `src/moss/` shrinks over time until it's just re-exports
- Never add new functionality to `src/moss/`

## Key Insight

Two orthogonal concerns, both differentiators:

1. **Code Intelligence** - Understanding code structure (skeleton, call graphs, deps)
2. **Context Management** - Working memory for agents (generic, not code-specific)

These compose but don't depend on each other:
- A code agent uses both
- A non-code agent uses only context management
- A tool (no agent) uses only code intelligence

## Package Structure

```
moss                     # CLI + meta-package (depends on all)
moss-intelligence        # Code understanding
moss-context             # Generic working memory (no code knowledge)
moss-orchestration       # Agent loops, sessions, drivers

# Frontends (each separate)
moss-mcp                 # MCP server
moss-tui                 # Terminal UI
moss-lsp                 # Language Server Protocol
moss-acp                 # ACP server
moss-http                # HTTP API
```

## Dependency Graph

```
                    moss (CLI, meta)
                    /    |    \
                   /     |     \
    moss-orchestration   |    moss-mcp
           /   \         |        |
          v     v        v        v
  moss-context  moss-intelligence
                         |
                         v
                    (Rust CLI)
```

**Key insight**: moss-context and moss-intelligence are independent peers.
- moss-intelligence: stateless, pure (code in → structure out)
- moss-context: working memory interface (domain-agnostic)
- moss-orchestration: uses both (agent thinks in working memory about code)
- Frontends: use intelligence directly (no memory needed for tools)

## Package Contents

### moss-intelligence (Code Understanding)
- `skeleton.py` - Skeleton view (signatures without bodies)
- `tree.py` - Codebase tree (data model)
- `views.py` - View providers (skeleton is one view type)
- `complexity.py`
- `security.py`
- `dependencies.py`, `dependency_analysis.py`
- `clones.py`
- `patterns.py`
- `cfg.py`
- `call_graph` (from Rust)
- All analysis modules

### moss-context (Working Memory)
- `context.py`
- `context_memory.py` → `memory.py`
- `summarize.py`
- `cache.py`
- Token budgeting
- "What to keep, what to forget" logic
- NOT code-specific

### moss-orchestration (Agent Loops)
- `agents.py`
- `drivers.py`
- `execution/`
- `session.py`
- `shadow_git.py`
- `loop.py`
- `dwim.py`
- `workflows/`

### moss-mcp, moss-tui, etc. (Frontends)
- Thin wrappers over moss-intelligence
- Each is a separate installable package
- Minimal dependencies

## Current Module Classification

### Frontends (should be thin wrappers)
| Module | Current | Action |
|--------|---------|--------|
| `tui.py` | 2400 lines, some direct imports | Move to `frontends/tui/`, use MossAPI only |
| `mcp_server.py` | Already thin | Move to `frontends/mcp/` |
| `mcp_server_full.py` | Extended MCP | Move to `frontends/mcp/` |
| `acp_server.py` | ACP protocol | Move to `frontends/acp/` |
| `lsp_server.py` | LSP protocol | Move to `frontends/lsp/` |
| `cli/` | CLI commands | Move to `frontends/cli/` |
| `server/` | HTTP? | Move to `frontends/http/` |

### Orchestration (optional layer)
| Module | Current | Action |
|--------|---------|--------|
| `agents.py` | Multi-agent coordination | Move to `orchestration/` |
| `drivers.py` | Driver protocol | Move to `orchestration/` |
| `execution/` | Agent loops, workflows | Move to `orchestration/` |
| `execution_adapters.py` | Legacy bridge | Delete or move to `orchestration/` |
| `session.py` | Session management | Move to `orchestration/` |
| `session_analysis.py` | Session telemetry | Move to `orchestration/` |
| `shadow_git.py` | Git isolation | Move to `orchestration/` (code-safety) |
| `loop.py` | Validation loops | Move to `orchestration/` |
| `dwim.py` | DWIM agent | Move to `orchestration/` |
| `dwim_config.py` | DWIM config | Move to `orchestration/` |
| `hooks.py` | Execution hooks | Move to `orchestration/` |
| `parallel.py` | Parallel execution | Move to `orchestration/` |
| `task_tree.py` | Task hierarchy | Move to `orchestration/` |
| `watcher.py` | File watching | Move to `orchestration/` |
| `watch_tests.py` | Test watching | Move to `orchestration/` |
| `workflows/` | Workflow definitions | Move to `orchestration/` |

### moss-intelligence (Code Understanding)
| Module | Current | Action |
|--------|---------|--------|
| `skeleton.py` | Skeleton view (sigs only) | Keep (it's a view type) |
| `tree.py` | Codebase tree data model | Keep |
| `views.py` | View providers | Keep |
| `codebase.py` | Codebase model | Keep |
| `elided_literals.py` | Token efficiency for code | Keep |
| `rag.py` | RAG for code | Keep |
| `vector_store.py` | Code embeddings | Keep |
| `semantic_search.py` | Code semantic lookup | Keep |

### moss-context (Working Memory - Generic)
| Module | Current | Action |
|--------|---------|--------|
| `context.py` | Context building | Keep |
| `context_memory.py` | Working memory | Rename to `memory.py` |
| `summarize.py` | Summarization | Make generic (not code-specific) |
| `cache.py` | Content caching | Keep |

### Core Tools (MossAPI surface)
| Module | Current | Action |
|--------|---------|--------|
| `core_api.py` | View/Edit/Analyze | Keep, expand |
| `moss_api/` | Full API surface | Keep as main entry |
| `edit.py` | Edit primitives | Keep in `tools/` |
| `anchors.py` | Location anchors | Keep in `tools/` |
| `patches.py` | Patch application | Keep in `tools/` |

### Analysis (used by tools)
| Module | Current | Action |
|--------|---------|--------|
| `complexity.py` | Cyclomatic complexity | Move to `analysis/` |
| `security.py` | Security scanning | Move to `analysis/` |
| `dependencies.py` | Import extraction | Move to `analysis/` |
| `dependency_analysis.py` | Dep graph analysis | Move to `analysis/` |
| `clones.py` | Clone detection | Move to `analysis/` |
| `patterns.py` | Arch patterns | Move to `analysis/` |
| `cfg.py` | Control flow | Move to `analysis/` |
| `check_docs.py` | Doc checking | Move to `analysis/` |
| `check_refs.py` | Ref checking | Move to `analysis/` |
| `check_todos.py` | TODO checking | Move to `analysis/` |
| `diagnostics.py` | Diagnostic generation | Move to `analysis/` |
| `external_deps.py` | External dep analysis | Move to `analysis/` |
| `git_hotspots.py` | Git churn analysis | Move to `analysis/` |
| `weaknesses.py` | Weakness detection | Move to `analysis/` |
| `structural_analysis.py` | Structure analysis | Move to `analysis/` |
| `test_analysis.py` | Test analysis | Move to `analysis/` |
| `test_coverage.py` | Coverage analysis | Move to `analysis/` |
| `api_surface_analysis.py` | API analysis | Move to `analysis/` |

### Infrastructure
| Module | Current | Action |
|--------|---------|--------|
| `rust_shim.py` | Rust CLI bridge | Keep in `infra/` |
| `tree_sitter.py` | Tree-sitter bindings | Keep in `infra/` |
| `config.py` | Configuration | Keep in `infra/` |
| `toml_config.py` | TOML config | Keep in `infra/` |
| `toml_nav.py` | TOML navigation | Keep in `infra/` |
| `errors.py` | Error types | Keep in `infra/` |
| `events.py` | Event bus | Keep in `infra/` |
| `logging.py` | Logging | Keep in `infra/` |
| `metrics.py` | Metrics | Keep in `infra/` |
| `observability.py` | Observability | Keep in `infra/` |
| `profiling.py` | Profiling | Keep in `infra/` |
| `progress.py` | Progress tracking | Keep in `infra/` |
| `output.py` | Output formatting | Keep in `infra/` |
| `terminal.py` | Terminal utils | Keep in `infra/` |
| `handles.py` | Resource handles | Keep in `infra/` |
| `trust.py` | Trust model | Keep in `infra/` |
| `sandbox.py` | Sandboxing | Keep in `infra/` |
| `policy.py` | Policy engine | Keep in `infra/` |
| `rules.py` | Rule engine | Keep in `infra/` |
| `presets.py` | Presets | Keep in `infra/` |
| `heuristics.py` | Heuristics | Keep in `infra/` |

### Unclear / Need Review
| Module | Question |
|--------|----------|
| `architect_editor.py` | Orchestration or tool? |
| `autofix.py` | Orchestration (uses LLM) |
| `diff_analysis.py` | Analysis or context? |
| `guessability.py` | Analysis |
| `help.py` | Infrastructure |
| `live_cfg.py` | Orchestration? |
| `live_todos.py` | Orchestration? |
| `mutation.py` | Tool? |
| `pr_review.py` | Orchestration (uses LLM) |
| `refactoring.py` | Tool or orchestration? |
| `roadmap.py` | Infrastructure |
| `sarif.py` | Output format |
| `shell.py` | Tool |
| `status.py` | Tool |
| `validators.py` | Infrastructure |
| `visualization.py` | Context/output |
| `web.py` | Frontend? |
| `llm/` | Infrastructure |
| `gen/` | Code generation |
| `eval/` | Evaluation |
| `plugins/` | Infrastructure |
| `preferences/` | Infrastructure |
| `prompts/` | Orchestration |
| `rules/` | Infrastructure |
| `synthesis/` | Orchestration? |

## Priority Order

1. **First**: Make TUI use MossAPI exclusively (done for analysis methods)
2. **Second**: Create `context/` package, move core context modules
3. **Third**: Create `orchestration/` package, move agent/session code
4. **Fourth**: Create `frontends/` package, move servers
5. **Fifth**: Create `analysis/` package, consolidate analyzers
6. **Last**: Clean up infrastructure

## Open Questions

1. Should orchestration be a separate package (`moss-orchestration`) or just a subpackage?
2. How to handle circular imports during migration?
3. Should MossAPI stay flat or become hierarchical?
4. What's the public API surface we commit to?
