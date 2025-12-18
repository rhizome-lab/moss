# Moss Roadmap

See `CHANGELOG.md` for completed features (Phases 15-21, 23-25).

See `~/git/prose/moss/` for full synthesis design documents.

## In Progress

### Phase 22: Synthesis Integration

The synthesis framework is operational with a plugin architecture for generators, validators, and libraries.

#### 22a: Core Framework ✅
- [x] Directory structure (`src/moss/synthesis/`)
- [x] Abstract interfaces (`Specification`, `Context`, `Subproblem`)
- [x] `DecompositionStrategy` ABC
- [x] `Composer` ABC (SequentialComposer, FunctionComposer, CodeComposer)
- [x] `StrategyRouter` (TF-IDF-based)
- [x] `SynthesisFramework` engine
- [x] Integration points for shadow git, memory, event bus
- [x] Tests for framework structure

#### 22b: Code Synthesis Domain ✅
- [x] `TypeDrivenDecomposition` - decomposes by type signature
- [x] `TestDrivenDecomposition` - analyzes tests for subproblems
- [x] `PatternBasedDecomposition` - recognizes CRUD/validation patterns
- [x] **`TestValidator`** - run pytest/jest to validate code
- [x] **`TypeValidator`** - mypy/pyright type checking
- [x] **Code generators** - PlaceholderGenerator, TemplateGenerator
- [x] **Validation retry loop** - compose, validate, fix, repeat

#### 22c: CLI & Integration ✅
- [x] `moss synthesize` CLI command (shows decomposition)
- [x] `--dry-run` and `--show-decomposition` flags
- [x] **`moss edit` integration** - fallback for complex tasks
- [x] Synthesis configuration presets (default/research/production/minimal)

#### 22d: Optimization & Learning ✅
- [x] Caching infrastructure
- [x] Parallel subproblem solving (asyncio.gather)
- [x] Scale testing structure
- [x] Memory-based strategy learning (StrategyLearner with feature extraction)
- [x] Performance benchmarks (tests/test_synthesis_scale.py)

### Phase 25: Synthesis Plugin Architecture ✅

Plugin system for synthesis components inspired by Synquid, miniKanren, DreamCoder, and λ².

#### 25a: Plugin Protocols ✅
- [x] `CodeGenerator` protocol - pluggable code generation
- [x] `SynthesisValidator` protocol - pluggable validation
- [x] `LibraryPlugin` protocol - DreamCoder-style abstraction management
- [x] Metadata types for all plugins

#### 25b: Built-in Plugins ✅
- [x] `PlaceholderGenerator` - fallback TODO generation
- [x] `TemplateGenerator` - user-configurable templates (CRUD, validation, etc.)
- [x] `TestValidator` - pytest/jest test execution
- [x] `TypeValidator` - mypy/pyright type checking
- [x] `MemoryLibrary` - in-memory abstraction storage

#### 25c: Registry & Discovery ✅
- [x] `SynthesisRegistry` with sub-registries
- [x] Entry point discovery (`moss.synthesis.generators`, etc.)
- [x] Global registry with lazy initialization
- [x] 31 tests passing

#### 25d: Framework Integration ✅
- [x] `_solve_atomic()` uses generator plugins
- [x] `_validate_with_retry()` implements retry loop
- [x] Library plugin for abstraction lookup

## Future Work

### Phase D: Strategy Auto-Discovery ✅
- [x] Convert DecompositionStrategy to StrategyPlugin protocol
- [x] Entry point discovery for strategies
- [x] Config-based enable/disable

### Phase F: Configuration System ✅
- [x] `[synthesis.*]` sections in moss.toml
- [x] Template directory configuration
- [x] Plugin enable/disable
- [x] Validation retry settings

### Phase 26: LLM Integration ✅
- [x] `LLMGenerator` - Claude/GPT code generation via LiteLLM
- [x] `MockLLMProvider` - testing without API calls
- [x] `LiteLLMProvider` - unified access to multiple LLM backends
- [x] Streaming generation support
- [x] Cost estimation and budgeting
- [x] 48 tests for LLM generator

### Phase 27: Advanced Library Learning ✅
- [x] Frequency-based abstraction learning (LearnedLibrary)
- [x] Pattern extraction from code (PatternExtractor)
- [x] Persistent library storage (JSON file)
- [x] Compression gain estimation (simplified DreamCoder metric)
- [x] 31 tests for learned library

### Future: Non-LLM Code Generators

Alternative synthesis approaches that don't rely on LLMs. See `docs/synthesis-generators.md` for details.

#### High Priority
- [ ] `EnumerativeGenerator` - enumerate ASTs, test against examples (Escher/Myth)
- [ ] `ComponentGenerator` - combine library functions bottom-up (SyPet/InSynth)
- [ ] `SMTGenerator` - Z3-based type-guided synthesis (Synquid)

#### Medium Priority
- [ ] `PBEGenerator` - Programming by Example (FlashFill/PROSE)
- [ ] `SketchGenerator` - fill holes in user templates (Sketch/Rosette)
- [ ] `RelationalGenerator` - miniKanren-style logic programming

#### Research/Experimental
- [ ] `GeneticGenerator` - evolutionary search (PushGP)
- [ ] `NeuralGuidedGenerator` - small model guides enumeration (DeepCoder)
- [ ] `BidirectionalStrategy` - λ²-style type+example guided search

### Future: DreamCoder-style Learning
- [ ] Compression-based abstraction discovery
- [ ] MDL-based abstraction scoring

