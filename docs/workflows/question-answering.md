# Question Answering Workflow

Answering questions about a codebase: "How does X work?", "Where is Y defined?", "Why does Z happen?"

## Trigger

User asks a question about the codebase.

## Goal

Accurate answer with supporting evidence (file:line references).

## Prerequisites

- Codebase accessible
- (Optional) Index built for faster symbol lookup

## Decomposition Strategy

**Recursive widening search** (RLM-inspired):

```
1. Parse question → identify key terms (symbols, concepts, behaviors)
2. Search for terms → collect candidate locations
3. For each candidate:
   a. View at high level (--types-only or skeleton)
   b. If relevant, drill down (view specific symbol)
   c. If still unclear, widen search (callers, callees, usages)
4. Synthesize findings into answer
5. Validate: does answer address the original question?
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Term extraction | LLM reasoning |
| Search | `text-search`, `view --types-only` |
| Drill down | `view path/Symbol`, `analyze callers` |
| Widen | `analyze callees`, `text-search` for usages |
| Validate | LLM reasoning against original question |

## Validation

- Answer cites specific file:line locations
- Cited code actually supports the claim
- No hallucinated symbols or files
- Question is directly addressed (not tangentially)

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Symbol not found | Empty search results | Try aliases, partial matches, broader search |
| Too many results | >20 candidates | Add filters, use `--only`, ask for clarification |
| Answer doesn't match evidence | Manual review | Re-examine sources, revise answer |
| Circular references | Same files viewed repeatedly | Stop, summarize what's known, ask user |

## Example Session

**Question**: "How does moss handle syntax highlighting?"

```
Turn 1: Search for key terms
  $(text-search "highlight" --only "*.rs")
  → crates/moss/src/highlight.rs, crates/moss/src/commands/view/mod.rs

Turn 2: View high-level structure
  $(view crates/moss/src/highlight.rs --types-only)
  → Functions: highlight_code, get_highlight_config, ...

Turn 3: Drill into main function
  $(view crates/moss/src/highlight.rs/highlight_code)
  → Uses tree-sitter queries, loads highlight.scm files

Turn 4: Synthesize answer
  $(answer "Moss uses tree-sitter highlight queries (.scm files) loaded via
   highlight_code() in crates/moss/src/highlight.rs:45. Each grammar has
   a highlights.scm that maps node types to highlight groups...")
```

## Variations

### Behavioral Questions ("Why does X happen?")
Add execution tracing: run code, observe behavior, correlate with source.

### Architectural Questions ("How is X structured?")
Focus on `view --types-only`, dependency graphs, module boundaries.

### Historical Questions ("When did X change?")
Use `view --history`, git log, blame.

### Comparative Questions ("What's the difference between X and Y?")
View both, extract key properties, compare systematically.

## Metrics

- **Turns to answer**: Fewer is better (target: 2-5 for simple, 5-10 for complex)
- **Evidence quality**: All claims backed by file:line references
- **Accuracy**: Answer verified against ground truth (when available)
