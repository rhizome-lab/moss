# Grammar/Parser Generation Workflow

Creating parsers from examples, informal specifications, or partial documentation - when you need to parse a format but don't have a complete grammar.

## Trigger

- Need to parse custom DSL with no formal spec
- Legacy format with only examples as documentation
- Proprietary protocol with reverse-engineered samples
- Config format that "just grew" over time
- Need parser for language variant (dialect, extension)

## Goal

- Working parser that handles known inputs correctly
- Grammar that captures the actual language (not just seen examples)
- Handles edge cases gracefully (error recovery, partial parses)
- Maintainable grammar that can evolve with the format

## Prerequisites

- Examples of valid input (the more diverse, the better)
- Examples of invalid input (if available)
- Any documentation (even informal notes)
- Understanding of the domain
- Test corpus for validation

## Why This Is Hard

1. **Incomplete specification**: Examples show what's valid, not why
2. **Ambiguity**: Same syntax might parse multiple ways
3. **Edge cases**: Rare constructs not in your examples
4. **Error recovery**: Real parsers need to handle bad input
5. **Performance**: Naive grammars can be exponentially slow
6. **Evolution**: Format changes, grammar must adapt

## Parser Generator Landscape

| Tool | Grammar Type | Strengths | Use Case |
|------|-------------|-----------|----------|
| **tree-sitter** | GLR | Error recovery, incremental | Editor integration, code analysis |
| **ANTLR** | LL(*) | Rich ecosystem, multiple targets | Language tooling |
| **pest** | PEG | Rust-native, readable | Rust applications |
| **nom** | Parser combinators | Flexible, streaming | Binary formats, protocols |
| **lalrpop** | LALR(1) | Rust, good errors | Rust DSLs |
| **Lark** | Earley/LALR | Python, readable EBNF | Python tools |
| **Nearley** | Earley | Ambiguity handling | Experimental parsers |

## Core Strategy: Collect → Infer → Generate → Validate

```
┌─────────────────────────────────────────────────────────┐
│                      COLLECT                             │
│  Gather examples, documentation, domain knowledge       │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                       INFER                              │
│  Extract patterns, identify grammar rules               │
│  Hypothesis: "this looks like X → Y Z"                  │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     GENERATE                             │
│  Write grammar rules, handle ambiguity                  │
│  Iterate until examples parse correctly                 │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     VALIDATE                             │
│  Test against held-out examples                         │
│  Check edge cases, error handling                       │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Collect Evidence

### Gather Examples

```bash
# Find all files of the format
find . -name "*.custom" > corpus.txt

# Count unique patterns
cat corpus.txt | xargs grep -h "^" | sort -u > patterns.txt

# Identify structural markers
grep -E "^(begin|end|def|class)" *.custom | head -50
```

### Categorize Examples

```markdown
## Example Corpus Analysis

### Simple (minimal features)
- config_basic.custom: just key-value pairs
- empty.custom: empty file (is this valid?)

### Complex (many features)
- full_example.custom: uses all known constructs
- nested.custom: deeply nested structures

### Edge Cases
- unicode.custom: non-ASCII identifiers
- multiline.custom: strings spanning lines
- comments.custom: various comment styles

### Suspected Invalid
- broken.custom: known to cause errors
- partial.custom: incomplete file
```

### Extract Informal Spec

```markdown
## Observed Grammar Rules

### Tokens (lexical)
- Identifiers: [a-zA-Z_][a-zA-Z0-9_]*
- Numbers: [0-9]+ (decimals? negative?)
- Strings: "..." or '...' (escapes?)
- Comments: # to end of line (block comments?)

### Structure (syntactic)
- Top level: sequence of definitions
- Definition: keyword name { body }
- Body: key = value pairs? or nested definitions?
- Unclear: can definitions nest?

### Questions
- Is whitespace significant? (indentation-based?)
- Are trailing commas allowed?
- What's the expression precedence?
```

## Phase 2: Infer Grammar Rules

### Start with Tokens (Lexer)

```javascript
// tree-sitter grammar.js - lexer first
module.exports = grammar({
  name: 'custom',

  extras: $ => [
    /\s/,           // whitespace
    $.comment,
  ],

  rules: {
    // Start with tokens before structure
    comment: $ => /#.*/,

    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    number: $ => /\d+/,

    string: $ => choice(
      /"[^"]*"/,      // double-quoted (simple)
      /'[^']*'/,      // single-quoted (simple)
    ),
  }
});
```

### Build Up Structure Incrementally

```javascript
// Add one rule at a time, test each addition
rules: {
  source_file: $ => repeat($.definition),

  definition: $ => seq(
    $.keyword,
    $.identifier,
    '{',
    repeat($.statement),
    '}',
  ),

  keyword: $ => choice('def', 'class', 'module'),

  statement: $ => choice(
    $.assignment,
    $.definition,  // nesting!
  ),

  assignment: $ => seq(
    $.identifier,
    '=',
    $.expression,
  ),

  expression: $ => choice(
    $.identifier,
    $.number,
    $.string,
  ),
}
```

### Handle Ambiguity

```javascript
// When grammar is ambiguous, tree-sitter needs hints
module.exports = grammar({
  // ...

  // Precedence for expressions
  precedences: $ => [
    ['call', 'unary', 'binary'],
  ],

  // Conflict resolution
  conflicts: $ => [
    // identifier can be expression or type
    [$.expression, $.type],
  ],

  rules: {
    // Use prec() to resolve
    binary_expression: $ => prec.left('binary', seq(
      $.expression,
      choice('+', '-', '*', '/'),
      $.expression,
    )),

    call_expression: $ => prec('call', seq(
      $.expression,
      '(',
      optional($.arguments),
      ')',
    )),
  }
});
```

### PEG Alternative (pest/nom)

```pest
// pest grammar - PEG is often easier for simple formats

