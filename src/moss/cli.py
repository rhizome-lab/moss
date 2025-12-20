"""Command-line interface for Moss.

# See: docs/cli/commands.md
"""

from __future__ import annotations

import argparse
import asyncio
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any

from moss.output import Output, Verbosity, configure_output, get_output

if TYPE_CHECKING:
    from argparse import Namespace


def get_version() -> str:
    """Get the moss version."""
    from moss import __version__

    return __version__


def setup_output(args: Namespace) -> Output:
    """Configure global output based on CLI args."""
    # Determine verbosity
    if getattr(args, "quiet", False):
        verbosity = Verbosity.QUIET
    elif getattr(args, "debug", False):
        verbosity = Verbosity.DEBUG
    elif getattr(args, "verbose", False):
        verbosity = Verbosity.VERBOSE
    else:
        verbosity = Verbosity.NORMAL

    # Configure output
    output = configure_output(
        verbosity=verbosity,
        json_format=getattr(args, "json", False),
        compact=getattr(args, "compact", False),
        no_color=getattr(args, "no_color", False),
        jq_expr=getattr(args, "jq", None),
    )

    return output


def wants_json(args: Namespace) -> bool:
    """Check if JSON output is requested (via --json or --jq)."""
    return getattr(args, "json", False) or getattr(args, "jq", None) is not None


def normalize_symbol_args(args: list[str]) -> str:
    """Normalize flexible symbol arguments to file:symbol or symbol format.

    Supports:
    - "symbol" -> "symbol"
    - "file:symbol" -> "file:symbol"
    - "file symbol" -> "file:symbol"
    - "symbol file" -> "file:symbol"

    Detection heuristics:
    - File path: contains '/' or ends with common extension (.py, .rs, .js, etc.)
    - Everything else is assumed to be a symbol
    """
    if len(args) == 1:
        return args[0]

    if len(args) == 2:
        a, b = args
        # Detect which is the file
        file_extensions = (".py", ".rs", ".js", ".ts", ".go", ".java", ".c", ".cpp", ".h")

        def looks_like_file(s: str) -> bool:
            return "/" in s or s.endswith(file_extensions)

        a_is_file = looks_like_file(a)
        b_is_file = looks_like_file(b)

        if a_is_file and not b_is_file:
            return f"{a}:{b}"
        elif b_is_file and not a_is_file:
            return f"{b}:{a}"
        elif a_is_file and b_is_file:
            # Both look like files, use first as file
            return f"{a}:{b}"
        else:
            # Neither looks like file, use first as symbol target
            # and second as scope hint (file:symbol format)
            return f"{b}:{a}"

    # More than 2 args, join with :
    return ":".join(args)


def output_result(data: Any, args: Namespace) -> None:
    """Output result in appropriate format."""
    output = get_output()
    output.data(data)


def cmd_init(args: Namespace) -> int:
    """Initialize a new moss project."""
    output = setup_output(args)
    project_dir = Path(args.directory).resolve()

    if not project_dir.exists():
        output.error(f"Directory {project_dir} does not exist")
        return 1

    config_file = project_dir / "moss_config.py"

    if config_file.exists() and not args.force:
        output.error(f"{config_file} already exists. Use --force to overwrite.")
        return 1

    # Determine distro
    distro_name = args.distro or "python"

    config_content = f'''"""Moss configuration for this project."""

from pathlib import Path

from moss.config import MossConfig, get_distro

# Start from a base distro
base = get_distro("{distro_name}")
config = base.create_config() if base else MossConfig()

# Configure for this project
config = (
    config
    .with_project(Path(__file__).parent, "{project_dir.name}")
    .with_validators(syntax=True, ruff=True, pytest=False)
    .with_policies(velocity=True, quarantine=True, path=True)
    .with_loop(max_iterations=10, auto_commit=True)
)

# Add static context files (architecture docs, etc.)
# config = config.with_static_context(Path("docs/architecture/overview.md"))

# Add custom validators
# from moss.validators import CommandValidator
# config = config.add_validator(CommandValidator("mypy", ["mypy", "."]))
'''

    config_file.write_text(config_content)
    output.success(f"Created {config_file}")

    # Create .moss directory for runtime data
    moss_dir = project_dir / ".moss"
    if not moss_dir.exists():
        moss_dir.mkdir()
        (moss_dir / ".gitignore").write_text("*\n")
        output.verbose(f"Created {moss_dir}/")

    output.info(f"Moss initialized in {project_dir}")
    output.info(f"  Config: {config_file.name}")
    output.info(f"  Distro: {distro_name}")
    output.blank()
    output.step("Next steps:")
    output.info("  1. Edit moss_config.py to customize your configuration")
    output.info("  2. Run 'moss run \"your task\"' to execute a task")

    return 0


def cmd_run(args: Namespace) -> int:
    """Run a task through moss."""
    from moss.agents import create_manager
    from moss.api import TaskRequest, create_api_handler
    from moss.config import MossConfig, load_config_file
    from moss.events import EventBus
    from moss.shadow_git import ShadowGit

    output = setup_output(args)
    project_dir = Path(args.directory).resolve()
    config_file = project_dir / "moss_config.py"

    # Load config
    if config_file.exists():
        try:
            config = load_config_file(config_file)
        except Exception as e:
            output.error(f"Error loading config: {e}")
            return 1
    else:
        config = MossConfig().with_project(project_dir, project_dir.name)

    # Validate config
    errors = config.validate()
    if errors:
        output.error("Configuration errors:")
        for error in errors:
            output.error(f"  - {error}")
        return 1

    # Set up components
    event_bus = EventBus()
    shadow_git = ShadowGit(project_dir)
    manager = create_manager(shadow_git, event_bus)
    handler = create_api_handler(manager, event_bus)

    # Create task request
    request = TaskRequest(
        task=args.task,
        priority=args.priority,
        constraints=args.constraint or [],
    )

    async def run_task() -> int:
        response = await handler.create_task(request)
        output.success(f"Task created: {response.request_id}")
        output.info(f"Ticket: {response.ticket_id}")
        output.info(f"Status: {response.status.value}")

        if args.wait:
            output.step("Waiting for completion...")
            # Poll for status
            while True:
                status = await handler.get_task_status(response.request_id)
                if status is None:
                    output.error("Task not found")
                    return 1

                if status.status.value in ("COMPLETED", "FAILED", "CANCELLED"):
                    if status.status.value == "COMPLETED":
                        output.success(f"Final status: {status.status.value}")
                    else:
                        output.warning(f"Final status: {status.status.value}")
                    if status.result:
                        output.data(status.result)
                    break

                await asyncio.sleep(0.5)

        return 0

    return asyncio.run(run_task())


def cmd_status(args: Namespace) -> int:
    """Show status of moss tasks and workers."""
    from moss.agents import create_manager
    from moss.api import create_api_handler
    from moss.config import load_config_file
    from moss.events import EventBus
    from moss.shadow_git import ShadowGit

    output = setup_output(args)
    project_dir = Path(args.directory).resolve()
    config_file = project_dir / "moss_config.py"

    # Load config (validates it's readable)
    if config_file.exists():
        try:
            load_config_file(config_file)
        except Exception as e:
            output.error(f"Error loading config: {e}")
            return 1

    # Set up components
    event_bus = EventBus()
    shadow_git = ShadowGit(project_dir)
    manager = create_manager(shadow_git, event_bus)
    handler = create_api_handler(manager, event_bus)

    # Get stats
    stats = handler.get_stats()

    if getattr(args, "json", False):
        output_result(stats, args)
        return 0

    output.header("Moss Status")
    output.info(f"Project: {project_dir.name}")
    output.info(f"Config: {'moss_config.py' if config_file.exists() else '(default)'}")
    output.blank()
    output.step("API Handler:")
    output.info(f"  Active requests: {stats['active_requests']}")
    output.info(f"  Pending checkpoints: {stats['pending_checkpoints']}")
    output.info(f"  Active streams: {stats['active_streams']}")
    output.blank()
    output.step("Manager:")
    manager_stats = stats["manager_stats"]
    output.info(f"  Active workers: {manager_stats['active_workers']}")
    output.info(f"  Total tickets: {manager_stats['total_tickets']}")
    tickets_by_status = manager_stats.get("tickets_by_status", {})
    if tickets_by_status:
        output.info(f"  Tickets by status: {tickets_by_status}")

    # Show verbose info using output.verbose()
    output.verbose("Workers:")
    for worker_id, worker_info in manager_stats.get("workers", {}).items():
        output.verbose(f"  {worker_id}: {worker_info}")

    return 0


def cmd_config(args: Namespace) -> int:
    """Show or validate configuration."""
    from moss.config import list_distros, load_config_file

    output = setup_output(args)

    if args.list_distros:
        output.info("Available distros:")
        for name in list_distros():
            output.info(f"  - {name}")
        return 0

    project_dir = Path(args.directory).resolve()
    config_file = project_dir / "moss_config.py"

    if not config_file.exists():
        output.error(f"No config file found at {config_file}")
        output.info("Run 'moss init' to create one.")
        return 1

    try:
        config = load_config_file(config_file)
    except Exception as e:
        output.error(f"Error loading config: {e}")
        return 1

    if args.validate:
        errors = config.validate()
        if errors:
            output.error("Configuration errors:")
            for error in errors:
                output.error(f"  - {error}")
            return 1
        output.success("Configuration is valid.")
        return 0

    # Show config
    output.header("Configuration")
    output.info(f"Project: {config.project_name}")
    output.info(f"Root: {config.project_root}")
    output.info(f"Extends: {', '.join(config.extends) or '(none)'}")
    output.blank()
    output.step("Validators:")
    output.info(f"  syntax: {config.validators.syntax}")
    output.info(f"  ruff: {config.validators.ruff}")
    output.info(f"  pytest: {config.validators.pytest}")
    output.info(f"  custom: {len(config.validators.custom)}")
    output.blank()
    output.step("Policies:")
    output.info(f"  velocity: {config.policies.velocity}")
    output.info(f"  quarantine: {config.policies.quarantine}")
    output.info(f"  rate_limit: {config.policies.rate_limit}")
    output.info(f"  path: {config.policies.path}")
    output.blank()
    output.step("Loop:")
    output.info(f"  max_iterations: {config.loop.max_iterations}")
    output.info(f"  timeout_seconds: {config.loop.timeout_seconds}")
    output.info(f"  auto_commit: {config.loop.auto_commit}")

    return 0


def cmd_distros(args: Namespace) -> int:
    """List available configuration distros."""
    from moss.config import get_distro, list_distros

    output = setup_output(args)
    distros = list_distros()

    if getattr(args, "json", False):
        result = []
        for name in sorted(distros):
            distro = get_distro(name)
            if distro:
                result.append({"name": name, "description": distro.description})
        output_result(result, args)
        return 0

    output.header("Available Distros")

    for name in sorted(distros):
        distro = get_distro(name)
        if distro:
            desc = distro.description or "(no description)"
            output.info(f"  {name}: {desc}")

    return 0


# =============================================================================
# Introspection Commands
# =============================================================================


def cmd_skeleton(args: Namespace) -> int:
    """Extract code skeleton from a file or directory."""
    from moss import MossAPI

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
        return 1

    api = MossAPI.for_project(path if path.is_dir() else path.parent)

    if path.is_file():
        files = [path]
    else:
        pattern = args.pattern or "**/*.py"
        files = list(path.glob(pattern))

    results = []
    include_docstrings = not getattr(args, "no_docstrings", False)

    for file_path in files:
        if wants_json(args):
            # For JSON, extract symbols as structured data
            try:
                symbols = api.skeleton.extract(file_path)
                results.append(
                    {
                        "file": str(file_path),
                        "symbols": [s.to_dict() for s in symbols],
                    }
                )
            except Exception as e:
                results.append({"file": str(file_path), "error": str(e)})
        else:
            # For text output, use formatted skeleton
            content = api.skeleton.format(file_path, include_docstrings=include_docstrings)
            if content.startswith("Error:") or content.startswith("File not found:"):
                output.error(f"{file_path}: {content}")
            else:
                if len(files) > 1:
                    output.header(str(file_path))
                if content:
                    output.print(content)
                else:
                    output.verbose("(no symbols found)")

    if wants_json(args):
        output_result(results if len(results) > 1 else results[0] if results else {}, args)

    return 0


# =============================================================================
# New Codebase Tree Commands (see docs/codebase-tree.md)
# =============================================================================


def cmd_path(args: Namespace) -> int:
    """Resolve a fuzzy path to exact location(s)."""
    from moss.rust_shim import rust_available, rust_path

    output = setup_output(args)
    query = args.query

    # Try Rust CLI first for speed
    if rust_available():
        matches = rust_path(query)
        if matches is not None:
            if not matches:
                output.error(f"No matches for: {query}")
                return 1
            if wants_json(args):
                output.data(matches)
            else:
                for m in matches:
                    output.print(f"{m['path']} ({m['kind']})")
            return 0

    # Fall back to Python implementation
    from moss.codebase import resolve_path

    matches = resolve_path(query)

    if not matches:
        output.error(f"No matches for: {query}")
        return 1

    if wants_json(args):
        output.data([{"path": m.full_path, "kind": m.kind} for m in matches])
    else:
        for m in matches:
            output.print(f"{m.full_path} ({m.kind})")

    return 0


def cmd_view(args: Namespace) -> int:
    """View a node in the codebase tree."""
    from moss.codebase import build_tree

    output = setup_output(args)
    query = args.target
    root = Path.cwd()

    tree = build_tree(root)
    nodes = tree.resolve(query)

    if not nodes:
        output.error(f"No matches for: {query}")
        return 1

    # If multiple matches, show them as options
    if len(nodes) > 1:
        output.print(f"Multiple matches for '{query}':")
        for n in nodes:
            output.print(f"  {n.full_path} ({n.kind.value})")
        return 0

    node = nodes[0]

    # Show context (where this node lives)
    if node.parent and node.parent.kind.value != "root":
        output.print(f"in {node.parent.full_path}/")
        output.print("")

    # Show the node itself
    output.print(f"{node.name} ({node.kind.value})")
    if node.description:
        output.print(f"  {node.description}")
    if node.signature:
        output.print(f"  {node.signature}")
    output.print("")

    # Show children grouped by kind
    if node.children:
        from collections import defaultdict

        by_kind: dict[str, list] = defaultdict(list)
        for child in node.children:
            by_kind[child.kind.value].append(child)

        for kind in ["class", "function", "method", "constant", "directory", "file"]:
            children = by_kind.get(kind, [])
            if children:
                label = "Classes" if kind == "class" else f"{kind.title()}s"
                output.print(f"{label}:")
                for child in children[:20]:  # Limit to 20 per category
                    desc = f"  {child.description}" if child.description else ""
                    output.print(f"  {child.name}{desc}")
                if len(children) > 20:
                    output.print(f"  ... +{len(children) - 20} more")
                output.print("")

    return 0


def cmd_search_tree(args: Namespace) -> int:
    """Search for nodes in the codebase tree."""
    from moss.codebase import build_tree

    output = setup_output(args)
    query = args.query
    scope = getattr(args, "scope", None)
    root = Path.cwd()

    tree = build_tree(root)
    matches = tree.search(query, scope=scope)

    if not matches:
        output.error(f"No matches for: {query}")
        return 1

    if wants_json(args):
        output.data([{"path": m.full_path, "kind": m.kind.value} for m in matches])
    else:
        for m in matches[:50]:  # Limit output
            desc = f" - {m.description}" if m.description else ""
            output.print(f"{m.full_path} ({m.kind.value}){desc}")
        if len(matches) > 50:
            output.print(f"... +{len(matches) - 50} more")

    return 0


def cmd_expand(args: Namespace) -> int:
    """Show full source of a symbol."""
    from moss.codebase import build_tree

    output = setup_output(args)
    target = normalize_symbol_args(args.target)
    root = Path.cwd()

    tree = build_tree(root)
    nodes = tree.resolve(target)

    if not nodes:
        output.error(f"No matches for: {target}")
        return 1

    if len(nodes) > 1:
        output.print(f"Multiple matches for '{target}':")
        for n in nodes:
            output.print(f"  {n.full_path} ({n.kind.value})")
        return 0

    node = nodes[0]

    # Can only expand symbols with line numbers
    if node.lineno == 0:
        output.error(f"Cannot expand {node.kind.value}: {node.name}")
        return 1

    # Read the source
    try:
        lines = node.path.read_text().splitlines()
        source_lines = lines[node.lineno - 1 : node.end_lineno]
        output.print("\n".join(source_lines))
    except Exception as e:
        output.error(f"Failed to read source: {e}")
        return 1

    return 0


def cmd_callers(args: Namespace) -> int:
    """Find callers of a symbol."""
    from moss.codebase import build_tree

    output = setup_output(args)
    target = normalize_symbol_args(args.target)
    root = Path.cwd()

    tree = build_tree(root)

    # First resolve the symbol to get its name
    nodes = tree.resolve(target)
    if not nodes:
        output.error(f"No matches for: {target}")
        return 1

    symbol_name = nodes[0].name
    refs = tree.find_references(symbol_name)

    if not refs:
        output.print(f"No callers found for: {symbol_name}")
        return 0

    if wants_json(args):
        output.data([{"path": r.full_path, "kind": r.kind.value} for r in refs])
    else:
        output.print(f"Callers of {symbol_name}:")
        for r in refs[:30]:
            output.print(f"  {r.full_path} ({r.kind.value})")
        if len(refs) > 30:
            output.print(f"  ... +{len(refs) - 30} more")

    return 0


def cmd_callees(args: Namespace) -> int:
    """Find what a symbol calls."""
    from moss.codebase import build_tree

    output = setup_output(args)
    target = normalize_symbol_args(args.target)
    root = Path.cwd()

    tree = build_tree(root)
    nodes = tree.resolve(target)

    if not nodes:
        output.error(f"No matches for: {target}")
        return 1

    node = nodes[0]
    if node.kind.value not in ("function", "method"):
        output.error(f"Can only find callees of functions/methods, not {node.kind.value}")
        return 1

    callees = tree.find_callees(node)

    if not callees:
        output.print(f"No callees found for: {node.name}")
        return 0

    if wants_json(args):
        output.data(callees)
    else:
        output.print(f"Callees of {node.name}:")
        for c in callees:
            output.print(f"  {c}")

    return 0


def cmd_tree(args: Namespace) -> int:
    """Show git-aware file tree."""
    from moss import MossAPI

    output = setup_output(args)
    path = Path(getattr(args, "path", ".")).resolve()

    if not path.exists():
        output.error(f"Path not found: {path}")
        return 1

    api = MossAPI.for_project(path)
    tracked_only = getattr(args, "tracked", False)
    gitignore = not getattr(args, "all", False)

    result = api.tree.generate(tracked_only=tracked_only, gitignore=gitignore)

    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(result.to_compact())
    elif wants_json(args):
        output.data(result.to_dict())
    else:
        output.print(result.to_text())

    return 0


def cmd_anchors(args: Namespace) -> int:
    """Find anchors (functions, classes, methods) in code."""
    import re

    from moss import MossAPI

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
        return 1

    api = MossAPI.for_project(path if path.is_dir() else path.parent)

    # Filter types
    type_filter = args.type if args.type != "all" else None
    name_pattern = re.compile(args.name) if args.name else None

    results = []

    if path.is_file():
        files = [path]
    else:
        pattern = args.pattern or "**/*.py"
        files = list(path.glob(pattern))

    def collect_symbols(symbols: list, file_path: Path, parent: str | None = None) -> None:
        """Recursively collect symbols from skeleton."""
        for sym in symbols:
            kind = sym.kind
            # Map skeleton kinds to anchor types
            if type_filter:
                if type_filter == "function" and kind not in ("function",):
                    if sym.children:
                        collect_symbols(sym.children, file_path, sym.name)
                    continue
                if type_filter == "class" and kind != "class":
                    if sym.children:
                        collect_symbols(sym.children, file_path, sym.name)
                    continue
                if type_filter == "method" and kind != "method":
                    if sym.children:
                        collect_symbols(sym.children, file_path, sym.name)
                    continue

            # Apply name filter
            if name_pattern and not name_pattern.search(sym.name):
                if sym.children:
                    collect_symbols(sym.children, file_path, sym.name)
                continue

            anchor_info = {
                "file": str(file_path),
                "name": sym.name,
                "type": kind,
                "line": sym.lineno,
            }
            if parent:
                anchor_info["context"] = parent
            if sym.signature:
                anchor_info["signature"] = sym.signature
            results.append(anchor_info)

            # Recurse into children
            if sym.children:
                collect_symbols(sym.children, file_path, sym.name)

    for file_path in files:
        try:
            symbols = api.skeleton.extract(file_path)
            collect_symbols(symbols, file_path)
        except SyntaxError as e:
            output.verbose(f"Syntax error in {file_path}: {e}")

    if getattr(args, "json", False):
        output_result(results, args)
    else:
        for r in results:
            ctx = f" (in {r['context']})" if r.get("context") else ""
            output.print(f"{r['file']}:{r['line']} {r['type']} {r['name']}{ctx}")

    return 0


