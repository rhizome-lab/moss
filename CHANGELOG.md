# Changelog

## v0.6.7

### Single-Tool MCP Server

**Token Efficiency**
- New single-tool MCP server: `moss(command: str)` - 99% token reduction (~8K → ~50 tokens)
- Original multi-tool server preserved as `moss-mcp-full` for IDEs
- CLI: `moss mcp-server` (single-tool, default) or `moss mcp-server --full` (multi-tool)
- Entry points: `moss-mcp` (single) and `moss-mcp-full` (full)

## v0.6.6

### Phase 36: query/search CLI Migration, Agent Learning

**SearchAPI**
- `search_query` - Query symbols with pattern matching and regex filters
  - Filters: kind, name regex, signature regex, inheritance, line counts
  - Returns `QueryMatch` dataclass with full symbol info

**LessonsAPI** (Agent Learning)
- `lessons_add` - Record a lesson with auto-extracted keywords
- `lessons_list` - List lessons, optionally filtered by category
- `lessons_search` - Search lessons by keyword
- `lessons_find_relevant` - Find lessons relevant to current context
- Stored in `.moss/lessons.md` with categories and timestamps

**CLI Migration to MossAPI**
- 18 commands now use MossAPI (was 16)
- Newly migrated: query, search
- `cmd_query` now uses `MossAPI.search.query()`
- `cmd_search` now uses `MossAPI.rag` for semantic search

## v0.6.5

### Phase 35: find_related_files, summarize_module, CLI Migration

**SearchAPI** (Dec 2025)
- `search_find_related_files` - Find files that import/are imported by a given file
- `search_summarize_module` - "What does this module do?" with docstrings, public exports
- `search_resolve_file` - DWIM for file names with fuzzy matching
- `search_explain_symbol` - Show callers/callees for any symbol
- `search_find_symbols` - Now recursively finds methods inside classes

**DependencyAPI**
- `dependencies_build_graph` - Build module dependency graph
- `dependencies_graph_to_dot` - Convert graph to DOT format for visualization
- `dependencies_find_reverse` - Find files that import a given module

**CLI Migration to MossAPI**
- 16 commands now use MossAPI (was 12)
- Newly migrated: anchors, cfg, deps, context
- Pattern: Replace direct imports with `MossAPI.for_project()`
- Reduces duplication, enables generated CLI

### Phase 34: Module DWIM, CLI Migration, explain_symbol

**HealthAPI Filtering**
- `health_check(focus=..., severity=...)` - Filter weak spots in API
- Moved filtering logic from CLI into HealthAPI
- Enables targeted health checks (e.g., only high-severity deps issues)

**Working Style Convention**
- Added CLAUDE.md guidance: work through ALL "Next Up" items by default
- Sessions should complete the full roadmap section

**MCP Server Improvements**
- Lists with `to_compact()` items now call it (was losing info)
- `skeleton_format` returns "File not found" for missing files

**Dogfooding Observations**
- Updated CLAUDE.md with stronger moss-first guidance
- Added Agent Lessons section to TODO.md

## v0.6.4

### Phase 33: Search, Async Docs, Self-Improvement, Guessability

**SearchAPI** (Dec 2025)
- `search_find_symbols` - find symbols by name across codebase
- `search_find_definitions` - find where a symbol is defined
- `search_find_files` - find files matching glob patterns
- `search_find_usages` - find references to a symbol
- `search_grep` - text pattern search with regex support
- Dogfood moss search instead of raw grep/glob

**Async Task Documentation**
- `docs/async-tasks.md` - background task management guide
- Covers: spawning, waiting, hang detection, cancellation
- Patterns for parallel workers and when to join

**Recursive Self-Improvement**
- `loop_critic_loop` - meta-loop that critiques loop definitions
- `loop_optimizer_loop` - optimizes loops for token efficiency
- `self_improving_docstring_loop` - docstrings with self-critique
- `docs/recursive-improvement.md` - patterns and best practices

