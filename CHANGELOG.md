# Changelog

## v0.1.0 (Dec 2025)

First release. See `docs/` for design docs and `README.md` for usage.

### Symbol Extraction Consolidation

Unified extraction layer in `extract.rs`:
- Both `skeleton.rs` and `symbols.rs` now use shared `Extractor` class
- Eliminates ~250 lines of duplicate tree-walking code
- `compute_complexity` moved to shared module
- Each consumer still processes output differently:
  - skeleton: nested children, signatures, docstrings (for viewing)
  - symbols: flat with parent refs, complexity (for indexing)

### View Command Improvements

Unified ViewNode abstraction for consistent output:
- All `moss view` paths (directory, file, symbol) now use ViewNode
- Text output shows tree structure with line ranges (L{start}-{end}, N lines)
- FormatOptions: docstrings, line_numbers, skip_root, max_depth
- Useless docstring filtering (skips docstrings that just repeat the name)
- JSON and text output are now structurally consistent

### OutputFormatter Trait

Unified output formatting infrastructure (`output.rs`):
- `OutputFormat` enum: Text, Json, JsonPretty
- `OutputFormatter` trait for types that support multiple output formats
- Helper functions: `print_json`, `print_json_pretty`, `print_formatted`
- Implemented for `GrepResult` as reference pattern

### Server Protocols

All three server protocols now implemented:
- **moss serve mcp**: MCP server for LLM integration (stdio, `--features mcp`)
- **moss serve http**: REST API on configurable port (axum-based)
  - `/health`, `/files`, `/files/*path`, `/symbols`, `/symbols/:name`, `/search`
- **moss serve lsp**: LSP server for IDE integration (tower-lsp)
  - Document symbols (nested structure from skeleton extraction)
  - Workspace symbol search (from index)
  - Hover (symbol kind, signature, docstring)
  - Go to definition (jumps to symbol definition)
  - Find references (finds all callers via call graph)
  - **Rename symbol** (cross-file refactoring with prepare_rename support)

### Daemon Integration

Complete FileIndex API exposed via daemon socket:
- File search: `find_by_name`, `find_by_stem`, `find_like`
- Symbol queries: `find_symbols`, `find_symbol`, `find_callers`, `find_callees`
- Import resolution: `resolve_import`, `find_callees_resolved`, `find_importers`
- Cross-language references: `find_cross_refs`, `find_cross_ref_sources`, `all_cross_refs`
- Status now includes uptime, query count, PID

### Cross-Language Reference Tracking

Detects and indexes cross-language FFI bindings:
- **Rust → Python**: PyO3 crates detected via Cargo.toml
- **Rust → JavaScript**: wasm-bindgen crates
- **Rust → Node.js**: napi-rs crates
- **Rust → C ABI**: cdylib crates
- **Python → C**: ctypes/cffi usage detection
- All refs stored in `cross_refs` table, queryable via daemon
- **Trait-based architecture**: `FfiBinding` trait in moss-languages allows custom binding detectors

### Package Management

New `moss package` subcommands:
- **why**: Show reverse dependency path for a package (`moss package why tokio`)
- **audit**: Security vulnerability scanning across ecosystems
  - cargo (cargo-audit), npm (npm audit), python (pip-audit), go (govulncheck), gem (bundle-audit)
  - Human-readable and `--json` output

### Analyze Command Enhancements

New flags for comprehensive codebase analysis:
- **--lint**: Run all detected linters and show diagnostics
- **--hotspots**: Git history analysis (churn × complexity scoring)
  - Shows top 20 bug-prone files sorted by risk score
  - Score = commits × √churn × (1 + avgComplexity/10)
  - Combines git log data with indexed complexity

### Unified Linting

New adapters added to moss-tools:
- **mypy**: Python type checker
- **pyright**: Python type checker (faster)
- **eslint**: JavaScript/TypeScript linter
- **deno-check**: TypeScript type checking via Deno

Watch mode (`moss lint --watch`):
- Monitors file changes with 500ms debounce
- Auto-filters by relevant file extensions
- Skips hidden files/directories

### Package Registry Queries

New `moss package` command queries package registries without web search:
- **Subcommands**: `info`, `list`, `tree`, `outdated`
- **moss-packages crate**: Ecosystem trait with implementations for 11 ecosystems
- **Ecosystems**: cargo, npm, python, go, hex, gem, composer, maven, nuget, nix, conan
- Auto-detection from manifest files (Cargo.toml → cargo, package.json → npm, etc.)
- **Multi-ecosystem projects**: `list` and `tree` show all ecosystems when multiple detected
- Tool detection from lockfiles with fallback to fastest available
- Most ecosystems use HTTP APIs directly (no local tool required)
- Unified output: name, version, description, license, features, dependencies
- `list`: Parse manifest for declared dependencies
- `tree`: Show full dependency tree from lockfile (handles workspaces)
- `outdated`: Compare installed versions (from lockfile) to latest
- `--json` for structured output (DependencyTree with nested TreeNode objects)

### Language Node Kind Audits

Added `validate_unused_kinds_audit()` tests to all 34 language files:
- Each language explicitly documents which tree-sitter node kinds exist but aren't used
- Tests fail if documented kinds don't exist, are actually used, or if useful kinds are undocumented
- Catches stale documentation when grammars update or Language trait implementations change

### Embedded Content Support

Template languages now extract symbols and imports from embedded code:
- Vue/Svelte/HTML: `<script>` blocks parsed with JavaScript/TypeScript grammar
- Vue/Svelte/HTML: `<style>` blocks parsed with CSS/SCSS grammar
- Automatic lang detection: `<script lang="ts">` uses TypeScript, `<style lang="scss">` uses SCSS
- Line numbers correctly adjusted for embedded content offset

### CLI Surface Cleanup

Major refactoring to align with three-primitive philosophy (view, edit, analyze):
- **CLI reduced from 29 to 8 commands** (-5000+ lines)
- **Unified ViewNode abstraction**: Directories, files, and symbols share the same tree structure
  - JSON output returns structured ViewNode data instead of string lines
  - `--calls`/`--called-by` moved to `moss analyze` (call graph analysis, not viewing)
- Removed 20 redundant commands:
  - callers, callees → analyze --calls/--called-by
  - complexity → analyze --complexity
  - cfg, scopes, health → analyze (inscrutable output removed)
  - symbols, anchors, expand, context → view with depth/--full
  - path, search-tree → view (fuzzy matching, lists multiple matches)
  - deps, summarize, imports → view --deps
  - reindex, index-stats, list-files, index-packages → `moss index` subcommand
  - overview → analyze --overview
- Fixes:
  - view --calls/--called-by semantics were swapped (now in analyze)
  - view --deps now shows exports (was missing), doesn't show symbols
  - view lists all matches when query is ambiguous (was silently picking first)

### Tree View Improvements

