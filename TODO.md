# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- [x] Rule sharing/import: `moss rules add/update/list/remove` (Phase 1 complete)
- [x] Auto-fix support: `moss analyze rules --fix` with fix templates
- [x] Expand #[cfg(test)] detection for Rust rules (rust.is_test_file)

## Remaining Work
- Namespace-qualified lookups: `moss view std::vector`, `moss view com.example.Foo`
  - Requires language-specific namespace semantics - low priority
- Shadow worktree: true shadow-first mode (edit in shadow, then apply)
  - Current: --shadow flag works, but not default for all edits
  - Zero user interruption (user can edit while agent tests in background)

### Configuration System
Sections: `[daemon]`, `[index]`, `[aliases]`, `[view]`, `[analyze]`, `[text-search]`, `[pretty]`, `[serve]`

Adding a new section (3 places):
1. Define `XxxConfig` struct with `#[derive(Merge)]` + `XxxArgs` with `#[derive(Args)]` in command module
2. Add field to MossConfig
3. Add `run(args, json)` function that loads config and merges

Candidates: `[workflow]` (directory, auto-run)

### Trait-Based Extensibility
All trait-based crates follow the moss-languages pattern for extensibility:
- Global registry with `register()` function for user implementations
- Built-ins initialized lazily via `init_builtin()` + `OnceLock`
- No feature gates (implementations are small, not worth the complexity)

Crates with registries:
- [x] moss-languages: `Language` trait, `register()` in registry.rs
- [x] moss-cli-parser: `CliFormat` trait, `register()` in formats/mod.rs
- [x] moss-sessions: `LogFormat` trait, `register()` in formats/mod.rs
- [x] moss-tools: `Tool` trait (`register_tool()`), `TestRunner` trait (`register()`)
- [x] moss-packages: `Ecosystem` trait, `register_ecosystem()` in ecosystems/mod.rs
- [x] moss-jsonschema: `JsonSchemaGenerator` trait, `register()` in lib.rs
- [x] moss-openapi: `OpenApiClientGenerator` trait, `register()` in lib.rs

Pattern: traits are the extensibility mechanism. Users implement traits in their own code, register at runtime. moss CLI can add Lua bindings at application layer for scripting.

### CLI API Consistency
Audit found fragmentation across commands. Fix for consistent UX:

**High priority:** (DONE)
- [x] `--exclude`/`--only` parsing: unified to comma-delimited across all commands
- [x] Output flags in `analyze`: removed local flags, uses root-level `--json`/`--jq`/`--pretty`/`--compact`
- [x] Short flag `-n` collision: changed to `-l` for `--limit` (consistent with sessions)
- [x] `--root` vs `--project`: sessions now uses `--root` like other commands
- [x] `--jq` semantics: documented - root filters whole JSON, sessions filters per-line (JSONL) - intentional

**Medium priority:**
- [x] Subcommand defaults: reviewed - intentional design (commands with clear primary action default to it, e.g., lint→run, test→run, analyze→health; commands with no clear primary require explicit, e.g., package, index)
- [x] `--allow` semantics: reviewed - intentional (different analysis types need different allowlist formats: patterns for files/hotspots, locations for duplicate-functions, pairs for duplicate-types; help text documents each)
- [x] `--type` vs `--kind`: standardized to `--kind` (view now uses `--kind` like analyze complexity)

### CLI Cleanup
- [x] Move `moss plans` to `moss sessions plans`: groups tool-specific data under sessions
- [x] Rename `moss filter aliases` to `moss aliases`: removes unnecessary namespace layer
- [x] Unify `lint`/`test` under `moss tools`: `moss tools lint [run|list]`, `moss tools test [run|list]`
- [x] Remove `analyze lint`: duplicate of `moss lint`, adds no value

### Rust Redesign Candidates
- Rules engine: consider semgrep/ruff integration instead of custom
- Plugin system: Rust trait-based plugins or external tool orchestration
- Edit routing: workflow engine with LLM decision points
- Session/checkpoint: workflow state persistence
- PR/diff analysis: `moss analyze --pr` or similar

## Backlog

### Workflow Engine
- [x] Streaming output for `auto{}` driver
- JSON Schema for complex action parameters (currently string-only)
- Workflow chaining: automatically trigger next workflow based on outcome (e.g., Investigation → Fix → Review)

### Workflow Documentation (see `docs/workflows/`)
Document edge-case workflows - unusual scenarios that don't fit standard patterns:

**Investigation:**
- [x] Reverse engineering code - undocumented/legacy code with no context
- [x] Reverse engineering binary formats - file formats, protocols without docs
- [x] Debugging production issues - logs/traces without local reproduction
- [x] Performance regression hunting - finding what made things slow
- [x] Flaky test debugging - non-deterministic failures, timing issues

**Modification:**
- [x] Merge conflict resolution - understanding both sides, correct resolution
- [x] Cross-language migration - porting code (Python→Rust, JS→TS)
- [x] Breaking API changes - upstream dependency changes that break your code
- [x] Dead code elimination - safely removing unused code paths

