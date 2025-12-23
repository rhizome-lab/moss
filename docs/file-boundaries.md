# File Boundaries Don't Exist

Design doc for seamless cross-file context in code editing.

## Problem

When editing a file, users manually specify context:
- "Add X to context"
- "Look at module Y"
- "Check the import for Z"

This is friction. The file's imports already define what's relevant.

## Insight

Imports ARE context. When editing `api.py` that imports from `models.py`:
```python
from .models import User, Session
```

The agent should automatically understand:
- `User` and `Session` class signatures
- Their methods and attributes
- Related types they reference

No explicit "add to context" step needed.

## Current State

We already have:
1. `dependencies.extract_dependencies()` - extracts imports/exports
2. `skeleton.extract_python_skeleton()` - gets class/function signatures
3. `views.fisheye()` - relative imports resolve to paths

Missing piece: chaining these together automatically.

## Design

### Phase 1: Implicit Import Context ✓ IMPLEMENTED

**Implementation:**
- `expand_import_context(file_path, root)` in `moss_intelligence.dependencies`
- `ViewOptions(expand_imports=True, project_root=...)` in skeleton provider
- Tests in `test_dependencies.py` and `test_skeleton.py`

When `view` or `edit` is called on a file:

1. Extract imports via `dependencies`
2. For each imported symbol:
   - Resolve to source file (via fisheye or path resolution)
   - Extract skeleton of that symbol only
3. Include in context as "Imported Types"

Example context output:
```
# file: api.py
from .models import User
from .auth import verify_token

# Imported Types:
# models.py:User
class User:
    id: int
    email: str
    def validate(self) -> bool: ...

# auth.py:verify_token
def verify_token(token: str) -> User | None: ...
```

### Phase 2: Available Modules Summary ✓ IMPLEMENTED

**Implementation:**
- `get_available_modules(file_path, root)` in `moss_intelligence.dependencies`
- `ViewOptions(show_available=True, project_root=...)` in skeleton provider
- Tests in `test_dependencies.py`

**Problem**: Agent doesn't know what exists in the codebase. It might:
- Reimplement something that already exists
- Miss an obvious utility function
- Create duplicate patterns

**Solution**: Show "Available Modules" summary BEFORE the agent writes code.

When editing `api.py`, include a summary of sibling modules:
```
# Available in this package:
# cache.py: CacheManager, cache_key(), invalidate()
# models.py: User, Session, Token
# auth.py: verify_token(), create_token(), hash_password()
# utils.py: retry(), timeout(), log_call()
```

This is NOT full skeletons - just module → exported symbols mapping.
Agent knows what's available, can choose to import instead of reimplement.

**Implementation**: Use `dependencies.extract_dependencies()` on sibling files to get exports, format as compact list.

### Phase 3: Transitive Context (Limited Depth) ✓ IMPLEMENTED

**Implementation:**
- `expand_import_context(file_path, root, depth=N)` follows N levels
- `ViewOptions(import_depth=2)` in skeleton provider
- Cycle detection prevents infinite loops
- Tests in `test_dependencies.py`

If `User` references `Address`:
```python
class User:
    address: Address  # from .address import Address
```

Optionally include `Address` skeleton too (depth=2).

Limit depth to avoid context explosion. Default: depth=1 (direct imports only).

## Implementation

### New Function: `expand_import_context()`

```python
def expand_import_context(
    file_path: Path,
    root: Path,
    depth: int = 1
) -> dict[str, str]:
    """Expand imports to skeleton context.

    Returns:
        Dict mapping "module:symbol" to skeleton text
    """
    deps = extract_dependencies(file_path)
    context = {}

    for imp in deps.imports:
        resolved = resolve_import(imp, file_path, root)
        if resolved:
            skeleton = format_skeleton(resolved.file, symbols=[imp.name])
            context[f"{imp.module}:{imp.name}"] = skeleton

    return context
```

### Integration Points

1. **view primitive**: Add `--expand-imports` flag
2. **edit primitive**: Always expand (LLM needs full context)
3. **MCP tools**: `skeleton_format_skeleton` gains `expand_imports` param

## Token Budget

Import expansion increases token usage. Mitigations:

1. **Symbol-level filtering**: Only include the specific imported symbols, not full files
2. **Signature-only mode**: For depth>1, use signatures only (no docstrings)
3. **Budget parameter**: `max_import_tokens=2000` limits expansion

## Open Questions

1. Should we cache import resolution? (Probably yes, via daemon)
2. Handle star imports (`from .models import *`)? (Probably expand to explicit list)
3. Handle conditional imports inside functions? (Probably ignore)

## Non-Goals

- Full type inference (too expensive)
- Runtime behavior analysis (static only)
- External package internals (only project-local imports)