def cmd_query(args: Namespace) -> int:
    """Query code with pattern matching and filters."""
    from moss import MossAPI

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
        return 1

    # Use project root for API, pass path as search path
    api = MossAPI.for_project(path if path.is_dir() else path.parent)

    results = api.search.query(
        path=path,
        pattern=args.pattern or "**/*.py",
        kind=args.type if args.type != "all" else None,
        name=args.name,
        signature=args.signature,
        inherits=args.inherits,
        min_lines=args.min_lines,
        max_lines=args.max_lines,
    )

    if getattr(args, "json", False):
        json_results = [r.to_dict() for r in results]
        if getattr(args, "group_by", None) == "file":
            # Group results by file for JSON output
            grouped: dict[str, list] = {}
            for r in json_results:
                grouped.setdefault(r["file"], []).append(r)
            output_result(grouped, args)
        else:
            output_result(json_results, args)
    else:
        if getattr(args, "group_by", None) == "file":
            # Group results by file for text output
            grouped_text: dict[str, list] = {}
            for r in results:
                grouped_text.setdefault(r.file, []).append(r)
            for file_path_str, file_results in grouped_text.items():
                output.header(file_path_str)
                for r in file_results:
                    ctx = f" (in {r.context})" if r.context else ""
                    output.print(f"  :{r.line} {r.kind} {r.name}{ctx}")
                    if r.signature:
                        output.print(f"    {r.signature}")
        else:
            for r in results:
                ctx = f" (in {r.context})" if r.context else ""
                output.print(f"{r.file}:{r.line} {r.kind} {r.name}{ctx}")
                if r.signature:
                    output.print(f"  {r.signature}")
                if r.docstring:
                    output.verbose(f"  {r.docstring[:50]}...")

    if not results:
        output.warning("No matches found")

    return 0


def cmd_cfg(args: Namespace) -> int:
    """Build and display control flow graph."""
    from moss import MossAPI

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
        return 1

    if not path.is_file():
        output.error(f"{path} must be a file")
        return 1

    # Handle --live mode
    if getattr(args, "live", False):
        from moss.live_cfg import start_live_cfg

        start_live_cfg(
            path=path,
            function_name=args.function,
            port=getattr(args, "port", 8765),
        )
        return 0

    api = MossAPI.for_project(path.parent)

    try:
        cfg_objects = api.cfg.build(path)
    except Exception as e:
        output.error(f"Failed to build CFG: {e}")
        return 1

    # Filter by function name if specified
    if args.function:
        cfg_objects = [cfg for cfg in cfg_objects if cfg.name == args.function]

    if not cfg_objects:
        output.warning("No functions found")
        return 1

    # Determine output format
    output_format = None
    if args.output:
        # Auto-detect from file extension
        ext = Path(args.output).suffix.lstrip(".")
        if ext in ("svg", "png", "html", "dot", "mermaid", "md"):
            output_format = ext

    if getattr(args, "json", False):
        results = []
        for cfg in cfg_objects:
            result = {
                "name": cfg.name,
                "node_count": cfg.node_count,
                "edge_count": cfg.edge_count,
                "cyclomatic_complexity": cfg.cyclomatic_complexity,
            }
            # Include full graph details unless --summary
            if not args.summary:
                result["entry"] = cfg.entry
                result["exit"] = cfg.exit
                result["nodes"] = {
                    nid: {"type": n.node_type.name, "label": n.label, "lineno": n.lineno}
                    for nid, n in cfg.nodes.items()
                }
                result["edges"] = [
                    {"source": e.source, "target": e.target, "type": e.edge_type.name}
                    for e in cfg.edges
                ]
            results.append(result)
        output_result(results, args)
    elif args.html or output_format == "html":
        # HTML output with embedded Mermaid
        from moss.visualization import visualize_cfgs

        content = visualize_cfgs(cfg_objects, format="html")
        if args.output:
            Path(args.output).write_text(content)
            output.success(f"Saved to {args.output}")
        else:
            output.print(content)
    elif args.mermaid or output_format == "mermaid":
        # Mermaid output
        mermaid_parts = [cfg.to_mermaid() for cfg in cfg_objects]
        mermaid_lines = "\n\n".join(mermaid_parts)

        if args.output:
            Path(args.output).write_text(mermaid_lines)
            output.success(f"Saved to {args.output}")
        else:
            output.print(mermaid_lines)
    elif args.summary:
        # Summary mode: just show counts and complexity
        for cfg in cfg_objects:
            output.info(
                f"{cfg.name}: {cfg.node_count} nodes, "
                f"{cfg.edge_count} edges, "
                f"complexity {cfg.cyclomatic_complexity}"
            )
    elif args.dot or output_format == "dot":
        # DOT output
        dot_parts = [cfg.to_dot() for cfg in cfg_objects]
        dot_content = "\n\n".join(dot_parts)
        if args.output:
            Path(args.output).write_text(dot_content)
            output.success(f"Saved to {args.output}")
        else:
            output.print(dot_content)
    elif output_format == "svg":
        from moss.visualization import render_dot_to_svg

        dot_parts = [cfg.to_dot() for cfg in cfg_objects]
        dot_content = "\n\n".join(dot_parts)
        if dot_content:
            svg = render_dot_to_svg(dot_content)
            Path(args.output).write_text(svg)
            output.success(f"Saved to {args.output}")
        else:
            output.error("No DOT content available for SVG rendering")
            return 1
    elif output_format == "png":
        from moss.visualization import render_dot_to_png

        dot_parts = [cfg.to_dot() for cfg in cfg_objects]
        dot_content = "\n\n".join(dot_parts)
        if dot_content:
            png = render_dot_to_png(dot_content)
            Path(args.output).write_bytes(png)
            output.success(f"Saved to {args.output}")
        else:
            output.error("No DOT content available for PNG rendering")
            return 1
    else:
        # Default: text output
        for cfg in cfg_objects:
            output.print(cfg.to_text())

    return 0


def cmd_deps(args: Namespace) -> int:
    """Extract dependencies (imports/exports) from code."""
    from moss import MossAPI

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
        return 1

    api = MossAPI.for_project(path if path.is_dir() else path.parent)

    # Handle --dot mode: generate dependency graph visualization
    if getattr(args, "dot", False):
        if not path.is_dir():
            output.error("--dot requires a directory path")
            return 1

        pattern = args.pattern or "**/*.py"
        graph = api.dependencies.build_graph(path, pattern)

        if not graph:
            output.warning("No internal dependencies found")
            return 1

        dot_output = api.dependencies.graph_to_dot(graph, title=path.name)
        output.print(dot_output)
        return 0

    # Handle --reverse mode: find what imports the target module
    if args.reverse:
        search_dir = args.search_dir or "."
        pattern = args.pattern or "**/*.py"
        reverse_deps = api.dependencies.find_reverse(args.reverse, search_dir, pattern)

        if getattr(args, "json", False):
            results = [
                {
                    "file": rd.file,
                    "line": rd.import_line,
                    "type": rd.import_type,
                    "names": rd.names,
                }
                for rd in reverse_deps
            ]
            output_result({"target": args.reverse, "importers": results}, args)
        else:
            if reverse_deps:
                output.info(f"Files that import '{args.reverse}':")
                for rd in reverse_deps:
                    names = f" ({', '.join(rd.names)})" if rd.names else ""
                    output.print(f"  {rd.file}:{rd.import_line} {rd.import_type}{names}")
            else:
                output.warning(f"No files found that import '{args.reverse}'")

        return 0

    # Normal mode: show dependencies of file(s)
    results = []

    if path.is_file():
        files = [path]
    else:
        pattern = args.pattern or "**/*.py"
        files = list(path.glob(pattern))

    for file_path in files:
        try:
            info = api.dependencies.extract(file_path)
            content = api.dependencies.format(file_path)

            if getattr(args, "json", False):
                results.append(
                    {
                        "file": str(file_path),
                        "imports": [
                            {"module": imp.module, "names": imp.names, "line": imp.lineno}
                            for imp in info.imports
                        ],
                        "exports": [
                            {"name": exp.name, "type": exp.export_type, "line": exp.lineno}
                            for exp in info.exports
                        ],
                    }
                )
            else:
                if len(files) > 1:
                    output.header(str(file_path))
                if content:
                    output.print(content)
        except Exception as e:
            output.verbose(f"Error in {file_path}: {e}")
            if getattr(args, "json", False):
                results.append({"file": str(file_path), "error": str(e)})

    if getattr(args, "json", False):
        output_result(results if len(results) > 1 else results[0] if results else {}, args)

    return 0


def cmd_context(args: Namespace) -> int:
    """Generate compiled context for a file (skeleton + deps + summary)."""
    from moss import MossAPI
    from moss.rust_shim import rust_available, rust_context

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
        return 1

    if not path.is_file():
        output.error(f"{path} must be a file")
        return 1

    # Try Rust CLI for speed (10-100x faster)
    if rust_available():
        result = rust_context(str(path), root=str(path.parent))
        if result:
            if getattr(args, "json", False):
                output_result(result, args)
            else:
                # Format text output from Rust result
                summary = result.get("summary", {})
                output.header(path.name)
                output.info(f"Lines: {summary.get('lines', 0)}")
                output.info(
                    f"Classes: {summary.get('classes', 0)}, "
                    f"Functions: {summary.get('functions', 0)}, "
                    f"Methods: {summary.get('methods', 0)}"
                )
                output.info(
                    f"Imports: {summary.get('imports', 0)}, Exports: {summary.get('exports', 0)}"
                )
                output.blank()

                # Print imports
                imports = result.get("imports", [])
                if imports:
                    output.step("Imports")
                    for imp in imports:
                        module = imp.get("module", "")
                        names = imp.get("names", [])
                        if names:
                            output.print(f"from {module} import {', '.join(names)}")
                        else:
                            output.print(f"import {module}")
                    output.blank()

                # Print skeleton
                output.step("Skeleton")
                symbols = result.get("symbols", [])
                if symbols:
                    for sym in symbols:
                        sig = sym.get("signature", sym.get("name", ""))
                        output.print(sig)
                else:
                    output.verbose("(no symbols)")
            return 0

    api = MossAPI.for_project(path.parent)

    # Get skeleton and dependencies
    try:
        symbols = api.skeleton.extract(path)
        skeleton_content = api.skeleton.format(path)
    except Exception as e:
        output.error(f"Failed to extract skeleton: {e}")
        return 1

    try:
        deps_info = api.dependencies.extract(path)
        deps_content = api.dependencies.format(path)
    except Exception as e:
        output.error(f"Failed to extract dependencies: {e}")
        return 1

    # Count symbols recursively
    def count_symbols(syms: list) -> dict:
        counts = {"classes": 0, "functions": 0, "methods": 0}
        for s in syms:
            kind = s.kind
            if kind == "class":
                counts["classes"] += 1
            elif kind == "function":
                counts["functions"] += 1
            elif kind == "method":
                counts["methods"] += 1
            if s.children:
                child_counts = count_symbols(s.children)
                for k, v in child_counts.items():
                    counts[k] += v
        return counts

    counts = count_symbols(symbols)
    source = path.read_text()
    line_count = len(source.splitlines())

    if getattr(args, "json", False):
        result = {
            "file": str(path),
            "summary": {
                "lines": line_count,
                "classes": counts["classes"],
                "functions": counts["functions"],
                "methods": counts["methods"],
                "imports": len(deps_info.imports),
                "exports": len(deps_info.exports),
            },
            "symbols": [s.to_dict() for s in symbols],
            "imports": [
                {"module": imp.module, "names": imp.names, "line": imp.lineno}
                for imp in deps_info.imports
            ],
            "exports": [
                {"name": exp.name, "type": exp.export_type, "line": exp.lineno}
                for exp in deps_info.exports
            ],
        }
        output_result(result, args)
    else:
        output.header(path.name)
        output.info(f"Lines: {line_count}")
        output.info(
            f"Classes: {counts['classes']}, "
            f"Functions: {counts['functions']}, Methods: {counts['methods']}"
        )
        output.info(f"Imports: {len(deps_info.imports)}, Exports: {len(deps_info.exports)}")
        output.blank()

        if deps_info.imports and deps_content:
            output.step("Imports")
            # Extract just the imports section from deps content
            imports_section = deps_content.split("Exports:")[0].strip()
            output.print(imports_section)
            output.blank()

        output.step("Skeleton")
        if skeleton_content:
            output.print(skeleton_content)
        else:
            output.verbose("(no symbols)")

    return 0


def cmd_search(args: Namespace) -> int:
    """Semantic search across codebase."""
    from moss import MossAPI

    out = get_output()
    directory = Path(args.directory).resolve()
    if not directory.exists():
        out.error(f"Directory {directory} does not exist")
        return 1

    api = MossAPI.for_project(directory)

    async def run_search():
        # Index if requested
        if args.index:
            patterns = args.patterns.split(",") if args.patterns else None
            count = await api.rag.index(patterns=patterns, force=False)
            if not args.query:
                out.success(f"Indexed {count} chunks from {directory}")
                return None

        if not args.query:
            out.error("No query provided. Use --query or --index")
            return None

        # Search
        return await api.rag.search(
            args.query,
            limit=args.limit,
            mode=args.mode,
        )

    results = asyncio.run(run_search())

    if results is None:
        return 0 if args.index else 1

    if not results:
        out.warning("No results found.")
        return 0

    if getattr(args, "json", False):
        json_results = [r.to_dict() for r in results]
        output_result(json_results, args)
    else:
        out.success(f"Found {len(results)} results:")
        out.blank()
        for i, r in enumerate(results, 1):
            location = f"{r.file_path}:{r.line_start}"
            name = r.symbol_name or r.file_path
            kind = r.symbol_kind or "file"
            score = f"{r.score:.2f}"

            out.info(f"{i}. [{kind}] {name}")
            out.print(f"   Location: {location}")
            out.print(f"   Score: {score} ({r.match_type})")

            # Show snippet
            if r.snippet:
                snippet = r.snippet[:200]
                if len(r.snippet) > 200:
                    snippet += "..."
                snippet_lines = snippet.split("\n")[:3]
                for line in snippet_lines:
                    out.print(f"   | {line}")
            out.blank()

    return 0


def cmd_mcp_server(args: Namespace) -> int:
    """Start the MCP server for LLM tool access."""
    output = setup_output(args)
    try:
        if getattr(args, "full", False):
            from moss.mcp_server_full import main as mcp_main
        else:
            from moss.mcp_server import main as mcp_main

        mcp_main()
        return 0
    except ImportError as e:
        output.error("MCP SDK not installed. Install with: pip install 'moss[mcp]'")
        output.debug(f"Details: {e}")
        return 1
    except KeyboardInterrupt:
        return 0


def cmd_acp_server(args: Namespace) -> int:
    """Start the ACP server for IDE integration (Zed, JetBrains)."""
    try:
        from moss.acp_server import run_acp_server

        run_acp_server()
        return 0
    except KeyboardInterrupt:
        return 0


def cmd_gen(args: Namespace) -> int:
    """Generate interface code from MossAPI introspection."""
    import json as json_mod

    output = setup_output(args)
    target = getattr(args, "target", "mcp")
    out_file = getattr(args, "output", None)
    show_list = getattr(args, "list", False)

    try:
        if target == "mcp":
            from moss.gen.mcp import MCPGenerator

            generator = MCPGenerator()
            if show_list:
                tools = generator.generate_tools()
                result = [
                    {"name": t.name, "description": t.description, "api_path": t.api_path}
                    for t in tools
                ]
                output.data(result)
            else:
                definitions = generator.generate_tool_definitions()
                content = json_mod.dumps(definitions, indent=2)
                if out_file:
                    Path(out_file).write_text(content)
                    output.success(
                        f"Generated {len(definitions)} MCP tool definitions to {out_file}"
                    )
                else:
                    print(content)

        elif target == "http":
            from moss.gen.http import HTTPGenerator

            generator = HTTPGenerator()
            if show_list:
                routers = generator.generate_routers()
                result = []
                for router in routers:
                    for endpoint in router.endpoints:
                        result.append(
                            {
                                "path": endpoint.path,
                                "method": endpoint.method,
                                "description": endpoint.description,
                            }
                        )
                output.data(result)
            else:
                spec = generator.generate_openapi_spec()
                content = json_mod.dumps(spec, indent=2)
                if out_file:
                    Path(out_file).write_text(content)
                    output.success(f"Generated OpenAPI spec to {out_file}")
                else:
                    print(content)

        elif target == "cli":
            from moss.gen.cli import CLIGenerator

            generator = CLIGenerator()
            if show_list:
                groups = generator.generate_groups()
                result = []
                for group in groups:
                    for cmd in group.commands:
                        result.append(
                            {
                                "command": f"{group.name} {cmd.name}",
                                "description": cmd.description,
                            }
                        )
                output.data(result)
            else:
                # Generate help text showing all commands
                parser = generator.generate_parser()
                parser.print_help()

        elif target == "openapi":
            from moss.gen.http import HTTPGenerator

            generator = HTTPGenerator()
            spec = generator.generate_openapi_spec()
            content = json_mod.dumps(spec, indent=2)
            if out_file:
                Path(out_file).write_text(content)
                output.success(f"Generated OpenAPI spec to {out_file}")
            else:
                print(content)

        elif target == "grpc":
            from moss.gen.grpc import GRPCGenerator

            generator = GRPCGenerator()
            if show_list:
                rpcs = generator.generate_rpcs()
                result = [
                    {
                        "name": rpc.name,
                        "request": rpc.request_type,
                        "response": rpc.response_type,
                    }
                    for rpc in rpcs
                ]
                output.data(result)
            else:
                content = generator.generate_proto()
                if out_file:
                    Path(out_file).write_text(content)
                    output.success(f"Generated proto file to {out_file}")
                else:
                    print(content)

        elif target == "lsp":
            from moss.gen.lsp import LSPGenerator

            generator = LSPGenerator()
            if show_list:
                commands = generator.generate_commands()
                result = [
                    {
                        "command": cmd.command,
                        "title": cmd.title,
                        "description": cmd.description,
                    }
                    for cmd in commands
                ]
                output.data(result)
            else:
                # Output command list as JSON
                commands = generator.generate_command_list()
                content = json_mod.dumps(commands, indent=2)
                if out_file:
                    Path(out_file).write_text(content)
                    output.success(f"Generated {len(commands)} LSP commands to {out_file}")
                else:
                    print(content)

        else:
            output.error(f"Unknown target: {target}")
            return 1

        return 0
    except Exception as e:
        output.error(f"Generation failed: {e}")
        output.debug_traceback()
        return 1


def cmd_tui(args: Namespace) -> int:
    """Start the interactive terminal UI."""
    output = setup_output(args)
    try:
        from moss.gen.tui import run_tui

        directory = Path(getattr(args, "directory", ".")).resolve()
        run_tui(directory)
        return 0
    except ImportError as e:
        output.error("TUI dependencies not installed. Install with: pip install 'moss[tui]'")
        output.debug(f"Details: {e}")
        return 1
    except KeyboardInterrupt:
        return 0


def cmd_lsp(args: Namespace) -> int:
    """Start the LSP server for IDE integration."""
    output = setup_output(args)
    try:
        from moss.lsp_server import start_server

        transport = getattr(args, "transport", "stdio")
        start_server(transport)
        return 0
    except ImportError as e:
        output.error("LSP dependencies not installed. Install with: pip install 'moss[lsp]'")
        output.debug(f"Details: {e}")
        return 1
    except KeyboardInterrupt:
        return 0


def cmd_shell(args: Namespace) -> int:
    """Start interactive shell."""
    from moss.shell import start_shell

    directory = Path(getattr(args, "directory", ".")).resolve()
    return start_shell(directory)


