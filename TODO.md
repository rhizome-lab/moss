# Moss Roadmap

See `CHANGELOG.md` for completed features.

## Backlog

### Phase 22: Synthesis Integration

See `~/git/prose/moss/` for detailed design documents.

#### 22a: Core Framework
- [ ] Directory structure (`src/moss/synthesis/`)
- [ ] Abstract interfaces (`Specification`, `Context`, `Subproblem`)
- [ ] `DecompositionStrategy` ABC
- [ ] `Composer` ABC
- [ ] `StrategyRouter` (reuse DWIM TFIDFIndex)
- [ ] `SynthesisFramework` (domain-agnostic engine)
- [ ] Integration with shadow git, memory, event bus
- [ ] Tests (>80% coverage)

#### 22b: Code Synthesis Domain
- [ ] Domain structure (`src/moss/domains/synthesis/`)
- [ ] `TypeDrivenDecomposition` strategy
- [ ] `TestDrivenDecomposition` strategy
- [ ] `TestExecutorValidator`
- [ ] `CodeComposer`
- [ ] Integration tests

#### 22c: CLI & Integration
- [ ] `moss synthesize` CLI command
- [ ] Integrate with `moss edit` (fallback for complex tasks)
- [ ] Synthesis configuration presets
- [ ] Event emission for progress tracking

#### 22d: Optimization & Learning
- [ ] Test execution caching
- [ ] Parallel subproblem solving
- [ ] Memory-based strategy learning
- [ ] Scale testing (depth 20+ problems)
- [ ] Performance benchmarks

### Phase 23: Context & Memory

#### Recursive Document Summarization
- [ ] Merkle hash → summary map for docs (md files, etc.)
- [ ] Incremental updates (only re-summarize changed subtrees)
- [ ] Integration with context host

#### Chatlog Management
- [ ] Summarize agent-user chatlogs for context retention
- [ ] Drop stale content from active context
- [ ] Persist original chatlogs as JSON (full history)
- [ ] Retrieval for relevant past conversations

### Phase 24: Refactoring Tools

#### Codemods
- [ ] Easy refactor primitives (rename, extract, inline)
- [ ] Codemod DSL or API
- [ ] Multi-file atomic refactors
- [ ] Preview/dry-run mode
