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
    from moss.plugins import get_registry
    from moss.views import ViewOptions, ViewTarget

    path = Path(args.path).resolve()

    if not path.exists():
        print(f"Error: Path {path} does not exist", file=sys.stderr)
        return 1

    results = []
    registry = get_registry()

    if path.is_file():
        files = [path]
    else:
        pattern = args.pattern or "**/*.py"
        files = list(path.glob(pattern))

    # Determine if we should include private symbols
    include_private = not getattr(args, "public_only", False)
    options = ViewOptions(include_private=include_private)

    async def render_file(file_path: Path) -> dict | None:
        """Render skeleton for a single file."""
        target = ViewTarget(path=file_path)
        plugin = registry.find_plugin(target, "skeleton")

        if plugin is None:
            return {"file": str(file_path), "error": "No plugin found for this file type"}

        view = await plugin.render(target, options)

        if "error" in view.metadata:
            return {"file": str(file_path), "error": view.metadata["error"]}

        return {
            "file": str(file_path),
            "content": view.content,
            "symbols": view.metadata.get("symbols", []),
        }

    # Run async rendering
    async def render_all() -> list[dict]:
        render_results = []
        for file_path in files:
            result = await render_file(file_path)
            if result:
                render_results.append(result)
        return render_results

    rendered = asyncio.run(render_all())

    for result in rendered:
        if "error" in result:
            if getattr(args, "json", False):
                results.append({"file": result["file"], "error": result["error"]})
            else:
                print(f"Error in {result['file']}: {result['error']}", file=sys.stderr)
        else:
            if getattr(args, "json", False):
                results.append({"file": result["file"], "symbols": result["symbols"]})
            else:
                if len(files) > 1:
                    print(f"\n=== {result['file']} ===")
                content = result["content"]
                if content:
                    print(content)
                elif not args.quiet:
                    print("(no symbols found)")

    if getattr(args, "json", False):
        output_result(results if len(results) > 1 else results[0] if results else {}, args)

    return 0


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
    from moss.plugins import get_registry
    from moss.views import ViewOptions, ViewTarget

    path = Path(args.path).resolve()

    if not path.exists():
        print(f"Error: Path {path} does not exist", file=sys.stderr)
        return 1

    if not path.is_file():
        print(f"Error: {path} must be a file", file=sys.stderr)
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

    registry = get_registry()
    target = ViewTarget(path=path)
    plugin = registry.find_plugin(target, "cfg")

    if plugin is None:
        print("Error: No CFG plugin available for this file type", file=sys.stderr)
        return 1

    options = ViewOptions(extra={"function_name": args.function})

    async def render_cfg():
        return await plugin.render(target, options)

    view = asyncio.run(render_cfg())

    if "error" in view.metadata:
        print(f"Error: {view.metadata['error']}", file=sys.stderr)
        return 1

    cfgs = view.metadata.get("cfgs", [])

    if not cfgs:
        print("No functions found", file=sys.stderr)
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
        for cfg_data in cfgs:
            result = {
                "name": cfg_data["name"],
                "node_count": cfg_data["node_count"],
                "edge_count": cfg_data["edge_count"],
                "cyclomatic_complexity": cfg_data["cyclomatic_complexity"],
            }
            # Include full graph details unless --summary
            if not args.summary:
                result["entry"] = cfg_data.get("entry")
                result["exit"] = cfg_data.get("exit")
                result["nodes"] = cfg_data.get("nodes", {})
                result["edges"] = cfg_data.get("edges", [])
            results.append(result)
        output_result(results, args)
    elif args.html or output_format == "html":
        # HTML output with embedded Mermaid
        from moss.cfg import CFGBuilder
        from moss.visualization import visualize_cfgs

        builder = CFGBuilder()
        cfg_objects = []
        source = path.read_text()
        for cfg_data in cfgs:
            cfg = builder.build_from_source(source, cfg_data["name"])
            if cfg:
                cfg_objects.append(cfg)

        content = visualize_cfgs(cfg_objects, format="html")
        if args.output:
            Path(args.output).write_text(content)
            print(f"Saved to {args.output}")
        else:
            print(content)
    elif args.mermaid or output_format == "mermaid":
        # Mermaid output
        mermaid_lines = view.metadata.get("mermaid", "")
        if not mermaid_lines:
            # Generate from CFGs
            from moss.cfg import CFGBuilder

            builder = CFGBuilder()
            source = path.read_text()
            mermaid_parts = []
            for cfg_data in cfgs:
                cfg = builder.build_from_source(source, cfg_data["name"])
                if cfg:
                    mermaid_parts.append(cfg.to_mermaid())
            mermaid_lines = "\n\n".join(mermaid_parts)

        if args.output:
            Path(args.output).write_text(mermaid_lines)
            print(f"Saved to {args.output}")
        else:
            print(mermaid_lines)
    elif args.summary:
        # Summary mode: just show counts and complexity
        for cfg_data in cfgs:
            print(
                f"{cfg_data['name']}: {cfg_data['node_count']} nodes, "
                f"{cfg_data['edge_count']} edges, "
                f"complexity {cfg_data['cyclomatic_complexity']}"
            )
    elif args.dot or output_format == "dot":
        # DOT output - use raw content from view
        dot_content = view.metadata.get("dot", view.content)
        if args.output:
            Path(args.output).write_text(dot_content)
            print(f"Saved to {args.output}")
        else:
            print(dot_content)
    elif output_format == "svg":
        from moss.visualization import render_dot_to_svg

        dot_content = view.metadata.get("dot", "")
        if dot_content:
            svg = render_dot_to_svg(dot_content)
            Path(args.output).write_text(svg)
            print(f"Saved to {args.output}")
        else:
            print("Error: No DOT content available for SVG rendering", file=sys.stderr)
            return 1
    elif output_format == "png":
        from moss.visualization import render_dot_to_png

        dot_content = view.metadata.get("dot", "")
        if dot_content:
            png = render_dot_to_png(dot_content)
            Path(args.output).write_bytes(png)
            print(f"Saved to {args.output}")
        else:
            print("Error: No DOT content available for PNG rendering", file=sys.stderr)
            return 1
    else:
        print(view.content)

    return 0