- **Collapse single-child folders**: `src/moss_intelligence/` shown as one line instead of two
- **Smart display by default**: `--raw` flag to disable collapsing
- **Removed `tree` command**: consolidated into `view` (use `moss view <dir>`)
- **Removed `skeleton` command**: consolidated into `view` (docstrings now shown by default)
- **Added `--dirs-only` and `--raw` flags** to `view` command for directory trees

### Performance

- **analyze --health 5x faster**: Uses indexed complexity data instead of re-parsing (~160ms vs 866ms previously)
- **Complexity stored in index**: Computed during symbol indexing, queried via SQL for health reports
- **File index excludes .git/**: Was incorrectly including ~8600 git object files in counts

### Session Analysis Command

New `moss sessions` command for analyzing Claude Code and Gemini CLI logs:
- `moss sessions` - list sessions (newest first)
- `moss sessions <id>` - dump raw JSONL
- `moss sessions <id> --jq '.type'` - filter with jq expressions
- `moss sessions <id> --analyze` - full analysis report
- `moss sessions "agent-*" --analyze` - aggregate multiple sessions

Plugin architecture for log formats (Claude Code JSONL, Gemini CLI JSON).
Ported Python session_analysis.py (~1200 lines) to Rust with jaq integration.

Added `scripts/session-corrections.sh` for correction pattern extraction:
- Finds acknowledgments (You're right, Good point), apologies, self-corrections
- Extracts user messages that triggered corrections

Derived CLAUDE.md design principles from correction pattern analysis:
- "Unify, don't multiply" - fewer concepts = less mental load
- "Simplicity over cleverness" - stdlib > dependencies
- "When stuck, reconsider the problem" - not just try more solutions

### Phase 5 Language Support

Added 5 new languages with full Language trait implementations:

**Kotlin:**
- AST parsing for classes, objects, interfaces, functions, type aliases
- KDoc comment extraction
- Visibility detection (public/private/protected/internal)
- Import resolution via Maven/Gradle (shared infrastructure with Java)
- Extensions: `.kt`, `.kts`

**C#:**
- Classes, structs, interfaces, records, namespaces
- XML doc comment extraction with tag stripping
- NuGet cache detection hints
- Extensions: `.cs`

**Swift:**
- Classes, structs, protocols, enums, actors
- Swift doc comment extraction
- Swift Package Manager version detection
- Extensions: `.swift`

**PHP:**
- Classes, interfaces, traits, enums, namespaces
- PHPDoc comment extraction
- PSR-4 path resolution hints, Composer integration
- Extensions: `.php`, `.phtml`

**Dockerfile:**
- FROM stage extraction as containers
- Image name and stage alias parsing
- Simplified implementation for infrastructure files
- Extensions: `.dockerfile`

Total languages now supported: 24

### Phase 5 Language Support (continued)

Added 11 more languages:

**Systems/Low-level:**
- Lua: require(), LDoc comments, local/global visibility
- Zig: @import(), /// doc comments, pub visibility

**Functional/Backend:**
- Elixir: defmodule, def/defp, @doc/@moduledoc, Hex hints
- Erlang: -module(), function clauses, -export, rebar3 hints
- F#: modules, functions, /// XML docs, NuGet hints

**Mobile:**
- Dart: classes, functions, /// docs, pub.dev hints

**Data/Config:**
- SQL: CREATE TABLE/VIEW/FUNCTION/TYPE extraction
- GraphQL: types, interfaces, enums, unions, scalars
- HCL: Terraform blocks (resource, data, module, variable)

**Frontend:**
- SCSS: @mixin, @function, @import/@use/@forward, SassDoc
- Svelte: script/style extraction, SvelteKit $lib alias

Total languages now supported: 35

### Tree View (additional improvements)

- **Boilerplate-aware depth**: Directories like `src/`, `lib/`, `crates/` don't count against `max_depth` limit
  - Shows more useful content at shallow depths
  - Example: `moss view . --depth=2` now shows files inside `src/commands/`
- Added 2 tests for boilerplate-aware depth limiting

### CLI Cleanup

- Fixed `--calls`/`--called-by` help text (was swapped, code was correct)
- Cleaned up view.rs: removed unused parameters, unified symbol search functions
- Fixed clippy warnings: if-let patterns, range contains, unnecessary casts

### Language Trait Defaults Removed (Phase 4)

Removed all default implementations from the `Language` trait following CLAUDE.md principle: "defaults let you 'implement' a trait without implementing it. That's a silent bug."

- 24 methods now require explicit implementation: `has_symbols`, `extract_type`, `extract_docstring`, `extract_imports`, `extract_public_symbols`, `is_public`, `get_visibility`, `container_body`, `body_has_docstring`, `node_name`, `file_path_to_module_name`, `module_name_to_paths`, `lang_key`, `resolve_local_import`, `resolve_external_import`, `is_stdlib_import`, `get_version`, `find_package_cache`, `indexable_extensions`, `find_stdlib`, `package_module_name`, `package_sources`, `discover_packages`, `find_package_entry`
- 3 methods kept as provided defaults (utility methods): `discover_flat_packages`, `discover_recursive_packages`, `discover_npm_scoped_packages`
- All 18 language files updated with explicit implementations
- Languages with `has_symbols() -> false`: JSON, YAML, TOML, Markdown, HTML, CSS (data formats)
- Languages with `has_symbols() -> true`: Python, Rust, Go, JavaScript, TypeScript, TSX, Java, C, C++, Ruby, Scala, Vue, Bash
- All 72 tests pass

### Language Feature Flags

Added feature flags to moss-cli for selective language support:
- `all-languages` (default), `tier1`, individual `lang-*` flags
- Enables smaller builds: `--no-default-features --features lang-python,lang-rust`

### Command Module Extraction

Completed extraction of all commands from main.rs to individual modules:
- main.rs reduced from 2122 to 821 lines (-62%)
- 29 command modules in `commands/` directory (one per command)
- Removed Profiler (added complexity for marginal value)
- New modules: path_cmd, symbols_cmd, callers, tree_cmd, expand, summarize_cmd, find_symbols, analyze_cmd, view_cmd

### Remove Language-Specific CLI Code

Removed Python subprocess calls and hardcoded language counters from moss-cli:
- Deleted `run_python_test_coverage`, `run_python_scopes`, `run_python_test_health` functions
- Removed `--test-coverage`, `--scopes`, `--test-health` CLI flags (shelled out to Python)
- Replaced `python_files`/`rust_files`/`other_files` with dynamic `files_by_language: HashMap<String, usize>`
- `HealthReport` and `OverviewReport` now use `moss_languages::support_for_extension()` for file classification
- Removed ~270 lines of dead code from complexity.rs (legacy `analyze_python`, `analyze_rust` methods)
- Removed ~145 lines of dead code from edit.rs (legacy `find_symbol_in_node`, `check_node_is_container` methods)
- Fixed unnecessary `&mut self` in Editor methods
- Added `find_package_entry()` to Language trait for Python's `__init__.py` handling
- Added `file_path_to_module_name()` to Language trait for module name resolution
- Added `has_language_support()` helper, replaced hardcoded `.py`/`.rs` extension filters
- Refactored registry to use mutable `RwLock<Vec>` with `register()` function
- Extension map built dynamically from each language's `extensions()` method

### Package Indexing Trait Methods

Added package indexing methods to `Language` trait:
- `find_stdlib()` - locate language standard library
- `should_skip_package_entry()` - filter entries during indexing
- `package_module_name()` - extract module name from filename
- Helper functions `skip_dotfiles()` and `has_extension()` for common patterns
- Refactored Python, Go, JavaScript, C++, Rust indexers to use trait methods
- C++ now handles extensionless stdlib headers (vector, iostream, etc.)
- Removed standalone `is_cpp_header` function (-10 net lines in main.rs)
- Renamed `LanguageSupport` trait to `Language` (each struct IS the language)

### Language Module Consolidation

Code deduplication in moss-languages:
- Inlined `go_mod.rs` into `go.rs` (GoModule struct, parse/find/resolve functions)
- Created `ecmascript.rs` with shared JS/TS/TSX implementation (~400 lines deduplicated)
- JavaScript, TypeScript, and TSX now use shared ecmascript module while keeping separate feature flags

### Import Resolution Consolidation

Moved all import resolution logic to `Language` trait:
- Added `resolve_local_import()` and `resolve_external_import()` methods to trait
- Moved `external_packages.rs` and `go_mod.rs` from moss-cli to moss-languages
- Each language implements resolution: Python, Go, Rust, JavaScript, TypeScript, C, C++, Java
- Single 12-line `resolve_import()` function in main.rs replaces 550+ lines of per-language code
- Deleted obsolete `resolution.rs` module (ImportResolver trait merged into Language)

### Trait-based Language Architecture

Major refactor: each language struct IS its support implementation.
- Deleted `Language` enum from moss-core - no longer needed
- Renamed `PythonSupport` → `Python`, `RustSupport` → `Rust`, etc.
- Added `support_for_path()` and `support_for_extension()` for dynamic lookup
- Replaced `parse_lang(Language::X, ...)` with `parse_with_grammar("x", ...)`
- External crates can now implement `Language` for new languages
- Improved trait API: `export_kinds()` → `public_symbol_kinds()` + `visibility_mechanism()` enum
- All 76 tests pass

This makes the language system extensible - add new languages by implementing a single trait.

### Arborium Migration

Replaced ~17 individual tree-sitter-* grammar crates with single arborium dependency:
- Unified grammar management via GrammarStore
- Added Scala and Vue language support
- Access to 70+ languages (vs 17 before)
- Simplified Parsers API: `parse_lang()` replaces `get().parse()`
- Net code reduction: -108 lines

**Scala/Vue Skeleton Extractors:**
- Scala: class, object, trait, function definitions
- Vue: functions from script sections (function declarations, const arrow functions)
- Fixed Python docstring extraction for arborium grammar (string nodes vs expression_statement)
- All 71 tests pass

### Test Suite Cleanup

- Removed obsolete tests for deleted MossAPI/health module (8 tests)
- Removed 70 obsolete DWIM NL matching tests (xfail/xpass → deleted)
- Final count: 2098 passed, 42 skipped (all optional deps)

**Executor Refactoring:**
- CLIExecutor and MCPToolExecutor now use `_get_tool_dispatcher()` instead of MossAPI
- Fixed dispatcher bugs: complexity handler pattern=None, security handler invalid arg

**Skipped Test Fixes:**
- Deleted 14 placeholder test files (test_synthesis*.py stubs, etc.)
- Fixed dogfooding test paths (src/moss → packages/moss-intelligence)
- Added `Anchor.parse()` classmethod for MCP tool string parsing
- Updated TestAllToolsReturnCompact to test only dispatcher-backed tools

**DWIM Simplification:**
- Removed NL matching tests (embeddings removed, DWIM simplified to 3 primitives)
- Kept 202 tests for alias resolution, typo tolerance, case/separator equivalence
- 506 lines removed from test files

### Features

**External Package Resolution** (Dec 24 2025)
- Python stdlib resolution: finds modules in `sys.prefix/lib/pythonX.Y/`
- Python site-packages resolution: finds installed packages in venv
- Go stdlib resolution: finds packages in `$GOROOT/src/`
- Go mod cache resolution: finds dependencies in `$GOMODCACHE` or `~/go/pkg/mod/`
- `view --focus` now falls back to external packages when local resolution fails
- Global package index database: `~/.cache/moss/packages.db` with version ranges (major, minor)
- PackageIndex API: insert/find packages and symbols with version filtering
- Lazy indexing: packages indexed on first resolution, cached for future lookups
- `moss index-packages` command: pre-index packages with `--only=python,go,js,deno,java,cpp,rust`
- JavaScript/TypeScript: node_modules resolution with package.json parsing (exports/module/main)
- Rust: cargo registry resolution in `~/.cargo/registry/src/`
- Deno: URL import cache resolution (`~/.cache/deno/deps/`) and npm: imports
- Java: Maven repository (`~/.m2/repository/`) and Gradle cache resolution
- C/C++: System include path detection and header indexing

**Additional Analysis Modules** (Dec 24 2025)
- Binary detection in call graph: detects binary files by null byte check (8KB sample)
- Rust in-file tests: `#[cfg(test)]` module detection in test_gaps analysis
- Test health module: extracts pytest markers (@skip, @xfail, @skipif, @parametrize)
- Go imports in Rust index: `moss imports file.go` works like other languages
- Go module parsing: go_mod.rs for go.mod parsing and import resolution

**CLI/MCP Integration for Analysis Modules** (Dec 24 2025)
- Added `moss analyze --test-coverage` flag for test coverage analysis via `moss_intelligence.test_gaps`
- Added `moss analyze --scopes` flag for public/private symbol statistics via `moss_intelligence.scopes`
- Added `moss analyze --test-health` flag for pytest marker extraction
- Regenerated `specs/mcp_tools.json` to include `test_gaps_*`, `scopes_*`, `test_health_*` MCP tools
- All modules now accessible via CLI and MCP

**File Boundaries** (Dec 24 2025)
- `expand_import_context()` + ViewOptions.expand_imports
- `get_available_modules()` + ViewOptions.show_available
- `expand_import_context(depth=N)` + ViewOptions.import_depth
- See `docs/file-boundaries.md` for design

### Performance

**Python Path Detection** (Dec 24 2025)
- Removed Python subprocess calls from Rust (violated architecture boundary)
- Now detects Python version from filesystem structure (lib/pythonX.Y/)
- `view --focus` drops from 0.55s to 0.028s (20x faster)

### Bug Fixes

**CLI Argument Parsing** (Dec 24 2025)
- `--focus validators` now correctly parses target as "validators" (was consuming it as focus filter)
- Added `require_equals=true` to `--focus` flag (use `--focus=module` syntax)
- Added validation: `--focus` without file target now errors instead of silently showing project tree

**Focus Mode External Imports** (Dec 24 2025)
- `--focus` now shows skeletons of external packages (stdlib, site-packages), not just local files
- External imports display as `[module]` in output header

**Package Restructuring** (Dec 23 2025)
- Extracted core functionality into separate installable packages:
  - `moss-intelligence`: Code analysis (skeleton, complexity, security, deps, clones)
  - `moss-context`: Generic working memory (domain-agnostic, token-budgeted)
  - `moss-orchestration`: Agent loops, sessions, drivers, shadow git
  - `moss-llm`: LLM adapters using litellm for provider abstraction
- Created frontend wrapper packages:
  - `moss-mcp`: MCP server (single-tool and multi-tool modes)
  - `moss-lsp`: Language Server Protocol for IDE integration
  - `moss-tui`: Textual-based terminal UI
  - `moss-acp`: Agent Client Protocol for Zed/JetBrains
- Clean separation of concerns: code intelligence, context management, orchestration
- Plugin architecture: core packages define protocols, callers provide implementations
- See `docs/restructuring-plan.md` and `docs/api-boundaries.md`

**Driver Plugin Architecture** (Dec 23 2025)
- Unified execution model: all task automation flows through pluggable drivers
- Driver protocol: `decide_next_step()` and `on_action_complete()`
- Built-in drivers: UserDriver, LLMDriver, WorkflowDriver, StateMachineDriver
- DriverRegistry with entry point discovery for plugin drivers
- Generic `run_task()` loop works with any driver
- Sync adapters: wrap agent_loop, step_loop, state_machine_loop for async use
- See `docs/driver-architecture.md` for design details

**TUI Cleanup** (Dec 23 2025)
- Removed command input usage from code paths:
  - `navigate_branch` now calls `switch_branch` API directly
  - File/symbol selection calls `action_primitive_view()` uniformly
  - `navigate()` calls `_expand_and_select_path` directly
  - Directory double-click works in all modes
- Wired async edit execution:
  - Edit primitive now executes using `edit()` from `moss/edit.py`
  - Shows real-time progress and results in explore panel
  - Background task execution with proper lifecycle management

**TUI Redesign Phase 3** (Dec 23 2025)
- Edit modal: press `e` on file to open modal dialog
  - Enter edit task description in modal
  - No command input needed for editing
- Contextual footer actions:
  - Footer shows context-specific hints (sub-view nav in Analysis, Resume in Tasks)
  - Updates dynamically when selection changes

**TUI Redesign Phase 2** (Dec 23 2025)
- Simplified to three modes: Code, Analysis, Tasks
  - Removed: PLAN, READ, WRITE, DIFF, BRANCH, SWARM, COMMIT modes
- Analysis sub-views with `[` and `]` navigation:
  - Complexity: cyclomatic complexity ranking
  - Security: vulnerability scanning
  - Scopes: public/private symbol visibility
  - Imports: dependency graph with circular dep detection
- Shadow branches for tasks:
  - `Session.start()` creates `shadow/task-{id}` branch
  - `Session.resume()` checks out task's shadow branch
  - `Session.get_diff()` returns changes vs base branch
- Task diff view: click task to see its changes

**TUI Redesign Phase 1** (Dec 23 2025)
- Unified Task model: sessions, workflows, and agents are all "tasks"
  - Each task has: parent_id, children, shadow_branch, driver (user/agent)
  - Shadow branch naming: `shadow/task-{id}` (persistent, never deleted)
  - Type aliases: `Task = Session`, `TaskManager = SessionManager`
- Tasks panel (renamed from Sessions):
  - Shows hierarchical task tree with parent/children
  - Color-coded by driver: cyan=user, magenta=agent
  - Status icons: ○ created, ● running, ◐ paused, ✓ completed, ✗ failed
  - Click to resume task (stub implementation)
- TUI fixes:
  - Graceful handling when Shadow Git not initialized
  - Mode names in Title case (was UPPERCASE)
- Design doc: `docs/tui-design.md`

**Distribution** (Dec 23 2025)
- GitHub Actions workflow for building release binaries
  - Linux (x86_64, aarch64, musl)
  - macOS (Intel, Apple Silicon)
  - Windows (x86_64)
  - SHA256 checksums for verification
- `moss update` command for self-updating
  - Downloads and installs new binary from GitHub releases
  - SHA256 checksum verification against release checksums
  - Handles tar.gz (Unix) and zip (Windows) archives
  - `--check` flag to check without installing
  - JSON output for programmatic access

**Variable Scope Analysis** (Dec 23 2025)
- `moss scopes <file>`: show scope hierarchy with all variable bindings
  - Tracks functions, classes, loops, comprehensions, lambdas
  - Shows parameters, variables, imports, for-loop targets
  - Supports Python and Rust
- `moss scopes <file> --line N`: show bindings visible at a specific line
  - Lists all variables/functions in scope at that point
  - Useful for understanding what names are available
- `moss scopes <file> --line N --find <name>`: find where a name is defined
  - Shows the exact definition location for a name at a given line
  - Handles shadowing correctly (returns innermost definition)
- Type inference for Python assignments:
  - Constructor calls: `x = MyClass()` shows `x: MyClass`
  - Qualified constructors: `x = module.Class()` shows full path
  - Literals: `x = {}` → dict, `x = []` → list, `x = 1.5` → float

**Import Graph Commands** (Dec 23 2025)
- `moss imports --graph <file>`: bidirectional import graph
  - Shows what the file imports
  - Shows what files import it (via module name resolution)
  - JSON output includes full import details
- `moss imports --who-imports <module>`: reverse import lookup
  - Find all files that import a given module
  - Shows specific symbols imported and line numbers

**View Primitive Enhancements** (Dec 23 2025)
- `--types-only` flag: filters skeleton to show only type definitions (class, struct, enum, interface)
  - Strips methods/functions for architectural overview
  - Works with all skeleton output (Python, Rust, TypeScript, etc.)
- `--focus[=module]` mode: shows target file at full detail, plus skeletons of imported modules
  - Resolves Python imports (relative and absolute) to local files
  - Resolves Rust crate-local imports (`crate::`, `self::`, `super::`)
  - Resolves TypeScript/JavaScript imports (relative paths with extension inference)
  - Shows imported module skeletons at signature level (depth 1)
  - Barrel file hoisting: traces through re-exports (`export * from`, `export { x } from`)
  - Selective filtering: `--focus=models` only expands matching imports
  - Useless docstring detection: skips docstrings that just repeat function name
  - Combines with `--types-only` for types-only focus view
- `--resolve-imports`: shows only the specific imported symbols (more targeted than focus)
  - Lists each imported symbol with its signature, grouped by module
  - Ideal for understanding what a file uses vs. what modules offer
- `--all`: shows all symbols including private ones (normally filtered by convention)
  - Useful for debugging or understanding internal implementation details
- Flags work together for maximum token efficiency

**Package Restructuring** (Dec 23 2025)
- cli.py (5687 lines) → cli/ package with _main.py (backwards compatible)
- moss_api.py (4148 lines) → moss_api/ package with _main.py (backwards compatible)
- Gradual extraction can now happen incrementally

**State Machine Workflows** (Dec 23 2025)
- New `state_machine_loop()` for graph-based execution with conditional transitions
- States defined via `[[states]]` in TOML with `[[states.transitions]]`
- Condition plugin system: `has_errors`, `success`, `empty`, `contains:X`
- Extensible via `CONDITION_PLUGINS` registry
- Example workflow: `validate-fix.toml` (analyze → fix → verify loop)
- State lifecycle hooks: `on_entry` (when entering state), `on_exit` (before leaving)
- Parallel state execution: `parallel` (list of states) + `join` (target state)
- Fork/join semantics via ThreadPoolExecutor, results collected as `parallel_result`
- Nested state machines: `workflow` field on states runs another workflow TOML
- LLM-driven state selection: `llm_select` field lets LLM choose next state from transitions

**Nested Steps** (Dec 23 2025)
- WorkflowStep now supports compound steps (with sub-steps)
- Compound steps execute in child Scope with inherited strategies
- Recursive step parsing in load_workflow for nested TOML structures
- Design doc for multi-agent communication: docs/nested-execution.md
- Context modes for compound steps: `isolated` (default), `shared`, `inherited`
- InheritedContext wrapper: child sees parent context (read), writes to own storage
- `summarize` option for compound steps: generates child result summary for parent
- StepResult dataclass with success, summary, child_results
- _summarize_children for TaskTreeContext and generic context summarization

**Explore TUI** (Dec 23 2025)
- Ctrl+P opens command palette with Goto File, View, Analyze commands
- Goto uses Rust index (`find-symbols`) for symbol search - works without git
- Tree file listing uses new `list-files` Rust command (no more git ls-files dependency)
- `g` shortcut still works as hidden alias for quick goto
- Modal keybinds: TUIMode.bindings, active_bindings property, KeybindBar refresh on mode change
- Mode indicator in footer bar (right side, next to palette) - clickable, color-coded

**Rust Crate Consolidation** (Dec 23 2025)
- Merged moss-daemon into moss-cli - `moss daemon run` now runs daemon in foreground
- Single binary: moss-cli now includes all daemon functionality
- ~1000 lines of duplicate code removed

### Bug Fixes

**CLI Fixes** (Dec 23 2025)
- `moss tree crates/` (trailing slash) now works correctly - uses resolve_unified
- Callers/callees commands use incremental call graph refresh (respects mtime)

### Documentation

**Architecture Boundary** (Dec 23 2025)
- Added docs/rust-python-boundary.md: decision framework for Rust vs Python features
- Rust = plumbing (deterministic, performance-critical, syntax-aware)
- Python = interface (LLM, orchestration, TUI, plugins)

### Refactoring

**Code Cleanup** (Dec 23 2025)
- SkeletonAPI.expand now uses rust_view() instead of raw call_rust()
- Python edit assessment: EditAPI (file ops) is used; complexity-routed edit() is stubs

**Workflow Unification** (Dec 23 2025)
- Unified `moss agent` and `moss workflow run` under execution primitives
- `moss agent "task"` is now an alias for `moss workflow run dwim --arg task="..."`
- Removed old workflow system: AgentLoop, MossToolExecutor (2742 lines deleted)
- Removed old workflow files: generator.py, examples.py, validate-fix.toml, vanilla.toml
- Simplified workflows/__init__.py (697 → 9 lines)
- Updated workflow templates to new execution primitives format (agentic, step)
- Updated AgentAPI.run_dwim to use execution primitives instead of old workflows
- Removed obsolete tests: test_agent_loop.py, test_workflows.py, test_dwim_loop.py

### Bug Fixes

**Workflow Path Resolution** (Dec 23 2025)
- Fixed workflow discovery after cli/ package split (was looking in cli/workflows, now moss/workflows)
- Affected: `workflow list`, `workflow run`, `agent`, `MossAPI.run_agent()`

**UX Improvements** (Dec 23 2025)
- `analyze --complexity` now shows helpful message when file target required
- Added "Skipped" section to analyze report for better user feedback
- Removed wasteful theme toggle keybind (T) from TUI

**Test Coverage** (Dec 23 2025)
- Added test for `filter_types()` method in skeleton.rs

**Test Fixes** (Dec 22 2025)
- Fixed `TreeSitterSkeletonProvider` call in `get_symbols_at_line` (uses `from_file`)
- Fixed `ComplexityAnalyzer.analyze` to handle absolute paths (was glob-only)
- Fixed `SkeletonAPI.extract` to raise `FileNotFoundError` for missing files
- Fixed Rust `passthrough` to capture output when stdout is redirected (MCP server)
- Fixed `test_typo_correction_search` expectation ("search" alias resolves to "view")
- Removed 4 obsolete skipped tests for consolidated CLI commands

**check-docs False Positives Fixed** (Dec 22 2025)
- Reduced warnings from 48 to 0 (all were false positives or fixed)
- Added project_roots check: only flag refs whose root matches project modules
- Fixed package discovery: add names without `__init__` suffix (`moss.plugins`)
- Skip config extensions (`.toml`, `.yaml`, `.json`, etc.)
- Skip `self.*` references and incomplete refs ending with dot
- Added entry point group detection from pyproject.toml
- Skip references inside code blocks (triple backticks)
- Added `doc-check: ignore` comment syntax for exceptions

**Synthesis Plugin Refactoring** (Dec 22 2025)
- Aligned module paths with entry point group names
- `moss.synthesis.generators` (was `moss.synthesis.plugins.generators`)
- `moss.synthesis.validators` (was `moss.synthesis.plugins.validators`)
- `moss.synthesis.libraries` (was `moss.synthesis.plugins.libraries`)
- All plugin exports now available from `moss.synthesis`
- Backward compatibility shims in `synthesis/plugins/`

### Features

**TUI Improvements** (Dec 22 2025)
- Fixed CommandPalette CSS selectors (uses CommandInput, not Input)
- Added path autocomplete to command input (tab completion for file paths)
- Fixed transparency toggle (now actually applies CSS class to Screen)
- Better error handling for Rust CLI failures (shows helpful messages)

**Analyze Output Improvements** (Dec 22 2025)
- Added `--limit` (default 10) and `--all` flags for check-docs/check-todos
- Added `--changed` flag to only check git-modified files
- Simplified check-docs output: no boilerplate, grouped stale refs by file
- Stale references now show `file: ref1 @L10, ref2 @L20` format

**External Index Location** (Dec 22 2025)
- New `MOSS_INDEX_DIR` environment variable for custom data/index location
- Absolute path: uses directory directly (`MOSS_INDEX_DIR=/tmp/moss`)
- Relative path: uses `$XDG_DATA_HOME/moss/<path>` (`MOSS_INDEX_DIR=myproject`)
- Falls back to `.moss` in project root if not set
- Useful for repos that don't have `.moss` in `.gitignore`

**Multi-Language Call Graphs** (Dec 22 2025)
- Added call extraction for TypeScript, JavaScript, Java, and Go
- TS/JS: extracts function calls with qualifiers (e.g., `obj.method()`)
- Java: handles method invocations with object qualifiers
- Go: handles both package calls and method calls
- Centralized SOURCE_EXTENSIONS constant for consistent file filtering

### Performance

**Explore TUI Instant Startup** (Dec 22 2025)
- Fixed slow startup on large repos (was scanning ~100k files upfront)
- Directories now lazy-load children on expand (same as symbols)
- Uses `git ls-files` to respect `.gitignore` (no more hardcoded skip dirs)

**Reindexing 20x Faster** (Dec 22 2025)
- Fixed redundant parsing: `find_callees_for_symbol` takes pre-parsed Symbol
- Added parallel file processing with rayon (uses all CPU cores)
- Added prepared statements for batch SQL inserts
- Result: 20+ seconds → ~1 second on large repos (18k files, 66k symbols)

### Features

**Multi-Language Symbol Support** (Dec 22 2025)
- Extended symbol parsing to Java, TypeScript, TSX, JavaScript, Go (in addition to Python/Rust)
- Call graph indexer now includes all supported languages
- Added `moss index-stats` command to show DB size vs codebase size ratio
- Data file key extraction: JSON/YAML/TOML keys become navigable symbols
  - Objects/sections become "class" symbols with children
  - Leaf values become "variable" symbols
  - `moss view pyproject.toml` now shows TOML structure as tree
  - `moss symbols config.json` lists all keys hierarchically
- Tested on 72k file repo: 66k symbols indexed at 3.4% DB size ratio

**Markdown Support** (Dec 22 2025)
- Added tree-sitter-md to Rust CLI for proper markdown parsing
- `moss skeleton README.md` extracts headings as nested symbols
- `moss view README.md/Heading_Name` shows specific section content
- TUI uses unified skeleton API for Python, Rust, and Markdown files
- Removed Python heuristic extraction (was buggy with code blocks)

**Explore TUI** (Dec 22 2025)
- New `moss explore` command with tree + primitives paradigm
- `ExploreMode` is now default TUI mode (replaces READ/WRITE)
- Tree navigation with files + symbols (lazy-loaded)
- Keyboard shortcuts: `v`, `e`, `a` to apply primitives to selected node
- Left/right arrow keys expand/collapse tree nodes
- Command input supports both explicit (`view foo.py`) and implicit (`foo.py`)
- Detail panel shows view output, edit preview, or analyze reports
- Mode set now: EXPLORE (default), PLAN, DIFF, SESSION, BRANCH, SWARM, COMMIT
- Subdirectory navigation with breadcrumbs (`cd`, `-` to go up, Enter to navigate)
- Syntax highlighting for code in detail panel
- Files without symbols hide expand arrow after expansion attempt
- Data files (JSON, YAML, lockfiles) limited to 50 lines in preview
- Preview updates throttled to 100ms when holding arrow keys
- Markdown parent headings show tree structure of children
- Mode indicator moved to subtitle (compact header)
- Action bar shows clickable View/Edit/Analyze buttons

**Unified Resolution & Agent Improvements** (Dec 22 2025)
- `moss analyze` now uses unified path resolution for symbol targeting
  - Example: `moss analyze cli.py/cmd_telemetry --complexity`
- Symbol-level token tracking in telemetry (parses `moss view file/symbol` from bash commands)
- Agent retry loop fallback strategy:
  - Tracks failures by normalized operation key (verb:target)
  - `retry_threshold` config (default: 3 failures before fallback)
  - `FallbackStrategy`: SKIP (continue), REPORT (exit), ALTERNATIVE (suggest)
- Agent prompt now forbids guessing data ("Never guess data" rule)

**Telemetry Enhancements & Analyze Filters** (Dec 22 2025)
- Added `--kind` filter to `moss analyze` command (filter by function/method, avoids `-t` conflict)
- Added file token tracking to session analysis (`file_tokens` field)
- Added `moss telemetry --watch` for real-time telemetry monitoring
- Added `GeminiCliAnalyzer` for Gemini CLI session logs (JSON format)
- Auto-detection now recognizes Gemini CLI sessions

**CLI Consolidation & UX** (Dec 22 2025)
- Folded standalone commands into `analyze` primitive:
  - `--summary`: generate file/directory summary (was `moss summarize`)
  - `--check-docs`: check documentation freshness (was `moss check-docs`)
  - `--check-todos`: check TODO.md accuracy (was `moss check-todos`)
  - `--health`: already available in Rust CLI
- PTY auto-detection: non-TTY defaults to compact mode (machine-readable)
- Added "Never Extract Data Manually" principle to `docs/philosophy.md`
- Updated agent prompt to forbid guessing data (use `view` to discover)
- Added Command Philosophy section to `docs/cli/commands.md`

### Removed

**CLI Cleanup** (Dec 22 2025)
- Removed `moss loop` CLI command and all predefined loops (simple, critic, incremental, etc.)
- Removed `moss dwim` CLI command (module kept for alias resolution)
- Removed `moss health`, `moss summarize`, `moss check-docs`, `moss check-todos` (use `analyze` flags)
- Use DWIMLoop or TOML workflows instead

### Features

**CLI Improvements & Unified Plumbing** (Dec 22 2025)
- Added `--compact` mode to `moss patterns` command for token-efficient output
- Added large file detection to `analyze --health` (shows top 10 files >500 lines)
- Audited unified plumbing: path resolution already shared via `path_resolve::resolve_unified`
- Evaluated `patterns` and `git-hotspots` commands: NOT slow (6s, 2.5s), keeping both

**CLI Consolidation & Model Update** (Dec 22 2025)
- Updated default model to `gemini-3-flash-preview` across all components
- Expanded Rust CLI passthrough: cfg, complexity, context, deps, grep, health, overview
- Fixed CORE_PRIMITIVES: 4 → 3 (search folded into view via aliases)
- Updated vanilla prompt to use 3 primitives (view, edit, analyze)
- Redesigned TODO.md: merged dogfooding + CLI cleanup into single focus area
- Added --verbose flag to workflow runs for debugging LLM outputs

**Telemetry Command & Design Philosophy Update** (Dec 22 2025)
- New `moss telemetry` command for session analysis:
  - Default: aggregate stats across all moss sessions
  - `--session ID`: specific moss session stats
  - `--logs *.jsonl`: analyze Claude Code session logs (supports multiple)
  - `--html`: HTML dashboard output
- Log format plugin system:
  - `LogParser` protocol for pluggable parsers
  - `detect_log_format()` auto-detects format from file content
  - `analyze_log()` unified entry point with auto-detection
  - `ClaudeCodeAnalyzer` for Claude Code JSONL
  - `MossSessionAnalyzer` for internal moss sessions
- New design tenet: "Generalize, Don't Multiply"
  - Prefer one flexible solution over N specialized ones
  - Composability reduces cognitive load, maintenance burden, token cost
- Updated philosophy.md: 4 primitives → 3 (find folded into view)
- DWIM tool selection no longer needed with only 3 primitives
- Session modes: Fresh (default) vs Marathon (overnight runs)
- New design doc: `docs/telemetry.md`

**Core Primitive Tool Resolution** (Dec 22 2025)
- Added `analyze` to CLI passthrough commands (routes to Rust)
- New `resolve_core_primitive()` function for simple tool resolution
- CORE_PRIMITIVES: `view`, `edit`, `analyze` with aliases
- Basic typo correction using Levenshtein distance (threshold 0.7)
- NL-dependent tests marked xfail since embeddings were removed
- Phase 3 (Simplify tool interface) now complete

**Analyze Command & Core API Consolidation** (Dec 21 2025)
- New `moss analyze [path]` command in Rust CLI:
  - `--health`: Codebase health metrics (files, lines, complexity scores)
  - `--complexity`: Cyclomatic complexity analysis per function
  - `--security`: Security vulnerability scanning (shells out to bandit)
  - Runs all analyses by default if no flags specified
  - JSON output with `--json`
- Consolidated MossAPI from 30 sub-APIs to 3 core primitives:
  - `api.view` - ViewAPI wrapping Rust `view` command (includes find/search)
  - `api.structural_edit` - EditAPI wrapping Rust `edit` command
  - `api.analyze` - AnalyzeAPI wrapping Rust `analyze` command
- New `rust_shim` functions: `rust_view()`, `rust_edit()`, `rust_analyze()`
- New `core_api.py` module with ViewResult, EditResult, AnalyzeResult dataclasses

### Fixes

**Path Resolution & Test Fixes** (Dec 21 2025)
- Fixed unicode and absolute path resolution in Rust CLI (`moss view /tmp/日本語/test.py` now works)
- Updated `test_cli.py` to use subprocess for Rust passthrough commands (skeleton, anchors)
- Fixed `test_synthesis.py` import: TFIDFIndex moved to moss.semantic_search
- Updated synthesis router to use new TFIDFIndex interface

### Features

**View Filters & Edit Command** (Dec 21 2025)
- Extended `view` command with filtering: `--type class`, `--type function`, `--type method`
- Caller/callee integration: `view resolve_tool --calls`, `view resolve_tool --called-by`
- New `edit` command for structural code modification
- Smart whitespace handling: detects and preserves local blank line conventions (PEP8 2-blank, single-blank, etc.)
- Full `edit` operations:
  - `--delete`: Remove a symbol entirely
  - `--replace "code"`: Replace symbol with new code
  - `--before "code"` / `--after "code"`: Insert sibling before/after
  - `--prepend "code"` / `--append "code"`: Insert at start/end of container (class/impl) or file
  - `--move-before X` / `--move-after X`: Relocate symbol as sibling
  - `--move-prepend C` / `--move-append C`: Move symbol into container C
  - `--copy-before X` / `--copy-after X`: Duplicate symbol as sibling
  - `--copy-prepend C` / `--copy-append C`: Duplicate symbol into container C
  - `--swap X`: Exchange two symbols
  - `--dry-run`: Preview changes without applying
- Find functionality unified into view (no separate find command needed)
- See `docs/primitives-spec.md` for full specification

**Unified Tree Model & Simplified DWIM** (Dec 21 2025)
- `Unified path addressing`: `src/main.py/Foo/bar` resolves file + symbol
- Multiple separator support: `/`, `::`, `#`, `:` all normalize to canonical `/`
- Enhanced `view` command with `--depth` control:
  - Directories: show tree with depth limit
  - Files: skeleton at depth 1-2, full content at depth 3+
  - Symbols: show source code for specific symbol
- `--deps` flag shows imports in file view
- Removed fastembed/bge-small-en embedding dependency from DWIM
- DWIM now uses simpler TF-IDF + fuzzy string matching (no ML model)
- Added 16 failure mode tests for error handling robustness

**Rust Delegation & CLI Cleanup** (Dec 21 2025)
- Removed ~900 lines of dead Python code from cli.py and rust_shim.py
- Added `passthrough()` to bypass Python argparse for Rust-delegated commands
- Passthrough commands: tree, view, search-tree, expand, callers, callees, anchors, skeleton, path
- No more double-parsing: CLI args go directly to Rust CLI
- Moved heuristic-based symbol args to Rust (normalize_symbol_args)
- Supports flexible formats: `symbol`, `file:symbol`, `file::symbol`, `file#symbol`, `file symbol`, `symbol file`
- Phase 1 Rust delegation complete (all commands delegate to Rust)

**Session Dec 21 2025 (earlier)**
- `Symbol Hover in TUI`: Tree view shows symbol children with hover displaying signatures/docstrings
- `Context Elision Heuristics`: Anchor-preserving elision when token budget exceeded
- `Experiment Branching`: Multiple concurrent shadow branches for parallel approach testing
- `GBNF Grammar Support`: Constrained inference for llama.cpp with predefined and custom grammars
- `Claude vs Gemini CLI Analysis`: Documented edit paradigm differences (strict matching vs self-correction)
- `Lazy Imports`: Converted moss.__init__.py to lazy imports for reduced baseline memory
- `Extensible TUI Modes`: ModeRegistry with entry point + .moss/modes/ plugin discovery
- `Brute Force Mode`: BruteForceConfig for n_samples + voting with small/local models
- `Runtime Memory Bounds`: streaming LLM responses, context eviction (max_context_steps)
- `Brute Force Voting`: wired to LLMGenerator with majority/consensus/first_valid strategies
- `Session Log Comparison Tool`: compare_sessions() for Claude vs Gemini CLI edit analysis

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


### Single-Tool MCP Server

**Token Efficiency**
- New single-tool MCP server: `moss(command: str)` - 99% token reduction (~8K → ~50 tokens)
- Original multi-tool server preserved as `moss-mcp-full` for IDEs
- CLI: `moss mcp-server` (single-tool, default) or `moss mcp-server --full` (multi-tool)
- Entry points: `moss-mcp` (single) and `moss-mcp-full` (full)


### query/search CLI Migration, Agent Learning

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


### find_related_files, summarize_module, CLI Migration

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

### Module DWIM, CLI Migration, explain_symbol

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


### Search, Async Docs, Self-Improvement, Guessability

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


### WebAPI, Skeleton Expand, Loops Infrastructure

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


### Preference Extraction from Agent Logs

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


### Codebase Analysis Tools

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


### Library-First Architecture
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
- `moss.gen.http` - Generate FastAPI routes and OpenAPI spec from API
- `moss.gen.mcp` - Generate MCP tool definitions from API
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


### Comprehensive Health Analysis
Expanded `moss health` into a comprehensive project analysis tool:
- **Dependency Analysis**: Circular dependency detection, god modules (high fan-in), orphan modules, coupling metrics
- **Structural Hotspots**: Functions with too many parameters, classes with too many methods, deep nesting, long functions, complex conditionals
- **Test Coverage Analysis**: Module-to-test mapping, test-to-code ratio, untested public API surface
- **API Surface Analysis**: Public exports inventory, undocumented APIs, naming convention checking, breaking change risk
- **Health Command Refactor**: Concise single-screen output, `--severity` and `--focus` flags, `moss report` for verbose output, `--ci` flag with exit codes (0=healthy, 1=warnings, 2=critical)


### Advanced Library Learning
Frequency-based abstraction learning inspired by DreamCoder:
- `LearnedLibrary` plugin with pattern-based learning
- `PatternExtractor` for code pattern detection (functions, expressions, idioms)
- Persistent storage with JSON serialization
- Compression gain estimation for abstraction scoring
- Pattern frequency tracking across synthesis runs
- 31 tests for library learning

### LLM Integration
LLM-based code generation with mock support for testing:
- `LLMGenerator` plugin using LiteLLM for unified provider access
- `MockLLMProvider` for testing without API calls
- `LiteLLMProvider` supporting Anthropic, OpenAI, and other backends
- Streaming generation support
- Cost estimation and budgeting with per-model pricing
- Factory functions: `create_llm_generator()`, `create_mock_generator()`
- 48 tests for LLM generation

### CLI & Edit Integration
- `moss edit` command with intelligent complexity routing
- TaskComplexity analysis (simple/medium/complex/novel)
- Structural edit handler (rename, typo fix, refactoring)
- Synthesis fallback for complex/novel tasks
- Configuration presets: default, research, production, minimal

### Optimization & Learning
- StrategyLearner with feature extraction
- Feature-based strategy scoring (EMA updates)
- Similar problem lookup from history
- Router integration: 4-signal ranking (TF-IDF, estimate, history, learned)

### Strategy Auto-Discovery
- StrategyPlugin protocol for pluggable strategies
- StrategyRegistry with enable/disable support
- Entry point discovery (moss.synthesis.strategies)

### Configuration System
- SynthesisConfigWrapper for TOML-based config
- SynthesisConfigLoader fluent builder
- Subsystem configs: generators, validators, strategies, learning
- load_synthesis_config() for moss.toml

### Synthesis Plugin Architecture
Plugin system for synthesis components (inspired by Synquid, miniKanren, DreamCoder, λ²):
- `CodeGenerator` protocol with PlaceholderGenerator, TemplateGenerator
- `SynthesisValidator` protocol with TestValidator (pytest/jest), TypeValidator (mypy/pyright)
- `LibraryPlugin` protocol with MemoryLibrary (DreamCoder-style abstractions)
- `SynthesisRegistry` with sub-registries and entry point discovery
- Validation retry loop in `_validate_with_retry()`
- Framework integration: `_solve_atomic()` uses generator plugins
- User-configurable templates (CRUD, validation, transform patterns)
- 31 tests for plugin architecture


### Refactoring Tools
- Inline refactoring (function and variable inlining)
- Codemod DSL with pattern matching ($var placeholders)
- CodemodRunner for workspace-wide transformations
- Built-in codemod factories (deprecation, API migration)
- Preview/dry-run mode for all refactorings

### Context & Memory
- ContentHash merkle hashing for documents
- DocumentSummary with recursive child aggregation
- DocumentSummaryStore with caching and persistence
- ChatMessage and ChatSession management
- ChatlogStore with context window optimization
- SimpleSummarizer (extractive summarization)
- Session search with tag filtering

### Synthesis Framework (Scaffolding)
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


### Developer Experience & CI/CD
- Watch mode for tests (auto-run on file changes)
- Metrics dashboard (HTML report of codebase health)
- Custom analysis rules (user-defined patterns)
- Pre-commit hook integration
- Diff analysis (analyze changes between commits)
- PR review helper (summarize changes, detect issues)
- SARIF output (for CI/CD integration)
- GitHub Actions integration
- VS Code extension (`editors/vscode/`)

### Integration & Polish
- CLI improvements: global flags, consistent output module
- Interactive shell (`moss shell`)
- Performance: caching layer, parallel file analysis
- Configuration: `moss.toml`, per-directory overrides

### Advanced Features
- Configurable output verbosity
- Multi-file refactoring
- Progress indicators
- Live CFG rendering
- LSP integration
- Visual CFG output
- Auto-fix system
- Embedding-based search
- Non-code content plugins

### Plugin Architecture
- ViewPlugin protocol and PluginRegistry
- Entry points discovery for pip-installed plugins
- Tree-sitter skeleton plugin (multi-language)

### Introspection Improvements
- Enhanced skeleton views
- Dependency graph improvements

### DWIM Semantic Routing
- TF-IDF based command routing
- Fuzzy intent matching

### LLM Introspection Tooling
- Agent orchestration primitives
- Shadow git integration
- Validation loops


### Developer Experience
- CLI interface (`moss init`, `moss run`, `moss status`)
- README with architecture overview
- Usage examples and tutorials in `examples/`
- API documentation via docstrings

### Enhanced Capabilities
- Vector store integration (Chroma, in-memory)
- Tree-sitter integration for multi-language AST (Python, TypeScript, JavaScript, Go, Rust)
- Control Flow Graph (CFG) view provider
- Elided Literals view provider for token reduction

### Hardening & Quality
- Integration tests for component interactions
- E2E tests for full workflows
- Fuzzing tests for edge cases and malformed inputs
- CI/CD with GitHub Actions (lint, test, coverage, typecheck)

### Production Readiness
- FastAPI example server (`examples/server/`)
- Structured logging module (`moss.logging`)
- Observability module with metrics and tracing (`moss.observability`)
- Profiling utilities (`moss.profiling`)

### Dogfooding
- Self-analysis test suite (Moss analyzing its own codebase)
- Performance benchmarks on real code
- 621 tests passing with 86% coverage
