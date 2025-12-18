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
    from moss.plugins import get_registry
    from moss.views import ViewOptions, ViewTarget

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
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
                output.error(f"Error in {result['file']}: {result['error']}")
        else:
            if getattr(args, "json", False):
                results.append({"file": result["file"], "symbols": result["symbols"]})
            else:
                if len(files) > 1:
                    output.header(result["file"])
                content = result["content"]
                if content:
                    output.print(content)
                else:
                    output.verbose("(no symbols found)")

    if getattr(args, "json", False):
        output_result(results if len(results) > 1 else results[0] if results else {}, args)

    return 0


def cmd_anchors(args: Namespace) -> int:
    """Find anchors (functions, classes, methods) in code."""
    import re

    from moss.skeleton import extract_python_skeleton

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
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
    import re

    from moss.skeleton import extract_python_skeleton

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
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
            output.verbose(f"Syntax error in {file_path}: {e}")

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
            for file_path_str, file_results in grouped.items():
                output.header(file_path_str)
                for r in file_results:
                    ctx = f" (in {r['context']})" if r.get("context") else ""
                    output.print(f"  :{r['line']} {r['kind']} {r['name']}{ctx}")
                    if r.get("signature"):
                        output.print(f"    {r['signature']}")
        else:
            for r in results:
                ctx = f" (in {r['context']})" if r.get("context") else ""
                output.print(f"{r['file']}:{r['line']} {r['kind']} {r['name']}{ctx}")
                if r.get("signature"):
                    output.print(f"  {r['signature']}")
                if r.get("docstring"):
                    output.verbose(f"  {r['docstring'][:50]}...")

    if not results:
        output.warning("No matches found")

    return 0


def cmd_cfg(args: Namespace) -> int:
    """Build and display control flow graph."""
    from moss.plugins import get_registry
    from moss.views import ViewOptions, ViewTarget

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

    registry = get_registry()
    target = ViewTarget(path=path)
    plugin = registry.find_plugin(target, "cfg")

    if plugin is None:
        output.error("No CFG plugin available for this file type")
        return 1

    options = ViewOptions(extra={"function_name": args.function})

    async def render_cfg():
        return await plugin.render(target, options)

    view = asyncio.run(render_cfg())

    if "error" in view.metadata:
        output.error(view.metadata["error"])
        return 1

    cfgs = view.metadata.get("cfgs", [])

    if not cfgs:
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
            output.success(f"Saved to {args.output}")
        else:
            output.print(content)
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
            output.success(f"Saved to {args.output}")
        else:
            output.print(mermaid_lines)
    elif args.summary:
        # Summary mode: just show counts and complexity
        for cfg_data in cfgs:
            output.info(
                f"{cfg_data['name']}: {cfg_data['node_count']} nodes, "
                f"{cfg_data['edge_count']} edges, "
                f"complexity {cfg_data['cyclomatic_complexity']}"
            )
    elif args.dot or output_format == "dot":
        # DOT output - use raw content from view
        dot_content = view.metadata.get("dot", view.content)
        if args.output:
            Path(args.output).write_text(dot_content)
            output.success(f"Saved to {args.output}")
        else:
            output.print(dot_content)
    elif output_format == "svg":
        from moss.visualization import render_dot_to_svg

        dot_content = view.metadata.get("dot", "")
        if dot_content:
            svg = render_dot_to_svg(dot_content)
            Path(args.output).write_text(svg)
            output.success(f"Saved to {args.output}")
        else:
            output.error("No DOT content available for SVG rendering")
            return 1
    elif output_format == "png":
        from moss.visualization import render_dot_to_png

        dot_content = view.metadata.get("dot", "")
        if dot_content:
            png = render_dot_to_png(dot_content)
            Path(args.output).write_bytes(png)
            output.success(f"Saved to {args.output}")
        else:
            output.error("No DOT content available for PNG rendering")
            return 1
    else:
        output.print(view.content)

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

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
        return 1

    # Handle --dot mode: generate dependency graph visualization
    if getattr(args, "dot", False):
        if not path.is_dir():
            output.error("--dot requires a directory path")
            return 1

        pattern = args.pattern or "**/*.py"
        graph = build_dependency_graph(str(path), pattern)

        if not graph:
            output.warning("No internal dependencies found")
            return 1

        dot_output = dependency_graph_to_dot(graph, title=path.name)
        output.print(dot_output)
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
                output.info(f"Files that import '{args.reverse}':")
                for rd in reverse_deps:
                    names = f" ({', '.join(rd.names)})" if rd.names else ""
                    output.print(f"  {rd.file}:{rd.import_line} {rd.import_type}{names}")
            else:
                output.warning(f"No files found that import '{args.reverse}'")

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
            output.verbose(f"Error in {result['file']}: {result['error']}")
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
                    output.header(result["file"])
                content = result["content"]
                if content:
                    output.print(content)

    if getattr(args, "json", False):
        output_result(results if len(results) > 1 else results[0] if results else {}, args)

    return 0


