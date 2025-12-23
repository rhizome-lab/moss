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


def passthrough(subcommand: str, argv: list[str]) -> int:
    """Pass CLI args directly to Rust subcommand, streaming output.

    Args:
        subcommand: The Rust CLI subcommand (e.g., "expand", "callers")
        argv: Raw CLI arguments to pass through (after the subcommand)

    Returns:
        Exit code from Rust CLI.
    """
    import sys

    binary = get_rust_binary()
    if binary is None:
        print(
            f"Rust CLI required for {subcommand}. Build with: cargo build --release",
            file=sys.stderr,
        )
        return 1

    cmd = [str(binary), subcommand, *argv]

    # Check if stdout is being redirected (e.g., StringIO for MCP server)
    # In that case, capture output and write to the redirected stream
    if hasattr(sys.stdout, "getvalue"):
        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.stdout:
            sys.stdout.write(result.stdout)
        if result.stderr:
            sys.stderr.write(result.stderr)
        return result.returncode

    result = subprocess.run(cmd)
    return result.returncode


def rust_find_symbols(
    name: str,
    kind: str | None = None,
    fuzzy: bool = True,
    limit: int = 50,
    root: str | None = None,
) -> list[dict] | None:
    """Find symbols by name using Rust CLI.

    Returns list of symbol matches or None if Rust not available.
    Each match: {"name", "kind", "file", "line", "end_line", "parent"}
    """
    if not rust_available():
        return None

    args = ["find-symbols", "-l", str(limit)]
    if kind:
        args.extend(["-k", kind])
    if not fuzzy:
        args.extend(["-f", "false"])
    if root:
        args.extend(["-r", root])
    args.append(name)

    code, output = call_rust(args, json_output=True)
    if code != 0:
        return []

    return json.loads(output)


def rust_list_files(
    prefix: str = "",
    limit: int = 1000,
    root: str | None = None,
) -> list[str] | None:
    """List indexed files with optional prefix filter.

    Returns list of file paths or None if Rust not available.
    """
    if not rust_available():
        return None

    args = ["list-files", "-l", str(limit)]
    if root:
        args.extend(["-r", root])
    if prefix:
        args.append(prefix)

    code, output = call_rust(args, json_output=True)
    if code != 0:
        return None

    return json.loads(output)


def rust_grep(
    pattern: str,
    glob_pattern: str | None = None,
    limit: int = 100,
    ignore_case: bool = False,
    root: str | None = None,
) -> dict | None:
    """Search for text patterns using Rust CLI.

    Returns {"matches": [...], "total_matches": int, "files_searched": int}
    or None if Rust not available.
    """
    if not rust_available():
        return None

    args = ["grep", "-l", str(limit)]
    if ignore_case:
        args.append("-i")
    if glob_pattern:
        args.extend(["--glob", glob_pattern])
    if root:
        args.extend(["-r", root])
    args.append(pattern)

    code, output = call_rust(args, json_output=True)
    if code != 0:
        return {"matches": [], "total_matches": 0, "files_searched": 0}

    return json.loads(output)


def rust_context(file: str, root: str | None = None) -> dict | None:
    """Get compiled context for a file using Rust CLI.

    Returns {
        "file": str,
        "summary": {"lines", "classes", "functions", "methods", "imports", "exports"},
        "symbols": [...],
        "imports": [...],
        "exports": [...]
    } or None if Rust not available.
    """
    if not rust_available():
        return None

    args = ["context"]
    if root:
        args.extend(["-r", root])
    args.append(file)

    code, output = call_rust(args, json_output=True)
    if code != 0:
        return None

    return json.loads(output)


def rust_skeleton(file_path: str, root: str | None = None) -> str | None:
    """Extract code skeleton using Rust CLI.

    Returns formatted skeleton string or None if Rust not available.
    """
    if not rust_available():
        return None

    args = ["skeleton"]
    if root:
        args.extend(["-r", root])
    args.append(file_path)

    code, output = call_rust(args, json_output=False)
    if code != 0:
        return None

    return output


