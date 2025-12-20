"""Prompt loading with user override support.

Prompts are loaded from:
1. .moss/prompts/{name}.txt (user override)
2. src/moss/prompts/{name}.txt (built-in)

Plain text files only for now. TOML metadata support can be added later.
"""

from pathlib import Path

# Package directory for built-in prompts
_BUILTIN_DIR = Path(__file__).parent


def load_prompt(name: str, project_root: Path | None = None) -> str:
    """Load a prompt by name, checking user overrides first.

    Args:
        name: Prompt name (without .txt extension)
        project_root: Project root for .moss/ lookup. Defaults to cwd.

    Returns:
        Prompt text content.

    Raises:
        FileNotFoundError: If prompt not found in any location.
    """
    if project_root is None:
        project_root = Path.cwd()

    # Check user override first
    user_path = project_root / ".moss" / "prompts" / f"{name}.txt"
    if user_path.exists():
        return user_path.read_text()

    # Fall back to built-in
    builtin_path = _BUILTIN_DIR / f"{name}.txt"
    if builtin_path.exists():
        return builtin_path.read_text()

    raise FileNotFoundError(
        f"Prompt '{name}' not found. Searched:\n  - {user_path}\n  - {builtin_path}"
    )


def list_prompts(project_root: Path | None = None) -> list[str]:
    """List available prompt names.

    Returns prompts from both user and built-in directories,
    with user prompts taking precedence.
    """
    if project_root is None:
        project_root = Path.cwd()

    prompts: set[str] = set()

    # Built-in prompts
    for p in _BUILTIN_DIR.glob("*.txt"):
        prompts.add(p.stem)

    # User prompts (override built-ins)
    user_dir = project_root / ".moss" / "prompts"
    if user_dir.exists():
        for p in user_dir.glob("*.txt"):
            prompts.add(p.stem)

    return sorted(prompts)