def cmd_watch(args: Namespace) -> int:
    """Watch for file changes and re-run tests."""
    import asyncio
    import shlex

    from moss.watch_tests import WatchRunner, WatchTestConfig

    output = setup_output(args)
    directory = Path(getattr(args, "directory", ".")).resolve()

    # Parse test command
    test_command = None
    cmd_str = getattr(args, "command", None)
    if cmd_str:
        test_command = shlex.split(cmd_str)

    # Build config
    config = WatchTestConfig(
        debounce_ms=getattr(args, "debounce", 500),
        clear_screen=not getattr(args, "no_clear", False),
        run_on_start=not getattr(args, "no_initial", False),
    )
    if test_command:
        config.test_command = test_command

    watcher = WatchRunner(directory, config, output)

    try:
        asyncio.run(watcher.run())
        return 0
    except KeyboardInterrupt:
        return 0


def cmd_hooks(args: Namespace) -> int:
    """Manage git pre-commit hooks."""
    from moss.hooks import (
        check_hooks_installed,
        generate_hook_config_yaml,
        install_hooks,
        uninstall_hooks,
    )

    output = setup_output(args)
    project_dir = Path(getattr(args, "directory", ".")).resolve()
    action = getattr(args, "action", "status")

    if action == "install":
        try:
            force = getattr(args, "force", False)
            install_hooks(project_dir, force=force)
            output.success("Pre-commit hooks installed successfully")
            return 0
        except FileNotFoundError as e:
            output.error(str(e))
            return 1
        except FileExistsError as e:
            output.error(str(e))
            return 1

    elif action == "uninstall":
        if uninstall_hooks(project_dir):
            output.success("Pre-commit hooks uninstalled")
            return 0
        else:
            output.warning("No moss hooks found to uninstall")
            return 0

    elif action == "config":
        # Generate pre-commit config
        try:
            config_yaml = generate_hook_config_yaml()
            output.print(config_yaml)
            return 0
        except ImportError:
            output.error("PyYAML not installed. Install with: pip install pyyaml")
            return 1

    else:  # status
        if check_hooks_installed(project_dir):
            output.success("Moss pre-commit hooks are installed")
        else:
            output.info("Moss pre-commit hooks are not installed")
            output.info("Run 'moss hooks install' to install them")
        return 0


def cmd_rules(args: Namespace) -> int:
    """Check code against custom rules."""
    from moss.rules import (
        EngineConfig,
        Severity,
        create_engine_with_builtins,
        load_rules_from_config,
    )
    from moss.sarif import SARIFConfig, generate_sarif, write_sarif

    output = setup_output(args)
    directory = Path(getattr(args, "directory", ".")).resolve()

    if not directory.exists():
        output.error(f"Directory {directory} does not exist")
        return 1

    # Load rules
    include_builtins = not getattr(args, "no_builtins", False)
    custom_rules = load_rules_from_config(directory)

    # Configure engine with file pattern
    pattern = getattr(args, "pattern", "**/*.py")
    config = EngineConfig(include_patterns=[pattern])

    engine = create_engine_with_builtins(
        include_builtins=include_builtins,
        custom_rules=custom_rules,
        config=config,
    )

    if not engine.rules:
        output.warning("No rules configured")
        return 0

    # List rules if requested
    if getattr(args, "list", False):
        output.header("Available Rules")
        for rule in engine.rules.values():
            status = "[enabled]" if rule.enabled else "[disabled]"
            backends = ", ".join(rule.backends)
            output.info(f"  {rule.name} ({backends}): {rule.description} {status}")
        return 0

    # Run analysis
    result = engine.check_directory(directory)

    if getattr(args, "json", False):
        output.data(result.to_dict())
        return 0

    # SARIF output
    sarif_path = getattr(args, "sarif", None)
    if sarif_path:
        from moss import __version__

        config = SARIFConfig(
            tool_name="moss",
            tool_version=__version__,
            base_path=directory,
        )
        sarif = generate_sarif(result, config)
        write_sarif(sarif, Path(sarif_path))
        output.success(f"SARIF output written to {sarif_path}")
        return 0

    # Text output
    if not result.violations:
        output.success(f"No violations found in {result.files_checked} files")
        return 0

    output.header(f"Found {len(result.violations)} violations")
    output.blank()

    # Group by file
    by_file: dict[Path, list] = {}
    for v in result.violations:
        file_path = v.location.file_path
        if file_path not in by_file:
            by_file[file_path] = []
        by_file[file_path].append(v)

    for file_path, violations in sorted(by_file.items()):
        try:
            rel_path = file_path.relative_to(directory)
        except ValueError:
            rel_path = file_path
        output.step(str(rel_path))

        for v in violations:
            severity_marker = {
                Severity.ERROR: "E",
                Severity.WARNING: "W",
                Severity.INFO: "I",
            }.get(v.severity, "?")
            output.info(f"  {v.location.line}:{v.location.column} [{severity_marker}] {v.message}")

        output.blank()

    # Summary
    errors = result.error_count
    warnings = result.warning_count
    infos = result.info_count
    output.info(f"Summary: {errors} errors, {warnings} warnings, {infos} info")

    # Return non-zero if errors found
    return 1 if errors > 0 else 0


def cmd_edit(args: Namespace) -> int:
    """Edit code with intelligent complexity routing."""
    from moss.edit import EditContext, TaskComplexity, analyze_complexity, edit

    output = setup_output(args)
    project_dir = Path(getattr(args, "directory", ".")).resolve()
    task = args.task

    # Build context
    target_file = None
    if args.file:
        target_file = (project_dir / args.file).resolve()
        if not target_file.exists():
            output.error(f"File {target_file} does not exist")
            return 1

    context = EditContext(
        project_root=project_dir,
        target_file=target_file,
        target_symbol=getattr(args, "symbol", None),
        language=getattr(args, "language", "python"),
        constraints=args.constraint or [],
    )

    # Analyze complexity
    complexity = analyze_complexity(task, context)

    if getattr(args, "analyze_only", False):
        output.header("Complexity Analysis")
        output.info(f"Task: {task}")
        output.info(f"Complexity: {complexity.value}")

        # Show which patterns matched
        if complexity == TaskComplexity.SIMPLE:
            output.info("Handler: structural editing (refactoring)")
        elif complexity == TaskComplexity.MEDIUM:
            output.info("Handler: multi-agent decomposition")
        elif complexity == TaskComplexity.COMPLEX:
            output.info("Handler: synthesis (with multi-agent fallback)")
        else:
            output.info("Handler: synthesis (novel problem)")

        return 0

    # Show what we're doing
    output.step(f"Editing ({complexity.value} complexity)...")

    # Force specific handler if requested
    force_method = getattr(args, "method", None)
    if force_method:
        output.verbose(f"Forcing method: {force_method}")

    async def run_edit():
        if force_method == "structural":
            from moss.edit import structural_edit

            return await structural_edit(task, context)
        elif force_method == "synthesis":
            from moss.edit import synthesize_edit

            return await synthesize_edit(task, context)
        else:
            return await edit(task, context)

    try:
        result = asyncio.run(run_edit())
    except Exception as e:
        output.error(f"Edit failed: {e}")
        return 1

    # Output result
    if getattr(args, "json", False):
        output_result(
            {
                "success": result.success,
                "method": result.method,
                "changes": [
                    {
                        "file": str(c.path),
                        "has_changes": c.has_changes,
                        "description": c.description,
                    }
                    for c in result.changes
                ],
                "iterations": result.iterations,
                "error": result.error,
                "metadata": result.metadata,
            },
            args,
        )
    else:
        if result.success:
            output.success(f"Edit complete (method: {result.method})")

            if result.changes:
                output.blank()
                output.step(f"Changes ({len(result.changes)} files):")
                for change in result.changes:
                    if change.has_changes:
                        output.info(f"  {change.path}")
                        if change.description:
                            output.verbose(f"    {change.description}")

                # Show diff if requested
                if getattr(args, "diff", False):
                    output.blank()
                    output.step("Diff:")
                    for change in result.changes:
                        if change.has_changes:
                            import difflib

                            diff = difflib.unified_diff(
                                change.original.splitlines(keepends=True),
                                change.modified.splitlines(keepends=True),
                                fromfile=f"a/{change.path.name}",
                                tofile=f"b/{change.path.name}",
                            )
                            output.print("".join(diff))

                # Apply changes if not dry-run
                if not getattr(args, "dry_run", False):
                    for change in result.changes:
                        if change.has_changes:
                            change.path.parent.mkdir(parents=True, exist_ok=True)
                            change.path.write_text(change.modified)
                    output.success("Changes applied")
                else:
                    output.info("(dry-run mode, changes not applied)")
            else:
                output.info("No changes needed")
        else:
            output.error(f"Edit failed: {result.error}")
            return 1

    return 0


def cmd_synthesize(args: Namespace) -> int:
    """Synthesize code from specification."""
    from moss.synthesis import (
        Context,
        Specification,
        SynthesisFramework,
    )
    from moss.synthesis.framework import SynthesisConfig
    from moss.synthesis.strategies import (
        PatternBasedDecomposition,
        TestDrivenDecomposition,
        TypeDrivenDecomposition,
    )
    from moss.synthesis.strategy import DecompositionStrategy

    output = setup_output(args)

    # Parse examples from "input:output" format
    examples: list[tuple[str, str]] = []
    if args.examples:
        for ex in args.examples:
            if ":" in ex:
                inp, out = ex.split(":", 1)
                examples.append((inp.strip(), out.strip()))
            else:
                output.warning(f"Invalid example format: {ex} (expected 'input:output')")

    # Build specification
    spec = Specification(
        description=args.description,
        type_signature=getattr(args, "type_signature", None),
        examples=tuple(examples),
        constraints=tuple(args.constraints or []),
    )

    # Set up strategies
    strategies: list[DecompositionStrategy] = []
    strategy_name = getattr(args, "strategy", "auto")

    if strategy_name == "auto":
        strategies = [
            TypeDrivenDecomposition(),
            TestDrivenDecomposition(),
            PatternBasedDecomposition(),
        ]
    elif strategy_name == "type_driven":
        strategies = [TypeDrivenDecomposition()]
    elif strategy_name == "test_driven":
        strategies = [TestDrivenDecomposition()]
    elif strategy_name == "pattern_based":
        strategies = [PatternBasedDecomposition()]

    # Set up generator
    from moss.synthesis.plugins import CodeGenerator

    generator: CodeGenerator | None = None
    generator_name = getattr(args, "generator", "auto")

    if generator_name != "auto":
        from moss.synthesis.plugins.generators import (
            LLMGenerator,
            MockLLMProvider,
            PlaceholderGenerator,
            TemplateGenerator,
        )

        if generator_name == "placeholder":
            generator = PlaceholderGenerator()
        elif generator_name == "template":
            generator = TemplateGenerator()
        elif generator_name == "llm":
            # Use mock provider by default (safe for testing)
            # For real LLM, use environment variables or config
            generator = LLMGenerator(provider=MockLLMProvider())
            output.info("Using LLM generator with mock provider")
            output.info("Set ANTHROPIC_API_KEY or OPENAI_API_KEY for real LLM")

    # Create framework
    config = SynthesisConfig(max_depth=getattr(args, "max_depth", 5))
    framework = SynthesisFramework(
        strategies=strategies,
        config=config,
        generator=generator,
    )

    # Show specification
    output.header("Specification")
    output.info(f"Description: {spec.description}")
    if spec.type_signature:
        output.info(f"Type signature: {spec.type_signature}")
    if spec.examples:
        output.info(f"Examples: {len(spec.examples)}")
        for inp, out in spec.examples[:3]:
            output.print(f"  {inp} -> {out}")
    if spec.constraints:
        output.info(f"Constraints: {', '.join(spec.constraints)}")
    output.blank()

    # Show decomposition if requested
    if getattr(args, "show_decomposition", False) or getattr(args, "dry_run", False):
        output.step("Analyzing decomposition...")

        # Find applicable strategies
        ctx = Context()
        applicable = []
        for strategy in strategies:
            if strategy.can_handle(spec, ctx):
                score = strategy.estimate_success(spec, ctx)
                applicable.append((strategy, score))

        if not applicable:
            output.warning("No applicable strategies found for this specification")
            return 1

        applicable.sort(key=lambda x: x[1], reverse=True)
        best_strategy, best_score = applicable[0]

        output.info(f"Best strategy: {best_strategy.name} (score: {best_score:.2f})")
        output.blank()

        # Show decomposition
        subproblems = best_strategy.decompose(spec, ctx)
        if subproblems:
            output.step(f"Decomposition ({len(subproblems)} subproblems):")
            for i, sub in enumerate(subproblems):
                deps = f" [deps: {sub.dependencies}]" if sub.dependencies else ""
                output.print(f"  {i}. {sub.specification.description}{deps}")
                if sub.specification.type_signature:
                    output.print(f"     Type: {sub.specification.type_signature}")
        else:
            output.info("No decomposition needed (atomic problem)")

        if getattr(args, "dry_run", False):
            output.blank()
            output.info("(dry-run mode, stopping before synthesis)")
            return 0

    # Run synthesis
    output.step("Synthesizing...")

    async def run_synthesis():
        return await framework.synthesize(spec)

    try:
        result = asyncio.run(run_synthesis())
    except Exception as e:
        output.error(f"Synthesis failed: {e}")
        return 1

    # Output result
    if getattr(args, "json", False):
        output_result(
            {
                "success": result.success,
                "code": result.solution,
                "iterations": result.iterations,
                "strategy": result.strategy_used,
                "metadata": result.metadata,
            },
            args,
        )
    else:
        if result.success and result.solution:
            output.success(f"Synthesis complete ({result.iterations} iterations)")
            if result.strategy_used:
                output.info(f"Strategy: {result.strategy_used}")
            output.blank()
            output.print(result.solution)
        else:
            output.error("Synthesis did not produce a result")
            if result.error:
                output.error(f"Error: {result.error}")
            return 1

    return 0


def cmd_metrics(args: Namespace) -> int:
    """Generate codebase metrics dashboard."""
    from moss.metrics import collect_metrics, generate_dashboard

    output = setup_output(args)
    directory = Path(getattr(args, "directory", ".")).resolve()

    if not directory.exists():
        output.error(f"Directory {directory} does not exist")
        return 1

    # Collect metrics
    pattern = getattr(args, "pattern", "**/*.py")
    metrics = collect_metrics(directory, pattern=pattern)

    if metrics.total_files == 0:
        output.warning("No Python files found")
        return 0

    if getattr(args, "json", False):
        output.data(metrics.to_dict())
        return 0

    # Generate HTML dashboard
    if getattr(args, "html", False) or getattr(args, "output", None):
        title = getattr(args, "title", None) or directory.name
        dashboard_html = generate_dashboard(metrics, title=title)

        output_path = getattr(args, "output", None)
        if output_path:
            Path(output_path).write_text(dashboard_html)
            output.success(f"Dashboard saved to {output_path}")
        else:
            output.print(dashboard_html)
        return 0

    # Text summary
    output.header("Codebase Metrics")
    output.info(f"Directory: {directory}")
    output.blank()

    output.step("Overview")
    output.info(f"  Files: {metrics.total_files}")
    output.info(f"  Lines of code: {metrics.total_code_lines:,}")
    output.info(f"  Total lines: {metrics.total_lines:,}")
    output.info(f"  Avg file size: {metrics.avg_file_lines:.0f} lines")
    output.blank()

    output.step("Symbols")
    output.info(f"  Classes: {metrics.total_classes}")
    output.info(f"  Functions: {metrics.total_functions}")
    output.info(f"  Methods: {metrics.total_methods}")
    output.blank()

    if metrics.modules:
        output.step("Modules")
        for mod in metrics.modules[:10]:
            output.info(f"  {mod.name}: {mod.file_count} files, {mod.total_lines:,} lines")

    return 0


def cmd_pr(args: Namespace) -> int:
    """Generate PR review summary."""
    from moss.pr_review import analyze_pr

    output = setup_output(args)
    repo_path = Path(getattr(args, "directory", ".")).resolve()

    try:
        review = analyze_pr(
            repo_path,
            from_ref=getattr(args, "base", "main"),
            to_ref=getattr(args, "head", "HEAD"),
            staged=getattr(args, "staged", False),
        )
    except Exception as e:
        output.error(f"Failed to analyze: {e}")
        return 1

    if review.diff_analysis.files_changed == 0:
        output.info("No changes found")
        return 0

    if getattr(args, "json", False):
        output.data(review.to_dict())
        return 0

    # Show title suggestion
    if getattr(args, "title", False):
        output.print(review.title_suggestion)
        return 0

    # Show full summary
    output.print(review.summary)

    return 0


def cmd_diff(args: Namespace) -> int:
    """Analyze git diff and show symbol changes."""
    from moss.diff_analysis import (
        analyze_diff,
        get_commit_diff,
        get_staged_diff,
        get_working_diff,
    )

    output = setup_output(args)
    repo_path = Path(getattr(args, "directory", ".")).resolve()

    # Get the appropriate diff
    try:
        if getattr(args, "staged", False):
            diff_output = get_staged_diff(repo_path)
        elif getattr(args, "working", False):
            diff_output = get_working_diff(repo_path)
        else:
            from_ref = getattr(args, "from_ref", "HEAD~1")
            to_ref = getattr(args, "to_ref", "HEAD")
            diff_output = get_commit_diff(repo_path, from_ref, to_ref)
    except Exception as e:
        output.error(f"Failed to get diff: {e}")
        return 1

    if not diff_output.strip():
        output.info("No changes found")
        return 0

    # Analyze the diff
    analysis = analyze_diff(diff_output)

    if getattr(args, "json", False):
        output.data(analysis.to_dict())
        return 0

    # Show statistics summary only
    if getattr(args, "stat", False):
        output.info(f"Files: {analysis.files_changed} changed")
        if analysis.files_added:
            output.info(f"  {analysis.files_added} added")
        if analysis.files_deleted:
            output.info(f"  {analysis.files_deleted} deleted")
        if analysis.files_renamed:
            output.info(f"  {analysis.files_renamed} renamed")
        output.info(f"Lines: +{analysis.total_additions} -{analysis.total_deletions}")
        return 0

    # Full output
    output.print(analysis.summary)

    return 0


def cmd_summarize(args: Namespace) -> int:
    """Generate hierarchical codebase summary."""
    output = setup_output(args)
    path = Path(getattr(args, "path", ".")).resolve()

    if not path.exists():
        output.error(f"Path not found: {path}")
        return 1

    compact = getattr(args, "compact", False)

    # Single file mode
    if path.is_file():
        suffix = path.suffix.lower()

        # Documentation file (markdown, rst, txt)
        if suffix in (".md", ".rst", ".txt"):
            from moss.summarize import DocSummarizer

            summarizer = DocSummarizer()
            summary = summarizer.summarize_file(path)

            if summary is None:
                output.error(f"Failed to read file: {path}")
                return 1

            if compact and not wants_json(args):
                output.print(summary.to_compact())
            elif wants_json(args):
                output.data(summary.to_dict())
            else:
                output.print(summary.to_markdown())

            return 0

        # Python file
        elif suffix == ".py":
            from moss.summarize import Summarizer

            summarizer = Summarizer(
                include_private=getattr(args, "include_private", False),
                include_tests=True,  # Include if explicitly targeting a test file
            )
            summary = summarizer.summarize_file(path)

            if summary is None:
                output.error(f"Failed to summarize: {path}")
                return 1

            if compact and not wants_json(args):
                # Compact format for single Python file
                lines_fmt = (
                    f"{summary.line_count / 1000:.0f}K"
                    if summary.line_count >= 1000
                    else str(summary.line_count)
                )
                parts = [f"{summary.module_name}.py"]
                parts.append(f"{lines_fmt} lines")
                parts.append(f"{len(summary.classes)} classes, {len(summary.functions)} funcs")
                output.print(" | ".join(parts))
            elif wants_json(args):
                output.data(
                    {
                        "module": summary.module_name,
                        "path": str(summary.path),
                        "docstring": summary.docstring,
                        "line_count": summary.line_count,
                        "classes": [
                            {"name": c.name, "docstring": c.docstring} for c in summary.classes
                        ],
                        "functions": [
                            {"name": f.name, "signature": f.signature, "docstring": f.docstring}
                            for f in summary.functions
                        ],
                    }
                )
            else:
                output.print(summary.to_markdown())

            return 0

        else:
            output.error(f"Unsupported file type: {suffix}")
            return 1

    # Directory mode
    # Check if --docs mode
    if getattr(args, "docs", False):
        from moss.summarize import DocSummarizer

        output.info(f"Summarizing documentation in {path.name}...")
        summarizer = DocSummarizer()

        try:
            summary = summarizer.summarize_docs(path)
        except Exception as e:
            output.error(f"Failed to summarize docs: {e}")
            return 1

        if compact and not wants_json(args):
            output.print(summary.to_compact())
        elif wants_json(args):
            output.data(summary.to_dict())
        else:
            output.print(summary.to_markdown())

        return 0

    # Default: summarize code
    from moss.summarize import Summarizer

    output.info(f"Summarizing {path.name}...")

    summarizer = Summarizer(
        include_private=getattr(args, "include_private", False),
        include_tests=getattr(args, "include_tests", False),
    )

    try:
        summary = summarizer.summarize_project(path)
    except Exception as e:
        output.error(f"Failed to summarize: {e}")
        return 1

    # Output format
    if compact and not wants_json(args):
        output.print(summary.to_compact())
    elif wants_json(args):
        output.data(summary.to_dict())
    else:
        output.print(summary.to_markdown())

    return 0


