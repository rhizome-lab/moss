# Rule Sharing Design

Import rules from URLs or shared packages.

## Status: Draft

## Problem

Users want to:
1. Share rules across projects
2. Use community-maintained rule sets
3. Keep rules in sync with upstream

## Current State

Rules are loaded from:
1. Builtins (embedded in binary)
2. User global (`~/.config/moss/rules/*.scm`)
3. Project (`.moss/rules/*.scm`)

No mechanism for importing from external sources.

## Proposal

### Phase 1: URL-based import

Add `moss rules add <url>` command:

```bash
# Add a single rule from URL
moss rules add https://raw.githubusercontent.com/user/rules/main/no-dbg.scm

# Add to global rules (default is project)
moss rules add --global https://...

# List imported rules with sources
moss rules list --sources
```

**Behavior:**
- Downloads .scm file
- Saves to `.moss/rules/` (or `~/.config/moss/rules/` with `--global`)
- Creates `.moss/rules.lock` tracking source URLs

**rules.lock format:**
```toml
[rules."no-dbg"]
source = "https://raw.githubusercontent.com/user/rules/main/no-dbg.scm"
sha256 = "abc123..."
added = "2025-01-08"
```

**Update command:**
```bash
# Update all imported rules
moss rules update

# Update specific rule
moss rules update no-dbg
```

### Phase 2: Rule packages (future)

Reference a git repo with multiple rules:

```bash
# Add all rules from a repo
moss rules add-repo https://github.com/user/rust-rules.git

# Saves to .moss/rules/vendor/rust-rules/
```

Or reference in config:
```toml
# .moss/config.toml
[rules]
extends = [
    "https://github.com/user/rust-rules.git#v1.0",
]
```

### Phase 3: Registry (future)

If there's enough demand, a central registry like crates.io:

```bash
moss rules add rust-best-practices@1.0
```

## Implementation

### Phase 1 scope

1. Add `moss rules add <url>` command
2. Add `.moss/rules.lock` tracking
3. Add `moss rules update` command
4. Add `moss rules list --sources`

### File structure

```
.moss/
├── config.toml
├── rules/
│   ├── no-dbg.scm           # Downloaded rule
│   └── my-custom-rule.scm   # Local rule
└── rules.lock               # Tracks imported rules
```

### Considerations

**Security:**
- Downloaded rules are tree-sitter queries, not executable code
- Still show diff on update, require confirmation for changes
- Consider signature verification for official rules

**Conflicts:**
- If local rule has same ID as imported, local wins (current behavior)
- Warn on ID conflicts

**Offline:**
- Rules are downloaded, not fetched at runtime
- Works offline after initial add

## Alternatives Considered

### Inline URL references (not downloading)
```toml
[rules]
imports = ["https://..."]
```
Rejected: Requires network at runtime, slower, less inspectable.

### Git submodules
Rejected: Adds git complexity, overkill for single files.

### npm-style registry
Deferred: Requires infrastructure, not enough demand yet.

## Decisions

1. **Explicit updates** - `moss rules update` required, no auto-update
2. **Private URLs** - Deferred. Auth token storage is tricky (env vars? keychain?)
3. **Breaking changes** - Open. Options: show diff before applying, pin by hash, etc.
