# Synthesis Generator Roadmap

This document tracks alternative code generation approaches for moss synthesis that don't rely on LLMs.

## Current Generators

| Generator | Status | Description |
|-----------|--------|-------------|
| `PlaceholderGenerator` | Done | Generates TODO stubs |
| `TemplateGenerator` | Done | Pattern-based scaffolding |
| `LLMGenerator` | Done | LLM-based (currently MockLLMProvider) |

## Planned Generators (Non-LLM)

### High Priority

#### EnumerativeGenerator
- **Status**: TODO
- **Approach**: Enumerate ASTs bottom-up, test against I/O examples
- **Prior Art**: Escher, Myth, λ²
- **Best For**: Small DSLs, clear input/output examples
- **Complexity**: Medium
- **Notes**: Good baseline, simple to implement

#### ComponentGenerator
- **Status**: TODO
- **Approach**: Combine library/API functions to reach goal type
- **Prior Art**: SyPet, InSynth, Hoogle+
- **Best For**: API composition, type-directed search
- **Complexity**: Medium
- **Notes**: Practical for real codebases with existing libraries

#### SMTGenerator
- **Status**: TODO
- **Approach**: Encode types as constraints, use Z3 to solve
- **Prior Art**: Synquid, Leon
- **Best For**: Strongly-typed pure functions, refinement types
- **Complexity**: High
- **Dependencies**: z3-solver
- **Notes**: Powerful but requires rich type annotations

### Medium Priority

#### PBEGenerator (Programming by Example)
- **Status**: TODO
- **Approach**: Learn programs from input→output examples
- **Prior Art**: FlashFill, PROSE, BlinkFill
- **Best For**: String transformations, data wrangling
- **Complexity**: Medium-High
- **Notes**: Microsoft PROSE SDK could be referenced

#### SketchGenerator
- **Status**: TODO
- **Approach**: User provides template with holes (`??`), solver fills them
- **Prior Art**: Sketch, Rosette
- **Best For**: Domain experts who know structure but not details
- **Complexity**: High
- **Notes**: Could integrate with SMTGenerator

#### RelationalGenerator
- **Status**: TODO
- **Approach**: Logic programming, run programs "backwards"
- **Prior Art**: miniKanren, Barliman
- **Best For**: Parsers, interpreters, bidirectional transformations
- **Complexity**: High
- **Notes**: Interesting for generating inverses

### Lower Priority / Research

#### GeneticGenerator
- **Status**: TODO
- **Approach**: Evolutionary search over program space
- **Prior Art**: PushGP, Grammatical Evolution
- **Best For**: Numeric optimization, approximate solutions
- **Complexity**: Medium
- **Notes**: Less precise but handles noisy specs

#### NeuralGuidedGenerator
- **Status**: TODO
- **Approach**: Small neural model guides enumeration (not generates)
- **Prior Art**: DeepCoder, RobustFill
- **Best For**: Speeding up enumerative search
- **Complexity**: High
- **Notes**: Lighter than full LLM, trainable on domain

## Planned Strategies

#### BidirectionalStrategy
- **Status**: TODO
- **Approach**: Use types AND examples together to prune search
- **Prior Art**: λ², Myth
- **Best For**: When both type signature and examples are available
- **Notes**: Extends existing TypeDrivenDecomposition

## Library Learning (DreamCoder-style)

#### Abstraction Learning
- **Status**: Partial (LearnedAbstractionLibrary exists)
- **Approach**: Compress successful solutions into reusable abstractions
- **Prior Art**: DreamCoder, EC²
- **Current**: Frequency-based learning implemented
- **TODO**: Compression-based learning (anti-unification)

## Implementation Notes

### Generator Selection
The framework selects generators by priority. To add a new generator:

1. Implement `CodeGenerator` protocol
2. Set appropriate `priority` in metadata
3. Implement `can_generate()` to check prerequisites (e.g., SMT needs types)
4. Register via entry points or `register_builtins()`

### Recommended Implementation Order

1. **EnumerativeGenerator** - Simple baseline, validates framework
2. **ComponentGenerator** - Practical value for real codebases
3. **SMTGenerator** - Powerful for typed domains
4. **PBEGenerator** - High value for data tasks
5. **Others** - Based on need

## References

- [Synquid](https://bitbucket.org/nadiapolikarpova/synquid) - Type-driven synthesis
- [miniKanren](http://minikanren.org/) - Relational programming
- [DreamCoder](https://github.com/ellisk42/ec) - Library learning
- [PROSE](https://microsoft.github.io/prose/) - Microsoft PBE framework
- [Sketch](https://people.csail.mit.edu/asolar/sketch-1.7.6/sketch-manual.pdf) - Sketch-based synthesis
- [λ²](https://dl.acm.org/doi/10.1145/2837614.2837649) - Bidirectional type-directed synthesis