def cmd_deps(args: Namespace) -> int:
    """Extract dependencies (imports/exports) from code."""
    from moss.dependencies import (
        build_dependency_graph,
        dependency_graph_to_dot,
        find_reverse_dependencies,
    )
    from moss.plugins import get_registry
    from moss.views import ViewTarget

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

    # Normal mode: show dependencies of file(s) using plugin registry
    results = []
    registry = get_registry()

    if path.is_file():
        files = [path]
    else:
        pattern = args.pattern or "**/*.py"
        files = list(path.glob(pattern))

    async def render_file(file_path: Path) -> dict | None:
        """Render dependencies for a single file."""
        target = ViewTarget(path=file_path)
        plugin = registry.find_plugin(target, "dependency")

        if plugin is None:
            return {"file": str(file_path), "error": "No plugin found for this file type"}

        view = await plugin.render(target)

        if "error" in view.metadata:
            return {"file": str(file_path), "error": view.metadata["error"]}

        return {
            "file": str(file_path),
            "content": view.content,
            "imports": view.metadata.get("imports", []),
            "exports": view.metadata.get("exports", []),
        }

    async def render_all() -> list[dict]:
        render_results = []
        for file_path in files:
            result = await render_file(file_path)
            if result:
                render_results.append(result)
        return render_results

    rendered = asyncio.run(render_all())

    for result in rendered:
        if "error" in result:
            if not args.quiet:
                print(f"Error in {result['file']}: {result['error']}", file=sys.stderr)
            if getattr(args, "json", False):
                results.append({"file": result["file"], "error": result["error"]})
        else:
            if getattr(args, "json", False):
                results.append(
                    {
                        "file": result["file"],
                        "imports": result["imports"],
                        "exports": result["exports"],
                    }
                )
            else:
                if len(files) > 1:
                    print(f"\n=== {result['file']} ===")
                content = result["content"]
                if content:
                    print(content)

    if getattr(args, "json", False):
        output_result(results if len(results) > 1 else results[0] if results else {}, args)

    return 0


def cmd_context(args: Namespace) -> int:
    """Generate compiled context for a file (skeleton + deps + summary)."""
    from moss.plugins import get_registry
    from moss.views import ViewTarget

    path = Path(args.path).resolve()

    if not path.exists():
        print(f"Error: Path {path} does not exist", file=sys.stderr)
        return 1

    if not path.is_file():
        print(f"Error: {path} must be a file", file=sys.stderr)
        return 1

    registry = get_registry()
    target = ViewTarget(path=path)

    # Find plugins for skeleton and dependency
    skeleton_plugin = registry.find_plugin(target, "skeleton")
    deps_plugin = registry.find_plugin(target, "dependency")

    if skeleton_plugin is None and deps_plugin is None:
        print("Error: No plugins available for this file type", file=sys.stderr)
        return 1

    async def render_views():
        skeleton_view = None
        deps_view = None

        if skeleton_plugin:
            skeleton_view = await skeleton_plugin.render(target)
        if deps_plugin:
            deps_view = await deps_plugin.render(target)

        return skeleton_view, deps_view

    skeleton_view, deps_view = asyncio.run(render_views())

    # Check for errors
    if skeleton_view and "error" in skeleton_view.metadata:
        print(f"Error: {skeleton_view.metadata['error']}", file=sys.stderr)
        return 1

    if deps_view and "error" in deps_view.metadata:
        print(f"Error: {deps_view.metadata['error']}", file=sys.stderr)
        return 1

    # Get data from views
    symbols = skeleton_view.metadata.get("symbols", []) if skeleton_view else []
    imports = deps_view.metadata.get("imports", []) if deps_view else []
    exports = deps_view.metadata.get("exports", []) if deps_view else []
    skeleton_content = skeleton_view.content if skeleton_view else ""
    deps_content = deps_view.content if deps_view else ""

    # Count symbols from metadata
    def count_symbols(syms: list) -> dict:
        counts = {"classes": 0, "functions": 0, "methods": 0}
        for s in syms:
            kind = s.get("kind", "")
            if kind == "class":
                counts["classes"] += 1
            elif kind == "function":
                counts["functions"] += 1
            elif kind == "method":
                counts["methods"] += 1
            children = s.get("children", [])
            if children:
                child_counts = count_symbols(children)
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
                "imports": len(imports),
                "exports": len(exports),
            },
            "symbols": symbols,
            "imports": imports,
            "exports": exports,
        }
        output_result(result, args)
    else:
        print(f"=== {path.name} ===")
        print(f"Lines: {line_count}")
        print(
            f"Classes: {counts['classes']}, "
            f"Functions: {counts['functions']}, Methods: {counts['methods']}"
        )
        print(f"Imports: {len(imports)}, Exports: {len(exports)}")
        print()

        if imports and deps_content:
            print("--- Imports ---")
            # Extract just the imports section from deps content
            imports_section = deps_content.split("Exports:")[0].strip()
            print(imports_section)
            print()

        print("--- Skeleton ---")
        if skeleton_content:
            print(skeleton_content)
        else:
            print("(no symbols)")

    return 0