def rust_skeleton_json(file_path: str, root: str | None = None) -> list[dict] | None:
    """Extract code skeleton as structured data using Rust CLI.

    Returns list of symbol dicts or None if Rust not available.
    Each symbol has: name, kind, signature, docstring, start_line, end_line, children
    """
    if not rust_available():
        return None

    args = ["--json", "skeleton"]
    if root:
        args.extend(["-r", root])
    args.append(file_path)

    code, output = call_rust(args, json_output=False)
    if code != 0:
        return None

    try:
        return json.loads(output)
    except json.JSONDecodeError:
        return None


def rust_summarize(file_path: str, root: str | None = None) -> str | None:
    """Summarize a file using Rust CLI.

    Returns summary string or None if Rust not available.
    """
    if not rust_available():
        return None

    args = ["summarize"]
    if root:
        args.extend(["-r", root])
    args.append(file_path)

    code, output = call_rust(args, json_output=False)
    if code != 0:
        return None

    return output


def rust_view(
    target: str | None = None,
    depth: int = 1,
    line_numbers: bool = False,
    deps: bool = False,
    kind: str | None = None,
    calls: bool = False,
    called_by: bool = False,
    root: str | None = None,
) -> dict | None:
    """View a node in the codebase tree using Rust CLI.

    Returns view result as dict or None if Rust not available.
    """
    if not rust_available():
        return None

    args = ["view", "-d", str(depth)]
    if line_numbers:
        args.append("-n")
    if deps:
        args.append("--deps")
    if kind:
        args.extend(["-t", kind])
    if calls:
        args.append("--calls")
    if called_by:
        args.append("--called-by")
    if root:
        args.extend(["-r", root])
    if target:
        args.append(target)

    code, output = call_rust(args, json_output=True)
    if code != 0:
        return None

    return json.loads(output)


def rust_edit(
    target: str,
    delete: bool = False,
    replace: str | None = None,
    before: str | None = None,
    after: str | None = None,
    prepend: str | None = None,
    append: str | None = None,
    move_before: str | None = None,
    move_after: str | None = None,
    swap: str | None = None,
    dry_run: bool = False,
    root: str | None = None,
) -> dict | None:
    """Edit a node in the codebase tree using Rust CLI.

    Returns edit result as dict or None if Rust not available.
    """
    if not rust_available():
        return None

    args = ["edit", target]
    if delete:
        args.append("--delete")
    if replace:
        args.extend(["--replace", replace])
    if before:
        args.extend(["--before", before])
    if after:
        args.extend(["--after", after])
    if prepend:
        args.extend(["--prepend", prepend])
    if append:
        args.extend(["--append", append])
    if move_before:
        args.extend(["--move-before", move_before])
    if move_after:
        args.extend(["--move-after", move_after])
    if swap:
        args.extend(["--swap", swap])
    if dry_run:
        args.append("--dry-run")
    if root:
        args.extend(["-r", root])

    code, output = call_rust(args, json_output=True)
    if code != 0:
        return None

    return json.loads(output)


def rust_analyze(
    target: str | None = None,
    health: bool = False,
    complexity: bool = False,
    security: bool = False,
    threshold: int | None = None,
    root: str | None = None,
) -> dict | None:
    """Analyze codebase health, complexity, and security using Rust CLI.

    Returns analysis result as dict or None if Rust not available.
    """
    if not rust_available():
        return None

    args = ["analyze"]
    if health:
        args.append("--health")
    if complexity:
        args.append("--complexity")
    if security:
        args.append("--security")
    if threshold:
        args.extend(["-t", str(threshold)])
    if root:
        args.extend(["-r", root])
    if target:
        args.append(target)

    code, output = call_rust(args, json_output=True)
    if code != 0:
        return None

    return json.loads(output)