def cmd_context(args: Namespace) -> int:
    """Generate compiled context for a file (skeleton + deps + summary)."""
    from moss.plugins import get_registry
    from moss.views import ViewTarget

    output = setup_output(args)
    path = Path(args.path).resolve()

    if not path.exists():
        output.error(f"Path {path} does not exist")
        return 1

    if not path.is_file():
        output.error(f"{path} must be a file")
        return 1

    registry = get_registry()
    target = ViewTarget(path=path)

    # Find plugins for skeleton and dependency
    skeleton_plugin = registry.find_plugin(target, "skeleton")
    deps_plugin = registry.find_plugin(target, "dependency")

    if skeleton_plugin is None and deps_plugin is None:
        output.error("No plugins available for this file type")
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
        output.error(skeleton_view.metadata["error"])
        return 1

    if deps_view and "error" in deps_view.metadata:
        output.error(deps_view.metadata["error"])
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
        output.header(path.name)
        output.info(f"Lines: {line_count}")
        output.info(
            f"Classes: {counts['classes']}, "
            f"Functions: {counts['functions']}, Methods: {counts['methods']}"
        )
        output.info(f"Imports: {len(imports)}, Exports: {len(exports)}")
        output.blank()

        if imports and deps_content:
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
    from moss.semantic_search import create_search_system

    out = get_output()
    directory = Path(args.directory).resolve()
    if not directory.exists():
        out.error(f"Directory {directory} does not exist")
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
                out.success(f"Indexed {count} chunks from {directory}")
                return None

        if not args.query:
            out.error("No query provided. Use --query or --index")
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
        out.warning("No results found.")
        return 0

    if getattr(args, "json", False):
        json_results = [
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
        output_result(json_results, args)
    else:
        out.success(f"Found {len(results)} results:")
        out.blank()
        for i, hit in enumerate(results, 1):
            chunk = hit.chunk
            location = f"{chunk.file_path}:{chunk.line_start}"
            name = chunk.symbol_name or chunk.file_path
            kind = chunk.symbol_kind or "file"
            score = f"{hit.score:.2f}"

            out.info(f"{i}. [{kind}] {name}")
            out.print(f"   Location: {location}")
            out.print(f"   Score: {score} ({hit.match_type})")

            # Show snippet
            if chunk.content:
                snippet = chunk.content[:200]
                if len(chunk.content) > 200:
                    snippet += "..."
                # Indent snippet
                snippet_lines = snippet.split("\n")[:3]
                for line in snippet_lines:
                    out.print(f"   | {line}")
            out.blank()

    return 0


def cmd_mcp_server(args: Namespace) -> int:
    """Start the MCP server for LLM tool access."""
    output = setup_output(args)
    try:
        from moss.mcp_server import main as mcp_main

        mcp_main()
        return 0
    except ImportError as e:
        output.error("MCP SDK not installed. Install with: pip install 'moss[mcp]'")
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
    engine = create_engine_with_builtins(
        include_builtins=include_builtins, custom_rules=custom_rules
    )

    if not engine.rules:
        output.warning("No rules configured")
        return 0

    # List rules if requested
    if getattr(args, "list", False):
        output.header("Available Rules")
        for rule in engine.rules:
            status = "[enabled]" if rule.enabled else "[disabled]"
            output.info(f"  {rule.name}: {rule.message} {status}")
        return 0

    # Run analysis
    pattern = getattr(args, "pattern", "**/*.py")
    result = engine.check_directory(directory, pattern=pattern)

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
        if v.file_path not in by_file:
            by_file[v.file_path] = []
        by_file[v.file_path].append(v)

    for file_path, violations in sorted(by_file.items()):
        try:
            rel_path = file_path.relative_to(directory)
        except ValueError:
            rel_path = file_path
        output.step(str(rel_path))

        for v in violations:
            severity_marker = {"error": "E", "warning": "W", "info": "I"}.get(v.rule.severity, "?")
            output.info(f"  {v.line}:{v.column} [{severity_marker}] {v.rule.message}")

        output.blank()

    # Summary
    errors = len(result.by_severity("error"))
    warnings = len(result.by_severity("warning"))
    infos = len(result.by_severity("info"))
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
    strategies = []
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
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    # Check if --docs mode
    if getattr(args, "docs", False):
        from moss.summarize import DocSummarizer

        output.info(f"Summarizing documentation in {root.name}...")
        summarizer = DocSummarizer()

        try:
            summary = summarizer.summarize_docs(root)
        except Exception as e:
            output.error(f"Failed to summarize docs: {e}")
            return 1

        compact = getattr(args, "compact", False)
        if compact and not wants_json(args):
            output.print(summary.to_compact())
        elif wants_json(args):
            output.data(summary.to_dict())
        else:
            output.print(summary.to_markdown())

        return 0

    # Default: summarize code
    from moss.summarize import Summarizer

    output.info(f"Summarizing {root.name}...")

    summarizer = Summarizer(
        include_private=getattr(args, "include_private", False),
        include_tests=getattr(args, "include_tests", False),
    )

    try:
        summary = summarizer.summarize_project(root)
    except Exception as e:
        output.error(f"Failed to summarize: {e}")
        return 1

    # Output format
    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(summary.to_compact())
    elif wants_json(args):
        output.data(summary.to_dict())
    else:
        output.print(summary.to_markdown())

    return 0


def cmd_check_docs(args: Namespace) -> int:
    """Check documentation freshness against codebase."""
    from moss.check_docs import DocChecker

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Checking docs in {root.name}...")

    checker = DocChecker(root, check_links=getattr(args, "check_links", False))

    try:
        result = checker.check()
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
    from moss.check_todos import TodoChecker

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Checking TODOs in {root.name}...")

    checker = TodoChecker(root)

    try:
        result = checker.check()
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
    from moss.check_refs import RefChecker

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    staleness_days = getattr(args, "staleness_days", 30)
    checker = RefChecker(root, staleness_days=staleness_days)

    output.info(f"Checking references in {root.name}...")

    try:
        result = checker.check()
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
    from moss.external_deps import ExternalDependencyAnalyzer

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    analyzer = ExternalDependencyAnalyzer(root)
    resolve = getattr(args, "resolve", False)
    warn_weight = getattr(args, "warn_weight", 0)
    check_vulns = getattr(args, "check_vulns", False)
    check_licenses = getattr(args, "check_licenses", False)

    output.info(f"Analyzing dependencies in {root.name}...")

    try:
        result = analyzer.analyze(
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


def cmd_git_hotspots(args: Namespace) -> int:
    """Find frequently changed files in git history."""
    from moss.git_hotspots import analyze_hotspots

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()
    days = getattr(args, "days", 90)

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Analyzing git history for {root.name} (last {days} days)...")

    try:
        analysis = analyze_hotspots(root, days=days)
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


def cmd_complexity(args: Namespace) -> int:
    """Analyze cyclomatic complexity of functions."""
    from moss.complexity import analyze_complexity

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()
    pattern = getattr(args, "pattern", "src/**/*.py")

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Analyzing complexity for {root.name}...")

    try:
        report = analyze_complexity(root, pattern=pattern)
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


def cmd_health(args: Namespace) -> int:
    """Show project health and what needs attention."""
    from moss.status import StatusChecker

    output = setup_output(args)
    root = Path(getattr(args, "directory", ".")).resolve()

    if not root.exists():
        output.error(f"Directory not found: {root}")
        return 1

    output.info(f"Analyzing {root.name}...")

    checker = StatusChecker(root)

    try:
        status = checker.check()
    except Exception as e:
        output.error(f"Failed to analyze project: {e}")
        return 1

    # Filter by focus area
    focus = getattr(args, "focus", "all")
    if focus != "all":
        focus_category_map = {
            "deps": ["dependencies"],
            "tests": ["tests"],
            "complexity": ["complexity"],
            "api": ["api"],
        }
        allowed = focus_category_map.get(focus, [])
        status.weak_spots = [w for w in status.weak_spots if w.category in allowed]

    # Filter by severity
    severity = getattr(args, "severity", "low")
    severity_order = {"low": 0, "medium": 1, "high": 2}
    min_severity = severity_order.get(severity, 0)
    status.weak_spots = [
        w for w in status.weak_spots if severity_order.get(w.severity, 0) >= min_severity
    ]

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
    from moss.check_docs import DocChecker
    from moss.check_refs import RefChecker
    from moss.check_todos import TodoChecker
    from moss.external_deps import ExternalDependencyAnalyzer
    from moss.presets import AVAILABLE_CHECKS, get_preset, list_presets
    from moss.status import StatusChecker
    from moss.summarize import Summarizer

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
            checker = StatusChecker(root)
            status = checker.check()
            # Extract top issues by severity for display
            high_issues = [w for w in status.weak_spots if w.severity == "high"]
            med_issues = [w for w in status.weak_spots if w.severity == "medium"]
            # Get top packages for skeleton summary
            try:
                summarizer = Summarizer(include_private=False, include_tests=False)
                project_summary = summarizer.summarize_project(root)
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
            analyzer = ExternalDependencyAnalyzer(root)
            deps = analyzer.analyze()
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
            checker = DocChecker(root)
            docs = checker.check()
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
            checker = TodoChecker(root)
            todos = checker.check()
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
            checker = RefChecker(root)
            refs = checker.check()
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

    # shell command
    shell_parser = subparsers.add_parser("shell", help="Interactive shell for code exploration")
    shell_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Workspace directory (default: current)",
    )
    shell_parser.set_defaults(func=cmd_shell)

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
        "summarize", help="Generate hierarchical codebase summary"
    )
    summarize_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Directory to summarize (default: current)",
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