### Future: Multi-Language Expansion
- Full TypeScript/JavaScript synthesis support
- Go and Rust synthesis strategies

### Future: Dogfooding / Self-Analysis

Use moss to analyze itself and keep documentation current.

- [x] `moss summarize` - Recursive codebase summarization
  - Extract module purposes, key functions, architecture
  - Generate hierarchical summary (file → module → package → project)
  - Output as markdown or structured data
- [x] `moss check-docs` - Verify documentation freshness
  - Compare codebase summary against README, docs/, docstrings
  - Flag stale/missing documentation
  - Suggest updates based on code changes
- [x] `moss check-todos` - Verify TODO.md accuracy
  - Cross-reference TODOs with implementation status
  - Detect completed items still marked pending
  - Find undocumented TODOs in code comments
- [x] `moss health` - Basic project health overview (v1)
  - Health score with letter grade
  - Doc coverage, TODO stats
  - Next actions from TODO.md

#### Phase 28: Comprehensive Health Analysis

Expand `moss health` into a comprehensive but concise project analysis. Rename current verbose output to `moss report` if needed.

##### 28a: Dependency Analysis
- [x] Circular dependency detection (use existing `extract_dependencies()`)
- [x] "God modules" - modules with high fan-in (everything depends on them)
- [x] Orphan modules - modules nothing imports
- [x] Coupling metrics - inter-module dependency density
- [ ] Layer violation detection (if Architecture.md defines layers)
- [ ] Dependency graph visualization (optional `--graph` output)

##### 28b: Structural Hotspots
- [ ] Functions with too many parameters (>5)
- [ ] Classes with too many methods (>15)
- [ ] Files over threshold (>500 lines)
- [ ] Deep nesting detection (from CFG analysis)
- [ ] Long functions (>50 lines)
- [ ] Complex conditionals (high branching factor)
- [ ] Configurable thresholds in moss.toml

##### 28c: Test Coverage Analysis
- [ ] Module-to-test mapping (which modules have tests)
- [ ] Test-to-code ratio per package
- [ ] Untested public API surface (exports without tests)
- [ ] Test file organization health
- [ ] Missing test fixtures detection

##### 28d: API Surface Analysis
- [ ] Public exports inventory (`__all__`, non-underscore names)
- [ ] Public/private ratio per module
- [ ] "Breaking change risk" - widely-imported exports
- [ ] Undocumented public APIs
- [ ] Inconsistent naming patterns

##### 28e: Health Command Refactor
- [ ] Concise single-screen output (no scrolling for healthy projects)
- [ ] Severity-based filtering (show only issues above threshold)
- [ ] `moss report` for full verbose output
- [ ] `moss health --focus=deps|tests|complexity|api` for targeted analysis
- [ ] Machine-readable `--json` with all metrics
- [ ] Exit codes for CI integration (0=healthy, 1=warnings, 2=critical)

#### Phase 29: Library-First Architecture

Refactor moss into a hyper-modular library-first architecture where the core is an importable library and all interfaces (CLI, HTTP, MCP, LSP) are autogenerated wrappers.

##### 29a: Core Library Refactor
- [ ] Extract `moss.api` as the canonical typed API surface
- [ ] Full type hints + docstrings for introspection
- [ ] Library usable without any CLI/server dependencies
- [ ] `from moss import MossAPI` as primary entry point

##### 29b: Plugin Protocol (Everything is a Plugin)
- [ ] Unified `LinterPlugin` protocol for all tool integrations
- [ ] Native plugins (ruff, mypy) - fast path, optional deps
- [ ] SARIF plugin - universal adapter for SARIF-outputting tools
- [ ] JSON plugin - configurable parser for JSON output tools
- [ ] Version detection and compatibility handling
- [ ] Plugin discovery via entry points

##### 29c: Interface Generator Layer
- [ ] `moss.gen.cli` - Generate argparse CLI from API introspection
- [ ] `moss.gen.http` - Generate FastAPI routes from API
- [ ] `moss.gen.mcp` - Generate MCP tool definitions from API
- [ ] `moss.gen.lsp` - Generate LSP handlers from API
- [ ] `moss.gen.grpc` - Generate gRPC proto + handlers from API
- [ ] `moss.gen.openapi` - Generate OpenAPI spec from API

##### 29d: Wrapper Packages
- [ ] `moss` - Core library only
- [ ] `moss-cli` - CLI wrapper (depends on moss)
- [ ] `moss-server` - HTTP/WebSocket server (depends on moss)
- [ ] `moss-mcp` - MCP server (depends on moss)
- [ ] `moss-lsp` - LSP server (depends on moss)
- [ ] `moss-all` - Meta-package installing everything

##### 29e: Server Architecture
- [ ] Server as the canonical API (not CLI)
- [ ] HTTP + WebSocket for web clients
- [ ] Unix socket for local high-performance
- [ ] Persistent state (parse once, query many)
- [ ] Streaming results for long-running analysis
- [ ] Multiple concurrent clients

**Next step after Phase 29**: Continue dogfooding/self-eval with comprehensive `moss health` on moss itself.

### Future: Enterprise Features
- Team collaboration (shared caches)
- Role-based access control

---

## Notes

### PyPI Naming Collision

There's an existing `moss` package on PyPI (a data science tool requiring numpy/pandas). Before publishing, we need to either:
- Rename the package (e.g., `moss-tools`, `moss-orchestrate`, `toolmoss`)
- Check if the existing package is abandoned and claim the name
- Use a different registry
