"""Interactive shell for exploring codebases.

This module provides a REPL interface for exploring code with moss tools.

Usage:
    moss shell [directory]

Commands in the shell:
    skeleton <path>      - Extract code skeleton
    deps <path>          - Show dependencies
    cfg <path> [func]    - Show control flow graph
    query <pattern>      - Search for symbols
    search <query>       - Semantic search
    cd <path>            - Change directory
    ls [path]            - List files
    help                 - Show help
    exit                 - Exit the shell
"""

from __future__ import annotations

import asyncio
import os
import readline  # enables line editing - side effect import
import shlex
from pathlib import Path
from typing import TYPE_CHECKING

from moss.output import Output, Verbosity

if TYPE_CHECKING:
    pass


class MossShell:
    """Interactive shell for code exploration."""

    def __init__(self, workspace: Path | None = None) -> None:
        self.workspace = Path(workspace or ".").resolve()
        self.output = Output(verbosity=Verbosity.NORMAL)
        self.output.style.use_colors = True
        self.running = True
        self.history_file = Path.home() / ".moss_history"

        # Commands registry
        self.commands = {
            "help": self.cmd_help,
            "exit": self.cmd_exit,
            "quit": self.cmd_exit,
            "cd": self.cmd_cd,
            "pwd": self.cmd_pwd,
            "ls": self.cmd_ls,
            "skeleton": self.cmd_skeleton,
            "deps": self.cmd_deps,
            "cfg": self.cmd_cfg,
            "query": self.cmd_query,
            "search": self.cmd_search,
            "context": self.cmd_context,
            "anchors": self.cmd_anchors,
            "complexity": self.cmd_complexity,
            "tree": self.cmd_tree,
        }

        # Setup tab completion
        self._setup_completer()

    def _setup_completer(self) -> None:
        """Setup tab completion for commands and paths."""

        def completer(text: str, state: int) -> str | None:
            """Tab completion function."""
            line = readline.get_line_buffer()
            words = line.split()

            if not words or (len(words) == 1 and not line.endswith(" ")):
                # Complete command names
                matches = [c for c in self.commands if c.startswith(text)]
            else:
                # Complete file paths
                if text:
                    # Expand ~ and resolve relative paths
                    if text.startswith("~"):
                        base = Path.home()
                        text_path = text[2:] if text.startswith("~/") else ""
                    elif text.startswith("/"):
                        base = Path("/")
                        text_path = text[1:]
                    else:
                        base = self.workspace
                        text_path = text

                    # Split into directory and prefix
                    if "/" in text_path:
                        dir_part, prefix = text_path.rsplit("/", 1)
                        search_dir = base / dir_part
                    else:
                        prefix = text_path
                        search_dir = base

                    # Find matching files
                    try:
                        if search_dir.is_dir():
                            matches = []
                            for p in search_dir.iterdir():
                                name = p.name
                                if name.startswith(prefix):
                                    # Add trailing / for directories
                                    rel = p.relative_to(self.workspace)
                                    match = str(rel)
                                    if p.is_dir():
                                        match += "/"
                                    matches.append(match)
                        else:
                            matches = []
                    except (PermissionError, OSError):
                        matches = []
                else:
                    # No text, complete current directory
                    try:
                        matches = [
                            str(p.relative_to(self.workspace)) + ("/" if p.is_dir() else "")
                            for p in self.workspace.iterdir()
                            if not p.name.startswith(".")
                        ][:20]
                    except (PermissionError, OSError):
                        matches = []

            try:
                return matches[state]
            except IndexError:
                return None

        readline.set_completer(completer)
        readline.set_completer_delims(" \t\n")
        readline.parse_and_bind("tab: complete")

    def _load_history(self) -> None:
        """Load command history."""
        try:
            if self.history_file.exists():
                readline.read_history_file(str(self.history_file))
        except OSError:
            pass

    def _save_history(self) -> None:
        """Save command history."""
        try:
            readline.set_history_length(1000)
            readline.write_history_file(str(self.history_file))
        except OSError:
            pass

    def _get_prompt(self) -> str:
        """Get the shell prompt."""
        # Show relative path from home or absolute
        try:
            rel = self.workspace.relative_to(Path.home())
            path_str = f"~/{rel}"
        except ValueError:
            path_str = str(self.workspace)

        return f"\033[36mmoss\033[0m:\033[34m{path_str}\033[0m> "

    def run(self) -> None:
        """Run the interactive shell."""
        self._load_history()

        self.output.header("Moss Interactive Shell")
        self.output.info(f"Workspace: {self.workspace}")
        self.output.info("Type 'help' for available commands, 'exit' to quit.")
        self.output.blank()

        while self.running:
            try:
                line = input(self._get_prompt())
                line = line.strip()

                if not line:
                    continue

                self._execute(line)

            except EOFError:
                self.output.blank()
                self.running = False
            except KeyboardInterrupt:
                self.output.blank()
                continue

        self._save_history()
        self.output.info("Goodbye!")

    def _execute(self, line: str) -> None:
        """Execute a command line."""
        try:
            parts = shlex.split(line)
        except ValueError as e:
            self.output.error(f"Parse error: {e}")
            return

        if not parts:
            return

        cmd = parts[0].lower()
        args = parts[1:]

        if cmd in self.commands:
            try:
                self.commands[cmd](args)
            except Exception as e:
                self.output.error(f"Error: {e}")
        else:
            self.output.error(f"Unknown command: {cmd}")
            self.output.info("Type 'help' for available commands.")

    def cmd_help(self, args: list[str]) -> None:
        """Show help."""
        self.output.header("Available Commands")
        commands = [
            ("help", "Show this help message"),
            ("exit, quit", "Exit the shell"),
            ("", ""),
            ("cd <path>", "Change working directory"),
            ("pwd", "Print working directory"),
            ("ls [path]", "List files (Python files by default)"),
            ("tree [path]", "Show file tree"),
            ("", ""),
            ("skeleton <path>", "Extract code skeleton"),
            ("deps <path>", "Show file dependencies"),
            ("cfg <path> [func]", "Show control flow graph"),
            ("anchors <path>", "Find anchors (functions, classes)"),
            ("context <path>", "Show full context for a file"),
            ("", ""),
            ("query <pattern>", "Search symbols by name pattern"),
            ("search <query>", "Semantic search across codebase"),
            ("", ""),
            ("complexity [path]", "Show complexity analysis"),
            ("health", "Show project health summary"),
        ]
        for cmd, desc in commands:
            if cmd:
                self.output.info(f"  {cmd:20} {desc}")
            else:
                self.output.blank()

    def cmd_exit(self, args: list[str]) -> None:
        """Exit the shell."""
        self.running = False

    def cmd_cd(self, args: list[str]) -> None:
        """Change directory."""
        if not args:
            self.workspace = Path.home()
        else:
            new_path = Path(args[0])
            if not new_path.is_absolute():
                new_path = self.workspace / new_path

            new_path = new_path.resolve()
            if new_path.is_dir():
                self.workspace = new_path
                os.chdir(new_path)
            else:
                self.output.error(f"Not a directory: {new_path}")

    def cmd_pwd(self, args: list[str]) -> None:
        """Print working directory."""
        self.output.info(str(self.workspace))

    def cmd_ls(self, args: list[str]) -> None:
        """List files."""
        path = self.workspace
        if args:
            path = Path(args[0])
            if not path.is_absolute():
                path = self.workspace / path

        pattern = "**/*.py" if len(args) < 2 else args[1]

        if path.is_file():
            self.output.info(str(path))
        elif path.is_dir():
            files = sorted(path.glob(pattern))[:50]  # Limit output
            for f in files:
                try:
                    rel = f.relative_to(self.workspace)
                    self.output.info(str(rel))
                except ValueError:
                    self.output.info(str(f))
            if len(list(path.glob(pattern))) > 50:
                self.output.warning("(showing first 50 files)")
        else:
            self.output.error(f"Path not found: {path}")

    def cmd_skeleton(self, args: list[str]) -> None:
        """Extract code skeleton."""
        if not args:
            self.output.error("Usage: skeleton <path>")
            return

        path = self._resolve_path(args[0])
        if not path.exists():
            self.output.error(f"File not found: {path}")
            return

        from moss.plugins import get_registry
        from moss.views import ViewTarget

        registry = get_registry()
        target = ViewTarget(path=path)
        plugin = registry.find_plugin(target, "skeleton")

        if plugin is None:
            self.output.error("No skeleton plugin for this file type")
            return

        async def render():
            return await plugin.render(target)

        view = asyncio.run(render())
        if view.content:
            self.output.print(view.content)
        else:
            self.output.warning("No symbols found")

    def cmd_deps(self, args: list[str]) -> None:
        """Show dependencies."""
        if not args:
            self.output.error("Usage: deps <path>")
            return

        path = self._resolve_path(args[0])
        if not path.exists():
            self.output.error(f"File not found: {path}")
            return

        from moss.plugins import get_registry
        from moss.views import ViewTarget

        registry = get_registry()
        target = ViewTarget(path=path)
        plugin = registry.find_plugin(target, "dependency")

        if plugin is None:
            self.output.error("No dependency plugin for this file type")
            return

        async def render():
            return await plugin.render(target)

        view = asyncio.run(render())
        if view.content:
            self.output.print(view.content)
        else:
            self.output.warning("No dependencies found")

    def cmd_cfg(self, args: list[str]) -> None:
        """Show control flow graph."""
        if not args:
            self.output.error("Usage: cfg <path> [function_name]")
            return

        path = self._resolve_path(args[0])
        func_name = args[1] if len(args) > 1 else None

        if not path.exists():
            self.output.error(f"File not found: {path}")
            return

        from moss.plugins import get_registry
        from moss.views import ViewOptions, ViewTarget

        registry = get_registry()
        target = ViewTarget(path=path)
        plugin = registry.find_plugin(target, "cfg")

        if plugin is None:
            self.output.error("No CFG plugin for this file type")
            return

        options = ViewOptions(extra={"function_name": func_name})

        async def render():
            return await plugin.render(target, options)

        view = asyncio.run(render())

        cfgs = view.metadata.get("cfgs", [])
        if not cfgs:
            self.output.warning("No functions found")
            return

        for cfg_data in cfgs:
            self.output.info(
                f"{cfg_data['name']}: {cfg_data['node_count']} nodes, "
                f"{cfg_data['edge_count']} edges, "
                f"complexity {cfg_data['cyclomatic_complexity']}"
            )

    def cmd_query(self, args: list[str]) -> None:
        """Search for symbols by name pattern."""
        if not args:
            self.output.error("Usage: query <name_pattern>")
            return

        import re

        from moss.skeleton import extract_python_skeleton

        pattern = re.compile(args[0], re.IGNORECASE)
        results = []

        for py_file in self.workspace.glob("**/*.py"):
            try:
                source = py_file.read_text()
                symbols = extract_python_skeleton(source)
                self._collect_matching(symbols, py_file, pattern, results)
            except (SyntaxError, UnicodeDecodeError):
                pass

        if results:
            for r in results[:20]:  # Limit output
                self.output.info(f"{r['file']}:{r['line']} {r['kind']} {r['name']}")
            if len(results) > 20:
                self.output.warning(f"(showing 20 of {len(results)} matches)")
        else:
            self.output.warning("No matches found")

    def _collect_matching(self, symbols: list, file_path: Path, pattern, results: list) -> None:
        """Collect symbols matching pattern."""
        for sym in symbols:
            if pattern.search(sym.name):
                try:
                    rel = file_path.relative_to(self.workspace)
                except ValueError:
                    rel = file_path
                results.append(
                    {
                        "file": str(rel),
                        "name": sym.name,
                        "kind": sym.kind,
                        "line": sym.lineno,
                    }
                )
            if sym.children:
                self._collect_matching(sym.children, file_path, pattern, results)

    def cmd_search(self, args: list[str]) -> None:
        """Semantic search across codebase."""
        if not args:
            self.output.error("Usage: search <query>")
            return

        query = " ".join(args)

        from moss.semantic_search import create_search_system

        indexer, search = create_search_system("memory")

        async def run_search():
            # Index workspace first
            await indexer.index_directory(self.workspace)
            return await search.search(query, limit=10)

        self.output.step("Searching...")
        results = asyncio.run(run_search())

        if not results:
            self.output.warning("No results found")
            return

        self.output.success(f"Found {len(results)} results:")
        for i, hit in enumerate(results, 1):
            chunk = hit.chunk
            name = chunk.symbol_name or chunk.file_path
            self.output.info(f"{i}. [{chunk.symbol_kind or 'file'}] {name}")
            self.output.print(f"   {chunk.file_path}:{chunk.line_start}")

    def cmd_context(self, args: list[str]) -> None:
        """Show full context for a file."""
        if not args:
            self.output.error("Usage: context <path>")
            return

        path = self._resolve_path(args[0])
        if not path.exists():
            self.output.error(f"File not found: {path}")
            return

        from moss.plugins import get_registry
        from moss.views import ViewTarget

        registry = get_registry()
        target = ViewTarget(path=path)

        skeleton_plugin = registry.find_plugin(target, "skeleton")
        deps_plugin = registry.find_plugin(target, "dependency")

        async def render():
            skeleton_view = None
            deps_view = None
            if skeleton_plugin:
                skeleton_view = await skeleton_plugin.render(target)
            if deps_plugin:
                deps_view = await deps_plugin.render(target)
            return skeleton_view, deps_view

        skeleton_view, deps_view = asyncio.run(render())

        # Show summary
        source = path.read_text()
        line_count = len(source.splitlines())
        self.output.header(path.name)
        self.output.info(f"Lines: {line_count}")

        if deps_view and deps_view.content:
            self.output.blank()
            self.output.step("Dependencies")
            self.output.print(deps_view.content)

        if skeleton_view and skeleton_view.content:
            self.output.blank()
            self.output.step("Skeleton")
            self.output.print(skeleton_view.content)

    def cmd_anchors(self, args: list[str]) -> None:
        """Find anchors in a file or directory."""
        path = self._resolve_path(args[0]) if args else self.workspace

        from moss.skeleton import extract_python_skeleton

        results = []

        if path.is_file():
            files = [path]
        else:
            files = list(path.glob("**/*.py"))[:100]

        for py_file in files:
            try:
                source = py_file.read_text()
                symbols = extract_python_skeleton(source)
                self._collect_anchors(symbols, py_file, results)
            except (SyntaxError, UnicodeDecodeError):
                pass

        if results:
            for r in results[:30]:
                self.output.info(f"{r['file']}:{r['line']} {r['kind']} {r['name']}")
            if len(results) > 30:
                self.output.warning(f"(showing 30 of {len(results)} anchors)")
        else:
            self.output.warning("No anchors found")

    def _collect_anchors(self, symbols: list, file_path: Path, results: list) -> None:
        """Collect all anchors from symbols."""
        for sym in symbols:
            try:
                rel = file_path.relative_to(self.workspace)
            except ValueError:
                rel = file_path
            results.append(
                {
                    "file": str(rel),
                    "name": sym.name,
                    "kind": sym.kind,
                    "line": sym.lineno,
                }
            )
            if sym.children:
                self._collect_anchors(sym.children, file_path, results)

    def cmd_complexity(self, args: list[str]) -> None:
        """Show complexity analysis for a path."""
        path = self._resolve_path(args[0]) if args else self.workspace

        from moss.complexity import analyze_complexity

        try:
            report = analyze_complexity(path)
        except (OSError, ValueError) as e:
            self.output.error(f"Complexity analysis failed: {e}")
            return

        if not report.functions:
            self.output.warning("No functions found")
            return

        # Sort by complexity
        sorted_funcs = sorted(report.functions, key=lambda f: f.complexity, reverse=True)

        self.output.header("Complexity Analysis")
        self.output.info(f"Files: {report.total_files}, Functions: {report.total_functions}")
        self.output.info(f"Average complexity: {report.average_complexity:.1f}")
        self.output.blank()

        # Show top complex functions
        self.output.step("Most complex functions:")
        for func in sorted_funcs[:10]:
            if func.complexity <= 5:
                grade = "A"
            elif func.complexity <= 10:
                grade = "B"
            elif func.complexity <= 20:
                grade = "C"
            else:
                grade = "D"
            loc = f"{func.file}:{func.line}"
            self.output.info(f"  {func.name} ({loc}) - {func.complexity} [{grade}]")

    def cmd_tree(self, args: list[str]) -> None:
        """Show file tree."""
        path = self._resolve_path(args[0]) if args else self.workspace
        tracked_only = "--tracked" in args or "-t" in args

        from moss.tree import generate_tree

        try:
            result = generate_tree(path, tracked_only=tracked_only)
            self.output.print(result.tree)
            self.output.info(f"({result.file_count} files, {result.dir_count} directories)")
        except OSError as e:
            self.output.error(f"Tree generation failed: {e}")

    def _resolve_path(self, path_str: str) -> Path:
        """Resolve a path relative to workspace."""
        path = Path(path_str)
        if not path.is_absolute():
            path = self.workspace / path
        return path.resolve()


def start_shell(workspace: Path | None = None) -> int:
    """Start the interactive shell."""
    shell = MossShell(workspace)
    shell.run()
    return 0
