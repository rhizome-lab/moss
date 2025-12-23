# Dogfooding Findings

Systematic documentation of issues, patterns, and improvements discovered while using moss on itself.

## View Primitive

### What Works Well

**Fuzzy Path Resolution**
- `view framework.py` correctly finds `src/moss/synthesis/framework.py`
- Underscore/hyphen equivalence works (`moss-api` matches `moss_api`)
- Symbol paths work (`view cli.py/cmd_workflow`)

**--types-only Flag**
- Shows only type definitions (class, struct, enum, interface, trait, impl)
- Strips methods/functions for architectural overview
- Works for Python and Rust
- Output is compact and scannable

**--fisheye Mode**
- Resolves Python imports (relative and absolute) to local files
- Shows imported module skeletons at signature level
- Combines with `--types-only` for types-only fisheye view
- Useful for understanding a file's dependencies at a glance

**Combined Flags**
- `--types-only --fisheye` gives a compact architectural map
- Shows the file's types plus all imported module types
- Ideal for understanding module boundaries

### Gaps and Issues

**Language Support for Fisheye**
- Only Python imports are resolved
- Rust `use` statements not resolved
- TypeScript/JavaScript `import` not resolved
- Would be useful for Rust and TypeScript codebases

**Import Resolution Edge Cases**
- Third-party imports (e.g., `from textual import App`) are silently ignored
- Could show a note like "external: textual, rich, ..."
- Relative imports from parent directories (e.g., `from ...` ) need testing

**Missing --resolve-imports**
- Lower priority since fisheye covers most cases
- Would inline imported symbol signatures into the view
- Useful when you want to see the actual type definitions inline

### UX Observations

- Output format is consistent and readable
- Line numbers included by default (helpful for navigation)
- Depth parameter works but rarely needed with types-only/fisheye
- JSON output works for programmatic access

## Analyze Primitive

### What Works Well

**--health Flag**
- Codebase-wide metrics (files, lines, languages)
- Complexity summary (avg, max, high-risk count)
- Large file detection (>500 lines)
- Grade/score system (A-F)

**--complexity Flag**
- Per-function cyclomatic complexity
- Risk level categorization (low, medium, high, very-high)
- Symbol filtering works (`analyze cli.py/cmd_workflow --complexity`)
- Threshold filtering (`-t 10` shows only functions above 10)

**--security Flag**
- Integrates with bandit (when installed)
- Shows findings by severity
- Tools skipped section shows what's not installed

### Gaps and Issues

**Codebase-Wide Complexity**
- `analyze --complexity` now works without file target
- Shows top 10 most complex functions across entire codebase
- Uses parallel file scanning (rayon) for speed
- Threshold filter (`-t 15`) still works
- For specific file: `analyze <file> --complexity` shows all functions in that file

**Security Tool Integration**
- Only bandit integrated
- Could add: semgrep, cargo-audit, npm audit
- Currently shows "tools skipped" which is helpful

### UX Observations

- Output is structured and scannable
- JSON output works for CI/CD integration
- Target resolution uses same fuzzy matching as view
- Profile flag shows timing (useful for debugging)

## Edit Primitive

### What Works Well

**Analyze-Only Mode**
- `edit -f file.py -s symbol "task" --analyze-only` shows complexity assessment
- Useful to understand what kind of edit will be attempted
- Shows: simple (structural), medium, complex (LLM)

**Dry-Run Mode**
- Shows what would change without applying
- Useful for reviewing before committing

### Gaps and Issues

**Limited Testing**
- Most edit operations are LLM-routed
- Structural editing is stubs for now
- Need more dogfooding on actual edit tasks

## Workflow System

### What Works Well

**Workflow Discovery**
- `workflow list` shows available workflows
- `workflow show <name>` shows details
- Searches both builtin and project `.moss/workflows/`

**DWIM Workflow**
- `agent "task"` works as expected
- System prompt mentions view/edit/analyze flags
- Mock mode for testing (`--mock`)

**State Machine Workflows**
- `validate-fix.toml` demonstrates analyze→fix→verify loop
- Condition system works (has_errors, success, etc.)
- Parallel states and nested workflows supported

### Gaps and Issues

**Path Resolution Bug (Fixed)**
- After cli/ package split, workflows weren't found
- Was looking in `cli/workflows` instead of `moss/workflows`
- Fixed by updating path to `.parent.parent`

## TUI (Explore)

### What Works Well

**Tree Navigation**
- Uses Rust index for file listing (fast)
- Goto (g key, Ctrl+P palette) works well
- Collapse/expand with arrow keys

**Command Palette**
- Ctrl+P opens palette
- Goto File, View, Analyze commands available
- Fuzzy search for symbols

**Modal Keybinds**
- Different modes have different keybinds
- Status bar shows current mode
- v/e/a for View/Edit/Analyze

### Gaps and Issues

**Theme Toggle Removed**
- T keybind was wasteful (only light/dark toggle)
- Removed in favor of command palette

**Mode Indicator**
- Current mode shown in status bar
- Could be more prominent (bottom right)

## CLI General

### What Works Well

**Global Options**
- `--json` for structured output
- `--compact` for minimal output
- `--profile` for timing info
- `--quiet` for less noise

**Error Messages**
- Generally helpful
- Path resolution errors show what was tried
- Skipped analyses now shown

### Gaps and Issues

**Help Text**
- Some commands have minimal help
- Examples would be helpful

## Patterns Discovered

### Token Efficiency
1. Use `--types-only` for architectural overview
2. Use `--fisheye` when understanding a file's dependencies
3. Combine both for maximum efficiency
4. Use `analyze --health` before diving into specifics

### Investigation Flow
1. `view .` - get tree structure
2. `view <file> --types-only --fisheye` - understand architecture
3. `analyze <file> --complexity` - find complex areas
4. `view <file>/<symbol>` - drill into specific symbols

### Common Mistakes
1. Forgetting to specify file for `--complexity`
2. Expecting fisheye to work for Rust/TypeScript imports
3. Running `workflow list` before installing workflows

## Future Improvements

### High Priority
- Fisheye for Rust/TypeScript imports
- `--resolve-imports` for inline expansion
- `--visibility public|all` filter

### Medium Priority
- Barrel file hoisting for TypeScript
- Useless docstring detection
- Selective import resolution (`--fisheye=module`)

### Low Priority
- Third-party import acknowledgment
- Cross-language import tracking