**GuessabilityAPI**
- `guessability_analyze` - full codebase structure analysis
- `guessability_score` - overall score (0.0-1.0) and grade (A-F)
- `guessability_recommendations` - actionable improvements
- Metrics: name-content alignment, pattern consistency

## v0.6.3

### Phase 32: WebAPI, Skeleton Expand, Loops Infrastructure

**WebAPI to MCP** (Dec 2025)
- `web_fetch`, `web_search`, `web_extract_content`, `web_clear_cache` tools
- 64 total MCP tools (was 60)

**Skeleton Expand**
- `skeleton_expand` - get full source of named symbol
- `skeleton_get_enum_values` - extract enum member names

**Composable Loops**
- `LoopStep`, `AgentLoop`, `AgentLoopRunner`, `LoopMetrics` dataclasses
- `LLMConfig` + `LLMToolExecutor` for LLM integration
- `MCPToolExecutor` for external MCP server connections
- `CompositeToolExecutor` for prefix-based routing
- Loop serialization (YAML/JSON)
- `moss loop list/run/benchmark` CLI commands

**Web Module** (`moss.web`)
- `WebFetcher` - fetch with HTML extraction, caching
- `WebSearcher` - DuckDuckGo search with token-efficient results
- `ContentExtractor` - strip nav/footer/script, extract main content

**Other**
- litellm unified - all providers use litellm
- Multi-LLM rotation in LLMConfig
- Philosophy doc (`docs/philosophy.md`) with design tenets

## v0.6.2

### Phase 31: Preference Extraction from Agent Logs

Extract user preferences from AI coding assistant session logs and output to agent instruction formats.

