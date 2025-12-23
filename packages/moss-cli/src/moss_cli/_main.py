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

    from moss.session_analysis import SessionAnalysis


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

    # Determine compact mode
    # Explicit --compact always wins, otherwise default to compact when not a TTY
    compact = getattr(args, "compact", False)
    json_format = getattr(args, "json", False)
    if not compact and not json_format:
        compact = not sys.stdout.isatty()

    # Configure output
    output = configure_output(
        verbosity=verbosity,
        json_format=json_format,
        compact=compact,
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
    from moss.config import load_config_file
    from moss.events import EventBus
    from moss.shadow_git import ShadowGit

    output = setup_output(args)
    project_dir = Path(args.directory).resolve()
    config_file = project_dir / "moss_config.py"

    # Load config
    if config_file.exists():
        try:
            load_config_file(config_file)
        except Exception as e:
            output.error(f"Error loading config: {e}")
            return 1

    # Set up components
    event_bus = EventBus()

    # Listen for tool calls to show metrics
    def on_tool_call(event: Any) -> None:
        tool = event.payload.get("tool_name", "unknown")
        success = event.payload.get("success", True)
        duration = event.payload.get("duration_ms", 0)
        mem = event.payload.get("memory_bytes", 0) / 1024 / 1024
        ctx = event.payload.get("context_tokens", 0)
        breakdown = event.payload.get("memory_breakdown", {})

        # Format breakdown
        bd_str = ""
        if breakdown:
            sorted_bd = sorted(breakdown.items(), key=lambda x: x[1], reverse=True)
            bd_parts = [f"{k}={v / 1024 / 1024:.1f}MB" for k, v in sorted_bd[:2]]
            bd_str = f" [{', '.join(bd_parts)}]"

        status = "✓" if success else "✗"
        output.info(
            f"  {status} {tool} ({duration}ms) | RAM: {mem:.1f} MB{bd_str} | Context: {ctx} tokens"
        )

    from moss.events import EventType

    event_bus.subscribe(EventType.TOOL_CALL, on_tool_call)

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
# Codebase Tree Commands - delegated to Rust via passthrough in main()
# Python implementations removed, see git history for reference.
# =============================================================================


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
    from moss_intelligence.rust_shim import rust_available, rust_context

    from moss import MossAPI

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
    """Start the interactive terminal UI (API explorer)."""
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


def cmd_explore(args: Namespace) -> int:
    """Start the explore TUI (tree + view/edit/analyze primitives)."""
    output = setup_output(args)
    try:
        from moss.moss_api import MossAPI
        from moss.tui import run_tui

        directory = Path(getattr(args, "directory", ".")).resolve()
        api = MossAPI(directory)
        run_tui(api)
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
        incremental=getattr(args, "incremental", False),
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
    from moss_intelligence.edit import EditContext, TaskComplexity, analyze_complexity, edit

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
            from moss_intelligence.edit import structural_edit

            return await structural_edit(task, context)
        elif force_method == "synthesis":
            from moss_intelligence.edit import synthesize_edit

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
    from moss.synthesis import CodeGenerator

    generator: CodeGenerator | None = None
    generator_name = getattr(args, "generator", "auto")

    if generator_name != "auto":
        from moss.synthesis.generators import (
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
    # --plain/--compact explicitly sets plain text (good for LLMs)
    # --tui explicitly sets TUI
    # Default: TUI if stdout is a TTY, plain otherwise
    use_tui = getattr(args, "tui", False)
    use_plain = getattr(args, "plain", False)
    use_compact = getattr(args, "compact", False)

    if use_plain or use_compact:
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


def cmd_telemetry(args: Namespace) -> int:
    """Show aggregate telemetry across sessions."""
    from moss.moss_api import MossAPI
    from moss.session_analysis import analyze_session

    output = setup_output(args)
    log_paths = [Path(p) for p in getattr(args, "logs", []) or []]
    session_id = getattr(args, "session", None)
    html_output = getattr(args, "html", False)
    watch_mode = getattr(args, "watch", False)

    # Mode 1: Analyze specific moss session
    if session_id:
        api = MossAPI.for_project(Path.cwd())
        stats = api.telemetry.get_session_stats(session_id)
        if "error" in stats:
            output.error(stats["error"])
            return 1
        if wants_json(args):
            output.data(stats)
        else:
            output.print(_format_session_stats(stats))
        return 0

    # Mode 2: Analyze external Claude Code session logs
    if log_paths:
        for path in log_paths:
            if not path.exists():
                output.error(f"Log file not found: {path}")
                return 1

        if watch_mode:
            return _telemetry_watch_loop(log_paths, output, html_output, args)

        analyses = [analyze_session(p) for p in log_paths]

        if len(analyses) == 1:
            analysis = analyses[0]
        else:
            analysis = _aggregate_analyses(analyses)

        if html_output:
            html = _generate_telemetry_html(analysis)
            output.print(html)
        elif wants_json(args):
            output.data(analysis.to_dict())
        elif getattr(args, "compact", False):
            output.print(analysis.to_compact())
        else:
            output.print(analysis.to_markdown())
        return 0

    # Mode 3: Default - aggregate stats across all moss sessions
    api = MossAPI.for_project(Path.cwd())
    stats = api.telemetry.analyze_all_sessions()

    if html_output:
        html = _generate_aggregate_html(stats)
        output.print(html)
    elif wants_json(args):
        output.data(stats)
    else:
        output.print(_format_aggregate_stats(stats))

    return 0


def _telemetry_watch_loop(
    log_paths: list[Path], output: Any, html_output: bool, args: Namespace
) -> int:
    """Run continuous telemetry watch loop."""
    import sys
    import time

    from moss.session_analysis import analyze_session

    last_mtimes: dict[Path, float] = {}
    last_output = ""
    refresh_interval = 2.0  # seconds

    output.print(f"Watching {len(log_paths)} log file(s)... (Ctrl+C to stop)")

    try:
        while True:
            # Check if any files changed
            changed = False
            for path in log_paths:
                try:
                    mtime = path.stat().st_mtime
                    if path not in last_mtimes or mtime > last_mtimes[path]:
                        last_mtimes[path] = mtime
                        changed = True
                except OSError:
                    pass

            if changed or not last_output:
                # Re-analyze
                analyses = [analyze_session(p) for p in log_paths]
                if len(analyses) == 1:
                    analysis = analyses[0]
                else:
                    analysis = _aggregate_analyses(analyses)

                # Format output
                if html_output:
                    new_output = _generate_telemetry_html(analysis)
                elif getattr(args, "compact", False):
                    new_output = analysis.to_compact()
                else:
                    new_output = _format_watch_output(analysis)

                # Only redraw if output changed
                if new_output != last_output:
                    last_output = new_output
                    # Clear screen and move cursor to top
                    sys.stdout.write("\033[2J\033[H")
                    sys.stdout.write(new_output)
                    sys.stdout.write(
                        f"\n\n[Last updated: {time.strftime('%H:%M:%S')}] "
                        f"Watching... (Ctrl+C to stop)\n"
                    )
                    sys.stdout.flush()

            time.sleep(refresh_interval)
    except KeyboardInterrupt:
        output.print("\nWatch stopped.")
        return 0


def _format_watch_output(analysis: Any) -> str:
    """Format telemetry for watch mode (compact live view)."""
    lines = []
    lines.append("TELEMETRY WATCH")
    lines.append("=" * 50)
    lines.append("")

    # Summary stats
    lines.append(f"Tool calls: {analysis.total_tool_calls}")
    lines.append(f"Success rate: {analysis.overall_success_rate:.1%}")
    lines.append(f"Turns: {analysis.total_turns}")
    lines.append("")

    # Token stats
    if analysis.token_stats.api_calls:
        ts = analysis.token_stats
        lines.append("TOKENS")
        lines.append(f"  API calls: {ts.api_calls}")
        lines.append(f"  Avg context: {ts.avg_context:,}")
        lines.append(f"  Output: {ts.total_output:,}")
        if ts.cache_read:
            lines.append(f"  Cache read: {ts.cache_read:,}")
        lines.append("")

    # Top tools (compact)
    if analysis.tool_stats:
        lines.append("TOP TOOLS")
        sorted_tools = sorted(analysis.tool_stats.values(), key=lambda t: t.calls, reverse=True)[:5]
        for tool in sorted_tools:
            lines.append(f"  {tool.name}: {tool.calls}")
        lines.append("")

    # File hotspots
    if analysis.file_tokens:
        lines.append("FILE HOTSPOTS (by tokens)")
        sorted_files = sorted(analysis.file_tokens.items(), key=lambda x: -x[1])[:5]
        for path, tokens in sorted_files:
            # Truncate long paths
            display_path = path if len(path) <= 40 else "..." + path[-37:]
            lines.append(f"  {display_path}: {tokens:,}")
        lines.append("")

    return "\n".join(lines)


def _format_session_stats(stats: dict) -> str:
    """Format session stats for display."""
    lines = [
        f"Session: {stats.get('id', 'unknown')}",
        f"Task: {stats.get('task', 'N/A')}",
        f"Tokens: {stats.get('tokens', 0):,}",
        f"LLM Calls: {stats.get('llm_calls', 0)}",
        f"Tool Calls: {stats.get('tool_calls', 0)}",
        f"File Changes: {stats.get('file_changes', 0)}",
        f"Duration: {stats.get('duration', 0):.1f}s",
    ]

    if stats.get("access_patterns"):
        lines.append("\nTop Accessed Files:")
        for path, count in sorted(stats["access_patterns"].items(), key=lambda x: -x[1])[:5]:
            lines.append(f"  {path}: {count}")

    return "\n".join(lines)


def _format_aggregate_stats(stats: dict) -> str:
    """Format aggregate stats for display."""
    lines = [
        "Aggregate Telemetry",
        "=" * 40,
        f"Sessions: {stats.get('session_count', 0)}",
        f"Total Tokens: {stats.get('total_tokens', 0):,}",
        f"Total LLM Calls: {stats.get('total_llm_calls', 0)}",
        f"Max Memory: {stats.get('max_memory_bytes', 0) / 1024 / 1024:.1f} MB",
        f"Max Context: {stats.get('max_context_tokens', 0):,} tokens",
    ]

    hotspots = stats.get("hotspots", [])
    if hotspots:
        lines.append("\nFile Hotspots:")
        for path, count in hotspots[:10]:
            lines.append(f"  {path}: {count} accesses")

    return "\n".join(lines)


def _aggregate_analyses(analyses: list) -> SessionAnalysis:
    """Aggregate multiple SessionAnalysis objects."""
    from moss.session_analysis import SessionAnalysis, TokenStats, ToolStats

    if not analyses:
        return SessionAnalysis(session_path=Path("."))

    # Combine tool stats
    combined_tools: dict[str, ToolStats] = {}
    for analysis in analyses:
        for name, stats in analysis.tool_stats.items():
            if name not in combined_tools:
                combined_tools[name] = ToolStats(name=name)
            combined_tools[name].calls += stats.calls
            combined_tools[name].errors += stats.errors

    # Combine token stats
    combined_tokens = TokenStats()
    for analysis in analyses:
        ts = analysis.token_stats
        combined_tokens.total_input += ts.total_input
        combined_tokens.total_output += ts.total_output
        combined_tokens.cache_read += ts.cache_read
        combined_tokens.cache_create += ts.cache_create
        combined_tokens.api_calls += ts.api_calls
        if ts.max_context > combined_tokens.max_context:
            combined_tokens.max_context = ts.max_context
        if combined_tokens.min_context == 0 or (
            ts.min_context > 0 and ts.min_context < combined_tokens.min_context
        ):
            combined_tokens.min_context = ts.min_context

    # Combine message counts
    combined_messages: dict[str, int] = {}
    for analysis in analyses:
        for msg_type, count in analysis.message_counts.items():
            combined_messages[msg_type] = combined_messages.get(msg_type, 0) + count

    # Combine file tokens
    combined_file_tokens: dict[str, int] = {}
    for analysis in analyses:
        for path, tokens in analysis.file_tokens.items():
            combined_file_tokens[path] = combined_file_tokens.get(path, 0) + tokens

    result = SessionAnalysis(
        session_path=Path(f"<{len(analyses)} sessions>"),
        tool_stats=combined_tools,
        token_stats=combined_tokens,
        message_counts=combined_messages,
        file_tokens=combined_file_tokens,
        total_turns=sum(a.total_turns for a in analyses),
        parallel_opportunities=sum(a.parallel_opportunities for a in analyses),
    )

    return result


def _telemetry_css() -> str:
    """Shared CSS for telemetry HTML dashboards."""
    return """
        body { font-family: system-ui, sans-serif; margin: 2rem; background: #f5f5f5; }
        .card { background: white; border-radius: 8px; padding: 1.5rem; margin-bottom: 1rem; }
        h1 { color: #333; }
        h2 { color: #666; margin-top: 0; }
        table { width: 100%; border-collapse: collapse; }
        th, td { text-align: left; padding: 0.5rem; border-bottom: 1px solid #eee; }
        th { background: #f8f8f8; }
        .metric { font-size: 2rem; font-weight: bold; color: #2563eb; }
        .label { color: #666; font-size: 0.9rem; }
        .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); }
    """


def _generate_telemetry_html(analysis: SessionAnalysis) -> str:
    """Generate HTML dashboard for session analysis."""
    tool_rows = ""
    for tool in sorted(analysis.tool_stats.values(), key=lambda t: -t.calls):
        success_pct = f"{tool.success_rate:.0%}"
        row = f"<tr><td>{tool.name}</td><td>{tool.calls}</td>"
        row += f"<td>{tool.errors}</td><td>{success_pct}</td></tr>\n"
        tool_rows += row

    ts = analysis.token_stats
    css = _telemetry_css()
    return f"""<!DOCTYPE html>
<html>
<head>
    <title>Session Telemetry</title>
    <style>{css}</style>
</head>
<body>
    <h1>Session Telemetry</h1>
    <div class="grid">
        <div class="card">
            <div class="metric">{analysis.total_tool_calls}</div>
            <div class="label">Tool Calls</div>
        </div>
        <div class="card">
            <div class="metric">{analysis.overall_success_rate:.0%}</div>
            <div class="label">Success Rate</div>
        </div>
        <div class="card">
            <div class="metric">{ts.api_calls}</div>
            <div class="label">API Calls</div>
        </div>
        <div class="card">
            <div class="metric">{ts.avg_context // 1000}K</div>
            <div class="label">Avg Context</div>
        </div>
    </div>
    <div class="card">
        <h2>Tool Usage</h2>
        <table>
            <tr><th>Tool</th><th>Calls</th><th>Errors</th><th>Success Rate</th></tr>
            {tool_rows}
        </table>
    </div>
</body>
</html>"""


def _generate_aggregate_html(stats: dict) -> str:
    """Generate HTML dashboard for aggregate stats."""
    hotspot_rows = ""
    for path, count in stats.get("hotspots", [])[:15]:
        hotspot_rows += f"<tr><td>{path}</td><td>{count}</td></tr>\n"

    css = _telemetry_css()
    return f"""<!DOCTYPE html>
<html>
<head>
    <title>Aggregate Telemetry</title>
    <style>{css}</style>
</head>
<body>
    <h1>Aggregate Telemetry</h1>
    <div class="grid">
        <div class="card">
            <div class="metric">{stats.get("session_count", 0)}</div>
            <div class="label">Sessions</div>
        </div>
        <div class="card">
            <div class="metric">{stats.get("total_tokens", 0) // 1000}K</div>
            <div class="label">Total Tokens</div>
        </div>
        <div class="card">
            <div class="metric">{stats.get("total_llm_calls", 0)}</div>
            <div class="label">LLM Calls</div>
        </div>
        <div class="card">
            <div class="metric">{stats.get("max_context_tokens", 0) // 1000}K</div>
            <div class="label">Max Context</div>
        </div>
    </div>
    <div class="card">
        <h2>File Hotspots</h2>
        <table>
            <tr><th>File</th><th>Accesses</th></tr>
            {hotspot_rows}
        </table>
    </div>
</body>
</html>"""


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
    from moss_intelligence.test_coverage import analyze_coverage

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
    from moss_intelligence.clones import format_clone_analysis

    from moss import MossAPI

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
        output.print(format_clone_analysis(analysis, show_source=show_source, root=root))

    return 0


def cmd_security(args: Namespace) -> int:
    """Run security analysis with multiple tools."""
    from moss_intelligence.security import format_security_analysis

    from moss import MossAPI

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
    from moss_intelligence.patterns import analyze_patterns, format_pattern_analysis

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

    compact = getattr(args, "compact", False)
    if compact and not wants_json(args):
        output.print(analysis.to_compact())
    elif wants_json(args):
        output.data(analysis.to_dict())
    else:
        output.print(format_pattern_analysis(analysis))

    return 0


def cmd_weaknesses(args: Namespace) -> int:
    """Identify architectural weaknesses and gaps in the codebase."""
    from moss_intelligence.weaknesses import (
        format_weakness_fixes,
        get_fixable_weaknesses,
        weaknesses_to_sarif,
    )

    from moss import MossAPI

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


def cmd_workflow(args: Namespace) -> int:
    """Manage and run TOML-based workflows.

    Subcommands:
    - list: Show available workflows
    - show: Show workflow details
    - run: Execute a workflow on a file
    - generate: Auto-create workflows based on project
    - new: Scaffold a new workflow from template
    """
    import tomllib

    output = setup_output(args)
    action = getattr(args, "action", "list")
    project_root = Path(getattr(args, "directory", ".")).resolve()

    # Workflow directories
    builtin_dir = Path(__file__).parent.parent / "workflows"
    user_dir = project_root / ".moss" / "workflows"

    def find_workflow(name: str) -> Path | None:
        """Find workflow TOML by name."""
        for d in [user_dir, builtin_dir]:
            p = d / f"{name}.toml"
            if p.exists():
                return p
        return None

    if action == "list":
        output.header("Available Workflows")
        workflows: set[str] = set()
        for d in [builtin_dir, user_dir]:
            if d.exists():
                for p in d.glob("*.toml"):
                    workflows.add(p.stem)
        if not workflows:
            output.print("  (none found)")
        for name in sorted(workflows):
            try:
                path = find_workflow(name)
                if path:
                    with path.open("rb") as f:
                        data = tomllib.load(f)
                    desc = data.get("workflow", {}).get("description", "(no description)")
                    output.print(f"  {name}: {desc}")
            except Exception as e:
                output.print(f"  {name}: (error loading: {e})")
        return 0

    elif action == "new":
        from moss.workflows.templates import TEMPLATES

        name = getattr(args, "workflow_name", None)
        if not name:
            # Interactive prompt if no name provided
            if sys.stdin.isatty():
                try:
                    name = input("Enter workflow name (e.g., validate-fix): ").strip()
                except (KeyboardInterrupt, EOFError):
                    print()
                    return 1

            if not name:
                output.error("Workflow name required")
                return 1

        template_name = getattr(args, "template", "agentic")
        template_content = TEMPLATES.get(template_name)
        if not template_content:
            output.error(f"Unknown template: {template_name}")
            return 1

        workflow_dir = project_root / ".moss" / "workflows"
        workflow_dir.mkdir(parents=True, exist_ok=True)

        file_path = workflow_dir / f"{name}.toml"
        if file_path.exists() and not getattr(args, "force", False):
            output.error(f"Workflow '{name}' already exists at {file_path}")
            output.info("Use --force to overwrite")
            return 1

        content = template_content.format(name=name)
        file_path.write_text(content)
        output.success(f"Created workflow '{name}' at {file_path}")

        output.blank()
        output.step("Next steps:")
        output.info(f"  1. Edit {file_path}")
        output.info(f"  2. Run with: moss workflow run {name} --file <file>")
        return 0

    elif action == "show":
        name = getattr(args, "workflow_name", None)
        if not name:
            output.error("Workflow name required")
            return 1

        workflow_path = find_workflow(name)
        if not workflow_path:
            output.error(f"Workflow not found: {name}")
            return 1

        try:
            with workflow_path.open("rb") as f:
                data = tomllib.load(f)
        except Exception as e:
            output.error(f"Failed to load workflow: {e}")
            return 1

        wf = data.get("workflow", {})

        if wants_json(args):
            output.data(data)
        else:
            output.header(f"Workflow: {wf.get('name', name)}")
            output.print(f"Description: {wf.get('description', '(none)')}")
            output.print(f"Version: {wf.get('version', '1.0')}")

            # Limits
            limits = wf.get("limits", {})
            output.print(f"Max turns: {limits.get('max_turns', 20)}")
            if timeout := limits.get("timeout_seconds"):
                output.print(f"Timeout: {timeout}s")

            # LLM config
            if llm := wf.get("llm"):
                output.print(f"LLM strategy: {llm.get('strategy', 'simple')}")
                if model := llm.get("model"):
                    output.print(f"Model: {model}")

            # Steps (for step-based workflows)
            if steps := data.get("steps"):
                output.print("")
                output.header("Steps")
                for i, step in enumerate(steps, 1):
                    step_name = step.get("name", "unnamed")
                    step_action = step.get("action", "?")
                    output.print(f"  {i}. {step_name} ({step_action})")
        return 0

    elif action == "run":
        name = getattr(args, "workflow_name", None)
        mock = getattr(args, "mock", False)
        verbose = getattr(args, "verbose", False)
        workflow_args = getattr(args, "workflow_args", None) or []

        if not name:
            output.error("Workflow name required")
            return 1

        # Find workflow TOML file
        # Check: .moss/workflows/{name}.toml, then src/moss/workflows/{name}.toml
        workflow_path = None
        search_paths = [
            project_root / ".moss" / "workflows" / f"{name}.toml",
            Path(__file__).parent.parent / "workflows" / f"{name}.toml",
        ]
        for p in search_paths:
            if p.exists():
                workflow_path = p
                break

        if not workflow_path:
            output.error(f"Workflow not found: {name}")
            output.info(f"Searched: {', '.join(str(p) for p in search_paths)}")
            return 1

        # Parse arguments
        extra_args: dict[str, str] = {}
        for arg in workflow_args:
            if "=" not in arg:
                output.error(f"Invalid argument format: {arg} (expected KEY=VALUE)")
                return 1
            key, value = arg.split("=", 1)
            extra_args[key] = value

        # Load and run using execution primitives
        from moss.execution import (
            NoLLM,
            agent_loop,
            step_loop,
        )
        from moss.execution import (
            load_workflow as load_exec_workflow,
        )

        config = load_exec_workflow(str(workflow_path))
        if mock and config.llm:
            config.llm = NoLLM(actions=["view README.md", "done"])

        # Get task from args (for agentic workflows)
        task = extra_args.pop("task", extra_args.pop("instruction", ""))

        output.info(f"Running workflow '{name}'" + (f": {task}" if task else ""))
        if verbose:
            output.info(f"Path: {workflow_path}")
            if config.context:
                output.info(f"Context: {type(config.context).__name__}")
            if config.llm:
                output.info(f"LLM: {type(config.llm).__name__}")
        output.info("")

        try:
            # Run directly with loaded config (supports mock)
            if config.steps:
                result = step_loop(
                    steps=config.steps,
                    context=config.context,
                    cache=config.cache,
                    retry=config.retry,
                    initial_context=extra_args if extra_args else None,
                )
            else:
                result = agent_loop(
                    task=task,
                    context=config.context,
                    cache=config.cache,
                    retry=config.retry,
                    llm=config.llm,
                    max_turns=config.max_turns,
                )
            output.success("\nCompleted!")
            if verbose:
                output.info(f"Final context:\n{result}")
            return 0
        except Exception as e:
            output.error(f"Workflow failed: {e}")
            if verbose:
                import traceback

                output.info(traceback.format_exc())
            return 1

    else:
        output.error(f"Unknown action: {action}")
        return 1


def cmd_toml(args: Namespace) -> int:
    """Navigate TOML files with jq-like queries.

    Examples:
        moss toml pyproject.toml                    # Show summary
        moss toml pyproject.toml .project.name      # Get specific value
        moss toml Cargo.toml ".dependencies | keys" # List dependency names
        moss toml moss.toml --keys                  # List all keys
    """
    from moss.toml_nav import (
        format_result,
        list_keys,
        parse_toml,
        query,
        summarize_toml,
    )

    output = setup_output(args)
    file_path = Path(args.file).resolve()
    query_path = getattr(args, "query", None) or "."
    show_keys = getattr(args, "keys", False)
    show_summary = getattr(args, "summary", False)

    if not file_path.exists():
        output.error(f"File not found: {file_path}")
        return 1

    try:
        data = parse_toml(file_path)
    except Exception as e:
        output.error(f"Failed to parse TOML: {e}")
        return 1

    # Show keys mode
    if show_keys:
        keys = list_keys(data)
        if wants_json(args):
            output.data(keys)
        else:
            for key in keys:
                output.print(key)
        return 0

    # Summary mode (default when no query)
    if show_summary or query_path == ".":
        summary = summarize_toml(data)
        if wants_json(args):
            output.data(summary)
        else:
            output.header(f"TOML Summary: {file_path.name}")
            output.print(f"Sections: {', '.join(summary['sections'])}")
            output.print(f"Total keys: {summary['key_count']}")
            output.print(f"Max depth: {summary['nested_depth']}")
            types_str = ", ".join(f"{k}: {v}" for k, v in summary["types"].items())
            output.print(f"Types: {types_str}")
        return 0

    # Query mode
    try:
        result = query(data, query_path)
    except (KeyError, TypeError, IndexError, ValueError) as e:
        output.error(f"Query error: {e}")
        return 1

    if wants_json(args):
        output.data(result)
    else:
        output.print(format_result(result))

    return 0


def cmd_agent(args: Namespace) -> int:
    """Run DWIM agent on a task. Alias for 'moss workflow run dwim'.

    Uses composable execution primitives with task tree context.
    """
    from moss.execution import (
        NoLLM,
        agent_loop,
        load_workflow,
    )

    output = setup_output(args)
    task = getattr(args, "task", None)
    verbose = getattr(args, "verbose", False)
    mock = getattr(args, "mock", False)

    if not task:
        output.error("Usage: moss agent <task>")
        output.info('Example: moss agent "Fix the type error in Patch.apply"')
        return 1

    # Load dwim workflow
    dwim_toml = Path(__file__).parent.parent / "workflows" / "dwim.toml"
    if not dwim_toml.exists():
        output.error("dwim.toml workflow not found")
        return 1

    config = load_workflow(str(dwim_toml))
    if mock and config.llm:
        config.llm = NoLLM(actions=["view README.md", "done"])

    output.info(f"Starting agent: {task}")
    if verbose:
        output.info(f"Context: {type(config.context).__name__}")
        output.info(f"LLM: {type(config.llm).__name__}")
        output.info(f"Max turns: {config.max_turns}")
    output.info("")

    try:
        result = agent_loop(
            task=task,
            context=config.context,
            cache=config.cache,
            retry=config.retry,
            llm=config.llm,
            max_turns=config.max_turns,
        )
        output.success("\nCompleted!")
        if verbose:
            output.info(f"Final context:\n{result}")
        return 0
    except Exception as e:
        output.error(f"Agent failed: {e}")
        if verbose:
            import traceback

            output.info(traceback.format_exc())
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
    # Passthrough commands (tree, path, view, search-tree, expand, callers,
    # callees, skeleton, anchors) are handled directly in main() before
    # argparse. See RUST_PASSTHROUGH in main() and use `moss <cmd> --help`
    # for their usage.
    # ==========================================================================

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

    # tui command (API explorer - technical)
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

    # explore command - tree + primitives TUI
    explore_parser = subparsers.add_parser(
        "explore", help="Explore codebase with tree navigation + view/edit/analyze"
    )
    explore_parser.add_argument(
        "directory",
        nargs="?",
        default=".",
        help="Project root directory (default: current)",
    )
    explore_parser.set_defaults(func=cmd_explore)

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
    watch_parser.add_argument(
        "--incremental",
        "-i",
        action="store_true",
        help="Only run tests related to changed files",
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
    roadmap_parser.add_argument(
        "--compact",
        action="store_true",
        help="Compact output (same as --plain)",
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

    # telemetry command
    telemetry_parser = subparsers.add_parser(
        "telemetry",
        help="Show aggregate telemetry across sessions",
    )
    telemetry_parser.add_argument(
        "--session",
        "-s",
        help="Show stats for a specific moss session ID",
    )
    telemetry_parser.add_argument(
        "--logs",
        "-l",
        nargs="+",
        help="Analyze Claude Code session logs (supports multiple)",
    )
    telemetry_parser.add_argument(
        "--html",
        action="store_true",
        help="Output as HTML dashboard",
    )
    telemetry_parser.add_argument(
        "--watch",
        "-w",
        action="store_true",
        help="Watch mode - continuously update telemetry as logs change",
    )
    telemetry_parser.set_defaults(func=cmd_telemetry)

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

    # workflow command
    workflow_parser = subparsers.add_parser("workflow", help="Manage and run TOML-based workflows")
    workflow_parser.add_argument(
        "action",
        nargs="?",
        default="list",
        choices=["list", "show", "run", "new"],
        help="Action: list, show, run, new (scaffold)",
    )
    workflow_parser.add_argument(
        "workflow_name",
        nargs="?",
        help="Workflow name (e.g., validate-fix)",
    )
    workflow_parser.add_argument(
        "--file",
        "-f",
        help="File to process (optional, defaults to codebase root)",
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
    workflow_parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Show LLM outputs and step details",
    )
    workflow_parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite existing workflows",
    )
    workflow_parser.add_argument(
        "--template",
        "-t",
        default="agentic",
        choices=["agentic", "step"],
        help="Template for new workflow (default: agentic)",
    )
    workflow_parser.add_argument(
        "--arg",
        "-a",
        action="append",
        dest="workflow_args",
        metavar="KEY=VALUE",
        help="Pass argument to workflow (repeatable, e.g., --arg model=gpt-4)",
    )
    workflow_parser.set_defaults(func=cmd_workflow)

    # toml navigation command
    toml_parser = subparsers.add_parser("toml", help="Navigate TOML files with jq-like queries")
    toml_parser.add_argument(
        "file",
        help="TOML file to parse (e.g., pyproject.toml, Cargo.toml)",
    )
    toml_parser.add_argument(
        "query",
        nargs="?",
        default=".",
        help="jq-like query path (e.g., .project.name, '.dependencies | keys')",
    )
    toml_parser.add_argument(
        "--keys",
        "-k",
        action="store_true",
        help="List all key paths in the file",
    )
    toml_parser.add_argument(
        "--summary",
        "-s",
        action="store_true",
        help="Show file structure summary",
    )
    toml_parser.set_defaults(func=cmd_toml)

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

    # agent command - DWIM-driven agent loop
    agent_parser = subparsers.add_parser(
        "agent", help="Run DWIM agent (alias for 'workflow run dwim')"
    )
    agent_parser.add_argument(
        "task",
        nargs="?",
        help="Task description in natural language",
    )
    agent_parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Show detailed output",
    )
    agent_parser.add_argument(
        "--mock",
        action="store_true",
        help="Use mock LLM responses (for testing)",
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


def _cmd_analyze_python(argv: list[str]) -> int:
    """Handle Python-only analyze flags (--summary, --check-docs, --check-todos)."""
    import argparse
    import json

    from moss import MossAPI

    parser = argparse.ArgumentParser(prog="moss analyze")
    parser.add_argument("target", nargs="?", default=".", help="Target path")
    parser.add_argument("--summary", action="store_true", help="Generate summary")
    parser.add_argument("--check-docs", action="store_true", help="Check documentation")
    parser.add_argument("--check-todos", action="store_true", help="Check TODOs")
    parser.add_argument("--json", action="store_true", help="JSON output")
    parser.add_argument("--compact", action="store_true", help="Compact output")
    parser.add_argument("--strict", action="store_true", help="Strict mode (exit 1 on warnings)")
    parser.add_argument("--check-links", action="store_true", help="Check doc links")
    parser.add_argument("--limit", type=int, default=10, help="Max items per section (default: 10)")
    parser.add_argument("--all", action="store_true", help="Show all items (override --limit)")
    parser.add_argument("--changed", action="store_true", help="Only check git-modified files")
    args = parser.parse_args(argv)

    root = Path(args.target).resolve()
    if not root.exists():
        print(f"Error: Path not found: {root}", file=sys.stderr)
        return 1

    api = MossAPI.for_project(root)

    if args.summary:
        from moss_intelligence.summarize import Summarizer

        summarizer = Summarizer()
        if root.is_file():
            result = summarizer.summarize_file(root)
        else:
            result = summarizer.summarize_project(root)

        if result is None:
            print(f"Error: Failed to summarize {root}", file=sys.stderr)
            return 1

        if args.json:
            print(json.dumps(result.to_dict(), indent=2))
        elif args.compact:
            print(result.to_compact())
        else:
            print(result.to_markdown())
        return 0

    # Determine limit: None if --all, otherwise use --limit value
    limit = None if getattr(args, "all", False) else args.limit

    # Get git-changed files if --changed flag is set
    changed_files: set[str] | None = None
    if args.changed:
        import subprocess

        try:
            # Get modified files (staged + unstaged + untracked)
            result_git = subprocess.run(
                ["git", "status", "--porcelain"],
                capture_output=True,
                text=True,
                cwd=root,
            )
            if result_git.returncode == 0:
                changed_files = set()
                for line in result_git.stdout.splitlines():
                    if len(line) > 3:
                        # Extract path from git status output (format: "XY filename")
                        path = line[3:].strip()
                        # Handle renamed files
                        if " -> " in path:
                            path = path.split(" -> ")[1]
                        changed_files.add(str(root / path))
        except (subprocess.SubprocessError, OSError):
            pass

    if args.check_docs:
        result = api.health.check_docs(check_links=args.check_links)
        # Filter to changed files if requested
        if changed_files is not None:
            result.issues = [i for i in result.issues if i.file and str(i.file) in changed_files]
        if args.json:
            print(json.dumps(result.to_dict(), indent=2))
        elif args.compact:
            print(result.to_compact())
        else:
            print(result.to_markdown(limit=limit))
        if result.has_errors:
            return 1
        if args.strict and result.has_warnings:
            return 1
        return 0

    if args.check_todos:
        result = api.health.check_todos()
        # Filter to changed files if requested
        if changed_files is not None:
            result.code_todos = [
                t for t in result.code_todos if str(root / t.source) in changed_files
            ]
        if args.json:
            print(json.dumps(result.to_dict(), indent=2))
        elif args.compact:
            print(result.to_compact())
        else:
            print(result.to_markdown(limit=limit))
        if args.strict and result.orphan_count > 0:
            return 1
        return 0

    # Fallback to Rust for other flags
    from moss_intelligence.rust_shim import passthrough

    return passthrough("analyze", argv)


def main(argv: list[str] | None = None) -> int:
    """Main entry point."""
    if argv is None:
        argv = sys.argv[1:]

    # Commands that delegate entirely to Rust CLI (no Python parsing needed)
    # Core primitives: view, edit (structural), analyze
    # Note: Python edit kept for LLM-based intelligent editing
    RUST_PASSTHROUGH = {
        # Core primitives
        "view",
        "analyze",
        # Legacy commands (deprecated, use view/analyze instead)
        "anchors",
        "callers",
        "callees",
        "cfg",
        "complexity",
        "context",
        "deps",
        "expand",
        "grep",
        "overview",
        "path",
        "search-tree",
        "skeleton",
        "tree",
    }

    # Python-only analyze flags (intercept before Rust passthrough)
    PYTHON_ANALYZE_FLAGS = {"--summary", "--check-docs", "--check-todos"}

    # Check for passthrough before argparse to avoid double-parsing
    if argv and argv[0] in RUST_PASSTHROUGH:
        # Intercept analyze with Python-only flags
        if argv[0] == "analyze" and any(f in argv for f in PYTHON_ANALYZE_FLAGS):
            return _cmd_analyze_python(argv[1:])

        from moss_intelligence.rust_shim import passthrough

        return passthrough(argv[0], argv[1:])

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