**Synthesis:**
- [x] High-quality code synthesis - D×C verification for low-data domains
- [x] Binding generation - FFI/bindings for libraries
- [x] Grammar/parser generation - parsers from examples + informal specs

**Meta:**
- [x] Onboarding to unfamiliar codebase - systematic exploration
- [x] Documentation synthesis - generating docs from code
- [x] Cross-workflow analysis - extract shared insights, patterns, principles after all workflows documented

**Security/Forensic:**
- [x] Cryptanalysis - analyzing crypto implementations
- [x] Steganography detection - finding hidden data
- [x] Malware analysis - understanding malicious code (read-only)

**Example codebases for workflow testing:**
- viwo: DSL/framework/scripting language with insufficient testing, numerous bugs
  - Good for: debugging legacy code, reverse engineering code workflows
  - Details available on request when tackling this

**Research (completed):**
- [x] https://github.com/ChrisWiles/claude-code-showcase - Claude Code configuration patterns
  - Skills: markdown docs with frontmatter, auto-triggered by scoring (keywords 2pts, regex 3, paths 4, directory 5, intent 4)
  - Agents: specialized assistants with severity levels (Critical/Warning/Suggestion)
  - Hooks: PreToolUse, PostToolUse, UserPromptSubmit, Stop lifecycle events
  - GitHub Actions: scheduled maintenance (weekly quality, monthly docs sync, dependency audit)
  - **Actionable for moss:**
    - Script/workflow selection scoring (match prompts to relevant `.moss/scripts/`)
    - Formalize auditor severity levels in output format
    - Expand hook triggering beyond current implementation
    - CI integration patterns for automated quality checks

### Package Management
- `moss package install/uninstall`: proxy to ecosystem tools (cargo add, npm install, etc.)
  - Very low priority - needs concrete use case showing value beyond direct tool usage
  - Possible value-adds: install across all ecosystems, auto-audit after install, config-driven installs

### Package Index Fetchers (moss-packages)

**Full coverage tracking**: See `docs/repository-coverage.md` for complete repository list.

**API Verification Results**:

✅ WORKING:
- apk: Alpine - APKINDEX.tar.gz parsing (multi-member gzip + tar)
- artix: packages.artixlinux.org/packages/search/json/?name={name} (Arch-compatible format)
- conan: conan.io/api/search JSON API
- dnf: mdapi.fedoraproject.org/rawhide/pkg/{name} (JSON)
- freebsd: pkg.freebsd.org packagesite.pkg (zstd tar + JSON-lines)
- gentoo: packages.gentoo.org/packages/{cat}/{name}.json (JSON)
- guix: guix.gnu.org/packages.json (gzip-compressed JSON array, ~30k packages)
- nix: search.nixos.org Elasticsearch (requires POST with query JSON)
- opensuse: download.opensuse.org repodata/primary.xml.zst (zstd XML)
- pacman/aur: aur.archlinux.org/packages-meta-ext-v1.json.gz (full archive)
- void: repo-default.voidlinux.org x86_64-repodata (zstd tar + XML plist)

⚠️ XML ONLY (needs XML parsing):
- choco: community.chocolatey.org/api/v2 returns NuGet v2 OData/Atom XML

❌ NO PUBLIC API (removed from fetchers):
- openbsd: openports.pl - HTML only - removed
- netbsd: pkgsrc.se - HTML only - removed
- swiftpm: Swift Package Index requires authentication for API access
- stackage: No JSON API (endpoints redirect, snapshot URLs 404)
- ghcr: GitHub Container Registry requires authentication (401)
- gradle: Plugin portal API returning 404 (plugins.gradle.org/api/plugins)

**Implemented fetchers** (57 total: 17 distro, 4 Windows, 3 macOS, 2 cross-platform, 1 container, 2 mobile, 28 language):
- [x] APK (Alpine): APKINDEX.tar.gz with checksums, deps, archive URLs
- [x] Artix Linux: Arch-based, shares arch_common logic with pacman
- [x] NixOS/Nix: search.nixos.org Elasticsearch API
- [x] Void Linux: zstd tar + XML plist parsing
- [x] Gentoo: packages.gentoo.org API
- [x] Guix: packages.guix.gnu.org with fetch_all support
- [x] Slackware: SlackBuilds.org via GitHub raw .info files
- [x] FreeBSD: zstd tar + JSON-lines parsing (packagesite.pkg)
- [x] openSUSE: zstd XML parsing (repodata/primary.xml.zst)
- [x] CachyOS: Arch-based, uses arch_common
- [x] EndeavourOS: Arch-based, uses arch_common
- [x] Manjaro: repo.manjaro.org database parsing + AUR
- [x] Copr: Fedora community builds (copr.fedorainfracloud.org API)
- [x] Chaotic-AUR: chaotic-backend.garudalinux.org JSON API
- [x] MSYS2: packages.msys2.org API (Windows development)
- [x] MacPorts: ports.macports.org API
- [x] Snap: api.snapcraft.io (requires Snap-Device-Series header)
- [x] DUB: code.dlang.org API (D packages)
- [x] Clojars: clojars.org API (Clojure packages)
- [x] CTAN: ctan.org JSON API (TeX/LaTeX packages)
- [x] Racket: pkgs.racket-lang.org (Racket packages)
- [x] Bioconductor: bioconductor.r-universe.dev API (R bioinformatics)
- [x] Hunter: GitHub cmake parsing (C++ packages)
- [x] Docker: hub.docker.com API (container images)
- [x] F-Droid: f-droid.org API (Android FOSS apps)
- [x] vcpkg: GitHub baseline.json + port manifests (C++ packages)
- [x] Termux: GitHub build.sh parsing (Android terminal packages)
- [x] Conan: conan.io/api/search JSON API

