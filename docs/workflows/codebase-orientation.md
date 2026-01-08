# Codebase Orientation Workflow

Getting oriented in an unfamiliar codebase: "What is this project?", "How is it structured?"

## Trigger

- Starting work on unfamiliar project
- Joining a new team
- Reviewing a project for acquisition/audit
- Agent needs to understand codebase before task

## Goal

- Understand project purpose and scope
- Map high-level architecture
- Identify key entry points and patterns
- Know where to look for specific functionality

## Prerequisites

- Access to source code
- Ability to build/run (optional but helpful)

## Decomposition Strategy

**Survey → Trace → Map → Verify**

```
1. SURVEY: Get the big picture
   - Read README, documentation
   - Check package/dependency files
   - List top-level directories
   - Identify main entry points

2. TRACE: Follow key paths
   - Trace from entry point to core logic
   - Identify major subsystems
   - Note data flow patterns
   - Find configuration/initialization

3. MAP: Build mental model
   - Document component relationships
   - Identify layers/boundaries
   - Note naming conventions
   - Find where patterns are established

4. VERIFY: Test understanding
   - Try to predict where code lives
   - Make a small change
   - Run tests, see what breaks
   - Ask questions to validate model
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Survey | `view .`, `view README.md`, package files |
| Trace | `view --types-only`, `analyze callers/callees` |
| Map | `analyze overview`, `view` key files |
| Verify | Build, test, explore |

## Key Files to Examine

### Project Root
- `README.md` - Purpose, setup, usage
- `package.json` / `Cargo.toml` / `go.mod` - Dependencies
- `Makefile` / `justfile` / build scripts - Build process
- Configuration files - Project settings

### Entry Points
- `main.*` / `index.*` / `app.*` - Application entry
- `lib.*` / `mod.rs` - Library root
- `cmd/` / `bin/` - CLI entry points
- `src/` root - Source organization

### Architecture Clues
- Dependency injection / wiring
- Configuration loading
- Plugin/extension points
- Error handling patterns

## Exploration Strategies

### Top-Down
```
1. Start at entry point (main)
2. Follow initialization sequence
3. Map major subsystems
4. Dive into areas of interest
```

### Bottom-Up
```
1. Start from specific feature
2. Trace to find callers
3. Map the call hierarchy
4. Connect to overall structure
```

### Dependency-Driven
```
1. Examine dependency graph
2. Identify core vs. peripheral
3. Understand what's imported
4. Map integration points
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Lost in details | Hours in one file | Step back, return to big picture |
| Wrong mental model | Predictions fail | Revise model, trace again |
| Missing context | Code doesn't make sense | Find documentation, ask questions |
| Over-simplified | Model has gaps | Add nuance, explore edge cases |

## Example Session

**Goal**: Understand a web service codebase

```
Turn 1: Survey project structure
  $(view .)
  → src/, tests/, migrations/, docs/
  → Cargo.toml, docker-compose.yml

Turn 2: Check dependencies for clues
  $(view Cargo.toml)
  → axum (web), sqlx (db), tokio (async)
  → This is an async Rust web service

Turn 3: Find entry point
  $(view src/main.rs --types-only)
  → main(), setup_routes(), connect_db()

Turn 4: Trace initialization
  $(view src/main.rs)
  → Load config, connect to DB, setup routes, serve

Turn 5: Map route handlers
  $(view src/routes/mod.rs --types-only)
  → users/, posts/, auth/ modules
  → Standard REST resource structure

Turn 6: Understand a representative handler
  $(view src/routes/users/handlers.rs --types-only)
  → list_users, get_user, create_user, update_user, delete_user
  → CRUD pattern, follows conventions

Turn 7: Check database layer
  $(view src/db/mod.rs --types-only)
  → Repository pattern, trait-based abstraction
  → UserRepo, PostRepo, etc.

Turn 8: Verify understanding
  → Prediction: "posts follow same pattern as users"
  $(view src/routes/posts/handlers.rs --types-only)
  → Confirmed: same CRUD structure
```

## Output

After orientation, document:
1. **Purpose**: What does this project do?
2. **Architecture**: Major components and their relationships
3. **Entry points**: Where to start for different tasks
4. **Patterns**: Naming, structure, idioms used
5. **Key files**: Most important files to know
6. **Gotchas**: Non-obvious things to be aware of

## Variations

### Quick Orientation (15 min)
Focus on README, entry point, top-level structure only.

### Deep Orientation (2+ hours)
Thorough exploration, trace multiple paths, document extensively.

### Task-Focused Orientation
Orient just enough to complete a specific task, skip unrelated areas.

## See Also

- [Codebase Onboarding](codebase-onboarding.md) - More comprehensive onboarding process
- [Question Answering](question-answering.md) - Answering specific questions about code
