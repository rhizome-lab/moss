# Agent Commands Reference

Complete list of commands available to the moss agent.

## Exploration

```
$(view .)                          # View current directory structure
$(view <path>)                     # View file or directory
$(view <path/Symbol>)              # View specific symbol (function, class, etc.)
$(view --types-only <path>)        # Show only type definitions
$(view --deps <path>)              # Show dependencies/imports
$(view <path>:<start>-<end>)       # View specific line range
$(text-search "<pattern>")         # Search for text pattern
$(text-search "<pattern>" --only <glob>)  # Search in specific files
```

## Analysis

```
$(analyze complexity)              # Find complex functions
$(analyze callers <symbol>)        # Show what calls this symbol
$(analyze callees <symbol>)        # Show what this symbol calls
$(analyze hotspots)                # Git history hotspots
```

## Package Management

```
$(package list)                    # List declared dependencies
$(package tree)                    # Show dependency tree
$(package outdated)                # Show outdated packages
$(package audit)                   # Check for vulnerabilities
```

## Editing

```
$(edit <path/Symbol> delete)       # Delete a symbol
$(edit <path/Symbol> replace <code>)  # Replace symbol with code
$(edit <path/Symbol> insert --before <code>)  # Insert before symbol
$(edit <path/Symbol> insert --after <code>)   # Insert after symbol
$(batch-edit <t1> <a1> <c1> | <t2> <a2> <c2>)  # Multiple edits
```

## Shell

```
$(run <shell command>)             # Execute shell command
```

## Memory Management

```
$(note <finding>)                  # Record a finding for this session
$(keep)                            # Keep all outputs in working memory
$(keep 1 3)                        # Keep specific outputs by index
$(drop <id>)                       # Remove item from working memory
$(memorize <fact>)                 # Save to long-term memory (persists)
$(forget <pattern>)                # Remove notes matching pattern
```

## Session Control

```
$(checkpoint <progress> | <questions>)  # Save session for later
$(ask <question>)                  # Ask user for input
$(done <answer>)                   # End session with answer
```

## Notes

- Outputs disappear after each turn unless you `$(keep)` or `$(note)` them
- Use `$(note)` to record findings as you discover them
- `$(done)` should cite evidence from command outputs