**Note**: Debian-derivatives (Ubuntu, Mint, elementary) use apt fetcher.
Arch-derivatives (Manjaro, etc.) can use pacman fetcher.

**fetch_all implementations**:
- [x] APK: APKINDEX.tar.gz (main + community repos)
- [x] AUR: packages-meta-ext-v1.json.gz (~30MB, ~5min refresh)
- [x] Homebrew: formula.json
- [x] Deno: paginated API
- [x] Guix: packages.json
- Arch official: has package databases per repo (not yet implemented)
- Crates.io: has db-dump.tar.gz (not real-time, could implement)
- npm: has registry replicate API (massive - may not be practical)
- PyPI: has simple index but no bulk JSON API
- RubyGems: has versions dump at /versions endpoint
- NuGet: has catalog API for incremental updates

**Struct completeness audit**: Each fetcher should populate all available fields from their APIs:
- keywords, maintainers, published dates where available
- downloads counts from APIs that provide them
- archive_url and checksum for verification
- extra field for ecosystem-specific metadata not in normalized fields

**Performance improvements needed**:
- [ ] Streaming/iterator API: Currently fetchers load all packages into memory before filtering. For cross-referencing 50+ ecosystems, this is ~1GB+ in memory. Need lazy/streaming approach where we iterate packages without loading all into Vec first.
- [x] Parallel repo fetching: openSUSE fetches repos in parallel with rayon (~4x speedup)

**Multi-repo coverage done**:
- [x] openSUSE: 36 repos (Tumbleweed, Leap 16.0, Leap 15.6 × OSS/Non-OSS/Updates, Factory, source RPMs, debug symbols, community repos: Games, KDE, GNOME, Xfce, Mozilla, Science, Wine, Server)
- [x] Arch Linux: 12 repos (core, extra, multilib, testing, staging, gnome/kde-unstable, AUR)
- [x] Artix Linux: 15 repos (system, world, galaxy, lib32, asteroids × stable/gremlins/goblins)
- [x] Alpine/APK: 11 repos (edge, v3.21, v3.20, v3.19, v3.18 × main/community/testing)
- [x] FreeBSD: 5 repos (FreeBSD 13/14/15 × quarterly/latest)
- [x] Void Linux: 8 repos (x86_64/aarch64 × glibc/musl × free/nonfree)

**Multi-repo coverage done**:
- [x] Manjaro: 10 repos (stable/testing/unstable × core/extra/multilib + AUR)
- [x] Debian/APT: 21 repos (stable/testing/unstable/experimental/oldstable × main/contrib/non-free + backports)
- [x] Fedora/DNF: 6 repos (Fedora 39/40/41, Rawhide, EPEL 8/9)
- [x] Ubuntu: 22 repos (Noble 24.04/Jammy 22.04/Oracular 24.10 × main/restricted/universe/multiverse + updates/security/backports)
- [x] Nix: 5 channels (nixos-stable, nixos-unstable, nixpkgs-unstable, nixos-24.05, nixos-24.11)
- [x] CachyOS: 8 repos (cachyos, cachyos-v3/v4, core-v3/v4, extra-v3/v4, testing)
- [x] EndeavourOS: 5 repos (endeavouros, core, extra, multilib, testing)
- [x] Gentoo: 5 repos (gentoo, guru, science, haskell, games overlays)
- [x] Guix: 2 channels (guix, nonguix)
- [x] Slackware: 3 versions (current, 15.0, 14.2)
- [x] Scoop: 8 buckets (main, extras, versions, games, nerd-fonts, java, php, nonportable)
- [x] Chocolatey: community repository
- [x] WinGet: 2 sources (winget, msstore)
- [x] Flatpak: 2 remotes (flathub, flathub-beta)
- [x] Snap: 4 channels (stable, candidate, beta, edge)
- [x] Conda: 4 channels (conda-forge, defaults, bioconda, pytorch)
- [x] Maven: 3 repos (central, google, sonatype)
- [x] Docker: 4 registries (docker-hub, ghcr, quay, gcr)

**Multi-repo coverage remaining**:

All major package managers now have multi-repo support. Remaining unit-struct fetchers are single-source registries where multi-repo doesn't apply (npm, PyPI, crates.io, etc.).

