# Moss Implementation Checklist

## Phase 0: Foundation
- [x] Project structure (src/, tests/, pyproject.toml)
- [x] Linting setup (ruff config, pre-commit hook)
- [x] Basic test harness (pytest)

## Phase 1: Core Primitives
- [x] Event Bus (pub/sub with typed events)
- [x] Shadow Git wrapper (atomic commits, rollback, branch management)
- [x] Handle system (lazy references to files/artifacts)

## Phase 2: Context Engine
- [x] View Provider protocol (abstract base)
- [x] Skeleton provider (AST-based, Tree-sitter)
- [x] Dependency Graph provider
- [x] View compilation pipeline

## Phase 3: Structural Editing
- [x] Anchor resolution (fuzzy AST matching)
- [x] Patch application (AST-aware edits)
- [x] Fallback to text-based editing for broken AST

## Phase 4: Validation Loop
- [x] Validator protocol
- [x] Built-in validators (syntax, ruff, pytest)
- [x] Silent loop orchestration (draft → validate → fix → commit)
- [x] Velocity monitoring (detect oscillation/stalls)

## Phase 5: Policy & Safety
- [x] Policy engine (intercept tool calls)
- [x] Quarantine mode (lock broken files)
- [x] Velocity checks

## Phase 6: Memory Layer
- [x] Episodic store (state, action, outcome logging)
- [x] Vector indexing for episode retrieval
- [x] Semantic rule extraction (offline pattern matcher)

## Phase 7: Multi-Agent
- [x] Ticket protocol (task, handles, constraints)
- [x] Worker lifecycle (spawn, execute, die)
- [x] Manager/merge conflict resolution

## Phase 8: Configuration
- [ ] Executable config (TypeScript/Python DSL)
- [ ] Distro system (composable presets)

## Phase 9: API Surface
- [ ] Headless HTTP API
- [ ] SSE/WebSocket streaming
- [ ] Checkpoint approval endpoints
