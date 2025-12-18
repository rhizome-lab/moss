# Plugin System

Moss uses Python entry points for plugin discovery and registration.

## Plugin Types

| Plugin Type | Entry Point Group | Purpose |
|-------------|-------------------|---------|
| View Provider | `moss.plugins` | Extract structural info from code |
| Generator | `moss.synthesis.generators` | Generate code |
| Validator | `moss.synthesis.validators` | Validate generated code |
| Strategy | `moss.synthesis.strategies` | Decompose problems |
| Library | `moss.synthesis.libraries` | Store/retrieve abstractions |

## Creating a Plugin

### 1. Implement the Protocol

```python
# my_plugin/generator.py
from moss.synthesis.plugins import CodeGenerator, GeneratorMetadata, GeneratorType

class MyGenerator(CodeGenerator):
    @property
    def metadata(self) -> GeneratorMetadata:
        return GeneratorMetadata(
            name="my_generator",
            description="My custom generator",
            generator_type=GeneratorType.CUSTOM,
            priority=15,  # Higher = preferred
        )

    def can_generate(self, spec, context) -> bool:
        return "my_keyword" in spec.description

    async def generate(self, spec, context, hints=None):
        code = f"# Generated for: {spec.description}\npass"
        return GenerationResult(success=True, code=code, confidence=0.5)

    def estimate_cost(self, spec, context):
        return GenerationCost(time_estimate_ms=10, token_estimate=0)
```

### 2. Register via Entry Points

```toml
# pyproject.toml
[project.entry-points."moss.synthesis.generators"]
my_generator = "my_plugin.generator:MyGenerator"
```

### 3. Install and Use

```bash
pip install -e .
moss synthesize "my_keyword task"  # Will use MyGenerator
```

## View Provider Plugins

View providers extract structural information:

```python
from moss.plugins import ViewPlugin, ViewResult

class MyViewPlugin(ViewPlugin):
    name = "my_view"
    file_patterns = ["*.py"]

    async def extract(self, path: Path) -> ViewResult:
        content = path.read_text()
        # Extract structure...
        return ViewResult(
            summary="...",
            details={"key": "value"}
        )
```

## Strategy Plugins

Decomposition strategies break problems into subproblems:

```python
from moss.synthesis.strategy import DecompositionStrategy, StrategyMetadata

class MyStrategy(DecompositionStrategy):
    @property
    def metadata(self) -> StrategyMetadata:
        return StrategyMetadata(
            name="my_strategy",
            description="Custom decomposition",
            keywords=("custom", "special"),
        )

    def can_handle(self, spec, context) -> bool:
        return "special" in spec.description

    def decompose(self, spec, context) -> list[Subproblem]:
        return [
            Subproblem(
                specification=Specification(description="Step 1"),
                priority=0,
            ),
            Subproblem(
                specification=Specification(description="Step 2"),
                dependencies=(0,),
                priority=1,
            ),
        ]

    def estimate_success(self, spec, context) -> float:
        return 0.8
```

## Plugin Discovery

Plugins are discovered at runtime:

```python
from moss.synthesis.plugins import get_synthesis_registry

registry = get_synthesis_registry()

# List all generators
for gen in registry.generators.get_all():
    print(f"{gen.metadata.name}: {gen.metadata.description}")

# Find best generator for a spec
best = registry.generators.find_best(spec, context)
```

## Configuration

Enable/disable plugins via `moss.toml`:

```toml
[synthesis.generators]
enabled = ["template", "llm"]
disabled = ["placeholder"]

[synthesis.strategies]
enabled = ["type_driven", "pattern_based"]
```
