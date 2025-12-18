# Architecture Decisions

This document records key architectural decisions and their rationale.

## Language Choice: Python

**Decision**: Moss is implemented in Python.

**Considered alternatives**: TypeScript/Bun, Rust, Go

### Why Python?

**AST ecosystem superiority:**
- tree-sitter has excellent Python bindings
- libcst for concrete syntax trees (preserves formatting)
- rope for refactoring operations
- parso for error-tolerant parsing
- ast-grep, semgrep, ruff all have Python APIs or easy subprocess integration

**ML/AI library support:**
- sentence-transformers, transformers for local embeddings/summarization
- Z3 solver bindings (z3-solver) for synthesis
- All major LLM SDKs (anthropic, openai, etc.) are Python-first

**Tool integration:**
- MCP SDK is Python-native
- Most code analysis tools (bandit, pyright, mypy) are Python
- Easy subprocess orchestration for external tools

### Parallelism Considerations

**GIL limitations**: Python's Global Interpreter Lock prevents true CPU-bound parallelism within a single process.

**Why this is acceptable for Moss**:
1. **I/O bound workloads**: Most operations are file reads, subprocess calls, or LLM API calls
2. **Subprocess delegation**: Heavy lifting done by external tools (ast-grep, ruff, git)
3. **asyncio works fine**: Concurrent I/O parallelizes well
4. **Process-based parallelism**: `concurrent.futures.ProcessPoolExecutor` for CPU-bound work

**If we hit GIL limits**:
- Profile first to identify actual bottlenecks
- Move hot paths to Rust via PyO3 (what ruff does)
- Consider multiprocessing for CPU-intensive analysis

### Type Safety

**Current approach**: basedpyright in strict mode

**Comparison to TypeScript**:
- TS has better inference and stricter defaults
- Python typing is "good enough" for a project this size
- Runtime type checking available via beartype/typeguard if needed
- Tradeoff accepted for ecosystem benefits

## Local Neural Network Memory Budget

**Problem**: Local models for summarization/embeddings can be memory-hungry.

### Model Size Reference

| Model | Params | FP32 | FP16 |
|-------|--------|------|------|
| all-MiniLM-L6-v2 | 33M | 130MB | 65MB |
| distilbart-cnn | 139M | 560MB | 280MB |
| T5-small | 60M | 240MB | 120MB |
| T5-base | 220M | 880MB | 440MB |
| T5-large | 770M | 3GB | 1.5GB |
| T5-3B | 3B | 12GB | 6GB |

### Recommendations

1. **Default to smallest viable model**: T5-small or distilbart-cnn for summarization
2. **Make model configurable**: Users with more RAM can opt for larger models
3. **Lazy loading**: Don't load models until first use
4. **Graceful degradation**: If model loading fails (OOM), fall back to extractive methods or skip
5. **Consider quantization**: INT8 reduces memory ~4x with minimal quality loss

### Pre-summarization Strategy

For web fetching and document processing, use a tiered approach:

1. **Zero-cost extraction** (always): title, headings, OpenGraph metadata
2. **Extractive** (cheap): TextRank, TF-IDF sentence ranking - no NN needed
3. **Small NN** (optional): all-MiniLM for embeddings, distilbart for abstractive
4. **LLM** (expensive): Only when extractive/small NN insufficient

Configuration in `.moss/config.yaml`:
```yaml
summarization:
  model: "distilbart-cnn"  # or "t5-small", "extractive-only", "none"
  max_memory_mb: 500       # Skip NN if would exceed
  fallback: "extractive"   # What to do if model unavailable
```

## Future Considerations

### When to Consider Rewriting in Another Language

**Don't rewrite unless**:
- Profiling shows Python is the bottleneck (not I/O, not subprocesses)
- The bottleneck can't be solved with PyO3/Cython
- The rewrite scope is bounded (single hot module, not entire codebase)

**Likely candidates for Rust extraction**:
- AST diffing algorithms
- Large-scale structural matching
- Real-time incremental parsing

### Hybrid Architecture Pattern

If performance becomes critical, consider the ruff/uv pattern:
- Core algorithms in Rust
- Python wrapper for API/CLI
- Best of both worlds: performance + ecosystem
