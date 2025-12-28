# CLAUDE.md

Behavioral rules for Claude Code working in this repository.
Design philosophy: `docs/philosophy.md`. Key tenets: Generalize Don't Multiply, Separate Interface/Unify Plumbing, Minimize LLM usage, Structure > Text. Three primitives: view, edit, analyze.
Architecture: `docs/architecture-decisions.md`. Pure Rust, dynamic grammar loading, Lua workflows, index-optional design.

**Index-first architecture:** Core data extraction (symbols, imports, calls) goes in the Rust index. When adding language support: first add extraction to the indexer (deps.rs, skeleton.rs), then expose via commands. All commands should work without the index (graceful degradation via filesystem fallbacks).

## Core Rule

ALWAYS NOTE THINGS DOWN. When you discover something important, write it immediately:
- Bugs/issues → fix them or add to TODO.md
- Environment issues → TODO.md
- Design decisions → docs/ or code comments
- Future work → TODO.md
- Conventions → this file
- **Areas for improvement** → TODO.md (self-evaluate constantly, note friction points)
- **Key insights** → THIS FILE, immediately. If you learn something fundamental about design, coding, or this codebase, add it to CLAUDE.md before doing anything else.

**Triggers to document immediately:**
- User corrects you → write down what you learned before fixing
- Trial-and-error (2+ failed attempts) → document what actually works
- Framework/library quirk discovered → add to relevant docs/ file
- "I'll remember this" thought → you won't, write it down now
- **"Aha" moment about design** → add to CLAUDE.md Design Principles NOW
- **Citing CLAUDE.md as excuse** → if you say "CLAUDE.md says X" after failing to do X, the file failed its purpose. Adjust CLAUDE.md to actually prevent the failure, don't just note the rule exists.

**Don't say these phrases, instead edit first:**
- "Fair point" / "Good point" / "You're right" → edit TODO.md/CLAUDE.md BEFORE responding
- "Should have" / "I forgot to" → you're admitting failure, edit docs to prevent recurrence

## Dogfooding

**Use moss, not builtin tools.** Avoid Read/Grep/Glob at all costs - they waste tokens.

```
moss view [path[/symbol]] [--types-only]   # structure, skeleton, or symbol source
moss analyze [--complexity] [path]          # find complex functions
moss grep <pattern> [--only "*.rs"]         # search (real regex, not BRE: use | not \|)
```

Fall back to Read only for exact line content needed by Edit. If moss isn't useful, add to TODO.md and fix it.

## Negative Constraints

Do not:
- Announce actions with "I will now..." - just do them
- Use markdown formatting in LLM prompts (no bold, headers, code blocks unless required)
- Write preamble or summary in generated content
- Catch generic errors - catch specific error types
- Leave work uncommitted
- Create special cases - design to avoid them; if stuck, ask user rather than special-casing
- Deprecate things - no users, just remove; deprecation is for backwards compatibility we don't need
- **Create "legacy" APIs** - one API, one way. If the signature changes, update all callers. No `foo_legacy()` or `foo_v2()`.
- **Add to the monolith** - implementation goes in sub-crates (`moss-languages`, `moss-packages`, etc.), not all in `crates/moss/`. Split by domain.
- **Do half measures** - when adding a trait/abstraction, migrate ALL callers immediately. No "we can consolidate later" or asking whether to do partial vs full migration. Just do the full migration.
- **Ask permission on design when philosophy is clear** - if "Generalize Don't Multiply" or other tenets point to an obvious answer, don't present options. Just do the right thing.
- **Return tuples from functions** - use structs with named fields. Tuples obscure meaning and cause ordering bugs. Only use tuples when names would be pure ceremony (e.g., `(x, y)` coordinates).
- **Use trait default implementations** - defaults let you "implement" a trait without implementing it. That's a silent bug. Every method should be explicitly implemented; compiler enforces completeness, not convention.
- **String-match on source content for AST properties** - use tree-sitter node structure, not `text.contains("async")` or `text.starts_with("enum")`. Check node kinds, child nodes, field names. String matching is fragile and misses the point of having a parsed AST.
- **Replace content when editing lists** - when adding to TODO.md or similar, extend existing content, don't replace sections. Read carefully, add items, preserve what's there.
- **Dismiss tooling needs for "rare" operations** - error-prone manual operations need safety rails regardless of frequency. Build the tool.

## Design Principles

