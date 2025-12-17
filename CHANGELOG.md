# Changelog

## v0.1.0 (Initial Release)

### Phase 10: Developer Experience
- CLI interface (`moss init`, `moss run`, `moss status`)
- README with architecture overview
- Usage examples and tutorials in `examples/`
- API documentation via docstrings

### Phase 11: Enhanced Capabilities
- Vector store integration (Chroma, in-memory)
- Tree-sitter integration for multi-language AST (Python, TypeScript, JavaScript, Go, Rust)
- Control Flow Graph (CFG) view provider
- Elided Literals view provider for token reduction

### Phase 12: Hardening & Quality
- Integration tests for component interactions
- E2E tests for full workflows
- Fuzzing tests for edge cases and malformed inputs
- CI/CD with GitHub Actions (lint, test, coverage, typecheck)

### Phase 13: Production Readiness
- FastAPI example server (`examples/server/`)
- Structured logging module (`moss.logging`)
- Observability module with metrics and tracing (`moss.observability`)
- Profiling utilities (`moss.profiling`)

### Phase 14: Dogfooding
- Self-analysis test suite (Moss analyzing its own codebase)
- Performance benchmarks on real code
- 621 tests passing with 86% coverage
