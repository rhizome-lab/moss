# Moss Roadmap

## Phase 10: Developer Experience
- [ ] CLI interface (`moss init`, `moss run`, `moss status`)
- [ ] Expand README with architecture overview
- [ ] Usage examples and tutorials
- [ ] API documentation (docstrings → generated docs)

## Phase 11: Enhanced Capabilities
- [ ] Real vector store integration (Chroma, Pinecone, etc.)
- [ ] Tree-sitter integration for multi-language AST
- [ ] Control Flow Graph (CFG) view provider
- [ ] Elided Literals view provider
- [ ] Additional language support beyond Python

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
