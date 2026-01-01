# moss daemon

Manage the background daemon for faster operations.

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `start` | Start the daemon |
| `stop` | Stop the daemon |
| `status` | Check daemon status |
| `restart` | Restart the daemon |

## Examples

```bash
moss daemon start
moss daemon status
moss daemon stop
```

## Purpose

The daemon provides:
- Persistent grammar cache (faster parsing)
- File watching for index updates
- Reduced startup overhead for repeated commands

## Config

In `.moss/config.toml`:

```toml
[daemon]
enabled = true      # Enable daemon
auto_start = true   # Start automatically when needed
```
