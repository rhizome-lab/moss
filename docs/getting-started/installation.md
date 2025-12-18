# Installation

## Prerequisites

- Python 3.13+
- Git

## Using Nix (Recommended)

Moss uses Nix flakes for reproducible development environments:

```bash
# Clone the repository
git clone https://github.com/pterror/moss
cd moss

# Enter the development shell (automatic with direnv)
nix develop

# Or if using direnv
direnv allow
```

The Nix shell provides: Python 3.13, uv, ruff, ripgrep, and all dependencies.

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