**LLM Provider Module** (`moss.llm`)
- Protocol-based design with 9 provider implementations
- CLI provider (zero-dep fallback using llm/claude/gemini CLIs)
- Anthropic, OpenAI, LiteLLM (multi-provider gateway)
- llm (Simon Willison's library), Bifrost (high-performance gateway)
- Local LLM support: llama.cpp, KoboldCpp, ExLlamaV2
- Provider auto-discovery based on installed dependencies
- Convenience functions: `get_provider()`, `complete()`, `list_providers()`

**Multi-Format Session Log Parsing**
- Claude Code (Anthropic message format, JSONL)
- Gemini CLI (Google message format)
- Cline, Roo Code (VSCode extensions)
- Aider (markdown-based chat logs)
- Generic JSONL/chat fallback
- Auto-detection with explicit format override
- Tool name normalization across different agents

**Preference Extractors**
- `ExplicitExtractor`: "always/never/prefer" pattern matching
- `CorrectionsExtractor`: Detect user corrections after assistant actions
- `WorkflowExtractor`: Tool friction analysis, intervention patterns

**Output Format Adapters**
- Claude Code → `CLAUDE.md`
- Gemini CLI → `GEMINI.md`
- Google Antigravity → `.agent/rules/*.md`
- Cursor → `.cursorrules`
- Generic → Plain markdown
- JSON → Structured data

**Optional LLM Synthesis**
- Synthesize extracted preferences into natural language rules
- Configurable provider and model

**CLI Commands**
- `moss extract-preferences <paths>` - Extract and format preferences
  - `--format` (claude/gemini/antigravity/cursor/generic/json)
  - `--log-format` (auto/claude_code/gemini_cli/cline/roo/aider)
  - `--min-confidence` (low/medium/high)
  - `--synthesize` with `--provider` and `--model`
- `moss diff-preferences <old.json> <new.json>` - Compare preference sets

**Tests**
- 28 tests covering models, parsing, extractors, and adapters

## v0.6.1

### Phase 30: Codebase Analysis Tools

New analysis commands for comprehensive codebase insight:

**Session Analysis**
- `moss analyze-session <path>` - parse Claude Code JSONL logs
- Tool call frequency and success rates
- Token usage with proper context calculation
- Message type distribution
- Error pattern categorization

**Git Analysis**
- `moss git-hotspots` - identify frequently changed files
- Configurable time window (--days)
- Author count per file
- Last-changed timestamps

**Test Coverage**
- `moss coverage` - show pytest-cov statistics
- Per-file coverage breakdown
- Low coverage file highlighting
- Optional test run with --run flag

**Cyclomatic Complexity**
- `moss complexity` - analyze function complexity
- McCabe cyclomatic complexity per function
- Risk level categorization (low/moderate/high/very-high)
- Configurable file patterns

**Overview Enhancements**
- Added symbol counts (classes, functions) to `moss overview`
- Critical vulnerabilities shown inline with package and ID
- Skeleton summary showing top packages by size
- Updated both compact and markdown output formats

## v0.6.0

### Phase 29: Library-First Architecture
Hyper-modular refactor with auto-generated interfaces:

**29a: Core Library Refactor**
- `MossAPI` class as canonical typed API surface
- Full type hints + docstrings for introspection
- Library usable without CLI/server dependencies
- `from moss import MossAPI` as primary entry point

**29b: Plugin Protocol (Everything is a Plugin)**
- `LinterPlugin` protocol for unified tool integration
- Native plugins: RuffPlugin, MypyPlugin
- SARIFAdapter for universal SARIF-outputting tools
- LinterValidatorAdapter bridging to existing Validator system
- Version detection and availability checking
- Entry point discovery (`moss.linters`)

**29c: Interface Generator Layer**
- `moss.gen.cli` - Generate argparse CLI from API introspection
- `moss.gen.http` - Generate FastAPI routes from API
- `moss.gen.mcp` - Generate MCP tool definitions from API
- `moss.gen.openapi` - Generate OpenAPI spec from API
- 46 tests for interface generators

**29d: Wrapper Packages**
- `moss-server` CLI entry point with `--host`, `--port`, `--reload`
- `moss-mcp` CLI entry point for MCP server
- `moss[all]` meta-group for full installation
- All interfaces use optional dependencies for minimal core

**29e: Server Architecture**
- ServerState with persistent caching (CacheEntry, execute_cached)
- Cache invalidation by pattern and file mtime
- FastAPI application with REST endpoints for all MossAPI operations
- WebSocket endpoint for streaming operations
- Health check and cache management endpoints
- 20 tests for server module

### CLI: moss roadmap
- Parses TODO.md and visualizes project progress
- TUI mode with box drawing and progress bars
- Plain text mode for LLMs and piping
- Auto-detect: TUI at terminal, plain when piped
- Smart categorization of complete/in-progress/future phases

## v0.5.0

### Phase 28: Comprehensive Health Analysis
Expanded `moss health` into a comprehensive project analysis tool:
- **Dependency Analysis**: Circular dependency detection, god modules (high fan-in), orphan modules, coupling metrics
- **Structural Hotspots**: Functions with too many parameters, classes with too many methods, deep nesting, long functions, complex conditionals
- **Test Coverage Analysis**: Module-to-test mapping, test-to-code ratio, untested public API surface
- **API Surface Analysis**: Public exports inventory, undocumented APIs, naming convention checking, breaking change risk
- **Health Command Refactor**: Concise single-screen output, `--severity` and `--focus` flags, `moss report` for verbose output, `--ci` flag with exit codes (0=healthy, 1=warnings, 2=critical)

## v0.4.0

### Phase 27: Advanced Library Learning
Frequency-based abstraction learning inspired by DreamCoder:
- `LearnedLibrary` plugin with pattern-based learning
- `PatternExtractor` for code pattern detection (functions, expressions, idioms)
- Persistent storage with JSON serialization
- Compression gain estimation for abstraction scoring
- Pattern frequency tracking across synthesis runs
- 31 tests for library learning

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