def cmd_check_docs(args: Namespace) -> int:
    """Check documentation freshness against codebase."""
    from moss import MossAPI

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Checking docs in {root.name}...")

    api = MossAPI.for_project(root)

    try:
        result = api.health.check_docs(check_links=getattr(args, "check_links", False))
    except Exception as e:
        output.error(f"Failed to check docs: {e}")
        return 1

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(result.to_compact())
    elif wants_json(args):
        output.data(result.to_dict())
    else:
        output.print(result.to_markdown())

    # Exit code based on issues
    if result.has_errors:
        return 1
    if getattr(args, "strict", False) and result.has_warnings:
        return 1

    return 0


def cmd_check_todos(args: Namespace) -> int:
    """Check TODOs against implementation status."""
    from moss import MossAPI

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Checking TODOs in {root.name}...")

    api = MossAPI.for_project(root)

    try:
        result = api.health.check_todos()
    except Exception as e:
        output.error(f"Failed to check TODOs: {e}")
        return 1

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(result.to_compact())
    elif wants_json(args):
        output.data(result.to_dict())
    else:
        output.print(result.to_markdown())

    # Exit code based on issues
    if getattr(args, "strict", False) and result.orphan_count > 0:
        return 1

    return 0


def cmd_mutate(args: Namespace) -> int:
    """Run mutation testing to find undertested code."""
    import asyncio

    from moss.mutation import MutationAnalyzer

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    analyzer = MutationAnalyzer(root)

    if not analyzer.is_available():
        output.error("mutmut not installed. Run: pip install mutmut")
        return 1

    output.info("Running mutation testing (this may take several minutes)...")

    # Get options
    quick_check = getattr(args, "quick", False)
    since = getattr(args, "since", None)
    paths_arg = getattr(args, "paths", None)
    paths = [Path(p) for p in paths_arg] if paths_arg else None

    try:
        result = asyncio.run(
            analyzer.run(
                quick_check=quick_check,
                paths=paths,
                since=since,
            )
        )
    except Exception as e:
        output.error(f"Mutation testing failed: {e}")
        return 1

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(result.to_compact())
    elif wants_json(args):
        output.data(result.to_dict())
    else:
        output.print(result.to_markdown())

    # Exit code based on mutation score
    if getattr(args, "strict", False):
        if result.mutation_score < 0.8:  # 80% threshold
            output.warning(f"Mutation score {result.mutation_score:.0%} below 80% threshold")
            return 1

    return 0


def cmd_check_refs(args: Namespace) -> int:
    """Check bidirectional references between code and docs."""
    from moss import MossAPI

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    staleness_days = getattr(args, "staleness_days", 30)
    api = MossAPI.for_project(root)

    output.info(f"Checking references in {root.name}...")

    try:
        result = api.ref_check.check(staleness_days=staleness_days)
    except Exception as e:
        output.error(f"Failed to check references: {e}")
        return 1

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(result.to_compact())
    elif wants_json(args):
        output.data(result.to_dict())
    else:
        output.print(result.to_markdown())

    # Exit codes
    if result.has_errors:
        return 1
    if getattr(args, "strict", False) and result.has_warnings:
        return 1

    return 0


def cmd_external_deps(args: Namespace) -> int:
    """Analyze external dependencies from pyproject.toml/requirements.txt."""
    from moss import MossAPI

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    api = MossAPI.for_project(root)
    resolve = getattr(args, "resolve", False)
    warn_weight = getattr(args, "warn_weight", 0)
    check_vulns = getattr(args, "check_vulns", False)
    check_licenses = getattr(args, "check_licenses", False)

    output.info(f"Analyzing dependencies in {root.name}...")

    try:
        result = api.external_deps.analyze(
            resolve=resolve, check_vulns=check_vulns, check_licenses=check_licenses
        )
    except Exception as e:
        output.error(f"Failed to analyze dependencies: {e}")
        return 1

    if not result.sources:
        output.warning("No dependency files found (pyproject.toml, requirements.txt)")
        return 0

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(result.to_compact())
    elif wants_json(args):
        output.data(result.to_dict(weight_threshold=warn_weight))
    else:
        output.print(result.to_markdown(weight_threshold=warn_weight))

    # Exit with error if heavy deps found and threshold set
    if warn_weight > 0 and result.get_heavy_dependencies(warn_weight):
        return 1

    # Exit with error if vulnerabilities found
    if check_vulns and result.has_vulnerabilities:
        return 1

    # Exit with error if license issues found
    if check_licenses and result.has_license_issues:
        return 1

    return 0


def cmd_roadmap(args: Namespace) -> int:
    """Show project roadmap and progress from TODO.md."""
    from moss.roadmap import display_roadmap, find_todo_md

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    # Find TODO.md
    todo_path = find_todo_md(root)
    if todo_path is None:
        output.error("TODO.md not found")
        return 1

    # Determine display mode
    # --plain explicitly sets plain text (good for LLMs)
    # --tui explicitly sets TUI
    # Default: TUI if stdout is a TTY, plain otherwise
    use_tui = getattr(args, "tui", False)
    use_plain = getattr(args, "plain", False)

    if use_plain:
        tui = False
    elif use_tui:
        tui = True
    else:
        # Auto-detect: TUI for humans at terminal, plain for piping/LLMs
        import sys

        tui = sys.stdout.isatty()

    use_color = not getattr(args, "no_color", False) and tui
    width = getattr(args, "width", 80)
    show_completed = getattr(args, "completed", False)
    max_items = getattr(args, "max_items", 0)

    return display_roadmap(
        path=todo_path,
        tui=tui,
        show_completed=show_completed,
        use_color=use_color,
        width=width,
        max_items=max_items,
    )


def cmd_analyze_session(args: Namespace) -> int:
    """Analyze a Claude Code session log."""
    from moss.session_analysis import analyze_session

    output = setup_output(args)
    session_path = Path(getattr(args, "session_path", ""))

    if not session_path.exists():
        output.error(f"Session file not found: {session_path}")
        return 1

    output.info(f"Analyzing {session_path.name}...")

    try:
        analysis = analyze_session(session_path)
    except Exception as e:
        output.error(f"Failed to analyze session: {e}")
        return 1

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(analysis.to_compact())
    elif wants_json(args):
        output.data(analysis.to_dict())
    else:
        output.print(analysis.to_markdown())

    return 0


def cmd_extract_preferences(args: Namespace) -> int:
    """Extract user preferences from session logs."""
    from moss.preferences import LogFormat, extract_preferences, format_preferences

    output = setup_output(args)
    paths = [Path(p) for p in getattr(args, "session_paths", [])]
    output_format = getattr(args, "format", "generic")
    synthesize = getattr(args, "synthesize", False)
    min_confidence = getattr(args, "min_confidence", "low")
    log_format_str = getattr(args, "log_format", "auto")

    # Validate paths
    for path in paths:
        if not path.exists():
            output.error(f"Session file not found: {path}")
            return 1

    # Map log format
    log_format_map = {
        "auto": LogFormat.AUTO,
        "claude": LogFormat.CLAUDE_CODE,
        "gemini": LogFormat.GEMINI_CLI,
        "cline": LogFormat.CLINE,
        "roo": LogFormat.ROO_CODE,
        "aider": LogFormat.AIDER,
    }
    log_format = log_format_map.get(log_format_str, LogFormat.AUTO)

    # Map confidence
    from moss.preferences import ConfidenceLevel

    confidence_map = {
        "low": ConfidenceLevel.LOW,
        "medium": ConfidenceLevel.MEDIUM,
        "high": ConfidenceLevel.HIGH,
    }
    min_conf = confidence_map.get(min_confidence, ConfidenceLevel.LOW)

    output.info(f"Extracting preferences from {len(paths)} session(s)...")

    try:
        prefs = extract_preferences(paths, log_format=log_format, min_confidence=min_conf)
    except Exception as e:
        output.error(f"Failed to extract preferences: {e}")
        return 1

    # Optionally synthesize with LLM
    if synthesize:
        output.info("Synthesizing with LLM...")
        try:
            from moss.preferences.synthesis import synthesize_preferences

            provider = getattr(args, "provider", None)
            model = getattr(args, "model", None)
            result = synthesize_preferences(prefs, provider=provider, model=model)
            prefs = result.preferences
            output.verbose(f"Synthesis used {result.tokens_used} tokens")
        except Exception as e:
            output.warning(f"Synthesis failed, using raw extraction: {e}")

    # Output
    if wants_json(args):
        output.data(prefs.to_dict())
    else:
        formatted = format_preferences(prefs, output_format)
        output.print(formatted)

    output.success(f"Extracted {len(prefs.preferences)} preferences")
    return 0


def cmd_diff_preferences(args: Namespace) -> int:
    """Compare two preference extractions."""
    import json

    from moss.preferences import PreferenceSet, diff_preferences

    output = setup_output(args)
    old_path = Path(getattr(args, "old_path", ""))
    new_path = Path(getattr(args, "new_path", ""))

    # Validate paths
    if not old_path.exists():
        output.error(f"Old preferences file not found: {old_path}")
        return 1
    if not new_path.exists():
        output.error(f"New preferences file not found: {new_path}")
        return 1

    try:
        # Load preference sets from JSON
        with open(old_path) as f:
            old_data = json.load(f)
        with open(new_path) as f:
            new_data = json.load(f)

        # Reconstruct PreferenceSets
        from moss.preferences import ConfidenceLevel, Preference, PreferenceCategory

        def load_prefs(data: dict) -> PreferenceSet:
            ps = PreferenceSet(sources=data.get("sources", []))
            for cat_name, cat_prefs in data.get("by_category", {}).items():
                for p in cat_prefs:
                    ps.add(
                        Preference(
                            category=PreferenceCategory(cat_name),
                            rule=p["rule"],
                            confidence=ConfidenceLevel(p["confidence"]),
                        )
                    )
            return ps

        old_prefs = load_prefs(old_data)
        new_prefs = load_prefs(new_data)

        diff = diff_preferences(old_prefs, new_prefs)

    except Exception as e:
        output.error(f"Failed to compare preferences: {e}")
        return 1

    # Output
    if wants_json(args):
        output.data(diff.to_dict())
    else:
        output.print(diff.to_markdown())

    # Summary
    if diff.has_changes:
        added = len(diff.added)
        removed = len(diff.removed)
        changed = len(diff.changed)
        output.info(f"Changes: +{added} added, -{removed} removed, ~{changed} modified")
    else:
        output.success("No changes detected")

    return 0


def cmd_git_hotspots(args: Namespace) -> int:
    """Find frequently changed files in git history."""
    from moss import MossAPI

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()
    days = getattr(args, "days", 90)

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Analyzing git history for {root.name} (last {days} days)...")

    api = MossAPI.for_project(root)
    try:
        analysis = api.git_hotspots.analyze(days=days)
    except Exception as e:
        output.error(f"Failed to analyze git history: {e}")
        return 1

    if analysis.error:
        output.error(analysis.error)
        return 1

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(analysis.to_compact())
    elif wants_json(args):
        output.data(analysis.to_dict())
    else:
        output.print(analysis.to_markdown())

    return 0


def cmd_coverage(args: Namespace) -> int:
    """Show test coverage statistics."""
    from moss.test_coverage import analyze_coverage

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()
    run_tests = getattr(args, "run", False)

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    if run_tests:
        output.info(f"Running tests with coverage for {root.name}...")
    else:
        output.info(f"Checking coverage data for {root.name}...")

    try:
        report = analyze_coverage(root, run_tests=run_tests)
    except Exception as e:
        output.error(f"Failed to analyze coverage: {e}")
        return 1

    if report.error:
        output.error(report.error)
        return 1

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(report.to_compact())
    elif wants_json(args):
        output.data(report.to_dict())
    else:
        output.print(report.to_markdown())

    return 0


def cmd_lint(args: Namespace) -> int:
    """Run unified linting across multiple tools.

    Runs configured linters (ruff, mypy, etc.) and combines their output
    into a unified format.
    """
    import asyncio

    from moss.plugins.linters import get_linter_registry

    output = setup_output(args)
    paths_arg = getattr(args, "paths", None) or ["."]
    paths = [Path(p).resolve() for p in paths_arg]

    # Validate paths
    for path in paths:
        if not path.exists():
            output.error(f"Path not found: {path}")
            return 1

    # Get registry and available linters
    registry = get_linter_registry()
    registry.register_builtins()

    # Filter by linter name if specified
    linter_names = getattr(args, "linters", None)
    if linter_names:
        linters = [registry.get(name) for name in linter_names.split(",")]
        linters = [linter for linter in linters if linter is not None]
        if not linters:
            output.error(f"No linters found matching: {linter_names}")
            output.info(
                f"Available: {', '.join(p.metadata.name for p in registry.get_available())}"
            )
            return 1
    else:
        linters = registry.get_available()

    if not linters:
        output.warning("No linters available")
        return 0

    linter_names_str = ", ".join(linter.metadata.name for linter in linters)
    output.info(f"Running {len(linters)} linter(s): {linter_names_str}")

    # Collect files to lint
    files_to_lint: list[Path] = []
    pattern = getattr(args, "pattern", "**/*.py")
    for path in paths:
        if path.is_file():
            files_to_lint.append(path)
        else:
            files_to_lint.extend(path.glob(pattern))

    if not files_to_lint:
        output.info("No files to lint")
        return 0

    output.info(f"Checking {len(files_to_lint)} file(s)...")

    # Run linters
    async def run_linters() -> list[tuple[str, Any]]:
        results = []
        for linter in linters:
            # Check file extension against supported languages
            supported_exts = {
                "python": {".py", ".pyi"},
                "javascript": {".js", ".jsx", ".mjs"},
                "typescript": {".ts", ".tsx"},
            }
            linter_exts: set[str] = set()
            for lang in linter.metadata.languages:
                linter_exts.update(supported_exts.get(lang, set()))

            for file_path in files_to_lint:
                if not linter_exts or file_path.suffix in linter_exts:
                    result = await linter.run(file_path)
                    results.append((linter.metadata.name, result))
        return results

    all_results = asyncio.run(run_linters())

    # Combine and format results
    total_issues = 0
    errors = 0
    warnings = 0
    grouped_by_file: dict[Path, list] = {}

    for linter_name, result in all_results:
        if not result.success:
            errors += 1
        for issue in result.issues:
            total_issues += 1
            if issue.severity.name == "ERROR":
                errors += 1
            elif issue.severity.name == "WARNING":
                warnings += 1

            file_key = issue.file or Path("unknown")
            if file_key not in grouped_by_file:
                grouped_by_file[file_key] = []
            grouped_by_file[file_key].append((linter_name, issue))

    # Output
    if wants_json(args):
        json_output = {
            "total_issues": total_issues,
            "errors": errors,
            "warnings": warnings,
            "files": {
                str(f): [
                    {
                        "linter": ln,
                        "message": i.message,
                        "severity": i.severity.name,
                        "line": i.line,
                        "column": i.column,
                        "rule_id": i.rule_id,
                    }
                    for ln, i in issues
                ]
                for f, issues in grouped_by_file.items()
            },
        }
        output.data(json_output)
    else:
        # Text output grouped by file
        for file_path, issues in sorted(grouped_by_file.items()):
            output.header(str(file_path))
            for _linter_name, issue in issues:
                loc = f":{issue.line}" if issue.line else ""
                loc += f":{issue.column}" if issue.column else ""
                rule = f" [{issue.rule_id}]" if issue.rule_id else ""
                severity = issue.severity.name.lower()
                output.print(f"  {loc} {severity}{rule}: {issue.message}")

        output.blank()
        if total_issues == 0:
            output.success("No issues found")
        else:
            output.info(f"Found {total_issues} issue(s): {errors} error(s), {warnings} warning(s)")

    # Return non-zero if errors found
    fix = getattr(args, "fix", False)
    if fix and errors == 0:
        output.info("Running fixes...")
        # TODO: Implement fix mode by calling linter.fix() methods
        output.warning("Fix mode not yet implemented")

    return 1 if errors > 0 else 0


def cmd_checkpoint(args: Namespace) -> int:
    """Manage checkpoints (shadow branches) for safe code modifications.

    Subcommands:
    - create: Create a checkpoint with current changes
    - list: List active checkpoints
    - diff: Show changes in a checkpoint
    - merge: Merge checkpoint changes into base branch
    - abort: Abandon a checkpoint
    - restore: Revert working directory to checkpoint state
    """
    import asyncio

    from moss import MossAPI

    output = setup_output(args)
    root = Path(".").resolve()
    action = getattr(args, "action", "list")
    name = getattr(args, "name", None)
    message = getattr(args, "message", None)

    # Verify we're in a git repo
    if not (root / ".git").exists():
        output.error("Not a git repository")
        return 1

    api = MossAPI.for_project(root)

    async def run_action() -> int:
        if action == "create":
            try:
                result = await api.git.create_checkpoint(name=name, message=message)
                output.success(f"Created checkpoint: {result['branch']}")
                output.info(f"Commit: {result['commit'][:8]}")
            except Exception as e:
                output.error(f"Failed to create checkpoint: {e}")
                return 1

        elif action == "list":
            try:
                checkpoints = await api.git.list_checkpoints()
                if not checkpoints:
                    output.info("No active checkpoints")
                else:
                    output.header("Active Checkpoints")
                    for cp in checkpoints:
                        output.print(f"    {cp['name']} ({cp['type']})")
            except Exception as e:
                output.error(f"Failed to list checkpoints: {e}")
                return 1

        elif action == "diff":
            if not name:
                output.error("Checkpoint name required for diff")
                return 1
            try:
                result = await api.git.diff_checkpoint(name)
                if result["diff"]:
                    output.print(result["diff"])
                else:
                    output.info("No differences")
            except Exception as e:
                output.error(f"Failed to get diff: {e}")
                return 1

        elif action == "merge":
            if not name:
                output.error("Checkpoint name required for merge")
                return 1
            try:
                result = await api.git.merge_checkpoint(name, message=message)
                output.success(f"Merged checkpoint {name}")
                output.info(f"Commit: {result['commit'][:8]}")
            except Exception as e:
                output.error(f"Failed to merge: {e}")
                return 1

        elif action == "abort":
            if not name:
                output.error("Checkpoint name required for abort")
                return 1
            try:
                await api.git.abort_checkpoint(name)
                output.success(f"Aborted checkpoint: {name}")
            except Exception as e:
                output.error(f"Failed to abort: {e}")
                return 1

        elif action == "restore":
            if not name:
                output.error("Checkpoint name required for restore")
                return 1
            try:
                result = await api.git.restore_checkpoint(name)
                output.success(f"Restored checkpoint: {name}")
                output.info(f"Now at commit: {result['commit'][:8]}")
            except Exception as e:
                output.error(f"Failed to restore: {e}")
                return 1

        else:
            output.error(f"Unknown action: {action}")
            return 1

        return 0

    return asyncio.run(run_action())


def cmd_complexity(args: Namespace) -> int:
    """Analyze cyclomatic complexity of functions."""
    from moss import MossAPI

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()
    pattern = getattr(args, "pattern", "**/*.py")

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Analyzing complexity for {root.name}...")

    try:
        api = MossAPI.for_project(root)
        report = api.complexity.analyze(pattern=pattern)
    except Exception as e:
        output.error(f"Failed to analyze complexity: {e}")
        return 1

    if report.error:
        output.error(report.error)
        return 1

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(report.to_compact())
    elif wants_json(args):
        output.data(report.to_dict())
    else:
        output.print(report.to_markdown())

    return 0


