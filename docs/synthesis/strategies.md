# Decomposition Strategies

Strategies determine how complex problems are broken into subproblems.

## Built-in Strategies

### TypeDrivenDecomposition

Uses type signatures to guide decomposition.

**Best for:** Strongly-typed transformations, collection operations

**How it works:**

1. Parse type signature (e.g., `List[int] -> List[str]`)
2. Identify transformation pattern
3. Decompose based on type structure

```bash
moss synthesize "Convert list of ints to strings" \
    --type "List[int] -> List[str]" --dry-run
```

```
Decomposition (2 subproblems):
  0. Transform element from int to str
  1. Apply transformation to collection [deps: (0,)]
```

**Decomposition patterns:**

| Input → Output | Strategy |
|----------------|----------|
| `List[A] → List[B]` | Element transform + map |
| `Tuple[A, B] → C` | Component extraction |
| `A → B` (complex types) | Intermediate conversion |

### TestDrivenDecomposition

Analyzes test cases to identify subproblems.

**Best for:** Specs with comprehensive test suites

**How it works:**

1. Extract test information (name, operations, assertions)
2. Categorize tests (happy path, error handling, edge cases)
3. Cluster related tests
4. Create subproblems per cluster

```python
spec = Specification(
    description="Implement calculator",
    tests=(
        "def test_add(): assert calc.add(2, 3) == 5",
        "def test_divide_zero(): pytest.raises(ZeroDivisionError)",
        "def test_negative(): assert calc.add(-1, 1) == 0",
    ),
)
```

**Test categories:**

- `happy_path` - Normal successful operations
- `error_handling` - Exception cases
- `edge_case` - Boundary conditions
- `validation` - Input validation

### PatternBasedDecomposition

Recognizes common patterns and applies templates.

**Best for:** Standard architectures (CRUD, auth, ETL)

**Built-in patterns:**

| Pattern | Keywords | Subproblems |
|---------|----------|-------------|
| `crud_api` | crud, rest, api | Create, Read, List, Update, Delete |
| `authentication` | auth, login, password | Lookup, Validate, Token, Response |
| `etl_pipeline` | etl, extract, transform | Extract, Transform, Validate, Load |
| `validation` | validate, check | Type, Rules, Messages, Aggregate |
| `search` | search, filter, query | Parse, Filter, Rank, Paginate |
| `caching` | cache, memoize | Key, Lookup, Store, Invalidate |

```bash
moss synthesize "Build user authentication" --dry-run
```

```
Decomposition (4 subproblems):
  0. Implement user lookup by username
  1. Implement password validation [deps: (0,)]
  2. Implement session/token generation [deps: (1,)]
  3. Implement authentication response [deps: (2,)]
```

## Creating Custom Strategies

```python
from moss.synthesis.strategy import DecompositionStrategy, StrategyMetadata
from moss.synthesis.types import Specification, Context, Subproblem

class DomainStrategy(DecompositionStrategy):
    @property
    def metadata(self) -> StrategyMetadata:
        return StrategyMetadata(
            name="domain_specific",
            description="Domain-specific decomposition",
            keywords=("domain", "specific"),
        )

    def can_handle(self, spec: Specification, context: Context) -> bool:
        return "domain_keyword" in spec.description.lower()

    def decompose(
        self, spec: Specification, context: Context
    ) -> list[Subproblem]:
        return [
            Subproblem(
                specification=Specification(description="Domain step 1"),
                priority=0,
            ),
            Subproblem(
                specification=Specification(description="Domain step 2"),
                dependencies=(0,),
                priority=1,
            ),
        ]

    def estimate_success(
        self, spec: Specification, context: Context
    ) -> float:
        return 0.85  # High confidence for domain problems
```

Register via entry points:

```toml
[project.entry-points."moss.synthesis.strategies"]
domain = "my_package:DomainStrategy"
```

## Strategy Selection

The router uses TF-IDF similarity and success history:

```python
# Selection algorithm
for strategy in strategies:
    if not strategy.can_handle(spec, context):
        continue

    score = (
        tfidf_similarity(spec, strategy.keywords) * 0.4 +
        strategy.estimate_success(spec, context) * 0.3 +
        historical_success_rate(strategy) * 0.3
    )
    candidates.append((strategy, score))

return max(candidates, key=lambda x: x[1])
```

## Configuration

```toml
[synthesis.strategies]
enabled = ["type_driven", "pattern_based", "test_driven"]
disabled = []

# Custom patterns for pattern_based
[synthesis.patterns.my_pattern]
keywords = ["custom", "pattern"]
template = ["Step 1", "Step 2", "Step 3"]
```