**Unify, don't multiply.** Fewer concepts = less mental load for humans and LLMs.
- One interface that handles multiple cases > separate interfaces per case
- Plugin/trait systems > hardcoded switches
- Extend existing abstractions > create parallel ones
- When user says "WTF is X" - ask: is this a naming issue or a design issue? Often the fix is unification, not renaming.
- Example: No `--jsonl`/`--jql` flags. Use `--jq '.matches[]'` instead. Adding jsonl would require jql for consistency (same reasons we have --jq: discoverability, convenience, perf). Tiny discoverability gain isn't worth the complexity.

**Simplicity over cleverness.**
- If proposing a new dependency, ask: can stdlib/existing code do this?
- HashMap > inventory crate. OnceLock > lazy_static. Functions > traits (until you need the trait).
- "Going in circles" = signal to simplify, not add complexity.

**Explicit over implicit.**
- Convenience = zero-config. Hiding information = pretending everything is okay.
- Location-based allowlists > hash-based (new occurrences shouldn't be silently ignored).
- Log when skipping something (e.g., "entry commented out, skipping") - user should know why.
- Respect user's file organization: insert near related content, don't blindly append.
- Show what's at stake before refusing: when blocking a destructive operation, display what would be affected.

**Separate niche data from shared config.**
- Don't bloat config.toml with feature-specific data (e.g., clone allowlist).
- Many commands load config.toml - adding hundreds of lines for a niche feature pollutes every load.
- Use separate files for large/specialized data: `.moss/clone-allow`, not `[clone.allow]` in config.

**Conversational architecture is flawed.**
The chatbot model (user → assistant → user, appending to a growing log) is wrong for agents. It leads to:
- Context that inevitably fills up, requiring compression/masking band-aids
- Lost-in-the-middle problems from accumulated history
- Treating sub-agents as garbage collectors for context isolation

Moss's alternative: dynamic context that can be reshaped throughout execution, not append-only accumulation. Combined with structural awareness (load only what's needed), this avoids the problem rather than managing symptoms.

**When stuck (2+ failed attempts):**
- Step back and reconsider the problem itself, not just try more solutions.
- Ask: "Am I solving the right problem?" (go-imports: naming issue vs architecture issue)
- Check docs/philosophy.md before questioning design decisions - the feature may be intentional.

## Recipes

Context Reset (before `/exit`):
1. Commit current work
2. Move completed tasks to CHANGELOG.md
3. Update TODO.md "Next Up" section
4. Note any open questions

## Conventions

### Updating CLAUDE.md
Add: workflow patterns, conventions, project-specific knowledge, tool usage patterns.
Don't add: temporary notes (TODO.md), implementation details (docs/), one-off decisions (commit messages).
Keep it slim: If CLAUDE.md grows past ~150 lines, refactor content to docs/ and reference it.

### Updating TODO.md
Proactively add features, ideas, patterns, technical debt, integration opportunities.
Keep TODO.md lean (<100 lines). Move completed items to CHANGELOG.md.
- Next Up: 3-5 concrete tasks for immediate work
- Active Backlog: pending items only, no completed
- Future Work: categories with brief items
- To Consolidate: new ideas before proper categorization
- When completing items: mark as `[x]`, don't delete or rewrite sections
- When cleaning up: ONLY delete `[x]` items, preserve everything else verbatim
- Avoid: verbose descriptions, code examples, duplicate entries

### Working Style

Start by checking TODO.md. Default: work through ALL items in "Next Up" unless user specifies otherwise.
Propose work queue, get confirmation, then work autonomously through all tasks.

Agentic by default - continue through tasks unless:
- Genuinely blocked and need clarification
- Decision has significant irreversible consequences
- User explicitly asked to be consulted

When you say "do X first" or "then we can Y" - add it to TODO.md immediately. Don't just say it, track it.

Bail out early if stuck in a loop rather than burning tokens.

Marathon mode: Work continuously through TODO.md until empty or blocked.
- Commit after each logical unit (creates resume points)
- Bail out if stuck in a loop (3+ retries on same error)
- Re-reading files repeatedly = context degrading, wrap up soon
- If genuinely blocked, document state in TODO.md and stop

See `docs/session-modes.md` for Fresh mode (default for normal sessions).

Write while researching, not after. Queue review items in TODO.md, don't block for them.

Self-evaluate constantly: After completing work, note friction points, areas for improvement, and what could be better. Log to TODO.md under "To Consolidate" or directly improve if quick.

Session handoffs: Add "Next Up" section to TODO.md with 3-5 tasks. Goal is to complete ALL of them next session.

### Commits

Commit consistently. Each commit = one logical change.
Move completed TODOs to CHANGELOG.md.