def cmd_clones(args: Namespace) -> int:
    """Detect structural clones via AST hashing."""
    from moss import MossAPI
    from moss.clones import format_clone_analysis

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()
    level = getattr(args, "level", 0)
    min_lines = getattr(args, "min_lines", 3)

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Detecting clones in {root.name} (level {level})...")

    api = MossAPI.for_project(root)
    try:
        analysis = api.clones.detect(level=level, min_lines=min_lines)
    except Exception as e:
        output.error(f"Failed to detect clones: {e}")
        return 1

    if wants_json(args):
        output.data(analysis.to_dict())
    else:
        show_source = getattr(args, "source", False)
        output.print(format_clone_analysis(analysis, show_source=show_source))

    return 0


def cmd_security(args: Namespace) -> int:
    """Run security analysis with multiple tools."""
    from moss import MossAPI
    from moss.security import format_security_analysis

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()
    tools = getattr(args, "tools", None)
    min_severity = getattr(args, "severity", "low")

    if tools:
        tools = [t.strip() for t in tools.split(",")]

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Running security analysis on {root.name}...")

    api = MossAPI.for_project(root)
    try:
        analysis = api.security.analyze(tools=tools, min_severity=min_severity)
    except Exception as e:
        output.error(f"Security analysis failed: {e}")
        return 1

    if wants_json(args):
        output.data(analysis.to_dict())
    else:
        output.print(format_security_analysis(analysis))

    # Return non-zero if critical/high findings
    if analysis.critical_count > 0 or analysis.high_count > 0:
        return 1

    return 0


def cmd_patterns(args: Namespace) -> int:
    """Detect architectural patterns in the codebase."""
    from moss.patterns import analyze_patterns, format_pattern_analysis

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()
    patterns_arg = getattr(args, "patterns", None)

    if patterns_arg:
        patterns_list = [p.strip() for p in patterns_arg.split(",")]
    else:
        patterns_list = None

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Analyzing patterns in {root.name}...")

    try:
        analysis = analyze_patterns(root, patterns=patterns_list)
    except Exception as e:
        output.error(f"Pattern analysis failed: {e}")
        return 1

    if wants_json(args):
        output.data(analysis.to_dict())
    else:
        output.print(format_pattern_analysis(analysis))

    return 0


def cmd_weaknesses(args: Namespace) -> int:
    """Identify architectural weaknesses and gaps in the codebase."""
    from moss import MossAPI
    from moss.weaknesses import (
        format_weakness_fixes,
        get_fixable_weaknesses,
        weaknesses_to_sarif,
    )

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()
    categories_arg = getattr(args, "categories", None)
    sarif_output = getattr(args, "sarif", None)
    show_fixes = getattr(args, "fix", False)

    if categories_arg:
        categories = [c.strip() for c in categories_arg.split(",")]
    else:
        categories = None

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Analyzing architectural weaknesses in {root.name}...")

    api = MossAPI.for_project(root)
    try:
        analysis = api.weaknesses.analyze(categories=categories)
    except Exception as e:
        output.error(f"Weakness analysis failed: {e}")
        return 1

    # Output SARIF if requested
    if sarif_output:
        sarif_path = Path(sarif_output)
        weaknesses_to_sarif(analysis, output_path=sarif_path)
        output.info(f"SARIF output written to {sarif_path}")
        if wants_json(args):
            output.data({"sarif_path": str(sarif_path), "weaknesses": len(analysis.weaknesses)})
        return 0

    # Show fix suggestions if requested
    if show_fixes:
        fixes = get_fixable_weaknesses(analysis)
        if wants_json(args):
            output.data(
                {
                    "total_weaknesses": len(analysis.weaknesses),
                    "fixable": len(fixes),
                    "fixes": [
                        {
                            "weakness": f.weakness.title,
                            "file": f.weakness.file_path,
                            "line": f.weakness.line_start,
                            "fix_type": f.fix_type,
                            "description": f.description,
                            "commands": f.commands,
                            "code_changes": f.code_changes,
                        }
                        for f in fixes
                    ],
                }
            )
        else:
            output.print(api.weaknesses.format(analysis))
            output.print("")
            output.print(format_weakness_fixes(fixes))
        return 0

    # Standard output
    if wants_json(args):
        output.data(analysis.to_dict())
    else:
        output.print(api.weaknesses.format(analysis))

    # Return non-zero if high severity issues found
    high_count = len(
        analysis.by_severity.get(
            __import__("moss.weaknesses", fromlist=["Severity"]).Severity.HIGH, []
        )
    )
    if high_count > 0:
        return 1

    return 0


def cmd_rag(args: Namespace) -> int:
    """Semantic search with RAG indexing."""
    import asyncio

    from moss import MossAPI
    from moss.rag import format_search_results

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()
    action = getattr(args, "action", "search")

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    api = MossAPI.for_project(root)

    if action == "index":
        output.info(f"Indexing {root.name}...")
        force = getattr(args, "force", False)
        try:
            count = asyncio.run(api.rag.index(force=force))
            output.success(f"Indexed {count} chunks")
            return 0
        except Exception as e:
            output.error(f"Indexing failed: {e}")
            return 1

    elif action == "search":
        query = getattr(args, "query", None)
        if not query:
            output.error("Query required for search")
            return 1

        limit = getattr(args, "limit", 10)
        mode = getattr(args, "mode", "hybrid")

        try:
            results = asyncio.run(api.rag.search(query, limit=limit, mode=mode))
        except Exception as e:
            output.error(f"Search failed: {e}")
            return 1

        if wants_json(args):
            output.data([r.to_dict() for r in results])
        else:
            output.print(format_search_results(results))

        return 0

    elif action == "stats":
        try:
            stats = asyncio.run(api.rag.stats())
        except Exception as e:
            output.error(f"Failed to get stats: {e}")
            return 1

        if wants_json(args):
            output.data(stats.to_dict())
        else:
            output.print("**Index Statistics**")
            output.print(f"  Backend: {stats.backend}")
            output.print(f"  Path: {stats.index_path}")
            output.print(f"  Documents: {stats.total_documents}")
            output.print(f"  Files indexed: {stats.files_indexed}")

        return 0

    elif action == "clear":
        try:
            asyncio.run(api.rag.clear())
            output.success("Index cleared")
            return 0
        except Exception as e:
            output.error(f"Failed to clear index: {e}")
            return 1

    else:
        output.error(f"Unknown action: {action}")
        return 1


def cmd_loop(args: Namespace) -> int:
    """Run composable agent loops.

    Subcommands:
    - list: Show available loop templates
    - run: Execute a loop on a file
    - benchmark: Compare loop performance
    """
    import asyncio

    from moss.agent_loop import (
        AgentLoopRunner,
        BenchmarkTask,
        LLMConfig,
        LLMToolExecutor,
        LoopBenchmark,
        analysis_loop,
        critic_loop,
        docstring_apply_loop,
        docstring_full_loop,
        docstring_loop,
        incremental_loop,
        simple_loop,
    )

    output = setup_output(args)
    action = getattr(args, "action", "list")

    # Built-in loop registry
    loops = {
        "simple": simple_loop,
        "critic": critic_loop,
        "incremental": incremental_loop,
        "analysis": analysis_loop,
        "docstring": docstring_loop,
        "docstring_full": docstring_full_loop,
        "docstring_apply": docstring_apply_loop,
    }

    if action == "list":
        output.header("Available Loops")
        for name, factory in loops.items():
            loop = factory()
            steps = ", ".join(s.name for s in loop.steps)
            output.print(f"  {name}: {steps}")
        return 0

    elif action == "run":
        loop_name = getattr(args, "loop_name", "simple")
        file_path = getattr(args, "file", None)
        mock = getattr(args, "mock", False)
        model = getattr(args, "model", None)

        if loop_name not in loops:
            output.error(f"Unknown loop: {loop_name}. Use 'moss loop list' to see options.")
            return 1

        if not file_path:
            output.error("File path required. Use --file <path>")
            return 1

        file_path = Path(file_path).resolve()
        if not file_path.exists():
            output.error(f"File not found: {file_path}")
            return 1

        loop = loops[loop_name]()
        config = LLMConfig(mock=mock)
        if model:
            config.model = model

        executor = LLMToolExecutor(config=config, root=file_path.parent)
        runner = AgentLoopRunner(executor)

        output.info(f"Running '{loop_name}' loop on {file_path.name}...")

        async def run_loop():
            return await runner.run(loop, initial_input={"file_path": str(file_path)})

        result = asyncio.run(run_loop())

        if wants_json(args):
            output.data(
                {
                    "status": result.status.name,
                    "success": result.success,
                    "metrics": {
                        "llm_calls": result.metrics.llm_calls,
                        "llm_tokens": result.metrics.llm_tokens_in + result.metrics.llm_tokens_out,
                        "tool_calls": result.metrics.tool_calls,
                        "wall_time": result.metrics.wall_time_seconds,
                    },
                    "error": result.error,
                }
            )
        else:
            status = "✓" if result.success else "✗"
            output.print(f"{status} {result.status.name}")
            output.print(
                f"  LLM: {result.metrics.llm_calls} calls, "
                f"{result.metrics.llm_tokens_in + result.metrics.llm_tokens_out} tokens"
            )
            output.print(f"  Tools: {result.metrics.tool_calls} calls")
            output.print(f"  Time: {result.metrics.wall_time_seconds:.2f}s")
            if result.error:
                output.error(f"  Error: {result.error}")

        return 0 if result.success else 1

    elif action == "benchmark":
        loop_names = getattr(args, "loops", None) or list(loops.keys())
        file_path = getattr(args, "file", None)
        mock = getattr(args, "mock", True)  # Default to mock for benchmarks

        if not file_path:
            output.error("File path required. Use --file <path>")
            return 1

        file_path = Path(file_path).resolve()
        if not file_path.exists():
            output.error(f"File not found: {file_path}")
            return 1

        config = LLMConfig(mock=mock)
        executor = LLMToolExecutor(config=config, root=file_path.parent)
        benchmark = LoopBenchmark(executor=executor)

        tasks = [BenchmarkTask(name=file_path.name, input_data={"file_path": str(file_path)})]

        output.info(f"Benchmarking {len(loop_names)} loops...")

        async def run_benchmark():
            results = []
            for name in loop_names:
                if name not in loops:
                    output.warning(f"Unknown loop: {name}, skipping")
                    continue
                loop = loops[name]()
                result = await benchmark.run(loop, tasks)
                results.append(result)
            return results

        results = asyncio.run(run_benchmark())

        if wants_json(args):
            output.data(
                [
                    {
                        "loop": r.loop_name,
                        "success_rate": r.success_rate,
                        "avg_llm_calls": r.avg_llm_calls,
                        "avg_tool_calls": r.avg_tool_calls,
                        "total_time": r.total_time_seconds,
                    }
                    for r in results
                ]
            )
        else:
            output.header("Loop Comparison")
            for r in sorted(results, key=lambda x: x.avg_llm_calls):
                output.print(
                    f"  {r.loop_name}: {r.success_rate:.0%} success, "
                    f"{r.avg_llm_calls:.1f} LLM calls, {r.avg_tool_calls:.1f} tool calls"
                )

        return 0

    else:
        output.error(f"Unknown action: {action}")
        return 1


def cmd_workflow(args: Namespace) -> int:
    """Manage and run TOML-based workflows.

    Subcommands:
    - list: Show available workflows
    - show: Show workflow details
    - run: Execute a workflow on a file
    """
    import asyncio

    from moss.workflows import (
        list_workflows,
        load_workflow,
        workflow_to_agent_loop,
    )

    output = setup_output(args)
    action = getattr(args, "action", "list")
    project_root = Path(getattr(args, "directory", ".")).resolve()

    if action == "list":
        output.header("Available Workflows")
        workflows = list_workflows(project_root)
        if not workflows:
            output.print("  (none found)")
        for name in workflows:
            try:
                wf = load_workflow(name, project_root)
                output.print(f"  {name}: {wf.description or '(no description)'}")
            except Exception as e:
                output.print(f"  {name}: (error loading: {e})")
        return 0

    elif action == "show":
        name = getattr(args, "workflow_name", None)
        if not name:
            output.error("Workflow name required")
            return 1

        try:
            wf = load_workflow(name, project_root)
        except FileNotFoundError:
            output.error(f"Workflow not found: {name}")
            return 1

        if wants_json(args):
            output.data(wf.to_dict())
        else:
            output.header(f"Workflow: {wf.name}")
            output.print(f"Description: {wf.description or '(none)'}")
            output.print(f"Version: {wf.version}")
            output.print(f"Max steps: {wf.limits.max_steps}")
            output.print(f"Token budget: {wf.limits.token_budget}")
            output.print(f"Timeout: {wf.limits.timeout_seconds}s")
            output.print(f"Model: {wf.llm.model}")
            output.print("")
            output.header("Steps")
            for i, step in enumerate(wf.steps, 1):
                output.print(f"  {i}. {step.name} ({step.tool})")
                if step.input_from:
                    output.print(f"     input_from: {step.input_from}")
                if step.on_error:
                    output.print(f"     on_error: {step.on_error}")
        return 0

    elif action == "run":
        name = getattr(args, "workflow_name", None)
        file_path = getattr(args, "file", None)
        mock = getattr(args, "mock", False)

        if not name:
            output.error("Workflow name required")
            return 1

        if not file_path:
            output.error("File path required. Use --file <path>")
            return 1

        file_path = Path(file_path).resolve()
        if not file_path.exists():
            output.error(f"File not found: {file_path}")
            return 1

        try:
            wf = load_workflow(name, project_root)
        except FileNotFoundError:
            output.error(f"Workflow not found: {name}")
            return 1

        output.info(f"Running workflow '{name}' on {file_path.name}...")

        # Convert and run
        from moss.agent_loop import AgentLoopRunner, LLMConfig, LLMToolExecutor

        loop = workflow_to_agent_loop(wf)
        config = LLMConfig(
            model=wf.llm.model,
            temperature=wf.llm.temperature,
            system_prompt=wf.llm.system_prompt,
            mock=mock,
        )
        executor = LLMToolExecutor(config=config, root=file_path.parent)
        runner = AgentLoopRunner(executor)

        async def do_run():
            return await runner.run(loop, initial_input={"file_path": str(file_path)})

        result = asyncio.run(do_run())

        if wants_json(args):
            output.data(
                {
                    "workflow": name,
                    "status": result.status.name,
                    "success": result.success,
                    "metrics": {
                        "llm_calls": result.metrics.llm_calls,
                        "llm_tokens": result.metrics.llm_tokens_in + result.metrics.llm_tokens_out,
                        "tool_calls": result.metrics.tool_calls,
                        "wall_time": result.metrics.wall_time_seconds,
                    },
                    "error": result.error,
                }
            )
        else:
            status = "✓" if result.success else "✗"
            output.print(f"{status} {result.status.name}")
            output.print(
                f"  LLM: {result.metrics.llm_calls} calls, "
                f"{result.metrics.llm_tokens_in + result.metrics.llm_tokens_out} tokens"
            )
            output.print(f"  Tools: {result.metrics.tool_calls} calls")
            output.print(f"  Time: {result.metrics.wall_time_seconds:.2f}s")
            if result.error:
                output.error(f"  Error: {result.error}")

        return 0 if result.success else 1

    else:
        output.error(f"Unknown action: {action}")
        return 1


def cmd_health(args: Namespace) -> int:
    """Show project health and what needs attention."""
    from moss import MossAPI

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Analyzing {root.name}...")

    # Get filter args
    focus = getattr(args, "focus", "all")
    if focus == "all":
        focus = None
    severity = getattr(args, "severity", "low")

    try:
        api = MossAPI.for_project(root)
        status = api.health.check(focus=focus, severity=severity)
    except Exception as e:
        output.error(f"Failed to analyze project: {e}")
        return 1

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(status.to_compact())
    elif wants_json(args):
        output.data(status.to_dict())
    else:
        output.print(_format_concise_health(status))

    # CI mode exit codes
    if getattr(args, "ci", False):
        grade = status.health_grade
        if grade in ("A", "B"):
            return 0  # Healthy
        elif grade in ("C", "D"):
            return 1  # Warnings
        else:
            return 2  # Critical

    return 0


def _format_concise_health(status) -> str:
    """Format health status concisely for terminal display."""
    lines = []

    # Header with grade
    grade = status.health_grade
    score = status.health_score
    lines.append(f"# {status.name}: {grade} ({score}/100)")
    lines.append("")

    # Compact stats line
    stats = []
    stats.append(f"{status.total_files} files")
    if status.doc_coverage > 0:
        stats.append(f"{status.doc_coverage:.0%} doc coverage")
    if status.test_files > 0:
        stats.append(f"{status.test_ratio:.1f}x test ratio")
    if status.dep_circular > 0:
        stats.append(f"{status.dep_circular} circular deps")
    if status.struct_hotspots > 0:
        stats.append(f"{status.struct_hotspots} hotspots")
    lines.append(" | ".join(stats))
    lines.append("")

    # Show only high-severity issues by default
    high_issues = [w for w in status.weak_spots if w.severity == "high"]
    if high_issues:
        lines.append("## Issues")
        for w in high_issues[:5]:
            lines.append(f"- [!] {w.category}: {w.message}")
        if len(high_issues) > 5:
            lines.append(f"  ... and {len(high_issues) - 5} more")
        lines.append("")

    # Next actions (top 3)
    if status.next_actions:
        lines.append("## Next Up")
        for action in sorted(status.next_actions, key=lambda a: a.priority)[:3]:
            lines.append(f"- {action.description}")
        lines.append("")

    # Hint for more details
    if len(status.weak_spots) > len(high_issues):
        other = len(status.weak_spots) - len(high_issues)
        lines.append(f"Run `moss report` for full details ({other} more issues)")

    return "\n".join(lines)


def cmd_dwim(args: Namespace) -> int:
    """Find the right moss tool using natural language.

    DWIM = Do What I Mean. Describe what you want to do and get tool suggestions.
    """
    from moss import MossAPI

    output = setup_output(args)
    query = getattr(args, "query", None)
    tool_name = getattr(args, "tool", None)
    top_k = getattr(args, "top", 5)

    # DWIMAPI doesn't need a project root, but MossAPI requires one
    api = MossAPI.for_project(Path.cwd())

    # Info mode: show details about a specific tool
    if tool_name:
        info = api.dwim.get_tool_info(tool_name)
        if info is None:
            output.error(f"Tool not found: {tool_name}")
            return 1
        output.info(f"Tool: {info.name}")
        output.info(f"Description: {info.description}")
        if info.keywords:
            output.info(f"Keywords: {', '.join(info.keywords)}")
        if info.aliases:
            output.info(f"Aliases: {', '.join(info.aliases)}")
        if info.parameters:
            output.info("Parameters:")
            for p in info.parameters:
                output.info(f"  - {p}")
        return 0

    # Query mode: find tools matching query
    if not query:
        output.error("Usage: moss dwim <query> or moss dwim --tool <name>")
        output.info("Examples:")
        output.info("  moss dwim 'summarize the codebase'")
        output.info("  moss dwim 'find complex functions'")
        output.info("  moss dwim --tool skeleton")
        return 1

    results = api.dwim.analyze_intent(query, top_k=top_k)

    if not results:
        output.warning(f"No tools match: {query}")
        return 1

    output.info(f"Tools matching '{query}':\n")
    for r in results:
        confidence_bar = "█" * int(r.confidence * 10) + "░" * (10 - int(r.confidence * 10))
        output.info(f"  {r.tool:<25} [{confidence_bar}] {r.confidence:.0%}")
        if r.message:
            # Truncate long messages
            msg = r.message[:60] + "..." if len(r.message) > 60 else r.message
            output.info(f"    {msg}")
        output.info("")

    # Show usage hint for top result
    if results:
        top = results[0]
        output.info(f"Try: moss {top.tool.replace('.', ' ').replace('_', '-')} ...")

    return 0


