# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- [x] Add `[serve]` config section (port, host settings)
- Documentation freshness: check docs/cli/*.md against --help output
- More agent dogfooding on different task types (refactoring, complex investigation)

## Remaining Work
- Namespace-qualified lookups: `moss view std::vector`, `moss view com.example.Foo`
  - Requires language-specific namespace semantics - low priority
- Shadow worktree: true shadow-first mode (edit in shadow, then apply)
  - Current: --shadow flag works, but not default for all edits
  - Zero user interruption (user can edit while agent tests in background)

### Configuration System
Sections: `[daemon]`, `[index]`, `[aliases]`, `[todo]`, `[view]`, `[analyze]`, `[text-search]`, `[pretty]`, `[serve]`

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

### Package Management
- `moss package install/uninstall`: proxy to ecosystem tools (cargo add, npm install, etc.)
  - Very low priority - needs concrete use case showing value beyond direct tool usage
  - Possible value-adds: install across all ecosystems, auto-audit after install, config-driven installs

### Code Quality
- [x] `--allow` for duplicate-functions: accept line range like output suggests (e.g., `--allow src/foo.rs:10-20`)
- Unnecessary aliases: `let x = Foo; x.bar()` → `Foo.bar()`. Lint for pointless intermediate bindings.
- [x] Chained if-let: edition 2024 allows `if let Ok(x) = foo() && let Some(y) = bar(x)`. Audit complete.
- PR/diff analysis: `moss analyze --pr` or `--diff` for changed code focus (needs broader analysis workflow design)
- Validate node kinds against grammars: `validate_unused_kinds_audit()` in each language file ensures documented unused kinds stay in sync with grammar
- Directory context: attach LLM-relevant context to directories (like CLAUDE.md but hierarchical)
- Deduplicate SQL queries in moss: many ad-hoc queries could use shared prepared statements or query builders (needs design: queries use different execution contexts - Connection vs Transaction)
- Detect reinvented wheels: hand-rolled JSON/escaping when serde exists, manual string building for structured formats, reimplemented stdlib. Heuristics unclear. Full codebase scan impractical. Maybe: (1) trigger on new code matching suspicious patterns, (2) index function signatures and flag known anti-patterns, (3) check unused crate features vs hand-rolled equivalents. Research problem.
- Syntax-based linting: custom rules like ESLint's `no-restricted-syntax` but tree-sitter based
  - Example: `GrammarLoader::new` restricted except in `grammar_loader()` singleton
  - Example: `Query::new` restricted except in cached getters
  - Define patterns via tree-sitter queries, whitelist locations

### `moss todo` Future
- Goal: port `todo.rs` to `@todo` script (Lua + `moss edit` primitives)
  - These are conceptually `moss edit` ops on markdown
  - `@todo` target prefix is the path toward unification
- `list` with filters, `clean`, `normalize` → port to Lua script (todo-specific semantics)
- Validates that view/edit primitives are sufficient for structural edits

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
- Symbol history: `moss view path/Symbol --history` or `moss history path/Symbol`
  - Show last N changes to a symbol via git blame
  - Extract symbol boundaries, then trace through git history
  - Useful for understanding evolution of a function/class
- Documentation freshness: tooling to keep docs in sync with code
  - For moss itself: keep docs/cli/*.md in sync with CLI behavior (lint? generate from --help?)
  - For user projects: detect stale docs in fresh projects (full moss assistance) and legacy codebases (missing/outdated docs)
  - Consider boy scout rule: when touching code, improve nearby docs
- Case-insensitive matching (`-i` flag): `text-search` ✓ has it, optionally add to `view`/`edit`/`analyze` path/symbol targets
- `moss fetch`: web content retrieval for LLM context (needs design: chunking, streaming, headless browser?)
- Multi-file batch edit: less latency than N sequential edits. Not for identical replacements (use sed) or semantic renames (use LSP). For structured batch edits where each file needs similar-but-contextual changes (e.g., adding a trait method to 35 language files).
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
