# Dependency Tracing Workflow

Understanding what depends on what: "What uses X?", "What does X depend on?"

## Trigger

- Planning a refactoring
- Assessing impact of a change
- Understanding coupling in the system
- Debugging unexpected interactions

## Goal

- Map dependencies for a component/symbol
- Understand impact radius of changes
- Identify hidden dependencies
- Find circular or problematic dependencies

## Prerequisites

- Codebase indexed (optional but faster)
- Understanding of what to trace

## Decomposition Strategy

**Identify → Trace → Analyze → Document**

```
1. IDENTIFY: Define what to trace
   - Symbol (function, type, module)
   - Direction (what uses it, what it uses)
   - Scope (file, module, project, external)

2. TRACE: Follow dependencies
   - Direct dependencies (immediate callers/callees)
   - Transitive dependencies (full chain)
   - Categorize by type (import, call, type reference)

3. ANALYZE: Understand the graph
   - Identify clusters and boundaries
   - Find unexpected dependencies
   - Check for cycles
   - Assess coupling level

4. DOCUMENT: Record findings
   - Dependency graph or list
   - Problem areas identified
   - Recommendations for changes
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Identify | Define scope |
| Trace | `analyze callers`, `analyze callees`, `text-search` |
| Analyze | `analyze trace`, `view` dependency files |
| Document | Write findings |

## Trace Types

### What Uses X (Reverse Dependencies)
```bash
# Find all callers of a function
moss analyze callers function_name

# Find all references to a type
moss text-search "TypeName" --only "*.rs"

# Find all imports of a module
moss text-search "use crate::module" --only "*.rs"
```

### What X Uses (Forward Dependencies)
```bash
# Find all functions called by X
moss analyze callees function_name

# View imports in a file
moss view src/module.rs --deps

# Check package dependencies
moss package deps
```

### Transitive Dependencies
```bash
# Trace full call chain
moss analyze trace symbol_name

# Build full dependency tree
moss package tree
```

## Dependency Categories

### Code Dependencies
- **Import/Use**: Module imports
- **Call**: Function/method calls
- **Type**: Type references (parameters, returns, fields)
- **Trait**: Trait implementations and bounds

### Data Dependencies
- **Read**: Data read from
- **Write**: Data written to
- **Shared**: Shared mutable state

### Build Dependencies
- **Compile-time**: Needed to build
- **Runtime**: Needed to run
- **Optional**: Feature-gated

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Missing dependencies | Change breaks unexpected code | Use static analysis, not just grep |
| Dynamic dependencies | Not found by static analysis | Check runtime behavior, reflection |
| Scope too narrow | Miss transitive dependencies | Expand scope, trace further |
| Circular dependencies | Infinite trace | Detect and document cycles |

## Example Session

**Goal**: What depends on the `UserService` class?

```
Turn 1: Find direct callers
  $(analyze callers UserService)
  → AuthController, ProfileController, AdminController
  → UserMigration, UserSeeder

Turn 2: Check type references
  $(text-search "UserService" --only "*.rs")
  → Also used in: dependency injection config, tests

Turn 3: Check for trait implementations
  $(text-search "impl.*for UserService" --only "*.rs")
  → Implements: Service, Cacheable

Turn 4: Trace transitive dependencies
  $(analyze callers AuthController)
  → AuthMiddleware, LoginHandler, RegisterHandler

Turn 5: Analyze the dependency graph
  → UserService is a core service
  → 3 direct controller dependents
  → 2 infrastructure dependents
  → ~10 transitive dependents
  → No circular dependencies

Turn 6: Document impact
  → Changing UserService interface affects:
    - 3 controllers (need interface update)
    - Tests (need mock update)
    - DI config (if constructor changes)
```

## Variations

### Package/Module Level
Trace dependencies between packages rather than symbols.

### External Dependencies
Focus on third-party library usage.

### Circular Dependency Detection
Specifically look for problematic cycles.

### Impact Analysis
Estimate blast radius of a proposed change.

## Visualization

For complex dependency graphs, consider:
- DOT/Graphviz for rendering
- Hierarchical grouping by module
- Highlighting problem areas
- Filtering to relevant subset

## Anti-patterns

- **God objects**: Everything depends on one thing
- **Circular dependencies**: A→B→C→A
- **Hidden dependencies**: Global state, singletons
- **Leaky abstractions**: Implementation details exposed

## See Also

- [Refactoring](refactoring.md) - Use after tracing to plan changes
- [Dead Code Elimination](dead-code-elimination.md) - Remove unused code