def cmd_agent(args: Namespace) -> int:
    """Run DWIM-driven agent loop on a task.

    Uses context-excluded model: each turn gets path + notes + last result,
    not conversation history. Supports task breakdown, notes with TTL.
    """
    import asyncio

    from moss import MossAPI
    from moss.dwim import analyze_intent
    from moss.dwim_loop import DWIMLoop, LoopConfig, LoopState, classify_task

    output = setup_output(args)
    task = getattr(args, "task", None)
    model = getattr(args, "model", None)
    max_turns = getattr(args, "max_turns", 50)
    verbose = getattr(args, "verbose", False)
    dry_run = getattr(args, "dry_run", False)

    if not task:
        output.error("Usage: moss agent <task>")
        output.info('Example: moss agent "Fix the type error in Patch.apply"')
        return 1

    api = MossAPI.for_project(Path.cwd())

    # Dry-run mode: show what would happen without executing
    if dry_run:
        task_type = classify_task(task)
        output.info(f"Task: {task}")
        output.info(f"Type: {task_type.name}")
        output.info("")

        # Show DWIM suggestions for the task
        matches = analyze_intent(task)
        if matches:
            output.info("Tool suggestions:")
            for m in matches[:5]:
                conf_pct = int(m.confidence * 100)
                output.info(f"  {m.tool} ({conf_pct}%)")
        else:
            output.info("No specific tools matched. Agent would use LLM guidance.")

        output.info("")
        output.info(f"Would run with model: {model or 'gemini/gemini-2.0-flash'}")
        output.info(f"Max turns: {max_turns}")
        return 0

    config = LoopConfig(max_turns=max_turns)
    if model:
        config.model = model

    loop = DWIMLoop(api, config)

    output.info(f"Starting agent: {task}")
    if verbose:
        output.info(f"Model: {config.model}")
        output.info(f"Max turns: {max_turns}")
    output.info("")

    try:
        result = asyncio.run(loop.run(task))
    except ImportError as e:
        output.error(f"Missing dependency: {e}")
        output.info("Install with: pip install moss[llm]")
        return 1
    except Exception as e:
        output.error(f"Agent failed: {e}")
        return 1

    # Show results
    if verbose:
        output.info(f"\nTurns: {len(result.turns)}")
        output.info(f"Duration: {result.total_duration_ms}ms")
        for i, turn in enumerate(result.turns, 1):
            output.info(f"\n--- Turn {i} ---")
            output.info(f"Intent: {turn.intent.verb} {turn.intent.target or ''}")
            if turn.error:
                output.warning(f"Error: {turn.error}")
            elif turn.tool_output:
                out_str = str(turn.tool_output)[:200]
                output.info(f"Output: {out_str}...")

    if result.state == LoopState.DONE:
        output.success(f"\nCompleted in {len(result.turns)} turns")
        if result.final_output:
            output.info(f"Final: {str(result.final_output)[:500]}")
        return 0
    elif result.state == LoopState.MAX_TURNS:
        output.warning(f"\nMax turns ({max_turns}) reached")
        return 1
    else:
        output.error(f"\nFailed: {result.error}")
        return 1


def cmd_report(args: Namespace) -> int:
    """Generate comprehensive project report (verbose health)."""
    from moss.status import StatusChecker

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Generating report for {root.name}...")

    checker = StatusChecker(root)

    try:
        status = checker.check()
    except Exception as e:
        output.error(f"Failed to analyze project: {e}")
        return 1

    # Output format
    if getattr(args, "json", False):
        output.data(status.to_dict())
    else:
        # Full markdown output
        output.print(status.to_markdown())

    return 0


def cmd_overview(args: Namespace) -> int:
    """Run multiple checks and output combined results.

    Runs configurable checks (health, deps, docs, todos, refs).
    Supports presets for common configurations.
    """
    from moss import MossAPI
    from moss.presets import AVAILABLE_CHECKS, get_preset, list_presets

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    # Handle --list-presets
    if getattr(args, "list_presets", False):
        presets = list_presets(root)
        if wants_json(args):
            output.data([p.to_dict() for p in presets])
        else:
            output.print("Available presets:")
            for p in presets:
                checks_str = ", ".join(p.checks)
                output.print(f"  {p.name}: {checks_str} ({p.output})")
        return 0

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    api = MossAPI.for_project(root)

    # Load preset if specified
    preset_name = getattr(args, "preset", None)
    preset = None
    if preset_name:
        preset = get_preset(preset_name, root)
        if not preset:
            output.error(f"Unknown preset: {preset_name}")
            output.info("Use --list-presets to see available presets")
            return 1

    # Determine which checks to run
    if preset:
        checks_to_run = set(preset.checks)
    else:
        checks_to_run = AVAILABLE_CHECKS.copy()

    # Allow --checks to override
    explicit_checks = getattr(args, "checks", None)
    if explicit_checks:
        checks_to_run = set(explicit_checks) & AVAILABLE_CHECKS

    output.info(f"Running checks on {root.name}...")

    results: dict = {}
    exit_code = 0
    has_warnings = False

    # Health check
    if "health" in checks_to_run:
        try:
            status = api.health.check()
            # Extract top issues by severity for display
            high_issues = [w for w in status.weak_spots if w.severity == "high"]
            med_issues = [w for w in status.weak_spots if w.severity == "medium"]
            # Get top packages for skeleton summary
            try:
                project_summary = api.health.summarize()
                top_packages = [
                    {"name": p.name, "files": len(p.all_files), "lines": p.total_lines}
                    for p in sorted(
                        project_summary.packages, key=lambda x: x.total_lines, reverse=True
                    )[:5]
                ]
            except Exception:
                top_packages = []

            results["health"] = {
                "grade": status.health_grade,
                "score": status.health_score,
                "files": status.total_files,
                "lines": status.total_lines,
                "modules": status.total_modules,
                "classes": status.struct_classes,
                "functions": status.struct_functions,
                "doc_coverage": status.doc_coverage,
                "test_ratio": status.test_ratio,
                "hotspots": status.struct_hotspots,
                "circular_deps": status.dep_circular,
                "issues": len(status.weak_spots),
                "high_issues": len(high_issues),
                "top_issues": [
                    {"cat": w.category, "msg": w.message} for w in (high_issues + med_issues)[:5]
                ],
                "next_actions": [a.description for a in status.next_actions[:3]],
                "top_packages": top_packages,
            }
            if status.weak_spots:
                has_warnings = True
        except Exception as e:
            results["health"] = {"error": str(e)}
            exit_code = 1

    # External deps
    if "deps" in checks_to_run:
        try:
            deps = api.external_deps.analyze()
            # Include top critical/high vulns for inline display
            critical_vulns = [
                {"pkg": v.package, "id": v.id, "sev": v.severity} for v in deps.critical_vulns[:3]
            ]
            high_vulns = [
                {"pkg": v.package, "id": v.id, "sev": v.severity} for v in deps.high_vulns[:3]
            ]
            results["deps"] = {
                "direct": deps.total_direct,
                "dev": deps.total_dev,
                "vulns": len(deps.vulnerabilities),
                "critical_vulns": critical_vulns,
                "high_vulns": high_vulns,
                "license_issues": len(deps.license_issues),
            }
            if deps.vulnerabilities or deps.license_issues:
                has_warnings = True
        except Exception as e:
            results["deps"] = {"error": str(e)}

    # Check docs
    if "docs" in checks_to_run:
        try:
            docs = api.health.check_docs()
            results["docs"] = {
                "coverage": docs.coverage,
                "errors": docs.error_count,
                "warnings": docs.warning_count,
            }
            if docs.error_count:
                exit_code = 1
            if docs.warning_count:
                has_warnings = True
        except Exception as e:
            results["docs"] = {"error": str(e)}

    # Check TODOs
    if "todos" in checks_to_run:
        try:
            todos = api.health.check_todos()
            # Get top pending items for display
            pending_items = [
                item.text for item in todos.tracked_items if item.status.name == "PENDING"
            ][:5]
            results["todos"] = {
                "pending": todos.pending_count,
                "done": todos.done_count,
                "orphan": todos.orphan_count,
                "top_pending": pending_items,
            }
            if todos.orphan_count:
                has_warnings = True
        except Exception as e:
            results["todos"] = {"error": str(e)}

    # Check refs
    if "refs" in checks_to_run:
        try:
            refs = api.ref_check.check()
            results["refs"] = {
                "valid": len(refs.code_to_docs) + len(refs.docs_to_code),
                "broken": refs.error_count,
                "stale": len(refs.stale_references),
            }
            if refs.error_count:
                exit_code = 1
            if refs.stale_references:
                has_warnings = True
        except Exception as e:
            results["refs"] = {"error": str(e)}

    # Determine output format (CLI flags override preset)
    compact = getattr(args, "compact", False)
    json_mode = wants_json(args)

    # Apply preset output format if no explicit flags
    if preset and not compact and not json_mode:
        if preset.output == "compact":
            compact = True
        elif preset.output == "json":
            json_mode = True

    # Output
    if compact and not json_mode:
        output.print(_format_overview_compact(results))
    elif json_mode:
        output.data(results)
    else:
        output.print(_format_overview_markdown(results))

    # Apply strict mode (preset or CLI flag)
    strict = getattr(args, "strict", False) or (preset and preset.strict)
    if strict and has_warnings:
        exit_code = 1

    return exit_code


def _format_overview_compact(results: dict) -> str:
    """Format overview results as informative multi-line summary.

    Compact but informative - explains WHY scores are what they are.
    """
    lines = []

    # Health with context
    if "health" in results and "error" not in results["health"]:
        h = results["health"]
        # Format lines count
        lines_k = h["lines"] / 1000 if h["lines"] >= 1000 else h["lines"]
        lines_fmt = f"{lines_k:.0f}K" if h["lines"] >= 1000 else str(h["lines"])

        # Include symbol counts
        symbols = []
        if h.get("classes"):
            symbols.append(f"{h['classes']} classes")
        if h.get("functions"):
            symbols.append(f"{h['functions']} funcs")
        symbols_str = f", {', '.join(symbols)}" if symbols else ""

        base = f"health: {h['grade']} ({h['score']:.0f}%) - {h['files']} files, {lines_fmt} lines"
        line = f"{base}{symbols_str}"

        # Add key metrics that explain the score
        details = []
        if h.get("doc_coverage", 0) < 0.5:
            details.append(f"{h['doc_coverage']:.0%} docs")
        if h.get("test_ratio", 0) < 0.5:
            details.append(f"{h['test_ratio']:.1f}x tests")
        if h.get("hotspots", 0) > 10:
            details.append(f"{h['hotspots']} hotspots")
        if h.get("circular_deps", 0) > 0:
            details.append(f"{h['circular_deps']} circular deps")

        if details:
            line += f" ({', '.join(details)})"
        lines.append(line)

        # Top issues
        if h.get("top_issues"):
            for issue in h["top_issues"][:3]:
                lines.append(f"  - {issue['cat']}: {issue['msg']}")
    elif "health" in results:
        lines.append("health: ERROR")

    # Deps
    if "deps" in results and "error" not in results["deps"]:
        d = results["deps"]
        deps_line = f"deps: {d['direct']} direct, {d['dev']} dev"
        if d["vulns"]:
            deps_line += f" - {d['vulns']} vulnerabilities!"
        if d["license_issues"]:
            deps_line += f" - {d['license_issues']} license issues"
        lines.append(deps_line)

        # Show critical/high vulns inline
        for vuln in d.get("critical_vulns", []):
            lines.append(f"  [CRITICAL] {vuln['pkg']}: {vuln['id']}")
        for vuln in d.get("high_vulns", []):
            lines.append(f"  [HIGH] {vuln['pkg']}: {vuln['id']}")

    # Docs
    if "docs" in results and "error" not in results["docs"]:
        doc = results["docs"]
        doc_line = f"docs: {doc['coverage']:.0%} coverage"
        if doc["errors"]:
            doc_line += f" - {doc['errors']} errors"
        lines.append(doc_line)

    # TODOs with actual items
    if "todos" in results and "error" not in results["todos"]:
        t = results["todos"]
        lines.append(f"todos: {t['pending']} pending, {t['done']} done")
        if t.get("top_pending"):
            for item in t["top_pending"][:3]:
                # Truncate long items
                text = item[:60] + "..." if len(item) > 60 else item
                lines.append(f"  - {text}")

    # Refs
    if "refs" in results and "error" not in results["refs"]:
        r = results["refs"]
        if r["broken"]:
            lines.append(f"refs: {r['broken']} broken!")
        elif r["stale"]:
            lines.append(f"refs: {r['stale']} stale")
        else:
            lines.append("refs: ok")

    return "\n".join(lines)


def _format_overview_markdown(results: dict) -> str:
    """Format overview results for terminal display - comprehensive view."""
    lines = ["Project Overview", "=" * 16, ""]

    # Health
    if "health" in results:
        if "error" not in results["health"]:
            h = results["health"]
            lines_k = h["lines"] / 1000 if h["lines"] >= 1000 else h["lines"]
            lines_fmt = f"{lines_k:.0f}K" if h["lines"] >= 1000 else str(h["lines"])

            lines.append(f"Health: {h['grade']} ({h['score']:.0f}/100)")
            lines.append(f"  {h['files']} files, {lines_fmt} lines, {h.get('modules', 0)} modules")
            # Symbol counts
            classes = h.get("classes", 0)
            funcs = h.get("functions", 0)
            if classes or funcs:
                lines.append(f"  {classes} classes, {funcs} functions")
            doc_cov = h.get("doc_coverage", 0)
            test_rat = h.get("test_ratio", 0)
            lines.append(f"  Docs: {doc_cov:.0%} | Tests: {test_rat:.1f}x ratio")
            if h.get("hotspots"):
                circ = h.get("circular_deps", 0)
                lines.append(f"  Hotspots: {h['hotspots']} | Circular deps: {circ}")

            # Show top issues
            if h.get("top_issues"):
                lines.append("")
                lines.append("Issues:")
                for issue in h["top_issues"]:
                    lines.append(f"  [!] {issue['cat']}: {issue['msg']}")

            # Show next actions
            if h.get("next_actions"):
                lines.append("")
                lines.append("Next up:")
                for action in h["next_actions"]:
                    text = action[:70] + "..." if len(action) > 70 else action
                    lines.append(f"  - {text}")

            # Show top packages (skeleton summary)
            if h.get("top_packages"):
                lines.append("")
                lines.append("Structure:")
                for pkg in h["top_packages"]:
                    lines.append(f"  {pkg['name']}/: {pkg['files']} files, {pkg['lines']} lines")
        else:
            lines.append(f"Health: ERROR - {results['health']['error']}")
        lines.append("")

    # Deps
    if "deps" in results:
        if "error" not in results["deps"]:
            d = results["deps"]
            lines.append(f"Dependencies: {d['direct']} direct, {d['dev']} dev")
            if d["vulns"]:
                lines.append(f"  [!] {d['vulns']} vulnerabilities")
                # Show critical/high vulns inline
                for vuln in d.get("critical_vulns", []):
                    lines.append(f"      [CRITICAL] {vuln['pkg']}: {vuln['id']}")
                for vuln in d.get("high_vulns", []):
                    lines.append(f"      [HIGH] {vuln['pkg']}: {vuln['id']}")
            if d["license_issues"]:
                lines.append(f"  [!] {d['license_issues']} license issues")
        else:
            lines.append(f"Dependencies: ERROR - {results['deps']['error']}")
        lines.append("")

    # Docs
    if "docs" in results:
        if "error" not in results["docs"]:
            doc = results["docs"]
            lines.append(f"Documentation: {doc['coverage']:.0%} coverage")
            if doc["errors"] or doc["warnings"]:
                lines.append(f"  {doc['errors']} errors, {doc['warnings']} warnings")
        else:
            lines.append(f"Documentation: ERROR - {results['docs']['error']}")
        lines.append("")

    # TODOs with actual items
    if "todos" in results:
        if "error" not in results["todos"]:
            t = results["todos"]
            lines.append(f"TODOs: {t['pending']} pending, {t['done']} done")
            if t["orphan"]:
                lines.append(f"  {t['orphan']} orphaned (in code but not tracked)")

            # Show top pending items
            if t.get("top_pending"):
                lines.append("")
                lines.append("Pending:")
                for item in t["top_pending"]:
                    text = item[:70] + "..." if len(item) > 70 else item
                    lines.append(f"  - {text}")
        else:
            lines.append(f"TODOs: ERROR - {results['todos']['error']}")
        lines.append("")

    # Refs
    if "refs" in results:
        if "error" not in results["refs"]:
            r = results["refs"]
            status = "OK" if not r["broken"] else f"{r['broken']} broken"
            lines.append(f"References: {status} ({r['valid']} valid)")
            if r["stale"]:
                lines.append(f"  {r['stale']} stale")
        else:
            lines.append(f"References: ERROR - {results['refs']['error']}")

    return "\n".join(lines)


def cmd_eval(args: Namespace) -> int:
    """Run evaluation benchmarks.

    Currently supports SWE-bench for evaluating code patching accuracy.
    """
    output = setup_output(args)
    benchmark = getattr(args, "benchmark", "swebench")

    if benchmark != "swebench":
        output.error(f"Unknown benchmark: {benchmark}")
        return 1

    # Handle swebench
    try:
        from moss.eval.swebench import (
            AgentStrategy,
            Subset,
            SWEBenchHarness,
        )
    except ImportError as e:
        output.error(f"Evaluation dependencies not installed: {e}")
        output.info("Install with: pip install 'moss[eval]'")
        return 1

    action = getattr(args, "action", "list")
    subset_name = getattr(args, "subset", "lite").upper()

    try:
        subset = Subset[subset_name]
    except KeyError:
        output.error(f"Unknown subset: {subset_name}")
        output.info(f"Available: {', '.join(s.name.lower() for s in Subset)}")
        return 1

    harness = SWEBenchHarness(
        strategy=AgentStrategy(getattr(args, "strategy", "moss")),
        max_iterations=getattr(args, "max_iterations", 10),
    )

    if action == "list":
        instances = harness.list_instances(subset, limit=getattr(args, "limit", None))
        output.info(f"Found {len(instances)} instances in {subset.name}")

        if wants_json(args):
            output.data([{"id": i.instance_id, "repo": i.repo} for i in instances])
        else:
            for inst in instances[:20]:
                output.print(f"  {inst.instance_id}")
            if len(instances) > 20:
                output.print(f"  ... and {len(instances) - 20} more")

    elif action == "run":
        instance_ids = getattr(args, "instance", None)
        limit = getattr(args, "limit", 10)

        output.info(f"Running SWE-bench evaluation ({subset.name})...")

        if instance_ids:
            results = harness.run(subset, instance_ids=instance_ids)
        else:
            results = harness.run(subset, limit=limit)

        if wants_json(args):
            output.data(results.to_json())
        else:
            output.print(results.summary())

    elif action == "info":
        instance_id = getattr(args, "instance", None)
        if not instance_id:
            output.error("Instance ID required for info action")
            return 1

        instance_id = instance_id[0] if isinstance(instance_id, list) else instance_id
        inst = harness.get_instance(instance_id)

        if not inst:
            output.error(f"Instance not found: {instance_id}")
            return 1

        if wants_json(args):
            output.data(
                {
                    "id": inst.instance_id,
                    "repo": inst.repo,
                    "base_commit": inst.base_commit,
                    "problem_statement": inst.problem_statement,
                    "hints": inst.hints_text,
                    "fail_to_pass": inst.fail_to_pass,
                    "pass_to_pass": inst.pass_to_pass,
                }
            )
        else:
            output.header(f"Instance: {inst.instance_id}")
            output.print(f"Repo: {inst.repo}")
            output.print(f"Base commit: {inst.base_commit[:12]}")
            output.blank()
            output.print("Problem Statement:")
            output.print(inst.problem_statement[:500])
            if len(inst.problem_statement) > 500:
                output.print("...")
            output.blank()
            if inst.fail_to_pass:
                output.print(f"Tests (fail→pass): {len(inst.fail_to_pass)}")

    return 0


def cmd_help(args: Namespace) -> int:
    """Show detailed help for commands."""
    from moss.help import (
        format_category_list,
        format_command_help,
        get_command_help,
    )

    output = setup_output(args)
    command = getattr(args, "topic", None)

    if not command:
        # Show categorized list
        output.print(format_category_list())
        return 0

    # Show help for specific command
    cmd = get_command_help(command)
    if not cmd:
        output.error(f"Unknown command: {command}")
        output.info("Run 'moss help' to see all commands.")
        return 1

    output.print(format_command_help(cmd))
    return 0


