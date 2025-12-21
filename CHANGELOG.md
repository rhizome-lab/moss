# Changelog

## Unreleased

### Features

**CLI & Workflow Improvements** (Dec 2025)
- `Workflow Arguments`: `--arg KEY=VALUE` option for passing parameters to workflows
- `Incremental Test Runner`: `--incremental` flag in watch command runs only related tests
- `TUI Syntax Highlighting`: Code highlighting in file previews (Python, Rust, JS, TS, Go, Ruby)
- `GEMINI.md Anti-stub Constraints`: Explicit rules preventing stub code and requiring verification

**Resource Monitoring** (Dec 2025)
- `Memory & Resource Metrics`: Real-time tracking of RAM usage and context token pressure for every command
- `Memory Breakdown`: Detailed RSS/VMS/USS breakdown showing exactly where memory is allocated during execution
- `TUI Resource Display`: Integrated resource metrics in the agent log with cyan/yellow indicators
- `CLI Resource telemetry`: Command-line output of tool execution costs including RAM and context
- `Telemetry Integration`: Aggregated resource high-water marks across sessions via TelemetryAPI

**Adaptive loop capabilities** (Dec 2025)
- `Adaptive Context Control`: Dynamic result preview limits based on task type (Read vs Write)
- `Adaptive Context Pruning`: Heuristic and LLM-driven importance scoring for intelligent elision
- `Adaptive Loop Depth`: Dynamic `max_steps` adjustment in `AgentLoopRunner` based on progress
- `Dynamic Turn Budgeting`: Per-turn token scaling based on estimated task complexity
- `Adaptive Model Selection`: Task-specific model routing (e.g., separate models for analysis vs generation)
- `LLM Benchmarking Harness`: Automated cross-model evaluation with markdown report generation

**Recursive improvement loops** (Dec 2025)
- `Adaptive Loop Strategy Refinement`: History-based switching between DWIM and Structured loops
- `Agentic Workflow Synthesis`: Automatic creation of new workflows from telemetry patterns

**Advanced TUI & UX** (Dec 2025)
- `Extensible Agent Modes`: Plugin-based TUI mode system (PLAN, READ, WRITE, DIFF, SESSION, BRANCH, SWARM, COMMIT)
- `TUI Git Dashboard`: Integrated view for branches, commits, hunks, and diffs with surgical rollback
- `TUI Session Resume`: Visual session history with one-click resumption and state recovery
- `Cross-file Symbol Jump`: Clickable references in TUI for quick navigation between files
- `Symbol Hover Info`: Metadata tooltips (skeletons, summaries) in the ProjectTree
- `TUI Exit Refinement`: Double `Ctrl+C` exit to avoid clipboard conflicts
- `Docs Styling`: Modern glassmorphism and rounded borders at `docs/stylesheets/custom.css`

**Safety & verification** (Dec 2025)
- `LLM Reliability Guardrails`: 'Critic-first' execution for high-risk operations
- `Heuristic Error Localization`: Trace-based bug identification from test failures
- `Mistake Detection`: Dedicated critic steps for turn-level logic analysis
- `Verification Loops & Heuristics`: Formalized structural guardrails before full validation
- `Shadow Git Access`: First-class LLM access to diffs, hunks, multi-commits, and smart merging
- `User Feedback Story`: Agent inbox for mid-task corrections
- `Editing Tools`: `EditAPI` for direct file manipulation (write, replace, insert)

**Agent & Core Infrastructure** (Dec 2025)
- `Sandbox Scoping`: Task-level workspace restriction with parent inheritance and automatic enforcement
- `Workflow Loader Abstraction`: Extracted `WorkflowLoader` protocol and `TOMLWorkflowLoader` with registry
- `Vanilla Workflow`: Minimal baseline agent loop refactored into a data-driven workflow
- `TelemetryAPI`: Unified analysis of multi-session token usage, tool patterns, and hotspots
- `Adaptive Workspace Scoping`: Dynamic sandbox control with `shrink_to_fit` and `expand_to_include`
- `RefCheck`: Cross-language reference tracking for Rust/Cargo with deduplication