### Complexity Hotspots (58 functions >21)
- [ ] `crates/moss/src/commands/edit.rs:handle_glob_edit` (76)
- [ ] `crates/moss/src/commands/view/file.rs:cmd_view_file` (69)
- [ ] `crates/moss/src/commands/edit.rs:cmd_edit` (67)
- [ ] `crates/moss/src/commands/daemon.rs:cmd_daemon` (66)
- [ ] `crates/moss/src/commands/view/symbol.rs:cmd_view_symbol` (65)
- [ ] `crates/moss-rules/src/runner.rs:evaluate_predicates` (53)
- [ ] `crates/moss/src/commands/tools/lint.rs:cmd_lint_run` (49)
- [ ] `crates/moss/src/commands/analyze/mod.rs:run` (49)
- [ ] `crates/moss/src/tree.rs:collect_highlight_spans` (48)
- [ ] `crates/moss-rules/src/runner.rs:run_rules` (44)
- [ ] `crates/moss/src/commands/analyze/report.rs:analyze` (44)
- [ ] `crates/moss/src/commands/tools/lint.rs:run_lint_once` (42)
- [ ] `crates/moss/src/commands/analyze/trace.rs:cmd_trace_async` (40)
- [ ] `crates/moss/src/commands/grammars.rs:cmd_install` (39)
- [ ] `crates/moss/src/commands/update.rs:cmd_update` (39)
- [ ] `crates/moss/src/commands/analyze/duplicates.rs:cmd_duplicate_functions_with_count` (38)
- [ ] `crates/moss/src/commands/view/mod.rs:cmd_view` (38)
- [ ] `crates/moss/src/commands/analyze/duplicates.rs:cmd_duplicate_types` (37)
- [ ] `crates/moss/src/commands/analyze/mod.rs:run_all_passes` (36)
- [ ] `crates/moss/src/commands/edit.rs:cmd_undo_redo` (35)
- [ ] `crates/moss/src/commands/edit.rs:cmd_batch_edit` (33)
- [ ] `crates/moss/src/path_resolve.rs:resolve_from_paths` (33)
- [ ] `crates/moss/src/commands/sessions/plans.rs:format_time` (32)
- [ ] `crates/moss/src/commands/analyze/call_graph.rs:cmd_call_graph_async` (30)
- [ ] `crates/moss/src/commands/analyze/check_examples.rs:cmd_check_examples` (29)
- [ ] `crates/moss/src/commands/analyze/check_refs.rs:cmd_check_refs_async` (29)
- [ ] `crates/moss/src/path_resolve.rs:resolve_unified` (29)
- [ ] `crates/moss/src/commands/analyze/duplicates.rs:cmd_allow_duplicate_function` (28)
- [ ] `crates/moss/src/commands/view/symbol.rs:cmd_view_symbol_at_line` (28)
- [ ] `crates/moss/src/commands/analyze/query.rs:cmd_query` (27)
- [ ] `crates/moss/src/commands/analyze/hotspots.rs:cmd_hotspots` (26)
- [ ] `crates/moss/src/commands/rules.rs:cmd_update` (25)
- [ ] `crates/moss/src/commands/view/tree.rs:cmd_view_filtered` (25)
- [ ] `crates/moss/src/commands/analyze/trace.rs:detect_branch_context` (25)
- [ ] `crates/moss-packages/src/ecosystems/deno.rs:strip_jsonc_comments` (24)
- [ ] `crates/moss-packages/src/ecosystems/python.rs:parse_requirement` (24)
- [ ] `crates/moss/src/commands/analyze/stale_docs.rs:cmd_stale_docs` (24)
- [ ] `crates/moss/src/commands/sessions/analyze.rs:cmd_sessions_jq` (24)
- [ ] `crates/moss/src/commands/analyze/duplicates.rs:detect_duplicate_function_groups` (24)
- [ ] `crates/moss-languages/src/c_cpp.rs:find_cpp_include_paths` (23)
- [ ] `crates/moss/src/tree.rs:capture_name_to_highlight_kind` (23)
- [ ] `crates/moss/src/tree.rs:render_highlighted` (23)
- [ ] `crates/moss/src/commands/view/lines.rs:collect_doc_comment_lines` (22)
- [ ] `crates/moss/src/commands/sessions/plans.rs:cmd_plans` (22)
- [ ] `crates/moss/src/commands/generate.rs:run` (22)
- [ ] `crates/moss-languages/src/registry.rs:validate_unused_kinds_audit` (22)
- [ ] `crates/moss/src/commands/analyze/duplicates.rs:find_node_at_line_recursive` (22)
- [ ] `crates/moss/src/commands/analyze/mod.rs:print_complexity_report_pretty` (22)
- [ ] `crates/moss/src/commands/index.rs:cmd_rebuild` (22)
- [ ] `crates/moss/src/health.rs:analyze_health_indexed` (22)
- [ ] `crates/moss-languages/src/ecmascript.rs:extract_container` (21)
- [ ] `crates/moss/src/commands/analyze/rules_cmd.rs:cmd_rules` (21)
- [ ] `crates/moss/src/commands/context.rs:run` (21)
- [ ] `crates/moss/src/text_search.rs:add_symbol_context` (21)
- [ ] `crates/moss-sessions/src/formats/claude_code.rs:analyze_file_tokens` (21)
- [ ] `crates/moss/src/commands/package.rs:cmd_outdated` (21)
- [ ] `crates/moss/src/tree.rs:docstring_style_for_grammar` (21)
- [ ] `crates/moss/src/main.rs:main` (21)