def cmd_search(args: Namespace) -> int:
    """Semantic search across codebase."""
    from moss.semantic_search import create_search_system

    directory = Path(args.directory).resolve()
    if not directory.exists():
        print(f"Error: Directory {directory} does not exist", file=sys.stderr)
        return 1

    # Create search system
    backend = "chroma" if args.persist else "memory"
    kwargs: dict[str, Any] = {}
    if args.persist:
        kwargs["collection_name"] = "moss_search"
        kwargs["persist_directory"] = str(directory / ".moss" / "search_index")

    indexer, search = create_search_system(backend, **kwargs)

    async def run_search():
        # Index if requested or if no index exists
        if args.index:
            patterns = args.patterns.split(",") if args.patterns else None
            exclude = args.exclude.split(",") if args.exclude else None
            count = await indexer.index_directory(directory, patterns, exclude)
            if not args.query:
                print(f"Indexed {count} chunks from {directory}")
                return None

        if not args.query:
            print("Error: No query provided. Use --query or --index", file=sys.stderr)
            return None

        # Search
        results = await search.search(
            args.query,
            limit=args.limit,
            mode=args.mode,
        )
        return results

    results = asyncio.run(run_search())

    if results is None:
        return 0 if args.index else 1

    if not results:
        print("No results found.")
        return 0

    if getattr(args, "json", False):
        output = [
            {
                "file": r.chunk.file_path,
                "symbol": r.chunk.symbol_name,
                "kind": r.chunk.symbol_kind,
                "line_start": r.chunk.line_start,
                "line_end": r.chunk.line_end,
                "score": r.score,
                "match_type": r.match_type,
            }
            for r in results
        ]
        output_result(output, args)
    else:
        print(f"Found {len(results)} results:\n")
        for i, hit in enumerate(results, 1):
            chunk = hit.chunk
            location = f"{chunk.file_path}:{chunk.line_start}"
            name = chunk.symbol_name or chunk.file_path
            kind = chunk.symbol_kind or "file"
            score = f"{hit.score:.2f}"

            print(f"{i}. [{kind}] {name}")
            print(f"   Location: {location}")
            print(f"   Score: {score} ({hit.match_type})")

            # Show snippet
            if chunk.content:
                snippet = chunk.content[:200]
                if len(chunk.content) > 200:
                    snippet += "..."
                # Indent snippet
                snippet_lines = snippet.split("\n")[:3]
                for line in snippet_lines:
                    print(f"   | {line}")
            print()

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


def cmd_lsp(args: Namespace) -> int:
    """Start the LSP server for IDE integration."""
    try:
        from moss.lsp_server import start_server

        transport = getattr(args, "transport", "stdio")
        start_server(transport)
        return 0
    except ImportError as e:
        print("Error: LSP dependencies not installed. Install with: pip install 'moss[lsp]'")
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
    search_parser.add_argument("--json", action="store_true", help="JSON output")
    search_parser.set_defaults(func=cmd_search)

    # mcp-server command
    mcp_parser = subparsers.add_parser("mcp-server", help="Start MCP server for LLM tool access")
    mcp_parser.set_defaults(func=cmd_mcp_server)

    # lsp command
    lsp_parser = subparsers.add_parser("lsp", help="Start LSP server for IDE integration")
    lsp_parser.add_argument(
        "--transport",
        "-t",
        default="stdio",
        help="Transport: 'stdio' (default) or 'tcp:host:port'",
    )
    lsp_parser.set_defaults(func=cmd_lsp)

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