**Workflow externalization** (expanded)
- Design doc for TOML-based workflow/prompt format (`docs/workflow-format.md`)
- Prompt loader with user override support (`src/moss/prompts/`)
- `load_prompt(name)` checks `.moss/prompts/` then built-ins
- `REPAIR_ENGINE_PROMPT` externalized as proof of concept
- `LLMConfig.system_prompt` now loads from `prompts/terse.txt` by default
- `get_system_prompt()` method for lazy loading with explicit override support
- Workflow loader (`src/moss/workflows/`) with TOML parsing
- `@prompts/name` and `@workflows/name` reference resolution
- `Workflow`, `WorkflowStep`, `WorkflowLimits`, `AgentDefinition` dataclasses
- Built-in `validate-fix.toml` workflow example
- User override examples in docs (`docs/workflow-format.md`)
- 19 tests for workflow loading
- Integration test for hunk-level rollback with verification failure
- `workflow_to_agent_loop()` - convert TOML workflows to executable AgentLoop
- `workflow_to_llm_config()` - convert workflow LLM config to LLMConfig
- `run_workflow()` - convenience function to load and run a workflow
- `moss workflow list` - list available workflows
- `moss workflow show <name>` - show workflow details (human or JSON)
- `moss workflow run <name> --file <path>` - execute a workflow
- `WorkflowProtocol` - protocol for static and dynamic workflows
- `WorkflowContext` - runtime context for dynamic step generation
- `build_steps(context)` method - enables Python workflows with conditional logic
- Example workflows: `ConditionalTestWorkflow`, `LanguageAwareWorkflow`

**Memory integration in agent loops** (new)
- `LLMToolExecutor` now accepts `memory: MemoryManager` parameter
- Automatic memory context injected into LLM system prompts
- Triggered memory checked before tool execution for warnings
- Episodes recorded after each tool call for future learning
- Non-blocking: memory errors don't break execution

**Checkpoint restore** (new)
- `moss checkpoint restore <name>` - revert working directory to checkpoint state
- `GitAPI.restore_checkpoint()` in moss_api.py
- Completes checkpoint lifecycle: create → diff → merge/abort/restore

**Diagnostics-validation integration** (new)
- `DiagnosticValidator` - uses signal-only parsers for structured error feedback
- `diagnostics_to_validation_result()` - bridge between diagnostics and validators
- Factory functions: `create_cargo_validator()`, `create_typescript_validator()`, etc.
- `create_rust_validator_chain()`, `create_typescript_validator_chain()`

**Agent sandboxing** (new)
- `CommandPolicy` in `policy.py` - evaluates bash/shell commands against allowlists/blocklists
- Blocks dangerous commands (rm, sudo, curl, etc.) and patterns (pipe to shell, rm -rf)
- Categories: ALLOWED (read-only), BUILD (compilers), GIT, TEST commands
- `SafeShell` wrapper in `sandbox.py` with safe versions of blocked commands
- `safe_curl()` - URL allowlisting, `safe_delete()` - path restrictions, `safe_git()` - blocks force push
- `SandboxedToolExecutor` for agent loop integration with policy checks

**First-class sessions** (new)
- `Session` class in `session.py` - resumable, observable work units
- Tracks tool calls, file changes, LLM usage, checkpoints
- Status lifecycle: created → running → paused/completed/failed
- `SessionManager` for persistence and listing
- Event emission for observability via `EventBus`
- JSON serialization for save/resume across restarts

**Signal-only diagnostics** (new)
- `diagnostics.py` - parse structured compiler/linter output, discard noise
- Parsers: Cargo, TypeScript, ESLint, Ruff, GCC/Clang, Generic fallback
- Extracts: severity, message, location, code, suggestions
- Strips ANSI codes and ASCII art from raw output
- `DiagnosticRegistry` for auto-detection of output format
- `get_structured_command()` returns flags for JSON output

**MCP ephemeral response handling** (new)
- Large responses (>500 chars) now use ResourceLink + ephemeral cache
- Preserves LLM context by storing full content separately (5 min TTL)
- Only preview (~2KB) goes inline, full content available via resources/read
- `EphemeralCache` class in `cache.py` with TTL-based expiration

**Concurrent agent execution** (new)
- `Manager.spawn_async()` - fire-and-forget agent execution
- `Manager.spawn_many_async()` - spawn multiple agents without blocking
- `Manager.wait_any()` - wait for first agent to complete
- `Manager.wait_all()` - wait for all agents with optional timeout
- Callback support via `on_complete` parameter

**Graceful error handling** (new)
- New `moss.errors` module with categorized error types
- `handle_error()` classifies exceptions and provides suggestions
- `ErrorCollector` for batch operations with aggregated reporting
- MCP servers now return structured error responses with suggestions

**Structured LLM summarization in agent loops**
- New `_build_structured_context()` method in `AgentLoopRunner`
- Goose-inspired sections: User Intent, Completed Steps, Current Work
- Better context preservation across multi-step loops
- Additional prompts for meta-loop operations (analyze_loop, estimate_tokens, find_redundancy)