### Code Quality
- [x] `--allow` for duplicate-functions: accept line range like output suggests (e.g., `--allow src/foo.rs:10-20`)
- Unnecessary aliases: `let x = Foo; x.bar()` → `Foo.bar()`. Lint for pointless intermediate bindings.
- [x] Chained if-let: edition 2024 allows `if let Ok(x) = foo() && let Some(y) = bar(x)`. Audit complete.
- PR/diff analysis: `moss analyze --pr` or `--diff` for changed code focus (needs broader analysis workflow design)
- [x] Validate node kinds against grammars: `validate_unused_kinds_audit()` in 99 language files, runs as test
- [x] Directory context: `moss context`, `view --dir-context`
- Deduplicate SQL queries in moss: many ad-hoc queries could use shared prepared statements or query builders (needs design: queries use different execution contexts - Connection vs Transaction)
- Detect reinvented wheels: hand-rolled JSON/escaping when serde exists, manual string building for structured formats, reimplemented stdlib. Heuristics unclear. Full codebase scan impractical. Maybe: (1) trigger on new code matching suspicious patterns, (2) index function signatures and flag known anti-patterns, (3) check unused crate features vs hand-rolled equivalents. Research problem.
- Syntax-based linting: see `docs/design/syntax-linting.md`
  - [x] Phase 1: `moss analyze ast`, `moss analyze query` (authoring tools)
  - [x] Phase 1b: `moss analyze rules` reads .moss/rules/*.scm with TOML frontmatter
  - [x] Phase 3a: builtin rules infrastructure (embedded + override + disable)
  - [x] Phase 2: severity config override, SARIF output
  - Phase 3b: more builtin rules, sharing, auto-fix (see `docs/design/builtin-rules.md`)
    - [x] Extended language coverage: Python (print-debug, breakpoint), Go (fmt-print), Ruby (binding-pry)
    - [x] Rule sharing/import mechanism (`moss rules add/update/list/remove`)
    - [x] Auto-fix support (`moss analyze rules --fix`)
  - [x] Project manifest parsing: extract version/config from project manifests
    - RustSource: Cargo.toml (edition, resolver, name, version)
    - TypeScriptSource: tsconfig.json + package.json (target, module, strict, node_version)
    - PythonSource: pyproject.toml (requires_python, name, version)
    - GoSource: go.mod (version, module)
    - Each source finds nearest manifest for the file being analyzed
  - [x] Rule conditionals: `requires` predicates beyond path-based `allow`
    - Pluggable RuleSource trait for data sources
    - Built-in sources: env, path, git, rust, typescript, python, go
    - Operators: exact match, >=, <=, !
    - Example: `requires = { "rust.edition" = ">=2024" }` for chained if-let
    - Example: `requires = { "env.CI" = "true" }` for stricter CI-only rules
  - Semantic rules system: for rules needing cross-file analysis (import cycles, unused exports, type mismatches). Current syntax-based rules are single-file AST queries; semantic rules need index-backed analysis. Separate infrastructure, triggered differently (post-index vs per-file).
  - [x] Phase 4: combined query optimization (single-traversal multi-rule matching)
    - Achieved via tree-sitter combined queries (simpler than full tree automata)
    - Performance: 4.3s → 0.75s (5.7x faster) for 13 rules, ~550 findings
    - Implementation: concatenate all rule queries per grammar, use pattern_index to map matches
    - Key insight: predicates scope per-pattern even with shared capture names

### Script System
- TOML workflow format: structured definition (steps, actions) - **deferred until use cases are clearer**
  - Builtin `workflow` runner script interprets TOML files
  - Users can also write pure Lua scripts directly
- Lua test framework: test discovery for `.moss/tests/` (test + test.property modules done)
  - Command naming: must clearly indicate "moss Lua scripts" not general testing (avoid `@test`, `@spec`, `@check`)
  - Alternative: no special command, just run test files directly via `moss <file>`
- [x] Agent module refactoring: extracted 6 submodules (parser, session, context, risk, commands, roles)
  - agent.lua reduced from ~2300 to ~1240 lines (46% reduction)
  - Remaining: run_state_machine (~400 lines), M.run (~650 lines) - core agent logic, self-contained
- Type system uses beyond validation
  - Done: `T.describe(schema)` for introspection, `type.generate` for property testing
  - Future: extract descriptions from comments (LuaDoc-style) instead of `description` field
- Format libraries (Lua): json, yaml, toml, kdl - **very low priority, defer until concrete use case**
  - Pure Lua implementations preferred (simple, no deps)
  - Key ordering: sort alphabetically by default, `__keyorder` metatable field for explicit order

### Tooling
- Read .git directly instead of spawning git commands where possible
  - Default branch detection, diff file listing, etc.
  - Trade-off: faster but more fragile (worktrees, packed refs, submodules)
- [x] Symbol history: `moss view path/Symbol --history [N]`
  - Shows last N changes to a symbol via git log -L (default: 5)
  - Works for both symbols and files
- Documentation freshness: tooling to keep docs in sync with code
  - For moss itself: keep docs/cli/*.md in sync with CLI behavior (lint? generate from --help?)
  - For user projects: detect stale docs in fresh projects (full moss assistance) and legacy codebases (missing/outdated docs)
  - Consider boy scout rule: when touching code, improve nearby docs
- [x] Case-insensitive matching (`-i` flag): `text-search` ✓, `view` ✓, `edit` ✓ all have it
- `moss fetch`: web content retrieval for LLM context (needs design: chunking, streaming, headless browser?)
- [x] Multi-file batch edit: `moss edit --batch edits.json` (see docs/design/batch-edit.md)
- Semantic refactoring: `moss edit <glob> --before 'fn extract_attributes' 'fn extract_attributes(...) { ... }'`
  - Insert method before/after another method across multiple files
  - Uses tree-sitter for semantic targeting (not regex)
  - `--batch` flag for multiple targets in one invocation
- Cross-file refactors: `moss move src/foo.rs/my_func src/bar.rs`
  - Move functions/types between files with import updates
  - Handles visibility changes (pub when crossing module boundaries)
  - Updates callers to use new path
- Structured config crate (`moss-config`): trait-based view/edit for known config formats (TOML, JSON, YAML, INI). Unified interface across formats. (xkcd 927 risk acknowledged)
  - Examples: .editorconfig, prettierrc, prettierignore, oxlintrc.json[c], oxfmtrc.json[c], eslint.config.js, pom.xml
  - Open: do build scripts belong here? (conan, bazel, package.json, cmake) - maybe separate `moss-build`
  - Open: linter vs formatter vs typechecker config - same trait or specialized?
  - Open: reconsider moss config format choice (TOML vs YAML, JSON, KDL) - rationalize decision

### Workspace/Context Management
- Persistent workspace concept (like Notion): files, tool results, context stored permanently
- Cross-session continuity without re-reading everything
- Investigate memory-mapped context, incremental updates

### Agent Future (deferred complex features)

**Test selection** - run only tests affected by changes
- Prerequisite: Call graph extraction in indexer (who calls what)
- Prerequisite: Test file detection (identify test functions/modules)
- Map modified functions → tests that call them
- Integration with test runners (cargo test, pytest, jest)

**Task decomposition** - break large tasks into validated subtasks
- Prerequisite: Better planning prompts (current --plan is basic)
- Prerequisite: Subtask validation (each step must pass before next)
- Agent creates plan with discrete steps
- Each step is a mini-agent session with its own validation
- Rollback entire task if any step fails

**Cross-file refactoring** - rename/move symbols across codebase
- Prerequisite: Symbol graph in indexer (callers, callees, types)
- Prerequisite: Import/export tracking per language
- Find all usages via `moss analyze --callers Symbol`
- Edit each usage atomically (all-or-nothing)
- Update imports/exports as needed

**Human-in-the-loop escalation** - ask user when stuck
- Prerequisite: Interactive mode in agent (currently non-blocking)
- Prerequisite: Stuck detection (beyond loop detection)
- When agent can't proceed, pause and ask user
- User provides guidance, agent continues
- Graceful degradation when non-interactive

**Partial success handling** - apply working edits, report failures
- Trade-off: Conflicts with atomic editing (all-or-nothing is often safer)
- Use case: Large batch where some files have issues
- Report which succeeded, which failed, why
- Consider: Is this actually desirable? Atomic may be better.

**Agent refactoring** - COMPLETE:
- Split into 6 modules: parser, session, context, risk, commands, roles
- Removed v1 freeform loop, kept only state machine
- agent.lua: 2300 → 762 lines (67% reduction)

### Agent Testing

**Observations** (74 sessions analyzed):
- Success rates: Anthropic 58%, Gemini 44%
- Auditor role completes in 2-4 turns for focused tasks
- Investigator can loop on complex questions (mitigated by cycle detection)
- --diff flag works well for PR-focused analysis
- Session logs: `.moss/agent/logs/*.jsonl`

**Ongoing**:
- Document friction points: where does the agent get stuck?
- Prompt tuning based on observed behavior

**Known Gemini issues** (still present):
- Hallucinates command outputs (answers before seeing results)
- Random Chinese characters mid-response
- Intermittent 500 errors and timeouts
- Occasionally outputs duplicate/excessive commands
- SSL certificate validation failures in some environments (`InvalidCertificate(UnknownIssuer)` - missing CA certs or SSL inspection proxy)
- **Google blocks Claude Code cloud environments**: 403 Forbidden on all Gemini API requests from Claude Code cloud infrastructure (even with valid API key and SSL bypass)

**OpenRouter in cloud environments**:
- SSL bypass works (connects to OpenRouter successfully)
- Gemini models via OpenRouter: 503 with upstream SSL error (unclear root cause, likely environment-specific)
- Claude models via OpenRouter: JSON parsing error (API response format mismatch with rig)
- Not worth debugging further in this environment - likely network/proxy/environment issues

**Roles implemented**:
- [x] Investigator (default): answers questions about the codebase
- [x] Auditor: finds issues (security, quality, patterns)
  - Usage: `moss @agent --audit "find unwrap on user input"`
  - Structured output: `$(note SECURITY:HIGH file:line - description)`
  - Planner creates systematic audit strategy

**Prompt tuning observations**:
- Claude sometimes uses bash-style `view ...` instead of `$(view ...)`
- Evaluator occasionally outputs commands in backticks

### Agent Future

Core agency features complete (shadow editing, validation, risk gates, retry, auto-commit).

**Remaining**:
- [ ] Test selection: run only tests affected by changes (use call graph)
- [ ] Task decomposition: break large tasks into validated subtasks
- [ ] Cross-file refactoring: rename symbol across codebase
- [ ] Partial success: apply working edits, report failures
- [ ] Human-in-the-loop escalation: ask user when stuck

**RLM-inspired** (see `docs/research/recursive-language-models.md`):
- [ ] Recursive investigation: agent self-invokes on subsets (e.g., `view --types-only` → pick symbols → `view symbol` → recurse if large)
- [ ] Decomposition prompting: system prompt guides "search before answering" strategy
- [ ] Chunked viewing: `view path --chunk N` or `view path --around "pattern"` for large files
- [ ] REPL-style persistence: extend ephemeral context beyond 1 turn for iterative refinement
- [ ] Depth/cost limits: cap recursion depth, token budgets per investigation

### Agent Observations

- **FOOTGUN: Claude Code cwd**: `cd` in Bash commands persists across calls. E.g., `cd foo && perl ...` breaks subsequent calls. Always use absolute paths.
- Claude works reliably with current prompt
- Context compaction unreliable in practice (Claude Code + Opus 4.5 lost in-progress work)
- Moss's dynamic context reshaping avoids append-only accumulation problems
- LLM code consistency: see `docs/llm-code-consistency.md`
- Large file edits: agentic tools struggle with large deletions (Edit tool match failures)
- **View loops**: Claude can get stuck viewing same files repeatedly without extracting info (session 67xvhqzk: 7× `view commands/`, 7× `view mod.rs`, 15 turns, task incomplete)
  - Likely cause: `view` output doesn't contain the info needed (e.g., CLI command names in Rust enums/structs require deeper inspection)
  - Possible fixes: better prompting, richer view output, or guide agent to use text-search for specific patterns
  - Contrast: text-search task succeeded in 1 turn (session 6ruc3djn) - tool output contained answer directly
  - Pattern: agent succeeds when tool output = answer, struggles when output requires interpretation/assembly
- **Pre-answering**: [FIXED] See `docs/experiments/agent-prompts.md` for full analysis
  - Root cause: task framing made single-turn look like correct completion
  - Fix: "investigator" role + concrete example + evidence requirement
  - Results: 3/3 correct with new prompt, 2-8 turns, no pre-answering
  - Key insight: concrete example in prompt prevents LLM defaulting to XML function calls
- **Ephemeral context**: Verified working correctly
  - Turn N outputs → visible in Turn N+1 `[outputs]` → gone by Turn N+2 unless `$(keep)`
  - 1-turn window is intentional: LLM needs to see results before deciding what to keep
- **Context uniqueness hypothesis**: identical context between any two LLM calls = error/loop
  - Risk: same command twice → same outputs → similar contexts → loop potential
  - Mitigation: `is_looping()` catches repeated commands, not identical context from different commands
- **CRITICAL: Using grep patterns with text-search** - Claude Code used `\|` (grep OR syntax) with text-search
  - text-search was specifically renamed from grep to avoid regex escaping confusion
  - Agent failed to use tool correctly despite it being in the command list
  - This shows agents don't understand tool semantics, just syntax
  - Need better tool descriptions or examples in prompt
- **Evaluator exploring instead of concluding**: [FIXED] Session zj3y5yu4 - evaluator output commands in backticks instead of $(answer)
  - Root cause: passive prompt "Do NOT run commands" → models interpret as "describe what to run"
  - Fix: strong role framing ("You are an EVALUATOR"), banned phrases ("NEVER say 'I need to'"), good/bad examples
  - Results: 4 turns vs 12 turns (no answer) for same query
  - Key insight: role assertion + explicit prohibitions + concrete examples beats instruction-only prompts
- **Dogfooding session (2026-01-07)**:
  - Gemini 500 errors remain intermittent (hit on first task, next 3 succeeded)
  - Agent occasionally uses `$(run ls -R)` instead of `$(view .)` - prefers shell over moss tools
  - Investigator: 4 turns for config structure query, correct answer, good line-range viewing
  - Auditor: 2 turns for unwrap() audit, parallel search commands, accurate file:line findings
  - Pattern: auditor role executes parallel searches efficiently (5 commands turn 1, synthesized turn 2)

### Session Analysis
- Web syntax highlighting: share tree-sitter grammars between native and web SPAs
  - Option A: embed tree-sitter WASM runtime, load .so grammars
  - Option B: `/api/highlight` endpoint, server-side highlighting
- Antigravity conversations: `~/.gemini/antigravity/conversations/*.pb` (protobuf - needs schema, files appear encrypted)
- Antigravity brain artifacts: `~/.gemini/antigravity/brain/*/` (task/plan/walkthrough metadata)
- Additional agent formats (need to find log locations/formats):
  - Windsurf (Codeium)
  - Cursor
  - Cline
  - Roo Code
  - Gemini Code Assist (VS Code extension)
  - GitHub Copilot (VS Code)
- Better `--compact` format: key:value pairs, no tables, all info preserved
- Better `--pretty` format: bar charts for tools, progress bar for success rate
- `moss sessions stats`: cross-session aggregates (session count, token hotspots, total usage)
- `moss sessions mark <id>`: mark as reviewed (store in `.moss/sessions-reviewed`)
- Friction signal detection: correction patterns, tool chains, avoidance
- Agent habit analysis: study session logs to identify builtin vs learned behaviors
  - Example: "git status before commit" - is this hardcoded or from CLAUDE.md guidance?
  - Test methodology: fresh/empty repo without project instructions
  - Cross-agent comparison: Claude Code, Gemini CLI, OpenAI Codex, etc.
  - Goal: understand what behaviors to encode in moss agent (model-agnostic reliability)
  - Maybe: automated agent testing harness (run same tasks across assistants)

### Friction Signals (see `docs/research/agent-adaptation.md`)
How do we know when tools aren't working? Implicit signals from agent behavior:
- Correction patterns: "You're right", "Should have" after tool calls
- Long tool chains: 5+ calls without acting
- Tool avoidance: grep instead of moss, spawning Explore agents
- Follow-up patterns: `--types-only` → immediately view symbol
- Repeated queries: same file viewed multiple times

### Distribution
- Wrapper packages for ecosystems: npm, PyPI, Homebrew, etc.
  - Auto-generate and publish in sync with GitHub releases
  - Single binary + thin wrapper scripts per ecosystem
- Direct download: platform-detected link to latest GitHub release binary (avoid cargo install overhead)

### Vision (Aspirational)
- **Friction Minimization Loop**: moss should make it easier to reduce friction, which accelerates development, which makes it easier to improve moss. Workflows documented → failure modes identified → encoded as tooling → friction reduced → faster iteration. The goal is tooling that catches problems automatically (high reliability) not documentation that hopes someone reads it (low reliability).
- Verification Loops: domain-specific validation (compiler, linter, tests) before accepting output
- Synthesis: decompose complex tasks into solvable subproblems (`moss synthesize`)
- Plugin Architecture: extensible view providers, synthesis strategies, code generators

### Agent / MCP
- Gemini Flash 3 prompt sensitivity: certain phrases ("shell", "execute", nested `[--opts]`) trigger 500 errors. Investigate if prompt can be further simplified to avoid safety filters entirely. See `docs/design/agent.md` for current workarounds.
- `moss @agent` (crates/moss/src/commands/scripts/agent.lua): MCP support as second-class citizen
  - Our own tools take priority, MCP as fallback/extension mechanism
  - Need to design how MCP servers are discovered/configured
- Context view management: extend/edit/remove code views already in agent context
  - Agents should be able to request "add more context around this symbol" or "remove this view"
  - Incremental context refinement vs full re-fetch
  - Blocked on: agent implementation existing at all

### CI/Infrastructure
(No current issues)

## Deferred

- VS Code extension: test and publish to marketplace (after first CLI release)
- Remaining docs: prior-art.md, hybrid-loops.md

## Python Features Not Yet Ported

### Orchestration
- Session management with checkpointing
- Driver protocol for agent decision-making
- Plugin system (partial - Rust traits exist)
- Event bus, validators, policies
- PR review, diff analysis
- TUI (Textual-based explorer)
- DWIM tool routing with aliases

### LLM-Powered
- Edit routing (complexity assessment → structural vs LLM)
- Summarization with local models
- Working memory with summarization

### Memory System
See `docs/design/memory.md`. Core API: `store(content, opts)`, `recall(query)`, `forget(query)`.
SQLite-backed persistence in `.moss/memory.db`. Slots are user-space (metadata), not special-cased.

### Local NN Budget (from deleted docs)
| Model | Params | FP16 RAM |
|-------|--------|----------|
| all-MiniLM-L6-v2 | 33M | 65MB |
| distilbart-cnn | 139M | 280MB |
| T5-small | 60M | 120MB |

Pre-summarization tiers: extractive (free) → small NN → LLM (expensive)

### Usage Patterns (from dogfooding)
- Investigation flow: `view .` → `view <file> --types-only` → `analyze --complexity` → `view <symbol>`
- Token efficiency: use `--types-only` for architecture, `--depth` sparingly

## Implementation Notes

### Self-update (`moss update`)
- Now in commands/update.rs
- GITHUB_REPO constant → "pterror/moss"
- Custom SHA256 implementation (Sha256 struct)
- Expects GitHub release with SHA256SUMS.txt

## When Ready

### First Release
```bash
git tag v0.1.0
git push --tags
```
- Verify cross-platform builds in GitHub Actions
- Test `moss update` against real release
- view: directory output shows dir name as first line (tree style) - intentional?
