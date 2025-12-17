# Changelog

## v0.4.0

### Phase 26: LLM Integration
LLM-based code generation with mock support for testing:
- `LLMGenerator` plugin using LiteLLM for unified provider access
- `MockLLMProvider` for testing without API calls
- `LiteLLMProvider` supporting Anthropic, OpenAI, and other backends
- Streaming generation support
- Cost estimation and budgeting with per-model pricing
- Factory functions: `create_llm_generator()`, `create_mock_generator()`
- 48 tests for LLM generation

### Phase 22c: CLI & Edit Integration
- `moss edit` command with intelligent complexity routing
- TaskComplexity analysis (simple/medium/complex/novel)
- Structural edit handler (rename, typo fix, refactoring)
- Synthesis fallback for complex/novel tasks
- Configuration presets: default, research, production, minimal

### Phase 22d: Optimization & Learning
- StrategyLearner with feature extraction
- Feature-based strategy scoring (EMA updates)
- Similar problem lookup from history
- Router integration: 4-signal ranking (TF-IDF, estimate, history, learned)

### Phase D: Strategy Auto-Discovery
- StrategyPlugin protocol for pluggable strategies
- StrategyRegistry with enable/disable support
- Entry point discovery (moss.synthesis.strategies)

### Phase F: Configuration System
- SynthesisConfigWrapper for TOML-based config
- SynthesisConfigLoader fluent builder
- Subsystem configs: generators, validators, strategies, learning
- load_synthesis_config() for moss.toml

### Phase 25: Synthesis Plugin Architecture
Plugin system for synthesis components (inspired by Synquid, miniKanren, DreamCoder, λ²):
- `CodeGenerator` protocol with PlaceholderGenerator, TemplateGenerator
- `SynthesisValidator` protocol with TestValidator (pytest/jest), TypeValidator (mypy/pyright)
- `LibraryPlugin` protocol with MemoryLibrary (DreamCoder-style abstractions)
- `SynthesisRegistry` with sub-registries and entry point discovery
- Validation retry loop in `_validate_with_retry()`
- Framework integration: `_solve_atomic()` uses generator plugins
- User-configurable templates (CRUD, validation, transform patterns)
- 31 tests for plugin architecture

## v0.3.0

### Phase 24: Refactoring Tools
- Inline refactoring (function and variable inlining)
- Codemod DSL with pattern matching ($var placeholders)
- CodemodRunner for workspace-wide transformations
- Built-in codemod factories (deprecation, API migration)
- Preview/dry-run mode for all refactorings

### Phase 23: Context & Memory
- ContentHash merkle hashing for documents
- DocumentSummary with recursive child aggregation
- DocumentSummaryStore with caching and persistence
- ChatMessage and ChatSession management
- ChatlogStore with context window optimization
- SimpleSummarizer (extractive summarization)
- Session search with tag filtering

### Phase 22: Synthesis Framework (Scaffolding)
- Core synthesis framework (`src/moss/synthesis/`)
- Abstract interfaces: Specification, Context, Subproblem, SynthesisResult
- DecompositionStrategy ABC with metadata
- Composer ABC: SequentialComposer, FunctionComposer, CodeComposer
- StrategyRouter with TF-IDF keyword matching
- SynthesisFramework engine with depth/iteration limits
- Strategies: TypeDriven, TestDriven, PatternBased (decomposition only)
- CLI: `moss synthesize --dry-run --show-decomposition`
- Caching infrastructure: SynthesisCache, SolutionCache, StrategyCache
- Scale testing (depth 20+ problems)
- **Note**: Code generation not implemented (returns placeholders)

## v0.2.0

### Phase 21: Developer Experience & CI/CD
- Watch mode for tests (auto-run on file changes)
- Metrics dashboard (HTML report of codebase health)
- Custom analysis rules (user-defined patterns)
- Pre-commit hook integration
- Diff analysis (analyze changes between commits)
- PR review helper (summarize changes, detect issues)
- SARIF output (for CI/CD integration)
- GitHub Actions integration
- VS Code extension (`editors/vscode/`)

### Phase 20: Integration & Polish
- CLI improvements: global flags, consistent output module
- Interactive shell (`moss shell`)
- Performance: caching layer, parallel file analysis
- Configuration: `moss.toml`, per-directory overrides

### Phase 19: Advanced Features
- Configurable output verbosity
- Multi-file refactoring
- Progress indicators
- Live CFG rendering
- LSP integration
- Visual CFG output
- Auto-fix system
- Embedding-based search
- Non-code content plugins

### Phase 18: Plugin Architecture
- ViewPlugin protocol and PluginRegistry
- Entry points discovery for pip-installed plugins
- Tree-sitter skeleton plugin (multi-language)

### Phase 17: Introspection Improvements
- Enhanced skeleton views
- Dependency graph improvements

### Phase 16: DWIM Semantic Routing
- TF-IDF based command routing
- Fuzzy intent matching

### Phase 15: LLM Introspection Tooling
- Agent orchestration primitives
- Shadow git integration
- Validation loops

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
