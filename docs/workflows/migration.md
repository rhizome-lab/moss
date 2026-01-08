# Migration Workflow

Updating code to new versions or patterns: framework upgrades, API changes, pattern migrations.

## Trigger

- Framework/library major version upgrade
- Moving to new architectural pattern
- Replacing deprecated APIs
- Technology stack migration

## Goal

- Successfully migrate to new version/pattern
- Maintain functionality throughout
- Minimize risk and disruption
- Document the migration for others

## Prerequisites

- Clear understanding of target state
- Tests passing before migration
- Rollback plan in place
- Time allocated for verification

## Decomposition Strategy

**Assess → Plan → Execute → Verify**

```
1. ASSESS: Understand scope
   - What needs to change?
   - What's the impact radius?
   - What are the breaking changes?
   - What's the migration path?

2. PLAN: Create migration strategy
   - Incremental vs. big-bang?
   - What order to migrate?
   - What can be automated?
   - What needs manual attention?

3. EXECUTE: Perform migration
   - Follow the plan
   - Handle exceptions
   - Keep tests passing
   - Commit frequently

4. VERIFY: Confirm success
   - Run full test suite
   - Manual smoke testing
   - Performance validation
   - User acceptance testing
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Assess | `view`, `text-search`, changelogs |
| Plan | Document, `analyze` impact |
| Execute | `edit`, codemods, refactoring tools |
| Verify | Tests, `moss analyze`, monitoring |

## Migration Strategies

### Incremental Migration
```
Best for: Large codebases, high-risk changes
Approach:
1. Add compatibility layer
2. Migrate piece by piece
3. Remove compatibility layer
Benefits: Lower risk, can pause/resume
Costs: Longer duration, dual maintenance
```

### Big-Bang Migration
```
Best for: Small codebases, low-risk changes
Approach:
1. Prepare all changes
2. Apply all at once
3. Fix any issues
Benefits: Faster, cleaner
Costs: Higher risk, harder to debug
```

### Strangler Pattern
```
Best for: System replacements
Approach:
1. New system handles new features
2. Route some traffic to new system
3. Gradually migrate old features
4. Decommission old system
Benefits: Low risk, easy rollback
Costs: Running two systems
```

## Common Migration Types

### Framework Upgrade
```
1. Read migration guide
2. Update dependencies
3. Fix compile errors
4. Fix deprecation warnings
5. Update tests
6. Verify functionality
```

### API Pattern Change
```
1. Create new API
2. Update callers to new API
3. Mark old API deprecated
4. Remove old API after grace period
```

### Database Migration
```
1. Create migration script
2. Add new schema alongside old
3. Backfill data
4. Switch to new schema
5. Remove old schema
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Incomplete migration | Old patterns still present | Search for old patterns, continue |
| Hidden dependency | Breaks at runtime | Add tests, fix dependency |
| Data loss | Data not migrated correctly | Rollback, fix migration script |
| Performance regression | Slower than before | Profile, optimize or rollback |

## Example Session

**Goal**: Migrate from React Router v5 to v6

```
Turn 1: Assess scope
  $(text-search "react-router-dom" path:package.json)
  → Currently on v5.3.0
  $(text-search "<Route" path:src)
  → 45 route definitions across 12 files

Turn 2: Read migration guide
  → Breaking changes:
    - Switch → Routes
    - Render prop → element prop
    - useHistory → useNavigate
    - Nested routes restructured

Turn 3: Plan migration
  - Incremental: Update component by component
  - Order: Leaf routes first, then containers
  - Automation: Codemod for simple patterns
  - Manual: Complex nested routes

Turn 4: Execute - update dependencies
  $(npm install react-router-dom@6)
  → Compile errors expected

Turn 5: Execute - fix Switch → Routes
  $(text-search "<Switch" path:src)
  → 8 occurrences
  $(edit src/App.tsx)
  → Replace <Switch> with <Routes>

Turn 6: Execute - fix render prop
  $(text-search "render=" path:src)
  → Convert render={...} to element={...}

Turn 7: Execute - fix useHistory
  $(text-search "useHistory" path:src)
  → Replace with useNavigate, update API calls

Turn 8: Verify
  $(npm test)
  → All tests pass
  $(npm start)
  → Manual testing: all routes work
```

## Automation Opportunities

### Codemods
- AST-based transformations
- Pattern replacement
- Mass renames

### IDE Refactoring
- Find/replace with regex
- Rename symbol
- Change signature

### Custom Scripts
- Data migration scripts
- Configuration updates
- Build process changes

## Anti-patterns

- **Boiling the ocean**: Trying to migrate everything at once
- **No rollback plan**: Can't undo if things go wrong
- **Skipping tests**: Migrating without verification
- **Ignoring deprecations**: Leaving deprecated code "for later"
- **Over-automation**: Automating incorrectly is worse than manual

## See Also

- [Cross-Language Migration](cross-language-migration.md) - Language-to-language porting
- [Breaking API Changes](breaking-api-changes.md) - Handling upstream breaks
- [Refactoring](refactoring.md) - Internal code restructuring
