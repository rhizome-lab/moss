# moss init

Initialize moss in a project directory.

## Usage

```bash
moss init [--index]
```

## What It Does

1. Creates `.moss/` directory
2. Creates `.moss/config.toml` with defaults
3. Detects TODO files (TODO.md, TASKS.md, etc.) and adds to aliases
4. Updates `.gitignore` with moss entries

## Options

- `--index` - Also build the file index after init

## Generated Files

### .moss/config.toml

```toml
# Moss configuration
# See: https://github.com/pterror/moss

[daemon]
# enabled = true
# auto_start = true

[analyze]
# clones = true

# [analyze.weights]
# health = 1.0
# complexity = 0.5
# security = 2.0
# clones = 0.3

[aliases]
todo = ["TODO.md"]  # If TODO.md exists
```

### .gitignore Entries

```gitignore
# Moss - ignore .moss/ in subdirectories entirely
**/.moss/

# Root .moss/ - ignore all but config/allow files
/.moss/*
!/.moss/config.toml
!/.moss/duplicate-functions-allow
!/.moss/duplicate-types-allow
!/.moss/hotspots-allow
!/.moss/large-files-allow
```

## Idempotent

Running `moss init` multiple times is safe:
- Skips existing files
- Only adds missing gitignore entries
- Reports what was created/skipped
