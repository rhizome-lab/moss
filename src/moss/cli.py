"""Command-line interface for Moss."""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from argparse import Namespace


def get_version() -> str:
    """Get the moss version."""
    from moss import __version__

    return __version__


def output_result(data: Any, args: Namespace) -> None:
    """Output result in appropriate format."""
    if getattr(args, "json", False):
        print(json.dumps(data, indent=2, default=str))
    else:
        if isinstance(data, str):
            print(data)
        elif isinstance(data, dict):
            for key, value in data.items():
                print(f"{key}: {value}")
        elif isinstance(data, list):
            for item in data:
                print(item)
        else:
            print(data)


def cmd_init(args: Namespace) -> int:
    """Initialize a new moss project."""
    project_dir = Path(args.directory).resolve()

    if not project_dir.exists():
        print(f"Error: Directory {project_dir} does not exist")
        return 1

    config_file = project_dir / "moss_config.py"

    if config_file.exists() and not args.force:
        print(f"Error: {config_file} already exists. Use --force to overwrite.")
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
# config = config.with_static_context(Path("docs/architecture.md"))

# Add custom validators
# from moss.validators import CommandValidator
# config = config.add_validator(CommandValidator("mypy", ["mypy", "."]))
'''

    config_file.write_text(config_content)
    print(f"Created {config_file}")

    # Create .moss directory for runtime data
    moss_dir = project_dir / ".moss"
    if not moss_dir.exists():
        moss_dir.mkdir()
        (moss_dir / ".gitignore").write_text("*\n")
        print(f"Created {moss_dir}/")

    print(f"\nMoss initialized in {project_dir}")
    print(f"  Config: {config_file.name}")
    print(f"  Distro: {distro_name}")
    print("\nNext steps:")
    print("  1. Edit moss_config.py to customize your configuration")
    print("  2. Run 'moss run \"your task\"' to execute a task")

    return 0


def cmd_run(args: Namespace) -> int:
    """Run a task through moss."""
    from moss.agents import create_manager
    from moss.api import TaskRequest, create_api_handler
    from moss.config import MossConfig, load_config_file
    from moss.events import EventBus
    from moss.shadow_git import ShadowGit

    project_dir = Path(args.directory).resolve()
    config_file = project_dir / "moss_config.py"

    # Load config
    if config_file.exists():
        try:
            config = load_config_file(config_file)
        except Exception as e:
            print(f"Error loading config: {e}")
            return 1
    else:
        config = MossConfig().with_project(project_dir, project_dir.name)

    # Validate config
    errors = config.validate()
    if errors:
        print("Configuration errors:")
        for error in errors:
            print(f"  - {error}")
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
        print(f"Task created: {response.request_id}")
        print(f"Ticket: {response.ticket_id}")
        print(f"Status: {response.status.value}")

        if args.wait:
            print("\nWaiting for completion...")
            # Poll for status
            while True:
                status = await handler.get_task_status(response.request_id)
                if status is None:
                    print("Task not found")
                    return 1

                if status.status.value in ("COMPLETED", "FAILED", "CANCELLED"):
                    print(f"\nFinal status: {status.status.value}")
                    if status.result:
                        print(f"Result: {json.dumps(status.result, indent=2)}")
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

    project_dir = Path(args.directory).resolve()
    config_file = project_dir / "moss_config.py"

    # Load config (validates it's readable)
    if config_file.exists():
        try:
            load_config_file(config_file)
        except Exception as e:
            print(f"Error loading config: {e}")
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

    print("Moss Status")
    print("=" * 40)
    print(f"Project: {project_dir.name}")
    print(f"Config: {'moss_config.py' if config_file.exists() else '(default)'}")
    print()
    print("API Handler:")
    print(f"  Active requests: {stats['active_requests']}")
    print(f"  Pending checkpoints: {stats['pending_checkpoints']}")
    print(f"  Active streams: {stats['active_streams']}")
    print()
    print("Manager:")
    manager_stats = stats["manager_stats"]
    print(f"  Active workers: {manager_stats['active_workers']}")
    print(f"  Total tickets: {manager_stats['total_tickets']}")
    tickets_by_status = manager_stats.get("tickets_by_status", {})
    if tickets_by_status:
        print(f"  Tickets by status: {tickets_by_status}")

    if args.verbose:
        print()
        print("Workers:")
        for worker_id, worker_info in manager_stats.get("workers", {}).items():
            print(f"  {worker_id}: {worker_info}")

    return 0


def cmd_config(args: Namespace) -> int:
    """Show or validate configuration."""
    from moss.config import list_distros, load_config_file

    if args.list_distros:
        print("Available distros:")
        for name in list_distros():
            print(f"  - {name}")
        return 0

    project_dir = Path(args.directory).resolve()
    config_file = project_dir / "moss_config.py"

    if not config_file.exists():
        print(f"No config file found at {config_file}")
        print("Run 'moss init' to create one.")
        return 1

    try:
        config = load_config_file(config_file)
    except Exception as e:
        print(f"Error loading config: {e}")
        return 1

    if args.validate:
        errors = config.validate()
        if errors:
            print("Configuration errors:")
            for error in errors:
                print(f"  - {error}")
            return 1
        print("Configuration is valid.")
        return 0

    # Show config
    print("Configuration")
    print("=" * 40)
    print(f"Project: {config.project_name}")
    print(f"Root: {config.project_root}")
    print(f"Extends: {', '.join(config.extends) or '(none)'}")
    print()
    print("Validators:")
    print(f"  syntax: {config.validators.syntax}")
    print(f"  ruff: {config.validators.ruff}")
    print(f"  pytest: {config.validators.pytest}")
    print(f"  custom: {len(config.validators.custom)}")
    print()
    print("Policies:")
    print(f"  velocity: {config.policies.velocity}")
    print(f"  quarantine: {config.policies.quarantine}")
    print(f"  rate_limit: {config.policies.rate_limit}")
    print(f"  path: {config.policies.path}")
    print()
    print("Loop:")
    print(f"  max_iterations: {config.loop.max_iterations}")
    print(f"  timeout_seconds: {config.loop.timeout_seconds}")
    print(f"  auto_commit: {config.loop.auto_commit}")

    return 0


def cmd_distros(args: Namespace) -> int:
    """List available configuration distros."""
    from moss.config import get_distro, list_distros

    distros = list_distros()

    if getattr(args, "json", False):
        result = []
        for name in sorted(distros):
            distro = get_distro(name)
            if distro:
                result.append({"name": name, "description": distro.description})
        output_result(result, args)
        return 0

    print("Available Distros")
    print("=" * 40)

    for name in sorted(distros):
        distro = get_distro(name)
        if distro:
            desc = distro.description or "(no description)"
            print(f"  {name}: {desc}")

    return 0


# =============================================================================
# Introspection Commands
# =============================================================================


def cmd_skeleton(args: Namespace) -> int:
    """Extract code skeleton from a file or directory."""
    from moss.skeleton import extract_python_skeleton, format_skeleton

    path = Path(args.path).resolve()

    if not path.exists():
        print(f"Error: Path {path} does not exist", file=sys.stderr)
        return 1

    results = []

    if path.is_file():
        files = [path]
    else:
        pattern = args.pattern or "**/*.py"
        files = list(path.glob(pattern))

    # Determine if we should include private symbols
    include_private = not getattr(args, "public_only", False)

    for file_path in files:
        try:
            source = file_path.read_text()
            symbols = extract_python_skeleton(source, include_private=include_private)

            if getattr(args, "json", False):
                results.append(
                    {
                        "file": str(file_path),
                        "symbols": [_symbol_to_dict(s) for s in symbols],
                    }
                )
            else:
                if len(files) > 1:
                    print(f"\n=== {file_path} ===")
                skeleton = format_skeleton(symbols)
                if skeleton:
                    print(skeleton)
                elif not args.quiet:
                    print("(no symbols found)")
        except SyntaxError as e:
            if getattr(args, "json", False):
                results.append({"file": str(file_path), "error": str(e)})
            else:
                print(f"Syntax error in {file_path}: {e}", file=sys.stderr)

    if getattr(args, "json", False):
        output_result(results if len(results) > 1 else results[0] if results else {}, args)

    return 0


def _symbol_to_dict(symbol: Any) -> dict:
    """Convert a Symbol to a dictionary."""
    result = {
        "name": symbol.name,
        "kind": symbol.kind,
        "line": symbol.lineno,
    }
    if symbol.end_lineno is not None:
        result["end_line"] = symbol.end_lineno
        result["line_count"] = symbol.line_count
    if symbol.signature:
        result["signature"] = symbol.signature
    if symbol.docstring:
        result["docstring"] = symbol.docstring
    if symbol.children:
        result["children"] = [_symbol_to_dict(c) for c in symbol.children]
    return result


def cmd_anchors(args: Namespace) -> int:
    """Find anchors (functions, classes, methods) in code."""
    import re

    from moss.skeleton import extract_python_skeleton

    path = Path(args.path).resolve()

    if not path.exists():
        print(f"Error: Path {path} does not exist", file=sys.stderr)
        return 1

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
            source = file_path.read_text()
            symbols = extract_python_skeleton(source)
            collect_symbols(symbols, file_path)
        except SyntaxError as e:
            if not args.quiet:
                print(f"Syntax error in {file_path}: {e}", file=sys.stderr)

    if getattr(args, "json", False):
        output_result(results, args)
    else:
        for r in results:
            ctx = f" (in {r['context']})" if r.get("context") else ""
            print(f"{r['file']}:{r['line']} {r['type']} {r['name']}{ctx}")

    return 0


def cmd_query(args: Namespace) -> int:
    """Query code with pattern matching and filters."""
    import re

    from moss.skeleton import extract_python_skeleton

    path = Path(args.path).resolve()

    if not path.exists():
        print(f"Error: Path {path} does not exist", file=sys.stderr)
        return 1

    if path.is_file():
        files = [path]
    else:
        pattern = args.pattern or "**/*.py"
        files = list(path.glob(pattern))

    # Compile patterns
    name_pattern = re.compile(args.name) if args.name else None
    sig_pattern = re.compile(args.signature) if args.signature else None

    results: list[dict] = []

    def matches_filters(sym: Any) -> bool:
        """Check if a symbol matches all filters."""
        # Type filter
        if args.type and args.type != "all":
            if sym.kind != args.type:
                return False

        # Name filter
        if name_pattern and not name_pattern.search(sym.name):
            return False

        # Signature filter
        if sig_pattern and sym.signature:
            if not sig_pattern.search(sym.signature):
                return False

        # Inheritance filter (for classes)
        if args.inherits and sym.kind == "class":
            # Check if class inherits from specified base (look for "(Base" pattern)
            if f"({args.inherits}" not in sym.signature:
                return False

        # Line count filters
        if args.min_lines is not None or args.max_lines is not None:
            line_count = sym.line_count
            if line_count is None:
                return False  # Can't filter if no line count
            if args.min_lines is not None and line_count < args.min_lines:
                return False
            if args.max_lines is not None and line_count > args.max_lines:
                return False

        return True

    def collect_matches(symbols: list, file_str: str, parent: str | None = None) -> None:
        """Recursively collect matching symbols."""
        for sym in symbols:
            if matches_filters(sym):
                result = {
                    "file": file_str,
                    "name": sym.name,
                    "kind": sym.kind,
                    "line": sym.lineno,
                    "signature": sym.signature,
                }
                if sym.end_lineno is not None:
                    result["end_line"] = sym.end_lineno
                    result["line_count"] = sym.line_count
                if sym.docstring:
                    result["docstring"] = sym.docstring
                if parent:
                    result["context"] = parent
                results.append(result)

            # Always recurse into children to find nested matches
            if sym.children:
                collect_matches(sym.children, file_str, sym.name)

    for file_path in files:
        try:
            source = file_path.read_text()
            symbols = extract_python_skeleton(source)
            collect_matches(symbols, str(file_path))
        except SyntaxError as e:
            if not args.quiet:
                print(f"Syntax error in {file_path}: {e}", file=sys.stderr)

    if getattr(args, "json", False):
        if getattr(args, "group_by", None) == "file":
            # Group results by file for JSON output
            grouped: dict[str, list] = {}
            for r in results:
                grouped.setdefault(r["file"], []).append(r)
            output_result(grouped, args)
        else:
            output_result(results, args)
    else:
        if getattr(args, "group_by", None) == "file":
            # Group results by file for text output
            grouped: dict[str, list] = {}
            for r in results:
                grouped.setdefault(r["file"], []).append(r)
            for file_path, file_results in grouped.items():
                print(f"\n{file_path}:")
                for r in file_results:
                    ctx = f" (in {r['context']})" if r.get("context") else ""
                    print(f"  :{r['line']} {r['kind']} {r['name']}{ctx}")
                    if r.get("signature"):
                        print(f"    {r['signature']}")
        else:
            for r in results:
                ctx = f" (in {r['context']})" if r.get("context") else ""
                doc = f" - {r['docstring'][:50]}..." if r.get("docstring") else ""
                print(f"{r['file']}:{r['line']} {r['kind']} {r['name']}{ctx}")
                if r.get("signature"):
                    print(f"  {r['signature']}")
                if doc:
                    print(f" {doc}")

    if not results:
        print("No matches found")

    return 0


def cmd_cfg(args: Namespace) -> int:
    """Build and display control flow graph."""
    from moss.cfg import build_cfg

    path = Path(args.path).resolve()

    if not path.exists():
        print(f"Error: Path {path} does not exist", file=sys.stderr)
        return 1

    if not path.is_file():
        print(f"Error: {path} must be a file", file=sys.stderr)
        return 1

    try:
        source = path.read_text()
        cfgs = build_cfg(source, function_name=args.function)
    except SyntaxError as e:
        print(f"Syntax error: {e}", file=sys.stderr)
        return 1

    if not cfgs:
        print("No functions found", file=sys.stderr)
        return 1

    if getattr(args, "json", False):
        results = []
        for cfg in cfgs:
            result = {
                "name": cfg.name,
                "node_count": cfg.node_count,
                "edge_count": cfg.edge_count,
                "cyclomatic_complexity": cfg.cyclomatic_complexity,
            }
            # Include full graph details unless --summary
            if not args.summary:
                result["entry"] = cfg.entry_node
                result["exit"] = cfg.exit_node
                result["nodes"] = {
                    nid: {
                        "type": n.node_type.value,
                        "statements": n.statements,
                        "line_start": n.line_start,
                    }
                    for nid, n in cfg.nodes.items()
                }
                result["edges"] = [
                    {
                        "source": e.source,
                        "target": e.target,
                        "type": e.edge_type.value,
                        "condition": e.condition,
                    }
                    for e in cfg.edges
                ]
            results.append(result)
        output_result(results, args)
    else:
        for cfg in cfgs:
            if args.summary:
                # Summary mode: just show counts and complexity
                print(
                    f"{cfg.name}: {cfg.node_count} nodes, {cfg.edge_count} edges, "
                    f"complexity {cfg.cyclomatic_complexity}"
                )
            elif args.dot:
                print(cfg.to_dot())
            else:
                print(cfg.to_text())
                print()

    return 0


def cmd_deps(args: Namespace) -> int:
    """Extract dependencies (imports/exports) from code."""
    from moss.dependencies import (
        build_dependency_graph,
        dependency_graph_to_dot,
        extract_dependencies,
        find_reverse_dependencies,
        format_dependencies,
    )

    path = Path(args.path).resolve()

    if not path.exists():
        print(f"Error: Path {path} does not exist", file=sys.stderr)
        return 1

    # Handle --dot mode: generate dependency graph visualization
    if getattr(args, "dot", False):
        if not path.is_dir():
            print("Error: --dot requires a directory path", file=sys.stderr)
            return 1

        pattern = args.pattern or "**/*.py"
        graph = build_dependency_graph(str(path), pattern)

        if not graph:
            print("No internal dependencies found", file=sys.stderr)
            return 1

        dot_output = dependency_graph_to_dot(graph, title=path.name)
        print(dot_output)
        return 0

    # Handle --reverse mode: find what imports the target module
    if args.reverse:
        search_dir = args.search_dir or "."
        pattern = args.pattern or "**/*.py"
        reverse_deps = find_reverse_dependencies(args.reverse, search_dir, pattern)

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
                print(f"Files that import '{args.reverse}':")
                for rd in reverse_deps:
                    names = f" ({', '.join(rd.names)})" if rd.names else ""
                    print(f"  {rd.file}:{rd.import_line} {rd.import_type}{names}")
            else:
                print(f"No files found that import '{args.reverse}'")

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
            source = file_path.read_text()
            deps = extract_dependencies(source)

            if getattr(args, "json", False):
                results.append(
                    {
                        "file": str(file_path),
                        "imports": [
                            {
                                "module": i.module,
                                "names": i.names,
                                "alias": i.alias,
                                "line": i.lineno,
                            }
                            for i in deps.imports
                        ],
                        "exports": [
                            {"name": e.name, "kind": e.kind, "line": e.lineno} for e in deps.exports
                        ],
                    }
                )
            else:
                if len(files) > 1:
                    print(f"\n=== {file_path} ===")
                formatted = format_dependencies(deps)
                if formatted:
                    print(formatted)

        except SyntaxError as e:
            if not args.quiet:
                print(f"Syntax error in {file_path}: {e}", file=sys.stderr)

    if getattr(args, "json", False):
        output_result(results if len(results) > 1 else results[0] if results else {}, args)

    return 0


def cmd_context(args: Namespace) -> int:
    """Generate compiled context for a file (skeleton + deps + summary)."""
    from moss.dependencies import extract_dependencies, format_dependencies
    from moss.skeleton import extract_python_skeleton, format_skeleton

    path = Path(args.path).resolve()

    if not path.exists():
        print(f"Error: Path {path} does not exist", file=sys.stderr)
        return 1

    if not path.is_file():
        print(f"Error: {path} must be a file", file=sys.stderr)
        return 1

    try:
        source = path.read_text()
        symbols = extract_python_skeleton(source)
        deps = extract_dependencies(source)
    except SyntaxError as e:
        print(f"Syntax error: {e}", file=sys.stderr)
        return 1

    # Count symbols
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
    line_count = len(source.splitlines())

    if getattr(args, "json", False):
        result = {
            "file": str(path),
            "summary": {
                "lines": line_count,
                "classes": counts["classes"],
                "functions": counts["functions"],
                "methods": counts["methods"],
                "imports": len(deps.imports),
                "exports": len(deps.exports),
            },
            "symbols": [_symbol_to_dict(s) for s in symbols],
            "imports": [
                {"module": i.module, "names": i.names, "alias": i.alias, "line": i.lineno}
                for i in deps.imports
            ],
            "exports": [{"name": e.name, "kind": e.kind, "line": e.lineno} for e in deps.exports],
        }
        output_result(result, args)
    else:
        print(f"=== {path.name} ===")
        print(f"Lines: {line_count}")
        print(
            f"Classes: {counts['classes']}, "
            f"Functions: {counts['functions']}, Methods: {counts['methods']}"
        )
        print(f"Imports: {len(deps.imports)}, Exports: {len(deps.exports)}")
        print()

        if deps.imports:
            print("--- Imports ---")
            print(format_dependencies(deps).split("Exports:")[0].strip())
            print()

        print("--- Skeleton ---")
        skeleton = format_skeleton(symbols)
        if skeleton:
            print(skeleton)
        else:
            print("(no symbols)")

    return 0


def cmd_mcp_server(args: Namespace) -> int:
    """Start the MCP server for LLM tool access."""
    try:
        from moss.mcp_server import main as mcp_main

        mcp_main()
        return 0
    except ImportError as e:
        print("Error: MCP SDK not installed. Install with: pip install 'moss[mcp]'")
        print(f"Details: {e}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 0


def create_parser() -> argparse.ArgumentParser:
    """Create the argument parser."""
    parser = argparse.ArgumentParser(
        prog="moss",
        description="Headless agent orchestration layer for AI engineering",
    )
    parser.add_argument("--version", action="version", version=f"%(prog)s {get_version()}")
    parser.add_argument("--json", "-j", action="store_true", help="Output in JSON format")

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
    status_parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Show verbose output",
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
    skeleton_parser.add_argument(
        "--quiet", "-q", action="store_true", help="Suppress empty file messages"
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
    anchors_parser.add_argument("--quiet", "-q", action="store_true", help="Suppress errors")
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
    query_parser.add_argument("--quiet", "-q", action="store_true", help="Suppress errors")
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
    cfg_parser.add_argument(
        "--summary", "-s", action="store_true", help="Show only node/edge counts"
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
    deps_parser.add_argument("--quiet", "-q", action="store_true", help="Suppress errors")
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

    # mcp-server command
    mcp_parser = subparsers.add_parser("mcp-server", help="Start MCP server for LLM tool access")
    mcp_parser.set_defaults(func=cmd_mcp_server)

    return parser


def main(argv: list[str] | None = None) -> int:
    """Main entry point."""
    parser = create_parser()
    args = parser.parse_args(argv)

    if not args.command:
        parser.print_help()
        return 0

    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