WHITESPACE = _{ " " | "\t" | "\n" }
COMMENT = _{ "#" ~ (!"\n" ~ ANY)* }

file = { SOI ~ definition* ~ EOI }

definition = { keyword ~ identifier ~ "{" ~ statement* ~ "}" }

keyword = { "def" | "class" | "module" }

statement = { assignment | definition }

assignment = { identifier ~ "=" ~ expression }

expression = { identifier | number | string }

identifier = @{ ASCII_ALPHA ~ ASCII_ALPHANUMERIC* }
number = @{ ASCII_DIGIT+ }
string = @{ "\"" ~ (!"\"" ~ ANY)* ~ "\"" }
```

## Phase 3: Generate Parser

### tree-sitter Workflow

```bash
# Initialize grammar
tree-sitter init-config
mkdir -p tree-sitter-custom
cd tree-sitter-custom
tree-sitter init

# Edit grammar.js

# Generate parser
tree-sitter generate

# Test with examples
tree-sitter parse ../examples/simple.custom

# Debug: see full tree
tree-sitter parse ../examples/simple.custom --debug

# Build for use
tree-sitter build-wasm  # For web
cargo build             # For Rust
```

### Test-Driven Grammar Development

```javascript
// tree-sitter test cases: test/corpus/basics.txt
==================
Simple definition
==================

def foo {
  x = 1
}

---

(source_file
  (definition
    (keyword)
    (identifier)
    (statement
      (assignment
        (identifier)
        (expression
          (number))))))

==================
Nested definitions
==================

module outer {
  class inner {
    x = 1
  }
}

---

(source_file
  (definition
    (keyword)
    (identifier)
    (statement
      (definition
        (keyword)
        (identifier)
        (statement
          (assignment
            (identifier)
            (expression
              (number))))))))
```

```bash
# Run tests
tree-sitter test

# See failures
tree-sitter test --filter "nested"
```

### Iterative Refinement

```
1. Write grammar for simplest example
2. Run: tree-sitter generate && tree-sitter parse simple.custom
3. Fix errors, repeat until parses
4. Add next example to tests
5. Extend grammar to handle it
6. Repeat until all examples pass
7. Try edge cases
8. Add error recovery
```

## Phase 4: Validation

### Hold-Out Testing

```python
# Don't test with training examples only
import random

all_examples = glob("corpus/*.custom")
random.shuffle(all_examples)

# 80/20 split
train = all_examples[:int(len(all_examples) * 0.8)]
test = all_examples[int(len(all_examples) * 0.8):]

# Develop grammar using train
# Validate with test (never seen during development)
```

### Fuzzing

```bash
# Use tree-sitter's fuzzer
tree-sitter fuzz

# Or generate random valid inputs from grammar
# (grammar-based fuzzing)
```

### Comparison with Reference

If there's an existing parser (even buggy/slow):

```python
def compare_parses(file):
    """Compare our parser with reference."""
    our_tree = our_parser.parse(file)
    ref_tree = reference_parser.parse(file)

    # Compare structure (not exact, might differ in representation)
    our_tokens = extract_tokens(our_tree)
    ref_tokens = extract_tokens(ref_tree)

    return our_tokens == ref_tokens
```

### Edge Case Checklist

```markdown
## Edge Cases to Test

### Lexical
- [ ] Empty file
- [ ] Only whitespace
- [ ] Only comments
- [ ] Unicode identifiers
- [ ] Very long lines
- [ ] Mixed line endings (CR, LF, CRLF)

### Structural
- [ ] Maximum nesting depth
- [ ] Empty blocks
- [ ] Trailing commas (if allowed)
- [ ] Missing delimiters

### Error Recovery
- [ ] Unclosed braces
- [ ] Missing semicolons
- [ ] Invalid tokens
- [ ] Partial input
```

## Common Challenges

### Left Recursion (for LL parsers)

```
// Problem: infinite loop
expression = expression '+' term | term

// Solution: convert to right recursion or iteration
expression = term ('+' term)*
```

### Keyword vs Identifier

```javascript
// Problem: "if" is both keyword and valid identifier
// Solution: explicit keyword list

keyword: $ => choice('if', 'else', 'while'),

