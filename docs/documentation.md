# Documentation Strategy

<!-- covers: crates/moss-cli/src/tree.rs, crates/moss-cli/src/commands/view.rs -->

Keep docs in sync with code through tooling, not discipline.

## Principles

1. **Single source of truth** - code is authoritative, docs derive from or reference it
2. **Drift detection** - tooling flags when referenced code changes
3. **Examples are tests** - runnable, can't lie

## Inline Docs

Doc comments live in source files (rustdoc style). Use double blank line to separate summary from extended docs:

```rust
/// Brief summary shown in default view.
///
///
/// Extended explanation only shown with `--docs` flag.
/// Can be multiple paragraphs, examples, etc.
fn foo() {}
```

View levels:
- `moss view file.rs --skeleton` - structure only, no docs
- `moss view file.rs` - code + summary (first paragraph)
- `moss view file.rs --docs` - code + full docs

## External Docs

For architecture, design decisions, tutorials - things that don't belong inline.

Each external doc declares what code it covers:

```markdown
<!-- covers: src/parser.rs, src/lexer.rs -->
<!-- covers: src/lib.rs:parse_* -->
# Parser Design

...
```

`moss analyze --docs` detects when covered code has changed significantly since doc was last updated. Uses git blame + doc modification time.

## Examples as Tests

Examples live in test files with markers:

```rust
#[test]
fn example_basic_usage() {
    // Setup (hidden from docs)
    let config = Config::default();

    // [example: basic-usage]
    let result = parse("foo", &config);
    assert!(result.is_ok());
    // [/example]

    // More assertions (hidden from docs)
    assert_eq!(result.unwrap().len(), 1);
}
```

Docs reference by marker:

```markdown
## Basic Usage

{{example: tests/parser_test.rs#basic-usage}}
```

`moss docs build` (or similar) expands these references. If the marker doesn't exist, build fails.

## Implementation

### Phase 1: Inline doc levels
- [x] Parse double-blank convention in doc comments
- [x] `--docs` flag for `moss view`
- [x] Update skeleton extraction to use summary only

### Phase 2: External doc tracking
- [x] `<!-- covers: ... -->` parser
- [x] `moss analyze --stale-docs` to detect stale docs
- [ ] Integration with `moss view` to show related docs

### Phase 3: Example extraction
- [ ] `[example: name]` marker parser
- [ ] `{{example: path#name}}` expansion
- [ ] Test that examples compile/run

## Open Questions

- Should `--docs` be the default and `--brief` hide extended docs?
- Exact syntax for covers declarations (glob patterns? symbol names?)
- How to handle examples that need async runtime or other heavy scaffolding?
