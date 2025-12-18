# Code Generators

Generators produce code for atomic problems that can't be decomposed further.

## Built-in Generators

### PlaceholderGenerator

Generates TODO stubs as fallback.

- **Priority:** -100 (lowest)
- **Use case:** Fallback when no other generator applies

```python
# Output example
def generated_function(*args, **kwargs):
    """TODO: implement

    Description: Sort a list of numbers
    Type: List[int] -> List[int]
    """
    raise NotImplementedError()
```

### TemplateGenerator

Uses pattern templates for common code structures.

- **Priority:** 10
- **Use case:** CRUD operations, validation, transformations

**Available templates:**

```
crud/create    - Create operation
crud/read      - Read by ID
crud/update    - Update operation
crud/delete    - Delete operation
function/pure  - Pure function scaffold
function/async - Async function scaffold
transform/map  - Map transformation
transform/filter - Filter transformation
validation/required - Required field check
validation/range - Range validation
```

**Adding custom templates:**

```toml
# moss.toml
[synthesis.generators.template]
dirs = ["templates/", "~/.moss/templates/"]
```

```python
# templates/custom/my_template.py.tmpl
def ${name}(${params}):
    """${description}"""
    # TODO: implement
    pass
```

### LLMGenerator

Uses language models for code generation.

- **Priority:** 20 (highest)
- **Use case:** Complex logic, natural language specs

**Configuration:**

```toml
# moss.toml
[synthesis.llm]
provider = "anthropic"  # or "openai", "ollama"
model = "claude-sonnet-4-20250514"
temperature = 0.2
max_tokens = 2048
```

**Environment variables:**

```bash
export ANTHROPIC_API_KEY="sk-..."
# or
export OPENAI_API_KEY="sk-..."
```

**Mock provider for testing:**

```python
from moss.synthesis.plugins.generators.llm import LLMGenerator, MockLLMProvider

generator = LLMGenerator(provider=MockLLMProvider())
```

## Generator Selection

Generators are tried in priority order:

```python
# Selection logic
for generator in sorted(generators, key=lambda g: -g.metadata.priority):
    if generator.can_generate(spec, context):
        result = await generator.generate(spec, context, hints)
        if result.success:
            return result

# Fallback to placeholder
return placeholder_generator.generate(spec, context)
```

## Generation Hints

Hints guide generation:

```python
from moss.synthesis.plugins import GenerationHints

hints = GenerationHints(
    preferred_style="crud/create",      # Template preference
    abstractions=[existing_funcs],       # Available functions
    examples=[(input, output), ...],     # I/O examples
    constraints=["must be pure", ...],   # Requirements
)

result = await generator.generate(spec, context, hints)
```

## Creating Custom Generators

```python
from moss.synthesis.plugins import (
    CodeGenerator,
    GeneratorMetadata,
    GeneratorType,
    GenerationResult,
    GenerationCost,
)

class MyGenerator(CodeGenerator):
    @property
    def metadata(self) -> GeneratorMetadata:
        return GeneratorMetadata(
            name="my_generator",
            description="Custom code generator",
            generator_type=GeneratorType.CUSTOM,
            priority=15,
        )

    def can_generate(self, spec, context) -> bool:
        # Check if this generator applies
        return "my_domain" in spec.description

    async def generate(self, spec, context, hints=None):
        # Generate code
        code = f"def solution(): pass  # {spec.description}"
        return GenerationResult(
            success=True,
            code=code,
            confidence=0.7,
            metadata={"generator": "my_generator"},
        )

    def estimate_cost(self, spec, context):
        return GenerationCost(
            time_estimate_ms=100,
            token_estimate=500,
            complexity_score=0.5,
        )
```

## Validation Retry

When validation fails, generators receive feedback:

```python
# After validation failure
hints = GenerationHints(
    constraints=[
        "Fix: NameError: 'undefined_var' is not defined",
        "Fix: Expected return type int, got str",
    ]
)

# Generator attempts to fix issues
result = await generator.generate(spec, context, hints)
```

## Future Generators

See [Roadmap](roadmap.md) for planned non-LLM generators:

- **EnumerativeGenerator** - AST enumeration
- **SMTGenerator** - Z3-based synthesis
- **ComponentGenerator** - Library composition
- **PBEGenerator** - Programming by example
