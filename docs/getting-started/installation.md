# Installation

## Quick Install (Recommended)

Download the pre-built binary for your platform:

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/pterror/moss/master/install.sh | bash
```

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/pterror/moss/master/install.ps1 | iex
```

Or download manually from [GitHub Releases](https://github.com/pterror/moss/releases).

After install, update anytime with:
```bash
moss update
```

## From Source

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- Git

### Build

```bash
# Clone the repository
git clone https://github.com/pterror/moss
cd moss

# Build release binary
cargo build --release -p moss-cli

# Install to PATH
sudo cp target/release/moss /usr/local/bin/
```

### Development

```bash
# Run tests
cargo test --workspace

# Run with verbose output
cargo run -p moss-cli -- --help

# Check code
cargo fmt --check
cargo clippy
```

## Verify Installation

```bash
# Check CLI is available
moss --help

# Check version
moss --version

# Run on current directory
moss view .
```

## Updating

The CLI can update itself:

```bash
# Check for updates
moss update --check

# Install update
moss update
```

## Uninstall

Remove the binary:

```bash
# Linux / macOS
sudo rm /usr/local/bin/moss

# Windows (PowerShell)
Remove-Item "$env:LOCALAPPDATA\moss" -Recurse
# Also remove from PATH in System Properties > Environment Variables
```
