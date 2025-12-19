"""Shim to delegate to Rust CLI for fast commands when available."""

import json
import shutil
import subprocess
from pathlib import Path


def find_rust_binary() -> Path | None:
    """Find the Rust moss binary if available."""
    # Check common locations
    candidates = [
        # Development: relative to repo root
        Path(__file__).parent.parent.parent / "target" / "release" / "moss",
        # Installed via cargo
        Path.home() / ".cargo" / "bin" / "moss-cli",
        # System PATH
        shutil.which("moss-cli"),
    ]

    for candidate in candidates:
        if candidate is None:
            continue
        path = Path(candidate) if isinstance(candidate, str) else candidate
        if path.exists() and path.is_file():
            return path

    return None


_RUST_BINARY: Path | None = None
_RUST_CHECKED: bool = False


def get_rust_binary() -> Path | None:
    """Get cached Rust binary path."""
    global _RUST_BINARY, _RUST_CHECKED
    if not _RUST_CHECKED:
        _RUST_BINARY = find_rust_binary()
        _RUST_CHECKED = True
    return _RUST_BINARY


def rust_available() -> bool:
    """Check if Rust CLI is available."""
    return get_rust_binary() is not None


def call_rust(args: list[str], json_output: bool = False) -> tuple[int, str]:
    """Call the Rust CLI with given arguments.

    Returns (exit_code, output).
    """
    binary = get_rust_binary()
    if binary is None:
        raise RuntimeError("Rust binary not available")

    cmd = [str(binary)]
    if json_output:
        cmd.append("--json")
    cmd.extend(args)

    result = subprocess.run(cmd, capture_output=True, text=True)
    output = result.stdout if result.returncode == 0 else result.stderr
    return result.returncode, output


def rust_path(query: str) -> list[dict] | None:
    """Resolve path using Rust CLI.

    Returns list of matches or None if Rust not available.
    """
    if not rust_available():
        return None

    code, output = call_rust(["path", query], json_output=True)
    if code != 0:
        return []

    return json.loads(output)


def rust_search_tree(query: str, limit: int = 20) -> list[dict] | None:
    """Search tree using Rust CLI.

    Returns list of matches or None if Rust not available.
    """
    if not rust_available():
        return None

    code, output = call_rust(["search-tree", query, "-l", str(limit)], json_output=True)
    if code != 0:
        return []

    return json.loads(output)


def rust_view(target: str, line_numbers: bool = False) -> dict | None:
    """View file using Rust CLI.

    Returns {"path": str, "content": str} or None if Rust not available.
    """
    if not rust_available():
        return None

    args = ["view", target]
    if line_numbers:
        args.append("-n")

    code, output = call_rust(args, json_output=True)
    if code != 0:
        return None

    return json.loads(output)
