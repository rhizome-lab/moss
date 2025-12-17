# Moss Roadmap

## Phase 10: Developer Experience ✓
- [x] CLI interface (`moss init`, `moss run`, `moss status`)
- [x] Expand README with architecture overview
- [x] Usage examples and tutorials
- [x] API documentation (docstrings → generated docs)

## Phase 11: Enhanced Capabilities ✓
- [x] Real vector store integration (Chroma, Pinecone, etc.)
- [x] Tree-sitter integration for multi-language AST
- [x] Control Flow Graph (CFG) view provider
- [x] Elided Literals view provider
- [x] Additional language support beyond Python (TypeScript, JS, Go, Rust)

## Phase 12: Hardening & Quality
- [ ] Integration tests (component interactions)
- [ ] E2E tests (full flows: user request → commit handle)
- [ ] Fuzzing tests (malformed inputs, AST edge cases)
- [ ] CI/CD setup (GitHub Actions for tests, lint)

## Phase 13: Production Readiness
- [ ] FastAPI/Flask example server
- [ ] Structured logging throughout
- [ ] Observability (metrics, tracing)
- [ ] Performance profiling and optimization
- [ ] Error handling audit

## Phase 14: Dogfooding
- [ ] Self-hosting test (use Moss on Moss)
- [ ] Real-world codebase testing
- [ ] Gap analysis and iteration
