# moss update

Self-update moss to the latest version.

## Usage

```bash
moss update
```

## Process

1. Checks GitHub releases for latest version
2. Downloads appropriate binary for platform
3. Verifies SHA256 checksum
4. Replaces current binary

## Options

- `--check` - Only check for updates, don't install
- `--force` - Update even if already on latest