identifier: $ => {
  const keywords = ['if', 'else', 'while'];
  return token(prec(-1, /[a-zA-Z_][a-zA-Z0-9_]*/));
},

// tree-sitter handles this with word boundary
word: $ => $.identifier,
```

### Significant Whitespace

```javascript
// Indentation-based (like Python)
// tree-sitter external scanners needed

externals: $ => [
  $._indent,
  $._dedent,
  $._newline,
],

// Implement in scanner.c
// Track indentation stack
```

### Expression Precedence

```javascript
// Define precedence levels
precedences: $ => [
  [
    'unary',      // highest
    'multiply',
    'add',
    'compare',
    'and',
    'or',         // lowest
  ],
],

rules: {
  binary_expression: $ => choice(
    prec.left('multiply', seq($.expression, choice('*', '/'), $.expression)),
    prec.left('add', seq($.expression, choice('+', '-'), $.expression)),
    prec.left('compare', seq($.expression, choice('<', '>', '=='), $.expression)),
    prec.left('and', seq($.expression, '&&', $.expression)),
    prec.left('or', seq($.expression, '||', $.expression)),
  ),
}
```

## LLM-Specific Techniques

### Grammar Inference from Examples

```
Given these valid inputs:

```
def foo { x = 1 }
def bar { x = 1; y = 2 }
class Baz { def inner { z = 3 } }
```

And these invalid inputs:

```
def { x = 1 }          # missing name
def foo x = 1          # missing braces
def foo { x = }        # missing value
```

Infer a grammar in tree-sitter format.
```

### Pattern Recognition

```
I have a configuration format. Here are examples:

```
server {
  host = "localhost"
  port = 8080
  ssl {
    enabled = true
    cert = "/path/to/cert"
  }
}
```

What grammar family does this belong to? (TOML-like? Nginx-like? HCL-like?)
What existing grammar could I adapt?
```

### Error Message Improvement

```
My parser produces this error:
"syntax error at line 5, column 12"

The input is:
```
def foo {
  x = 1
  y = [1, 2, 3
  z = 4
}
```

Generate a better error message explaining what's wrong
and suggesting a fix.
```

## Workflow for Unknown Format

### Step 1: Identify Format Family

```
Is it:
- Configuration (key-value, hierarchical)
- Data (JSON-like, XML-like, CSV-like)
- Code (expressions, statements, blocks)
- Markup (tags, attributes, content)
- Protocol (headers, body, delimiters)

Each family has common patterns to start from.
```

### Step 2: Find Closest Known Grammar

```bash
# Search tree-sitter grammars
gh search repos "tree-sitter-" --limit 100

# Look for similar formats
# HCL, TOML, INI, JSON, YAML for config
# Lua, Python, JavaScript for code
# HTML, XML, Markdown for markup
```

### Step 3: Fork and Modify

```bash
# Clone closest match
git clone https://github.com/tree-sitter/tree-sitter-toml
cd tree-sitter-toml

# Modify grammar.js incrementally
# Test against your examples
tree-sitter parse ../your-example.custom
```

### Step 4: Document Differences

```markdown
## Custom Format vs TOML

### Additions
- Nested blocks without tables
- Expression syntax in values

### Removals
- Array of tables
- Inline tables

### Changes
- Comments use # not //
- Strings use " only, not '
```

## Tools Reference

| Tool | Purpose |
|------|---------|
| tree-sitter CLI | Generate, test, debug grammars |
| tree-sitter playground | Visual grammar testing |
| ANTLR Lab | ANTLR grammar testing |
| PEG.js online | PEG grammar playground |
| railroad diagrams | Visualize grammar rules |
| grammarinator | Grammar-based fuzzing |

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Overfitted grammar | Fails on new examples | Add more training examples |
| Ambiguous grammar | Parser conflicts | Add precedence, resolve conflicts |
| Slow parsing | Timeouts on complex input | Check for exponential backtracking |
| Missing error recovery | Crashes on bad input | Add ERROR nodes, external scanner |

## Anti-patterns

- **Grammar from spec only**: Real usage differs from spec
- **No negative examples**: Can't tell if grammar is too permissive
- **Testing only happy path**: Need error cases too
- **Perfect grammar first**: Iterate - working > complete
- **Ignoring parser generator choice**: Wrong tool = wrong tradeoffs

## Open Questions

### Grammar Inference Automation

Can we automatically infer grammars from examples?
- Token patterns: regex inference works okay
- Structure: hard without semantic hints
- Research: grammar induction, program synthesis

### Format Evolution

How to handle format changes?
- Versioned grammars?
- Feature flags in grammar?
- Multiple parsers?

### Semantic vs Syntactic

Parser validates syntax, not semantics:
- `x = "not a number"` parses but might be wrong
- Where to put semantic checks?
- Can grammar express some semantics?

## See Also

- [Binding Generation](binding-generation.md) - Another code synthesis workflow
- [Reverse Engineering Code](reverse-engineering-code.md) - Understanding unknown formats
- [Code Synthesis](code-synthesis.md) - D×C verification applies

