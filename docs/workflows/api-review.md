# API Review Workflow

Evaluating whether an API is well-designed: consistent, ergonomic, evolvable.

## Trigger

- New API being proposed
- Existing API causing friction
- Major version upgrade planning
- Library/SDK design review

## Goal

- Identify design issues before implementation/release
- Ensure consistency with existing APIs
- Verify ergonomics and usability
- Assess future extensibility

## Prerequisites

- API surface defined (types, functions, endpoints)
- Usage context understood (who uses this, how?)
- Existing conventions documented (naming, patterns)

## Decomposition Strategy

**Survey → Analyze → Compare → Recommend**

```
1. SURVEY: Map the API surface
   - List all public types, functions, endpoints
   - Document parameters, return types, errors
   - Identify the core abstractions

2. ANALYZE: Evaluate each aspect
   - Naming: Clear, consistent, follows conventions?
   - Ergonomics: Easy to use correctly, hard to misuse?
   - Completeness: Covers all use cases?
   - Errors: Informative, actionable?
   - Defaults: Sensible, safe?

3. COMPARE: Check against standards
   - Internal consistency (similar things work similarly)
   - External consistency (follows ecosystem conventions)
   - Prior art (how do others solve this?)

4. RECOMMEND: Prioritized feedback
   - Breaking issues (must fix)
   - Improvements (should fix)
   - Suggestions (nice to have)
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Survey | `view --types-only`, `analyze` |
| Analyze | `view`, `text-search` for patterns |
| Compare | `view` reference APIs, docs |
| Recommend | Document findings |

## Review Criteria

### Naming
- [ ] Names are descriptive and unambiguous
- [ ] Follows language/ecosystem conventions
- [ ] Consistent with rest of codebase
- [ ] Avoids abbreviations (except well-known)

### Ergonomics
- [ ] Common operations are simple
- [ ] Rare operations are possible
- [ ] Pit of success (easy to use correctly)
- [ ] Hard to misuse accidentally

### Consistency
- [ ] Similar operations have similar signatures
- [ ] Ordering of parameters is consistent
- [ ] Error handling is uniform
- [ ] Null/optional handling is consistent

### Extensibility
- [ ] Can add features without breaking changes
- [ ] Versioning strategy is clear
- [ ] Backwards compatibility considered

### Documentation
- [ ] All public items documented
- [ ] Examples provided
- [ ] Error conditions explained
- [ ] Migration path documented

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Missing context | Don't understand usage | Interview users, read call sites |
| Bikeshedding | Stuck on naming | Time-box, focus on semantics |
| Inconsistent review | Different standards per area | Use checklist consistently |
| Over-engineering | Adding for hypothetical needs | Focus on current requirements |

## Example Session

**Review**: New configuration API

```
Turn 1: Survey the surface
  $(view src/config/mod.rs --types-only)
  → ConfigBuilder, Config, ConfigError
  → builder pattern with 12 methods

Turn 2: Check naming consistency
  $(text-search "fn with_" path:src/config)
  → with_timeout, with_retries, withMaxConnections
  Issue: Inconsistent casing (withMaxConnections)

Turn 3: Check error types
  $(view src/config/error.rs)
  → Single ConfigError enum with 8 variants
  Issue: No context in InvalidValue variant

Turn 4: Compare with existing patterns
  $(view src/http/builder.rs --types-only)
  → HttpBuilder uses same pattern, consistent naming

Turn 5: Check for footguns
  $(view src/config/mod.rs/build)
  → build() panics if required fields missing
  Issue: Should return Result, not panic

Turn 6: Document recommendations
  - BREAKING: build() should return Result
  - BREAKING: Rename withMaxConnections to with_max_connections
  - SHOULD: Add context to InvalidValue error
  - NIT: Consider derive(Default) for ConfigBuilder
```

## Variations

### REST API Review
Focus on HTTP semantics, status codes, resource naming, versioning.

### SDK/Library API Review
Focus on idiomatic usage, type safety, error handling.

### Internal API Review
Less strict on backwards compatibility, more on ergonomics.

### Breaking Change Review
Focus on migration path, deprecation strategy, user impact.

## Anti-patterns

- **Designing by committee**: Too many reviewers, inconsistent feedback
- **Review after implementation**: Expensive to change, review is rubber stamp
- **Ignoring users**: API designed for implementor, not consumer
- **Premature abstraction**: Generic API before concrete use cases
