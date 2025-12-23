# Installation

## Quick Install (CLI Binary)

The fastest way to get the `moss` CLI:

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

## From Source (Development)

### Prerequisites

- Python 3.13+
- Rust (for CLI)
- Git
- [uv](https://docs.astral.sh/uv/) (recommended) or pip

### Using uv (Recommended)

```bash
# Clone the repository
git clone https://github.com/pterror/moss
cd moss

# Install dependencies
uv sync --extra all --extra dev
```

## Using pip

```bash
# Clone the repository
git clone https://github.com/pterror/moss
cd moss

# Create virtual environment
python -m venv .venv
source .venv/bin/activate

# Install in development mode
pip install -e ".[dev]"
```

## Optional Dependencies

Install additional features as needed:

```bash
# Documentation tools
pip install -e ".[docs]"

# LLM integration
pip install -e ".[llm]"

# Tree-sitter parsing
pip install -e ".[tree-sitter]"

# All optional dependencies
pip install -e ".[dev,docs,llm,tree-sitter]"
```

## Verify Installation

```bash
# Check CLI is available
moss --help

# Run tests
pytest

# Check code quality
ruff check
ruff format --check
```

## Building the Rust CLI

The `moss` CLI is written in Rust for performance. To build from source:

```bash
# Install Rust if needed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build release binary
cargo build --release -p moss-cli

# Install to PATH
sudo cp target/release/moss /usr/local/bin/
```

## Editor Setup

### VS Code

Install the Python extension and configure:

```json
{
    "python.defaultInterpreterPath": ".venv/bin/python",
    "python.formatting.provider": "none",
    "[python]": {
        "editor.defaultFormatter": "charliermarsh.ruff",
        "editor.formatOnSave": true
    }
}
```

### Neovim

With nvim-lspconfig:

```lua
require('lspconfig').ruff.setup{}
require('lspconfig').pyright.setup{}
```