**Rust CLI overview command** (new)
- `moss overview` - comprehensive codebase overview in ~95ms
- Aggregates health, docs, complexity, imports, TODOs/FIXMEs
- Health score grading (A-F) based on complexity, risk, and doc coverage
- Compact mode (`-c`) for single-line summaries
- JSON output mode for programmatic use

**Rust CLI context command wired to Python**
- `moss context <file>` now delegates to Rust CLI when available
- 10-100x faster than pure Python for large files
- Falls back to Python implementation when Rust not found

### Bug Fixes

- Add `Symbol.to_dict()` method for JSON serialization in skeleton/context commands
- Add `ControlFlowGraph.entry/exit` properties for API compatibility
- Add `CFGNode.label/lineno` properties for JSON output
- Add `Export.export_type` property as alias for `kind`
- Add `@dataclass` decorator to `RAGAPI` to fix constructor signature

## v0.6.10

### Performance Improvements

**Rust CLI find-symbols command** (new)
- `moss find-symbols <name>` - fast symbol search using indexed SQLite database
- ~1ms for symbol queries (was 723ms with full Python codebase scan)
- Supports fuzzy matching (`-f true`), kind filtering (`-k function`), result limits (`-l`)
- JSON output mode with `--json` for programmatic use
- Python API `SearchAPI.find_symbols()` now calls Rust CLI when available
- Falls back to Python implementation when Rust CLI not found

**Rust CLI grep command** (new)
- `moss grep <pattern>` - fast text search using ripgrep's grep crate
- JSON output mode with `--json` for programmatic use
- Supports glob patterns (`--glob "*.py"`), case-insensitive (`-i`), result limits (`-l`)
- ~4ms for codebase-wide searches (was 9.7s with pure Python)
- Python API now calls Rust CLI when available

**Parallel health analysis**
- `moss health` now uses rayon for parallel file processing
- ~95ms down from ~500ms (5x faster)
- File counting and complexity analysis run concurrently

## v0.6.9

### Index Reliability

**Graceful Degradation**
- `moss imports` now falls back to direct file parsing when index unavailable
- Commands work without daemon - all have local fallback paths

**Error Recovery**
- Automatic database rebuild on corruption detection
- Quick integrity check (`PRAGMA quick_check`) on index open
- Removes corrupted DB files and journal/WAL files before rebuild

**Incremental Refresh**
- `incremental_refresh()` - only update changed files (faster than full reindex)
- `incremental_call_graph_refresh()` - only re-parse changed source files
- `get_changed_files()` - detect new/modified/deleted files since last index

**File Watching (Daemon)**
- Daemon auto-reindexes files on create/modify/delete events
- Uses `notify` crate for cross-platform file system events
- Skips `.moss` directory to avoid infinite loops

### Rust CLI Expansion

**18 Commands**
- path, view, search-tree, symbols, expand, callers, callees
- tree, skeleton, anchors, deps, cfg, complexity, health, summarize
- daemon (status/shutdown/start), reindex

### Daemon Architecture

- Unix socket IPC for fast local communication
- Idle timeout for automatic resource cleanup
- Chunked streaming for large responses

### Call Graph

- SQLite index for persistent call relationship storage
- 29,000x faster callers lookup (0.6ms vs 17.5s)

### Reference Tracing

**Cross-file Resolution**
- Import tracking (SQLite table: file → module, name, alias)
- `moss imports <file>` command to query imports from index
- `moss imports <file>:<name> --resolve` to trace name to source module
- Cross-file resolution via import alias JOIN for callers/callees
- Qualified names (module.func vs func) with callee_qualifier
- Wildcard import resolution (from X import * → check X's exports)
- Method call resolution (self.method() → Class.method)

### Benchmark Suite

- CI integration with regression detection thresholds
- Automated performance tracking across commits

## v0.6.8

### Tree Commands & Performance

**New CLI Commands**
- `moss path <query>` - Fuzzy path/symbol resolution
- `moss view <target>` - View node in codebase tree
- `moss search-tree <query>` - Search symbols in tree
- `moss expand <target>` - Show full source of symbol
- `moss callers <target>` - Find callers of a function
- `moss callees <target>` - Find what a function calls

**MCP DWIM Routing**
- All tree commands wired to MCP single-tool interface
- DWIM semantic matching for natural language queries
- Aliases: `expand` → `cli_expand`, `callers`, `callees`, etc.

**Performance (4x faster file lookups)**
- `os.walk` with in-place pruning (37x faster than `rglob`)
- Lazy AST parsing - only parse when symbols needed
- File lookups: 914ms → 222ms
- Symbol lookups still require parsing (~800ms)

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