def create_parser() -> argparse.ArgumentParser:
    """Create the argument parser."""
    parser = argparse.ArgumentParser(
        prog="moss",
        description="Headless agent orchestration layer for AI engineering",
    )
    parser.add_argument("--version", action="version", version=f"%(prog)s {get_version()}")

    # Global output options
    parser.add_argument("--json", "-j", action="store_true", help="Output in JSON format")
    parser.add_argument(
        "--compact",
        "-c",
        action="store_true",
        help="Compact output (token-efficient for AI agents)",
    )
    parser.add_argument(
        "--jq",
        metavar="EXPR",
        help="Filter JSON output with jq expression (e.g., '.stats', '.dependencies[0]')",
    )
    parser.add_argument("--quiet", "-q", action="store_true", help="Quiet mode (errors only)")
    parser.add_argument("--verbose", "-v", action="store_true", help="Verbose output")
    parser.add_argument("--debug", action="store_true", help="Debug output (most verbose)")
    parser.add_argument("--no-color", action="store_true", help="Disable colored output")

    subparsers = parser.add_subparsers(dest="command", help="Commands")

    # init command
    init_parser = subparsers.add_parser("init", help="Initialize a moss project")
    init_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Project directory (default: current)",
    )
    init_parser.add_argument(
        "--distro",
        "-d",
        help="Base distro to use (default: python)",
    )
    init_parser.add_argument(
        "--force",
        "-f",
        action="store_true",
        help="Overwrite existing config",
    )
    init_parser.set_defaults(func=cmd_init)

    # run command
    run_parser = subparsers.add_parser("run", help="Run a task")
    run_parser.add_argument("task", help="Task description")
    run_parser.add_argument(
        "--directory",
        "-C",
        default=".",
        help="Project directory (default: current)",
    )
    run_parser.add_argument(
        "--priority",
        "-p",
        default="normal",
        choices=["low", "normal", "high", "critical"],
        help="Task priority",
    )
    run_parser.add_argument(
        "--constraint",
        "-c",
        action="append",
        help="Add constraint (can be repeated)",
    )
    run_parser.add_argument(
        "--wait",
        "-w",
        action="store_true",
        help="Wait for task completion",
    )
    run_parser.set_defaults(func=cmd_run)

    # status command
    status_parser = subparsers.add_parser("status", help="Show status")
    status_parser.add_argument(
        "--directory",
        "-C",
        default=".",
        help="Project directory (default: current)",
    )
    status_parser.set_defaults(func=cmd_status)

    # config command
    config_parser = subparsers.add_parser("config", help="Show/validate configuration")
    config_parser.add_argument(
        "--directory",
        "-C",
        default=".",
        help="Project directory (default: current)",
    )
    config_parser.add_argument(
        "--validate",
        action="store_true",
        help="Validate configuration",
    )
    config_parser.add_argument(
        "--list-distros",
        action="store_true",
        help="List available distros",
    )
    config_parser.set_defaults(func=cmd_config)

    # distros command
    distros_parser = subparsers.add_parser("distros", help="List available distros")
    distros_parser.set_defaults(func=cmd_distros)

    # ==========================================================================
    # Introspection commands
    # ==========================================================================

    # tree command
    tree_parser = subparsers.add_parser("tree", help="Show git-aware file tree")
    tree_parser.add_argument(
        "path",
        nargs="?",
        default=".",
        help="Directory to show (default: current)",
    )
    tree_parser.add_argument(
        "--tracked",
        "-t",
        action="store_true",
        help="Only show git-tracked files",
    )
    tree_parser.add_argument(
        "--all",
        "-a",
        action="store_true",
        help="Show all files (ignore .gitignore)",
    )
    tree_parser.set_defaults(func=cmd_tree)

    # path command (new codebase tree)
    path_parser = subparsers.add_parser("path", help="Resolve fuzzy path to exact location(s)")
    path_parser.add_argument("query", help="Path or symbol to find (fuzzy matching)")
    path_parser.set_defaults(func=cmd_path)

    # view command (new codebase tree)
    view_parser = subparsers.add_parser("view", help="View a node in the codebase tree")
    view_parser.add_argument("target", help="Path or symbol to view (fuzzy matching)")
    view_parser.set_defaults(func=cmd_view)

    # search-tree command (new codebase tree, named to avoid conflict with existing search)
    search_tree_parser = subparsers.add_parser(
        "search-tree", help="Search for symbols in the codebase tree"
    )
    search_tree_parser.add_argument("query", help="Search term")
    search_tree_parser.add_argument(
        "scope", nargs="?", help="Scope to search within (file, directory, or symbol)"
    )
    search_tree_parser.set_defaults(func=cmd_search_tree)

    # expand command (new codebase tree)
    expand_parser = subparsers.add_parser("expand", help="Show full source of a symbol")
    expand_parser.add_argument(
        "target",
        nargs="+",
        help="Symbol to expand (supports: symbol, file:symbol, file symbol)",
    )
    expand_parser.set_defaults(func=cmd_expand)

    # callers command (new codebase tree)
    callers_parser = subparsers.add_parser("callers", help="Find callers of a symbol")
    callers_parser.add_argument(
        "target",
        nargs="+",
        help="Symbol to find callers of (supports: symbol, file:symbol, file symbol)",
    )
    callers_parser.set_defaults(func=cmd_callers)

    # callees command (new codebase tree)
    callees_parser = subparsers.add_parser("callees", help="Find what a symbol calls")
    callees_parser.add_argument(
        "target",
        nargs="+",
        help="Symbol to find callees of (supports: symbol, file:symbol, file symbol)",
    )
    callees_parser.set_defaults(func=cmd_callees)

    # skeleton command
    skeleton_parser = subparsers.add_parser(
        "skeleton", help="Extract code skeleton (functions, classes, methods)"
    )
    skeleton_parser.add_argument("path", help="File or directory to analyze")
    skeleton_parser.add_argument(
        "--pattern", "-p", help="Glob pattern for directory (default: **/*.py)"
    )
    skeleton_parser.add_argument(
        "--public-only", action="store_true", dest="public_only", help="Exclude private symbols"
    )
    skeleton_parser.set_defaults(func=cmd_skeleton)

    # anchors command
    anchors_parser = subparsers.add_parser(
        "anchors", help="Find anchors (functions, classes, methods)"
    )
    anchors_parser.add_argument("path", help="File or directory to analyze")
    anchors_parser.add_argument(
        "--type",
        "-t",
        default="all",
        choices=["function", "class", "method", "all"],
        help="Type of anchors to find",
    )
    anchors_parser.add_argument("--name", "-n", help="Filter by name (regex)")
    anchors_parser.add_argument(
        "--pattern", "-p", help="Glob pattern for directory (default: **/*.py)"
    )
    anchors_parser.set_defaults(func=cmd_anchors)

    # query command
    query_parser = subparsers.add_parser(
        "query", help="Query code with pattern matching and filters"
    )
    query_parser.add_argument("path", help="File or directory to search")
    query_parser.add_argument("--name", "-n", help="Name pattern (regex)")
    query_parser.add_argument("--signature", "-s", help="Signature pattern (regex)")
    query_parser.add_argument(
        "--type",
        "-t",
        choices=["function", "class", "method", "all"],
        help="Filter by type",
    )
    query_parser.add_argument("--inherits", "-i", help="Filter classes by base class")
    query_parser.add_argument(
        "--min-lines", type=int, dest="min_lines", help="Minimum lines (complexity)"
    )
    query_parser.add_argument(
        "--max-lines", type=int, dest="max_lines", help="Maximum lines (complexity)"
    )
    query_parser.add_argument(
        "--pattern", "-p", help="Glob pattern for directory (default: **/*.py)"
    )
    query_parser.add_argument(
        "--group-by", choices=["file"], dest="group_by", help="Group results by file"
    )
    query_parser.set_defaults(func=cmd_query)

    # cfg command
    cfg_parser = subparsers.add_parser("cfg", help="Build control flow graph")
    cfg_parser.add_argument("path", help="Python file to analyze")
    cfg_parser.add_argument("function", nargs="?", help="Specific function to analyze")
    cfg_parser.add_argument(
        "--dot", action="store_true", help="Output in DOT format (for graphviz)"
    )
    cfg_parser.add_argument("--mermaid", action="store_true", help="Output in Mermaid format")
    cfg_parser.add_argument(
        "--html", action="store_true", help="Output as HTML with embedded diagram"
    )
    cfg_parser.add_argument(
        "--output", "-o", help="Save output to file (format auto-detected from extension)"
    )
    cfg_parser.add_argument(
        "--summary", "-s", action="store_true", help="Show only node/edge counts"
    )
    cfg_parser.add_argument(
        "--live", action="store_true", help="Start live CFG viewer with auto-refresh"
    )
    cfg_parser.add_argument(
        "--port", type=int, default=8765, help="Port for live server (default: 8765)"
    )
    cfg_parser.set_defaults(func=cmd_cfg)

    # deps command
    deps_parser = subparsers.add_parser("deps", help="Extract dependencies (imports/exports)")
    deps_parser.add_argument("path", help="File or directory to analyze")
    deps_parser.add_argument(
        "--pattern", "-p", help="Glob pattern for directory (default: **/*.py)"
    )
    deps_parser.add_argument(
        "--reverse", "-r", help="Find files that import this module (reverse dependency)"
    )
    deps_parser.add_argument(
        "--search-dir", "-d", dest="search_dir", help="Directory to search for reverse deps"
    )
    deps_parser.add_argument(
        "--dot", action="store_true", help="Output dependency graph in DOT format"
    )
    deps_parser.set_defaults(func=cmd_deps)

    # context command
    context_parser = subparsers.add_parser(
        "context", help="Generate compiled context (skeleton + deps + summary)"
    )
    context_parser.add_argument("path", help="Python file to analyze")
    context_parser.set_defaults(func=cmd_context)

    # search command
    search_parser = subparsers.add_parser("search", help="Semantic search across codebase")
    search_parser.add_argument("--query", "-q", help="Search query (natural language or code)")
    search_parser.add_argument(
        "--directory", "-d", default=".", help="Directory to search (default: .)"
    )
    search_parser.add_argument(
        "--index", "-i", action="store_true", help="Index files before searching"
    )
    search_parser.add_argument(
        "--persist", "-p", action="store_true", help="Persist index to disk (uses ChromaDB)"
    )
    search_parser.add_argument("--patterns", help="Glob patterns to include (comma-separated)")
    search_parser.add_argument("--exclude", help="Glob patterns to exclude (comma-separated)")
    search_parser.add_argument(
        "--limit", "-n", type=int, default=10, help="Max results (default: 10)"
    )
    search_parser.add_argument(
        "--mode",
        choices=["hybrid", "tfidf", "embedding"],
        default="hybrid",
        help="Search mode (default: hybrid)",
    )
    search_parser.set_defaults(func=cmd_search)

    # mcp-server command
    mcp_parser = subparsers.add_parser("mcp-server", help="Start MCP server for LLM tool access")
    mcp_parser.add_argument(
        "--full",
        action="store_true",
        help="Use full multi-tool server (more tokens, better for IDEs)",
    )
    mcp_parser.set_defaults(func=cmd_mcp_server)

    # acp-server command
    acp_parser = subparsers.add_parser(
        "acp-server",
        help="Start ACP server for IDE integration (Zed, JetBrains)",
    )
    acp_parser.set_defaults(func=cmd_acp_server)

    # gen command
    gen_parser = subparsers.add_parser(
        "gen", help="Generate interface code from MossAPI introspection"
    )
    gen_parser.add_argument(
        "--target",
        "-t",
        default="mcp",
        choices=["mcp", "http", "cli", "openapi", "grpc", "lsp"],
        help="Generation target (default: mcp)",
    )
    gen_parser.add_argument(
        "--output",
        "-o",
        metavar="FILE",
        help="Output file (default: stdout)",
    )
    gen_parser.add_argument(
        "--list",
        "-l",
        action="store_true",
        help="List generated items instead of full output",
    )
    gen_parser.set_defaults(func=cmd_gen)

    # tui command
    tui_parser = subparsers.add_parser("tui", help="Interactive terminal UI for exploring MossAPI")
    tui_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Project root directory (default: current)",
    )
    tui_parser.set_defaults(func=cmd_tui)

    # lsp command
    lsp_parser = subparsers.add_parser("lsp", help="Start LSP server for IDE integration")
    lsp_parser.add_argument(
        "--transport",
        "-t",
        default="stdio",
        help="Transport: 'stdio' (default) or 'tcp:host:port'",
    )
    lsp_parser.set_defaults(func=cmd_lsp)

    # shell command
    shell_parser = subparsers.add_parser("shell", help="Interactive shell for code exploration")
    shell_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Workspace directory (default: current)",
    )
    shell_parser.set_defaults(func=cmd_shell)

    # explore command (alias for shell with better discoverability)
    explore_parser = subparsers.add_parser(
        "explore", help="Interactive REPL for codebase exploration (alias for shell)"
    )
    explore_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Workspace directory (default: current)",
    )
    explore_parser.set_defaults(func=cmd_shell)

    # watch command
    watch_parser = subparsers.add_parser("watch", help="Watch files and re-run tests on changes")
    watch_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to watch (default: current)",
    )
    watch_parser.add_argument(
        "-c",
        "--command",
        help="Custom test command (default: pytest -v)",
    )
    watch_parser.add_argument(
        "--debounce",
        type=int,
        default=500,
        help="Debounce delay in milliseconds (default: 500)",
    )
    watch_parser.add_argument(
        "--no-clear",
        action="store_true",
        help="Don't clear screen between runs",
    )
    watch_parser.add_argument(
        "--no-initial",
        action="store_true",
        help="Don't run tests on start",
    )
    watch_parser.set_defaults(func=cmd_watch)

    # hooks command
    hooks_parser = subparsers.add_parser("hooks", help="Manage git pre-commit hooks")
    hooks_parser.add_argument(
        "action",
        nargs="?",
        choices=["install", "uninstall", "status", "config"],
        default="status",
        help="Action to perform (default: status)",
    )
    hooks_parser.add_argument(
        "-C",
        "--directory",
        default=".",
        help="Project directory (default: current)",
    )
    hooks_parser.add_argument(
        "--force",
        "-f",
        action="store_true",
        help="Force overwrite existing hooks",
    )
    hooks_parser.set_defaults(func=cmd_hooks)

    # diff command
    diff_parser = subparsers.add_parser("diff", help="Analyze git diff and show symbol changes")
    diff_parser.add_argument(
        "from_ref",
        nargs="?",
        default="HEAD~1",
        help="Starting commit reference (default: HEAD~1)",
    )
    diff_parser.add_argument(
        "to_ref",
        nargs="?",
        default="HEAD",
        help="Ending commit reference (default: HEAD)",
    )
    diff_parser.add_argument(
        "-C",
        "--directory",
        default=".",
        help="Repository directory (default: current)",
    )
    diff_parser.add_argument(
        "--staged",
        action="store_true",
        help="Analyze staged changes instead of commits",
    )
    diff_parser.add_argument(
        "--working",
        action="store_true",
        help="Analyze working directory changes (unstaged)",
    )
    diff_parser.add_argument(
        "--stat",
        action="store_true",
        help="Show only statistics summary",
    )
    diff_parser.set_defaults(func=cmd_diff)

    # pr command
    pr_parser = subparsers.add_parser("pr", help="Generate PR review summary")
    pr_parser.add_argument(
        "--base",
        "-b",
        default="main",
        help="Base branch to compare against (default: main)",
    )
    pr_parser.add_argument(
        "--head",
        default="HEAD",
        help="Head commit/branch (default: HEAD)",
    )
    pr_parser.add_argument(
        "-C",
        "--directory",
        default=".",
        help="Repository directory (default: current)",
    )
    pr_parser.add_argument(
        "--staged",
        action="store_true",
        help="Analyze staged changes instead",
    )
    pr_parser.add_argument(
        "--title",
        "-t",
        action="store_true",
        help="Only output suggested PR title",
    )
    pr_parser.set_defaults(func=cmd_pr)

    # metrics command
    metrics_parser = subparsers.add_parser("metrics", help="Generate codebase metrics dashboard")
    metrics_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    metrics_parser.add_argument(
        "--pattern",
        "-p",
        default="**/*.py",
        help="Glob pattern for files (default: **/*.py)",
    )
    metrics_parser.add_argument(
        "--html",
        action="store_true",
        help="Output as HTML dashboard",
    )
    metrics_parser.add_argument(
        "--output",
        "-o",
        help="Save HTML dashboard to file",
    )
    metrics_parser.add_argument(
        "--title",
        "-t",
        help="Dashboard title (default: directory name)",
    )
    metrics_parser.set_defaults(func=cmd_metrics)

    # rules command
    rules_parser = subparsers.add_parser("rules", help="Check code against custom analysis rules")
    rules_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    rules_parser.add_argument(
        "--pattern",
        "-p",
        default="**/*.py",
        help="Glob pattern for files (default: **/*.py)",
    )
    rules_parser.add_argument(
        "--list",
        "-l",
        action="store_true",
        help="List available rules",
    )
    rules_parser.add_argument(
        "--no-builtins",
        action="store_true",
        help="Disable built-in rules",
    )
    rules_parser.add_argument(
        "--sarif",
        "-s",
        help="Output results in SARIF format to file",
    )
    rules_parser.set_defaults(func=cmd_rules)

    # synthesize command
    synth_parser = subparsers.add_parser("synthesize", help="Synthesize code from specification")
    synth_parser.add_argument(
        "description",
        help="Description of what to synthesize",
    )
    synth_parser.add_argument(
        "--type-signature",
        "-t",
        dest="type_signature",
        help="Type signature (e.g., 'List[int] -> List[str]')",
    )
    synth_parser.add_argument(
        "--example",
        "-e",
        action="append",
        dest="examples",
        help="Input-output example as 'input:output' (can be repeated)",
    )
    synth_parser.add_argument(
        "--constraint",
        "-c",
        action="append",
        dest="constraints",
        help="Add constraint (can be repeated)",
    )
    synth_parser.add_argument(
        "--strategy",
        "-s",
        choices=["type_driven", "test_driven", "pattern_based", "auto"],
        default="auto",
        help="Decomposition strategy (default: auto)",
    )
    synth_parser.add_argument(
        "--max-depth",
        type=int,
        default=5,
        dest="max_depth",
        help="Maximum decomposition depth (default: 5)",
    )
    synth_parser.add_argument(
        "--show-decomposition",
        "-d",
        action="store_true",
        dest="show_decomposition",
        help="Show problem decomposition tree",
    )
    synth_parser.add_argument(
        "--generator",
        "-g",
        choices=["auto", "placeholder", "template", "llm"],
        default="auto",
        help="Code generator to use (default: auto, uses highest priority)",
    )
    synth_parser.add_argument(
        "--dry-run",
        action="store_true",
        dest="dry_run",
        help="Show what would be synthesized without executing",
    )
    synth_parser.set_defaults(func=cmd_synthesize)

    # edit command
    edit_parser = subparsers.add_parser(
        "edit", help="Edit code with intelligent complexity routing"
    )
    edit_parser.add_argument(
        "task",
        help="Description of the edit task",
    )
    edit_parser.add_argument(
        "-f",
        "--file",
        help="Target file to edit",
    )
    edit_parser.add_argument(
        "-s",
        "--symbol",
        help="Target symbol (function, class, method) to edit",
    )
    edit_parser.add_argument(
        "-C",
        "--directory",
        default=".",
        help="Project directory (default: current)",
    )
    edit_parser.add_argument(
        "-l",
        "--language",
        default="python",
        help="Programming language (default: python)",
    )
    edit_parser.add_argument(
        "-c",
        "--constraint",
        action="append",
        help="Add constraint (can be repeated)",
    )
    edit_parser.add_argument(
        "--method",
        choices=["structural", "synthesis", "auto"],
        default="auto",
        help="Force specific edit method (default: auto)",
    )
    edit_parser.add_argument(
        "--analyze-only",
        "-a",
        action="store_true",
        dest="analyze_only",
        help="Only analyze complexity, don't edit",
    )
    edit_parser.add_argument(
        "--dry-run",
        action="store_true",
        dest="dry_run",
        help="Show what would change without applying",
    )
    edit_parser.add_argument(
        "--diff",
        "-d",
        action="store_true",
        help="Show unified diff of changes",
    )
    edit_parser.set_defaults(func=cmd_edit)

    # summarize command
    summarize_parser = subparsers.add_parser(
        "summarize", help="Summarize code or documentation files"
    )
    summarize_parser.add_argument(
        "path",
        nargs="?",
        default=".",
        help="File or directory to summarize (default: current directory)",
    )
    summarize_parser.add_argument(
        "--include-private",
        "-p",
        action="store_true",
        dest="include_private",
        help="Include private (_prefixed) modules and symbols",
    )
    summarize_parser.add_argument(
        "--include-tests",
        "-t",
        action="store_true",
        dest="include_tests",
        help="Include test files",
    )
    summarize_parser.add_argument(
        "--json",
        "-j",
        action="store_true",
        help="Output as JSON",
    )
    summarize_parser.add_argument(
        "--docs",
        "-d",
        action="store_true",
        help="Summarize documentation files instead of code",
    )
    summarize_parser.set_defaults(func=cmd_summarize)

    # check-docs command
    check_docs_parser = subparsers.add_parser(
        "check-docs", help="Check documentation freshness against codebase"
    )
    check_docs_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to check (default: current)",
    )
    check_docs_parser.add_argument(
        "--strict",
        "-s",
        action="store_true",
        help="Exit with error on warnings (not just errors)",
    )
    check_docs_parser.add_argument(
        "--json",
        "-j",
        action="store_true",
        help="Output as JSON",
    )
    check_docs_parser.add_argument(
        "--check-links",
        "-l",
        action="store_true",
        dest="check_links",
        help="Check for broken internal links in documentation",
    )
    check_docs_parser.set_defaults(func=cmd_check_docs)

    # check-todos command
    check_todos_parser = subparsers.add_parser(
        "check-todos", help="Check TODOs against implementation status"
    )
    check_todos_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to check (default: current)",
    )
    check_todos_parser.add_argument(
        "--strict",
        "-s",
        action="store_true",
        help="Exit with error on orphaned TODOs",
    )
    check_todos_parser.add_argument(
        "--json",
        "-j",
        action="store_true",
        help="Output as JSON",
    )
    check_todos_parser.set_defaults(func=cmd_check_todos)

    # mutate command
    mutate_parser = subparsers.add_parser(
        "mutate", help="Run mutation testing to find undertested code"
    )
    mutate_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    mutate_parser.add_argument(
        "--quick",
        "-q",
        action="store_true",
        help="Quick mode: test only a sample of mutations",
    )
    mutate_parser.add_argument(
        "--since",
        metavar="COMMIT",
        help="Only mutate files changed since COMMIT",
    )
    mutate_parser.add_argument(
        "--paths",
        nargs="+",
        metavar="PATH",
        help="Specific paths to mutate",
    )
    mutate_parser.add_argument(
        "--strict",
        "-s",
        action="store_true",
        help="Exit with error if mutation score < 80%%",
    )
    mutate_parser.add_argument(
        "--json",
        "-j",
        action="store_true",
        help="Output as JSON",
    )
    mutate_parser.set_defaults(func=cmd_mutate)

    # check-refs command
    check_refs_parser = subparsers.add_parser(
        "check-refs", help="Check bidirectional references between code and docs"
    )
    check_refs_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to check (default: current)",
    )
    check_refs_parser.add_argument(
        "--staleness-days",
        type=int,
        default=30,
        metavar="N",
        help="Warn if code changed more than N days after docs (default: 30)",
    )
    check_refs_parser.add_argument(
        "--strict",
        "-s",
        action="store_true",
        help="Exit with error on warnings (stale refs)",
    )
    check_refs_parser.add_argument(
        "--json",
        "-j",
        action="store_true",
        help="Output as JSON",
    )
    check_refs_parser.set_defaults(func=cmd_check_refs)

    # external-deps command
    external_deps_parser = subparsers.add_parser(
        "external-deps", help="Analyze external dependencies (PyPI packages)"
    )
    external_deps_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    external_deps_parser.add_argument(
        "--resolve",
        "-r",
        action="store_true",
        help="Resolve transitive dependencies (requires pip)",
    )
    external_deps_parser.add_argument(
        "--json",
        "-j",
        action="store_true",
        help="Output as JSON",
    )
    external_deps_parser.add_argument(
        "--warn-weight",
        "-w",
        type=int,
        default=0,
        metavar="N",
        help="Warn and exit 1 if any dependency has weight >= N (requires --resolve)",
    )
    external_deps_parser.add_argument(
        "--check-vulns",
        "-v",
        action="store_true",
        help="Check for known vulnerabilities via OSV API (exit 1 if found)",
    )
    external_deps_parser.add_argument(
        "--check-licenses",
        "-l",
        action="store_true",
        help="Check license compatibility (exit 1 if issues found)",
    )
    external_deps_parser.set_defaults(func=cmd_external_deps)

    # health command
    health_parser = subparsers.add_parser(
        "health", help="Show project health and what needs attention"
    )
    health_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    health_parser.add_argument(
        "--json",
        "-j",
        action="store_true",
        help="Output as JSON",
    )
    health_parser.add_argument(
        "--focus",
        "-f",
        choices=["deps", "tests", "complexity", "api", "all"],
        default="all",
        help="Focus on specific analysis area",
    )
    health_parser.add_argument(
        "--severity",
        "-s",
        choices=["low", "medium", "high"],
        default="low",
        help="Minimum severity to show (default: low = show all)",
    )
    health_parser.add_argument(
        "--ci",
        action="store_true",
        help="CI mode: exit 0=healthy, 1=warnings, 2=critical",
    )
    health_parser.set_defaults(func=cmd_health)

    # report command (verbose health)
    report_parser = subparsers.add_parser(
        "report", help="Generate comprehensive project report (verbose health)"
    )
    report_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    report_parser.add_argument(
        "--json",
        "-j",
        action="store_true",
        help="Output as JSON",
    )
    report_parser.set_defaults(func=cmd_report)

    # overview command (multi-check aggregation)
    overview_parser = subparsers.add_parser(
        "overview", help="Run all checks and show aggregated results"
    )
    overview_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    overview_parser.add_argument(
        "--preset",
        "-p",
        metavar="NAME",
        help="Use a named preset (ci, quick, full, or custom from config)",
    )
    overview_parser.add_argument(
        "--list-presets",
        action="store_true",
        help="List available presets and exit",
    )
    overview_parser.add_argument(
        "--checks",
        nargs="+",
        choices=["health", "deps", "docs", "todos", "refs"],
        help="Specific checks to run (overrides preset)",
    )
    overview_parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit non-zero on warnings (not just errors)",
    )
    overview_parser.set_defaults(func=cmd_overview)

    # roadmap command
    roadmap_parser = subparsers.add_parser(
        "roadmap", help="Show project roadmap and progress from TODO.md"
    )
    roadmap_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to search for TODO.md (default: current)",
    )
    roadmap_parser.add_argument(
        "--tui",
        "-t",
        action="store_true",
        help="Use TUI display with box drawing (default for humans)",
    )
    roadmap_parser.add_argument(
        "--plain",
        "-p",
        action="store_true",
        help="Use plain text display (better for LLMs)",
    )
    roadmap_parser.add_argument(
        "--completed",
        "-c",
        action="store_true",
        help="Include completed phases",
    )
    roadmap_parser.add_argument(
        "--width",
        "-w",
        type=int,
        default=80,
        help="Terminal width for TUI mode (default: 80)",
    )
    roadmap_parser.add_argument(
        "--no-color",
        action="store_true",
        help="Disable colors in output",
    )
    roadmap_parser.add_argument(
        "--max-items",
        "-m",
        type=int,
        default=0,
        help="Max items per section (0 = unlimited, default: 0)",
    )
    roadmap_parser.set_defaults(func=cmd_roadmap)

    # analyze-session command
    session_parser = subparsers.add_parser(
        "analyze-session", help="Analyze a Claude Code session log"
    )
    session_parser.add_argument(
        "session_path",
        type=Path,
        help="Path to the JSONL session file",
    )
    session_parser.set_defaults(func=cmd_analyze_session)

    # extract-preferences command
    extract_prefs_parser = subparsers.add_parser(
        "extract-preferences",
        help="Extract user preferences from agent session logs",
    )
    extract_prefs_parser.add_argument(
        "session_paths",
        nargs="+",
        help="Paths to session log files (supports multiple)",
    )
    extract_prefs_parser.add_argument(
        "--format",
        "-f",
        choices=["claude", "gemini", "antigravity", "cursor", "generic", "json"],
        default="generic",
        help="Output format (default: generic)",
    )
    extract_prefs_parser.add_argument(
        "--log-format",
        choices=["auto", "claude", "gemini", "cline", "roo", "aider"],
        default="auto",
        help="Session log format (default: auto-detect)",
    )
    extract_prefs_parser.add_argument(
        "--min-confidence",
        choices=["low", "medium", "high"],
        default="low",
        help="Minimum confidence level to include (default: low)",
    )
    extract_prefs_parser.add_argument(
        "--synthesize",
        "-s",
        action="store_true",
        help="Use LLM to synthesize preferences into natural language",
    )
    extract_prefs_parser.add_argument(
        "--provider",
        help="LLM provider for synthesis (default: from env)",
    )
    extract_prefs_parser.add_argument(
        "--model",
        "-m",
        help="LLM model for synthesis (default: provider default)",
    )
    extract_prefs_parser.set_defaults(func=cmd_extract_preferences)

    # diff-preferences command
    diff_prefs_parser = subparsers.add_parser(
        "diff-preferences",
        help="Compare two preference extractions to track drift",
    )
    diff_prefs_parser.add_argument(
        "old_path",
        type=Path,
        help="Path to old preferences JSON file",
    )
    diff_prefs_parser.add_argument(
        "new_path",
        type=Path,
        help="Path to new preferences JSON file",
    )
    diff_prefs_parser.set_defaults(func=cmd_diff_preferences)

    # git-hotspots command
    hotspots_parser = subparsers.add_parser(
        "git-hotspots", help="Find frequently changed files in git history"
    )
    hotspots_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    hotspots_parser.add_argument(
        "--days",
        "-d",
        type=int,
        default=90,
        help="Number of days to analyze (default: 90)",
    )
    hotspots_parser.set_defaults(func=cmd_git_hotspots)

    # coverage command
    coverage_parser = subparsers.add_parser("coverage", help="Show test coverage statistics")
    coverage_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    coverage_parser.add_argument(
        "--run",
        "-r",
        action="store_true",
        help="Run pytest with coverage first",
    )
    coverage_parser.set_defaults(func=cmd_coverage)

    # lint command
    lint_parser = subparsers.add_parser("lint", help="Run unified linting across multiple tools")
    lint_parser.add_argument(
        "paths",
        nargs="*",
        default=["."],
        help="Paths to lint (default: current directory)",
    )
    lint_parser.add_argument(
        "--pattern",
        "-p",
        default="**/*.py",
        help="Glob pattern for files (default: **/*.py)",
    )
    lint_parser.add_argument(
        "--linters",
        "-l",
        help="Comma-separated list of linters to run (default: all available)",
    )
    lint_parser.add_argument(
        "--fix",
        "-f",
        action="store_true",
        help="Attempt to fix issues automatically",
    )
    lint_parser.set_defaults(func=cmd_lint)

    # checkpoint command
    checkpoint_parser = subparsers.add_parser(
        "checkpoint", help="Manage checkpoints (shadow branches) for safe code modifications"
    )
    checkpoint_parser.add_argument(
        "action",
        nargs="?",
        default="list",
        choices=["create", "list", "diff", "merge", "abort", "restore"],
        help="Action to perform (default: list)",
    )
    checkpoint_parser.add_argument(
        "name",
        nargs="?",
        help="Checkpoint name (required for diff, merge, abort)",
    )
    checkpoint_parser.add_argument(
        "--message",
        "-m",
        help="Message for create/merge operations",
    )
    checkpoint_parser.set_defaults(func=cmd_checkpoint)

    # complexity command
    complexity_parser = subparsers.add_parser(
        "complexity", help="Analyze cyclomatic complexity of functions"
    )
    complexity_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    complexity_parser.add_argument(
        "--pattern",
        "-p",
        default="src/**/*.py",
        help="Glob pattern for files (default: src/**/*.py)",
    )
    complexity_parser.set_defaults(func=cmd_complexity)

    # clones command
    clones_parser = subparsers.add_parser("clones", help="Detect structural clones via AST hashing")
    clones_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    clones_parser.add_argument(
        "--level",
        "-l",
        type=int,
        choices=[0, 1, 2, 3],
        default=0,
        help="Elision level: 0=names, 1=+literals, 2=+calls, 3=control-flow (default: 0)",
    )
    clones_parser.add_argument(
        "--min-lines",
        type=int,
        default=3,
        help="Minimum function lines to consider (default: 3)",
    )
    clones_parser.add_argument(
        "--source",
        "-s",
        action="store_true",
        help="Show source code for clones",
    )
    clones_parser.set_defaults(func=cmd_clones)

    # rag command
    rag_parser = subparsers.add_parser("rag", help="Semantic search with RAG indexing")
    rag_parser.add_argument(
        "action",
        choices=["index", "search", "stats", "clear"],
        help="Action: index (build), search (query), stats (show info), clear (reset)",
    )
    rag_parser.add_argument(
        "query",
        nargs="?",
        help="Search query (required for search action)",
    )
    rag_parser.add_argument(
        "--directory",
        "-d",
        default=".",
        help="Project directory (default: current)",
    )
    rag_parser.add_argument(
        "--limit",
        "-n",
        type=int,
        default=10,
        help="Maximum search results (default: 10)",
    )
    rag_parser.add_argument(
        "--mode",
        "-m",
        choices=["hybrid", "embedding", "tfidf"],
        default="hybrid",
        help="Search mode (default: hybrid)",
    )
    rag_parser.add_argument(
        "--force",
        "-f",
        action="store_true",
        help="Force re-indexing (for index action)",
    )
    rag_parser.set_defaults(func=cmd_rag)

    # loop command
    loop_parser = subparsers.add_parser("loop", help="Run composable agent loops")
    loop_parser.add_argument(
        "action",
        nargs="?",
        default="list",
        choices=["list", "run", "benchmark"],
        help="Action: list (show loops), run (execute), benchmark (compare)",
    )
    loop_parser.add_argument(
        "loop_name",
        nargs="?",
        default="simple",
        help="Loop to run: simple, critic, incremental (default: simple)",
    )
    loop_parser.add_argument(
        "--file",
        "-f",
        help="File to process (required for run/benchmark)",
    )
    loop_parser.add_argument(
        "--mock",
        action="store_true",
        help="Use mock LLM responses (for testing)",
    )
    loop_parser.add_argument(
        "--model",
        "-m",
        help="LLM model to use (e.g., gemini/gemini-3-flash-preview)",
    )
    loop_parser.add_argument(
        "--loops",
        nargs="+",
        help="Loops to benchmark (default: all)",
    )
    loop_parser.set_defaults(func=cmd_loop)

    # workflow command
    workflow_parser = subparsers.add_parser("workflow", help="Manage and run TOML-based workflows")
    workflow_parser.add_argument(
        "action",
        nargs="?",
        default="list",
        choices=["list", "show", "run"],
        help="Action: list (show workflows), show (details), run (execute)",
    )
    workflow_parser.add_argument(
        "workflow_name",
        nargs="?",
        help="Workflow to show/run (e.g., validate-fix)",
    )
    workflow_parser.add_argument(
        "--file",
        "-f",
        help="File to process (required for run)",
    )
    workflow_parser.add_argument(
        "--directory",
        "-d",
        default=".",
        help="Project directory for .moss/ lookup (default: current)",
    )
    workflow_parser.add_argument(
        "--mock",
        action="store_true",
        help="Use mock LLM responses (for testing)",
    )
    workflow_parser.set_defaults(func=cmd_workflow)

    # security command
    security_parser = subparsers.add_parser(
        "security", help="Run security analysis with multiple tools"
    )
    security_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    security_parser.add_argument(
        "--tools",
        "-t",
        help="Comma-separated list of tools to use (default: all available)",
    )
    security_parser.add_argument(
        "--severity",
        "-s",
        choices=["low", "medium", "high", "critical"],
        default="low",
        help="Minimum severity to report (default: low)",
    )
    security_parser.set_defaults(func=cmd_security)

    # patterns command
    patterns_parser = subparsers.add_parser(
        "patterns", help="Detect architectural patterns in the codebase"
    )
    patterns_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    patterns_parser.add_argument(
        "--patterns",
        "-p",
        help="Comma-separated patterns to detect (plugin,factory,singleton,coupling)",
    )
    patterns_parser.set_defaults(func=cmd_patterns)

    # weaknesses command
    weaknesses_parser = subparsers.add_parser(
        "weaknesses", help="Identify architectural weaknesses and gaps"
    )
    weaknesses_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to analyze (default: current)",
    )
    weaknesses_parser.add_argument(
        "--categories",
        "-c",
        help="Comma-separated categories (coupling,abstraction,pattern,hardcoded,"
        "error_handling,complexity,duplication)",
    )
    weaknesses_parser.add_argument(
        "--sarif",
        metavar="FILE",
        help="Output results in SARIF format to FILE (for CI integration)",
    )
    weaknesses_parser.add_argument(
        "--fix",
        action="store_true",
        help="Show fix suggestions for auto-correctable issues",
    )
    weaknesses_parser.set_defaults(func=cmd_weaknesses)

    # eval command
    eval_parser = subparsers.add_parser("eval", help="Run evaluation benchmarks (SWE-bench, etc.)")
    eval_parser.add_argument(
        "benchmark",
        nargs="?",
        default="swebench",
        choices=["swebench"],
        help="Benchmark to run (default: swebench)",
    )
    eval_parser.add_argument(
        "action",
        nargs="?",
        default="list",
        choices=["list", "run", "info"],
        help="Action: list instances, run evaluation, or show info (default: list)",
    )
    eval_parser.add_argument(
        "--subset",
        "-s",
        default="lite",
        choices=["lite", "verified", "full"],
        help="SWE-bench subset (default: lite)",
    )
    eval_parser.add_argument(
        "--strategy",
        default="moss",
        choices=["moss", "bash", "hybrid"],
        help="Agent strategy: moss (structural), bash (minimal), hybrid (default: moss)",
    )
    eval_parser.add_argument(
        "--instance",
        "-i",
        nargs="+",
        help="Specific instance ID(s) to run or show info",
    )
    eval_parser.add_argument(
        "--limit",
        "-n",
        type=int,
        help="Maximum number of instances to run",
    )
    eval_parser.add_argument(
        "--max-iterations",
        type=int,
        default=10,
        help="Maximum agent iterations per instance (default: 10)",
    )
    eval_parser.set_defaults(func=cmd_eval)

    # dwim command (find the right tool)
    dwim_parser = subparsers.add_parser(
        "dwim", help="Find the right moss tool using natural language"
    )
    dwim_parser.add_argument(
        "query",
        nargs="?",
        help="Natural language description of what you want to do",
    )
    dwim_parser.add_argument(
        "--tool",
        "-t",
        help="Get info about a specific tool",
    )
    dwim_parser.add_argument(
        "--top",
        "-n",
        type=int,
        default=5,
        help="Number of results to show (default: 5)",
    )
    dwim_parser.set_defaults(func=cmd_dwim)

    # agent command - DWIM-driven agent loop
    agent_parser = subparsers.add_parser("agent", help="Run DWIM-driven agent loop on a task")
    agent_parser.add_argument(
        "task",
        nargs="?",
        help="Task description in natural language",
    )
    agent_parser.add_argument(
        "--model",
        "-m",
        help="LLM model to use (default: gemini/gemini-2.5-flash-preview-05-20)",
    )
    agent_parser.add_argument(
        "--max-turns",
        "-n",
        type=int,
        default=50,
        help="Maximum turns before stopping (default: 50)",
    )
    agent_parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Show detailed turn-by-turn output",
    )
    agent_parser.add_argument(
        "--dry-run",
        action="store_true",
        dest="dry_run",
        help="Show what would be executed without running",
    )
    agent_parser.set_defaults(func=cmd_agent)

    # help command (with examples and categories)
    help_parser = subparsers.add_parser(
        "help", help="Show detailed help for commands with examples"
    )
    help_parser.add_argument(
        "topic",
        nargs="?",
        help="Command to get help for (omit for category list)",
    )
    help_parser.set_defaults(func=cmd_help)

    return parser


def main(argv: list[str] | None = None) -> int:
    """Main entry point."""
    parser = create_parser()
    args = parser.parse_args(argv)

    if not args.command:
        parser.print_help()
        return 0

    # Configure output based on global flags
    setup_output(args)

    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
