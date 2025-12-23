"""TUI Interface: Interactive terminal UI for Moss.

Uses Textual for a modern, reactive terminal experience.
"""

from __future__ import annotations

import re
from enum import Enum, auto
from pathlib import Path
from typing import TYPE_CHECKING, Any, ClassVar, Protocol, runtime_checkable

from rich.console import RenderableType

try:
    from textual.app import App, ComposeResult
    from textual.binding import Binding
    from textual.containers import Container, Horizontal, Vertical
    from textual.reactive import reactive
    from textual.suggester import Suggester
    from textual.widgets import Input, Static, Tree
    from textual.widgets.tree import TreeNode
except ImportError:
    # TUI dependencies not installed
    class App:
        pass

    class ComposeResult:
        pass


if TYPE_CHECKING:
    from moss.moss_api import MossAPI
    from moss.task_tree import TaskNode, TaskTree


@runtime_checkable
class TUIMode(Protocol):
    """Protocol for TUI operating modes.

    Mode bindings extend global bindings. Same key in mode overrides global.
    """

    @property
    def name(self) -> str:
        """Mode name."""
        ...

    @property
    def color(self) -> str:
        """Mode color for indicator."""
        ...

    @property
    def placeholder(self) -> str:
        """Command input placeholder."""
        ...

    @property
    def bindings(self) -> list:
        """Mode-specific key bindings (optional, default [])."""
        ...

    async def on_enter(self, app: MossTUI) -> None:
        """Called when entering this mode."""
        ...


class PlanMode:
    name = "Plan"
    color = "blue"
    placeholder = "What is the plan? (e.g. breakdown...)"
    bindings: ClassVar[list] = []

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = True
        app.query_one("#git-view").display = False
        app.query_one("#content-header").update("Agent Log")
        app._update_tree("task")


class ReadMode:
    name = "Read"
    color = "green"
    placeholder = "Explore codebase... (e.g. skeleton, grep, expand)"
    bindings: ClassVar[list] = []

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = True
        app.query_one("#git-view").display = False
        app.query_one("#content-header").update("Agent Log")
        app._update_tree("file")


class WriteMode:
    name = "Write"
    color = "red"
    placeholder = "Modify code... (e.g. write, replace, insert)"
    bindings: ClassVar[list] = []

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = True
        app.query_one("#git-view").display = False
        app.query_one("#content-header").update("Agent Log")
        app._update_tree("file")


class DiffMode:
    name = "Diff"
    color = "magenta"
    placeholder = "Review changes... (revert <file> <line> to undo)"
    bindings: ClassVar[list] = []  # Future: r=revert, a=accept, n=next, p=prev

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = False
        app.query_one("#git-view").display = True
        app.query_one("#content-header").update("Shadow Git")
        await app._update_git_view()
        app._update_tree("task")


class TasksMode:
    """Unified task view showing all work (sessions, workflows, agents)."""

    name = "Tasks"
    color = "yellow"
    placeholder = "View all tasks..."
    bindings: ClassVar[list] = []

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = False
        app.query_one("#git-view").display = False
        app.query_one("#session-view").display = True
        app.query_one("#content-header").update("Tasks")
        await app._update_task_view()


# Backwards compatibility alias
SessionMode = TasksMode


class AgentMode(Enum):
    """Current operating mode of the agent UI."""

    PLAN = auto()  # Planning next steps
    READ = auto()  # Code exploration and search
    WRITE = auto()  # Applying changes and refactoring
    DIFF = auto()  # Reviewing shadow git changes
    SESSION = auto()  # Managing and resuming sessions
    BRANCH = auto()  # Managing multiple experiment branches
    SWARM = auto()  # Visualizing multi-agent swarm activity
    COMMIT = auto()  # Viewing grouped actions in a shadow commit


class BranchMode:
    name = "Branch"
    color = "cyan"
    placeholder = "Manage branches... (branch <name> to switch)"
    bindings: ClassVar[list] = []

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = False
        app.query_one("#git-view").display = True
        app.query_one("#session-view").display = False
        app.query_one("#content-header").update("Git Dashboard")
        await app._update_branch_view()


class SwarmMode:
    name = "Swarm"
    color = "white"
    placeholder = "Manage swarm... (wait for workers to complete)"
    bindings: ClassVar[list] = []

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = False
        app.query_one("#git-view").display = False
        app.query_one("#session-view").display = False
        app.query_one("#swarm-view").display = True
        app.query_one("#content-header").update("Swarm Dashboard")
        await app._update_swarm_view()


class CommitMode:
    name = "Commit"
    color = "green"
    placeholder = "Review commit actions... (select a hunk to view)"
    bindings: ClassVar[list] = []

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = False
        app.query_one("#git-view").display = True
        app.query_one("#session-view").display = False
        app.query_one("#swarm-view").display = False
        app.query_one("#content-header").update("Commit Dashboard")
        await app._update_git_view()


class ExploreMode:
    """Unified exploration mode using tree + three primitives (view/edit/analyze)."""

    name = "Explore"
    color = "cyan"
    placeholder = ""
    bindings: ClassVar[list] = []  # View/Edit/Analyze already in global bindings

    async def on_enter(self, app: MossTUI) -> None:
        try:
            app.query_one("#log-view").display = False
            app.query_one("#git-view").display = False
            app.query_one("#session-view").display = False
            app.query_one("#swarm-view").display = False
            app.query_one("#explore-view").display = True
            app.query_one("#content-header").update("Explore")
            app._update_tree("file")
        except Exception:
            pass  # Widgets may not exist during initial mount


class ModeRegistry:
    """Registry for extensible TUI modes.

    Supports multiple discovery mechanisms:
    1. Built-in modes (PLAN, READ, WRITE, etc.)
    2. Entry points: packages can register via 'moss.tui.modes' entry point
    3. Project modes: .moss/modes/*.py files in project root

    Example entry point in pyproject.toml:
        [project.entry-points."moss.tui.modes"]
        my_mode = "my_package:MyMode"

    Example project mode (.moss/modes/custom.py):
        class CustomMode:
            name = "CUSTOM"
            color = "orange"
            placeholder = "Custom mode..."
            async def on_enter(self, app): pass
    """

    _BUILTIN_MODES: ClassVar[list[type]] = [
        ExploreMode,  # Default mode - tree + primitives
        PlanMode,
        DiffMode,
        TasksMode,  # Unified task view (sessions, workflows, agents)
        BranchMode,
        SwarmMode,
        CommitMode,
    ]

    def __init__(self, discover: bool = True, project_root: Path | None = None):
        self._modes: dict[str, TUIMode] = {}
        self._order: list[str] = []
        self._project_root = project_root

        # Register built-in modes
        for mode_cls in self._BUILTIN_MODES:
            self._register(mode_cls())

        if discover:
            self._discover_entry_points()
            self._discover_project_modes()

    def _register(self, mode: TUIMode) -> None:
        """Internal registration without order modification check."""
        self._modes[mode.name] = mode
        if mode.name not in self._order:
            self._order.append(mode.name)

    def _discover_entry_points(self) -> None:
        """Discover modes from installed packages via entry points."""
        try:
            from importlib.metadata import entry_points

            eps = entry_points()
            # Python 3.10+ returns SelectableGroups, 3.9 returns dict
            if hasattr(eps, "select"):
                mode_eps = eps.select(group="moss.tui.modes")
            else:
                mode_eps = eps.get("moss.tui.modes", [])

            for ep in mode_eps:
                try:
                    mode_cls = ep.load()
                    mode = mode_cls() if callable(mode_cls) else mode_cls
                    if isinstance(mode, TUIMode):
                        self._register(mode)
                except (ImportError, AttributeError, TypeError):
                    pass  # Skip invalid entry points
        except ImportError:
            pass

    def _discover_project_modes(self) -> None:
        """Discover modes from .moss/modes/ directory."""
        if self._project_root is None:
            return

        modes_dir = self._project_root / ".moss" / "modes"
        if not modes_dir.is_dir():
            return

        import importlib.util

        for py_file in modes_dir.glob("*.py"):
            if py_file.name.startswith("_"):
                continue
            try:
                spec = importlib.util.spec_from_file_location(f"moss_modes_{py_file.stem}", py_file)
                if spec and spec.loader:
                    module = importlib.util.module_from_spec(spec)
                    spec.loader.exec_module(module)

                    # Find all TUIMode implementations in module
                    for attr_name in dir(module):
                        if attr_name.startswith("_"):
                            continue
                        attr = getattr(module, attr_name)
                        if isinstance(attr, type) and isinstance(attr, TUIMode):
                            self._register(attr())
                        elif isinstance(attr, TUIMode):
                            self._register(attr)
            except (ImportError, SyntaxError, AttributeError):
                pass  # Skip invalid mode files

    def get_mode(self, name: str) -> TUIMode | None:
        return self._modes.get(name)

    def next_mode(self, current_name: str) -> TUIMode:
        idx = self._order.index(current_name)
        next_idx = (idx + 1) % len(self._order)
        return self._modes[self._order[next_idx]]

    def register_mode(self, mode: TUIMode, position: int | None = None) -> None:
        """Register a mode, optionally at a specific position in cycle order."""
        self._modes[mode.name] = mode
        if mode.name in self._order:
            return
        if position is not None:
            self._order.insert(position, mode.name)
        else:
            self._order.append(mode.name)

    def unregister_mode(self, name: str) -> bool:
        """Remove a mode from the registry."""
        if name in self._modes:
            del self._modes[name]
            self._order = [n for n in self._order if n != name]
            return True
        return False

    def set_order(self, order: list[str]) -> None:
        """Set custom mode cycling order. Modes not in order are appended."""
        valid = [n for n in order if n in self._modes]
        missing = [n for n in self._order if n not in valid]
        self._order = valid + missing

    def list_modes(self) -> list[str]:
        """Return mode names in cycle order."""
        return list(self._order)


class ModeIndicator(Static):
    """Widget to display the current agent mode."""

    mode_name = reactive("Plan")
    mode_color = reactive("blue")

    def render(self) -> str:
        return f"Mode: [{self.mode_color} b]{self.mode_name}[/]"


class KeybindBar(Static):
    """Footer showing keybindings built from app bindings.

    Uses active_bindings which merges global + mode-specific bindings.
    """

    DEFAULT_CSS = """
    KeybindBar {
        dock: bottom;
        width: 100%;
        height: 1;
        background: $surface-darken-1;
    }
    """

    def render(self) -> str:
        parts = []
        if self.app:
            bindings = getattr(self.app, "active_bindings", self.app.BINDINGS)
            for binding in bindings:
                if not binding.show:
                    continue
                key = binding.key
                if key == "minus":
                    key = "-"
                elif key == "slash":
                    key = "/"
                desc = binding.description
                action = binding.action.replace("app.", "")
                # Wrap the key in brackets: [Q]uit, [-] Up
                # Use \[ to escape brackets in Textual markup
                idx = desc.lower().find(key.lower())
                if idx >= 0:
                    # Key found in description - wrap it
                    text = f"{desc[:idx]}\\[{desc[idx]}]{desc[idx + 1 :]}"
                else:
                    # Key not in description, prefix with [key]
                    text = f"\\[{key}] {desc}"
                parts.append(f"[@click=app.{action}]{text}[/]")
        left = " ".join(parts)

        # Mode indicator + Palette on the right
        mode_name = getattr(self.app, "current_mode_name", "Explore") if self.app else "Explore"
        mode = self.app._mode_registry.get_mode(mode_name) if self.app else None
        mode_color = getattr(mode, "color", "cyan") if mode else "cyan"
        mode_indicator = f"\\[Tab] [{mode_color}]{mode_name}[/]"
        palette = "\\[^p] Palette"
        mode_part = f"[@click=app.next_mode]{mode_indicator}[/]"
        palette_part = f"[@click=app.command_palette]{palette}[/]"
        right = f"{mode_part} {palette_part}"

        # Calculate padding
        bindings = getattr(self.app, "active_bindings", self.app.BINDINGS) if self.app else []
        shown = [b for b in bindings if b.show]
        left_len = sum(len(b.description) + 3 for b in shown) + max(0, len(shown) - 1)
        right_len = len(f"[Tab] {mode_name}") + len(" [^p] Palette") + 1
        width = self.size.width if self.size.width > 0 else 80
        padding = max(1, width - left_len - right_len - 2)
        return f"{left}{' ' * padding}{right}"


class Breadcrumb(Static):
    """Breadcrumb navigation showing path from project root."""

    path_parts: reactive[list[tuple[str, Path]]] = reactive(list)
    project_name: reactive[str] = reactive("")

    def render(self) -> str:
        # Always show clickable project root
        root_link = f"[@click=app.cd_root()]{self.project_name or 'root'}[/]"
        if not self.path_parts:
            return root_link
        parts = [root_link]
        for name, path in self.path_parts:
            parts.append(f"[@click=app.cd_to('{path}')]{name}[/]")
        return " [dim]/[/] ".join(parts)


class HoverTooltip(Static):
    """Tooltip displayed when a node is highlighted."""

    content = reactive("")
    file_path: Path | None = None  # Set when showing file content

    def render(self) -> RenderableType:
        from rich.text import Text

        if not self.content:
            return ""

        header = Text("Details:", style="bold")

        # Try syntax highlighting for file content
        if self.file_path and self.file_path.suffix in (".py", ".rs", ".js", ".ts", ".go", ".rb"):
            try:
                from rich.syntax import Syntax

                # Map suffix to lexer name
                lexer_map = {
                    ".py": "python",
                    ".rs": "rust",
                    ".js": "javascript",
                    ".ts": "typescript",
                    ".go": "go",
                    ".rb": "ruby",
                }
                lexer = lexer_map.get(self.file_path.suffix, "text")
                syntax = Syntax(
                    self.content,
                    lexer,
                    theme="monokai",
                    line_numbers=False,
                    word_wrap=True,
                )
                from rich.console import Group

                return Group(header, syntax)
            except ImportError:
                # Pygments not installed, fall back to plain text
                pass

        return f"[b]Details:[/b]\n{self.content}"


class ProjectTree(Tree[Any]):
    """Unified tree for task and file navigation."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self._api: MossAPI | None = None
        self._tree_root: Path | None = None
        self._indexed_files: set[str] = set()
        self.guide_depth = 2  # Minimal indentation

    def update_from_tasks(self, task_tree: TaskTree) -> None:
        self.clear()
        root = self.root
        root.label = f"[b]Tasks: {task_tree.root.goal}[/b]"
        self._add_task_nodes(root, task_tree.root)
        root.expand()

    def _add_task_nodes(self, tree_node: TreeNode[Any], task_node: TaskNode) -> None:
        for child in task_node.children:
            status_icon = "✓" if child.status.name == "DONE" else "→"
            label = f"{status_icon} {child.goal}"
            if child.summary:
                label += f" ({child.summary})"

            new_node = tree_node.add(label, expand=True, data={"type": "task", "node": child})
            self._add_task_nodes(new_node, child)

    # Extensions for files that can show symbols when expanded
    EXPANDABLE_EXTS = (
        ".py",
        ".rs",
        ".md",
        ".js",
        ".mjs",
        ".cjs",
        ".jsx",
        ".ts",
        ".mts",
        ".cts",
        ".tsx",
        ".go",
        ".java",
        ".c",
        ".h",
        ".cpp",
        ".cc",
        ".hpp",
        ".rb",
        ".sh",
    )

    def update_from_files(self, api: MossAPI, tree_root: Path | None = None) -> None:
        """Update tree from filesystem with lazy loading.

        Uses git ls-files to respect .gitignore. Only loads immediate children
        of the root directory; subdirectories load on expand.
        """
        self._api = api
        self._tree_root = tree_root or api.root
        self._indexed_files: set[str] = self._get_indexed_files(self._tree_root)
        self.clear()
        root = self.root
        root.label = f"[b]Files: {self._tree_root.name}[/b]"
        root.data = {"type": "dir", "path": self._tree_root, "loaded": True}

        # Only load immediate children of root
        self._load_dir_children(root, self._tree_root)
        root.expand()

    def _get_indexed_files(self, root: Path) -> set[str]:
        """Get all files from Rust index via list-files command.

        Falls back to git ls-files if Rust not available.
        """
        from moss.rust_shim import rust_list_files

        # Try Rust index first
        files = rust_list_files(prefix="", limit=10000, root=str(root))
        if files is not None:
            return set(files)

        # Fallback to git ls-files
        import subprocess

        try:
            result = subprocess.run(
                ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
                cwd=root,
                capture_output=True,
                text=True,
                check=True,
            )
            return set(result.stdout.strip().split("\n")) if result.stdout.strip() else set()
        except (subprocess.CalledProcessError, FileNotFoundError):
            return set()

    def _load_dir_children(self, tree_node: TreeNode[Any], path: Path) -> None:
        """Load immediate children of a directory into tree node."""
        # Get relative path from tree root
        try:
            rel_path = path.relative_to(self._tree_root)
            prefix = str(rel_path) + "/" if str(rel_path) != "." else ""
        except ValueError:
            return

        # Find immediate children from indexed files
        children: dict[str, bool] = {}  # name -> is_dir
        for f in self._indexed_files:
            if not f.startswith(prefix):
                continue
            rest = f[len(prefix) :]
            if "/" in rest:
                # It's in a subdirectory
                dir_name = rest.split("/")[0]
                if dir_name and not dir_name.startswith("."):
                    children[dir_name] = True  # is_dir
            elif rest and not rest.startswith("."):
                children[rest] = False  # is_file

        # Add nodes sorted: directories first, then files
        dirs = sorted(k for k, is_dir in children.items() if is_dir)
        files = sorted(k for k, is_dir in children.items() if not is_dir)

        for entry in dirs:
            full_path = path / entry
            data = {"type": "dir", "path": full_path, "loaded": False}
            tree_node.add(f"📁 {entry}", data=data)

        for entry in files:
            full_path = path / entry
            file_data = {"type": "file", "path": full_path, "loaded": False}
            if entry.endswith(self.EXPANDABLE_EXTS):
                tree_node.add(f"📄 {entry}", data=file_data)
            else:
                tree_node.add_leaf(f"  📄 {entry}", data=file_data)

    def on_tree_node_expanded(self, event: Tree.NodeExpanded) -> None:
        """Load children lazily when a node is expanded."""
        data = event.node.data
        if not data or data.get("loaded"):
            return

        node_type = data.get("type")
        path = data.get("path")
        if not path:
            return

        data["loaded"] = True

        if node_type == "dir":
            # Lazy load directory children
            self._load_dir_children(event.node, path)
        elif node_type == "file":
            # Lazy load file symbols
            self._load_file_symbols(event.node, path)

    def _load_file_symbols(self, tree_node: TreeNode[Any], path: Path) -> None:
        """Load symbols for a file into tree node."""
        # Symbol kind icons
        kind_icons = {
            "class": "📦",
            "function": "⚡",
            "method": "🔧",
            "variable": "📌",
            "heading": "📑",
        }

        def add_symbols(node: TreeNode[Any], symbols: list, file_path: Path) -> None:
            for symbol in symbols:
                icon = kind_icons.get(symbol.kind, "•")
                label = f"{icon} {symbol.name}"
                sym_data = {"type": "symbol", "symbol": symbol, "path": file_path}
                if symbol.children:
                    sym_node = node.add(label, data=sym_data)
                    add_symbols(sym_node, symbol.children, file_path)
                else:
                    node.add_leaf(f"  {label}", data=sym_data)

        # Extensions that support symbol extraction
        supported_exts = (
            ".py",
            ".rs",
            ".md",
            ".js",
            ".mjs",
            ".cjs",
            ".jsx",
            ".ts",
            ".mts",
            ".cts",
            ".tsx",
            ".go",
            ".java",
            ".c",
            ".h",
            ".cpp",
            ".cc",
            ".cxx",
            ".hpp",
            ".rb",
            ".sh",
            ".bash",
            ".json",
            ".yaml",
            ".yml",
            ".html",
            ".htm",
            ".css",
            ".toml",
        )
        symbols_found = False
        if self._api and path.suffix in supported_exts:
            try:
                symbols = self._api.skeleton.extract(path)
                if symbols:
                    add_symbols(tree_node, symbols, path)
                    symbols_found = True
            except (OSError, ValueError, SyntaxError):
                pass

        if not symbols_found:
            tree_node.allow_expand = False


class PathSuggester(Suggester):
    """Suggester that provides path completions relative to project root."""

    def __init__(self, root: Path, case_sensitive: bool = True):
        super().__init__(case_sensitive=case_sensitive)
        self.root = root

    async def get_suggestion(self, value: str) -> str | None:
        """Get path completion suggestion."""
        if not value:
            return None

        # Extract path portion from command (e.g., "view src/fo" -> "src/fo")
        parts = value.split()
        if len(parts) >= 2:
            path_part = parts[-1]
        else:
            path_part = value

        # Try to find matching paths
        try:
            target_path = self.root / path_part
            parent = target_path.parent
            prefix = target_path.name

            if parent.exists() and parent.is_dir():
                for item in parent.iterdir():
                    if item.name.startswith(prefix) and item.name != prefix:
                        # Return full command with completed path
                        rel_path = item.relative_to(self.root)
                        if len(parts) >= 2:
                            return " ".join(parts[:-1]) + " " + str(rel_path)
                        return str(rel_path)
        except (OSError, ValueError):
            pass

        return None


class MossTUI(App):
    """The main Moss TUI application."""

    CSS = """
    Screen {
        background: $surface;
    }

    Screen.transparent {
        background: transparent;
    }

    #main-container {
        height: 1fr;
    }

    #sidebar {
        width: 30%;
        height: 1fr;
        border-right: tall $primary;
        background: $surface-darken-1;
    }

    .transparent #sidebar {
        background: transparent 50%;
    }

    #breadcrumb {
        height: auto;
        padding: 0 1;
        background: $surface-darken-2;
    }

    .transparent #breadcrumb {
        background: transparent 30%;
    }

    .transparent #explore-header {
        background: transparent 30%;
    }

    .transparent HoverTooltip {
        background: transparent 50%;
    }

    #content-area {
        width: 70%;
        height: 1fr;
        padding: 1;
    }

    #command-input {
        dock: bottom;
        display: none;
        height: 3;
        margin: 0 1;
        padding: 0 1;
        border: solid $primary;
    }

    .log-entry {
        margin-bottom: 1;
        padding: 0 1;
        border-left: solid $accent;
    }

    #git-view {
        display: none;
    }

    #session-view {
        display: none;
    }

    #swarm-view {
        display: none;
    }

    #explore-view {
        display: none;
        height: 1fr;
    }

    #explore-header {
        height: auto;
        padding: 0 1;
        background: $surface-darken-1;
        text-style: bold;
    }

    #explore-detail {
        height: 1fr;
        border: solid $secondary;
        padding: 1;
        overflow-y: auto;
    }

    #diff-view {
        height: 1fr;
        border: solid $secondary;
    }

    #history-tree {
        height: 30%;
        border: solid $secondary;
    }

    HoverTooltip {
        dock: right;
        width: 25%;
        height: auto;
        max-height: 50%;
        background: $surface-lighten-1;
        border: solid $primary;
        padding: 1;
        display: none;
    }

    Tree {
        scrollbar-gutter: stable;
    }

    Tree > .tree--guides {
        color: $text-muted;
    }

    Tree > .tree--cursor {
        background: $accent;
    }

    ProjectTree {
        padding: 0;
    }

    ProjectTree > .tree--label {
        padding-left: 0;
    }

    CommandPalette > #--container > #--input {
        height: 3;
    }

    CommandPalette CommandInput {
        border: none;
    }
    """

    BINDINGS: ClassVar[list[Binding]] = [
        Binding("q", "quit", "Quit"),
        Binding("ctrl+c", "handle_ctrl_c", "Interrupt", show=False),
        Binding("ctrl+p", "command_palette", "Palette", priority=True),
        Binding("v", "primitive_view", "View"),
        Binding("e", "primitive_edit", "Edit"),
        Binding("a", "primitive_analyze", "Analyze"),
        Binding("minus", "cd_up", "Up"),
        Binding("slash", "toggle_command", "Cmd", show=False),
        Binding("g", "goto_node", "Goto", show=False),  # Keep for quick access
        Binding("tab", "next_mode", "Mode", show=False),
        Binding("enter", "enter_dir", "Enter", show=False),
        Binding("escape", "hide_command", show=False),
        Binding("left", "tree_collapse", "Collapse", show=False),
        Binding("right", "tree_expand", "Expand", show=False),
    ]

    current_mode_name = reactive("Explore")

    @property
    def active_bindings(self) -> list[Binding]:
        """Merge global + mode bindings. Mode overrides global on key conflict."""
        mode = self._mode_registry.get_mode(self.current_mode_name)
        mode_bindings = getattr(mode, "bindings", []) if mode else []
        if not mode_bindings:
            return list(self.BINDINGS)

        # Build lookup: key -> binding (mode overrides global)
        result = {b.key: b for b in self.BINDINGS}
        for b in mode_bindings:
            result[b.key] = b
        return list(result.values())

    def __init__(self, api: MossAPI):
        super().__init__()
        self.api = api
        self._task_tree: TaskTree | None = None
        self._mode_registry = ModeRegistry()
        self._last_ctrl_c: float = 0
        self._tree_root: Path = api.root  # Current root for file tree
        self._transparent_bg: bool = False  # For terminal opacity support
        self._last_preview_update: float = 0  # Throttle preview updates
        self._preview_throttle_ms: int = 100  # Min ms between updates

    def action_handle_ctrl_c(self) -> None:
        """Handle Ctrl+C with double-tap to exit."""
        import time

        now = time.time()
        if now - self._last_ctrl_c < 0.5:
            self.exit()
        else:
            self._last_ctrl_c = now
            self._log("Press Ctrl+C again to exit")

    SETTINGS_PATH = Path.home() / ".config" / "moss" / "tui_settings.json"

    def _load_settings(self) -> None:
        """Load saved settings (theme, transparent_bg)."""
        import json

        if self.SETTINGS_PATH.exists():
            try:
                data = json.loads(self.SETTINGS_PATH.read_text())
                if "theme" in data:
                    self.theme = data["theme"]
                self._transparent_bg = data.get("transparent_bg", False)
            except (json.JSONDecodeError, OSError):
                pass

    def _save_settings(self) -> None:
        """Save current settings."""
        import json

        self.SETTINGS_PATH.parent.mkdir(parents=True, exist_ok=True)
        data = {}
        if self.SETTINGS_PATH.exists():
            try:
                data = json.loads(self.SETTINGS_PATH.read_text())
            except (json.JSONDecodeError, OSError):
                pass
        data["theme"] = self.theme
        data["transparent_bg"] = getattr(self, "_transparent_bg", False)
        try:
            self.SETTINGS_PATH.write_text(json.dumps(data))
        except OSError:
            pass

    def watch_theme(self, theme: str) -> None:
        """Save settings when theme changed."""
        self._save_settings()

    def _get_syntax_theme(self) -> str:
        """Get syntax highlighting theme matching current UI theme."""
        dark_themes = {"textual-dark", "monokai", "dracula", "nord", "gruvbox"}
        if self.theme in dark_themes or "dark" in self.theme.lower():
            return "monokai"
        return "default"

    def _get_syntax_bg(self) -> str | None:
        """Get syntax background color (None for transparent/theme-matched)."""
        # Always return None to let theme background show through
        # Rich Syntax with background_color=None uses transparent bg
        return None

    def compose(self) -> ComposeResult:
        """Create child widgets for the app."""
        from textual.widgets import RichLog

        # No header - footer provides all needed info
        yield Container(
            Horizontal(
                Vertical(
                    Static("Navigation", id="sidebar-header"),
                    Breadcrumb(id="breadcrumb"),
                    ProjectTree("Project", id="project-tree"),
                    id="sidebar",
                ),
                Vertical(
                    Static("Agent Log", id="content-header"),
                    Container(id="log-view"),
                    Container(
                        Static("Shadow Git History", id="git-history-header"),
                        Tree("Commits", id="history-tree"),
                        Static("Diff", id="diff-header"),
                        RichLog(id="diff-view", highlight=True, markup=True),
                        id="git-view",
                    ),
                    Container(
                        Static("Past Sessions", classes="sidebar-header"),
                        Tree("Sessions", id="session-tree"),
                        id="session-view",
                    ),
                    Container(
                        Static("Active Swarm", classes="sidebar-header"),
                        Tree("Workers", id="swarm-tree"),
                        id="swarm-view",
                    ),
                    Container(
                        Static("", id="explore-header"),
                        RichLog(id="explore-detail", highlight=True, markup=True),
                        id="explore-view",
                    ),
                    id="content-area",
                ),
                id="main-container",
            ),
            # ActionBar removed - actions shown in footer bindings
            Input(
                placeholder="Enter command...",
                id="command-input",
                suggester=PathSuggester(self.api.root),
            ),
            HoverTooltip(id="hover-tooltip"),
        )
        yield KeybindBar()

    def on_mount(self) -> None:
        """Called when the app is mounted."""
        self.title = "Explore"
        self.sub_title = ""
        # Focus tree so keybindings are visible (Tab to input)
        self.query_one("#project-tree").focus()
        # Track selected node for action bar
        self._selected_path: str = ""
        self._selected_type: str = ""
        # Load theme preference and apply transparency
        self._load_settings()
        self._apply_transparency()

        # Subscribe to tool calls to show resources
        from moss.events import Event, EventType

        async def on_tool_call(event: Event) -> None:
            tool = event.payload.get("tool_name", "unknown")
            duration = event.payload.get("duration_ms", 0)
            mem = event.payload.get("memory_bytes", 0) / 1024 / 1024
            ctx = event.payload.get("context_tokens", 0)
            breakdown = event.payload.get("memory_breakdown", {})

            # Format breakdown for display
            bd_str = ""
            if breakdown:
                # Show top 3 components
                sorted_bd = sorted(breakdown.items(), key=lambda x: x[1], reverse=True)
                bd_parts = []
                for k, v in sorted_bd[:3]:
                    bd_parts.append(f"{k}: {v / 1024 / 1024:.1f}MB")
                bd_str = f" [dim][[{', '.join(bd_parts)}]][/]"

            msg = (
                f"Tool: [b]{tool}[/] ({duration}ms) | "
                f"RAM: [cyan]{mem:.1f} MB[/]{bd_str} | "
                f"Context: [yellow]{ctx}[/] tokens"
            )
            self.call_from_thread(self._log, msg)

        # In a real app, MossAPI would have an event_bus
        if hasattr(self.api, "event_bus") and self.api.event_bus:
            self.api.event_bus.subscribe(EventType.TOOL_CALL, on_tool_call)

    async def watch_current_mode_name(self, name: str) -> None:
        """React to mode changes."""
        mode = self._mode_registry.get_mode(name)
        if not mode:
            return

        # Update title with mode name
        self.title = mode.name.title()
        self.sub_title = ""

        self.query_one("#command-input").placeholder = mode.placeholder

        # Reset all views before entering new mode
        self.query_one("#log-view").display = False
        self.query_one("#git-view").display = False
        self.query_one("#session-view").display = False
        self.query_one("#swarm-view").display = False
        self.query_one("#explore-view").display = False

        await mode.on_enter(self)

        # Refresh keybind bar to show mode-specific bindings
        try:
            self.query_one(KeybindBar).refresh()
        except Exception:
            pass  # KeybindBar may not be mounted yet

    async def _update_branch_view(self) -> None:
        """Fetch and display all shadow branches."""
        tree = self.query_one("#history-tree")
        diff_view = self.query_one("#diff-view")

        try:
            branches = await self.api.shadow_git.list_branches()
            tree.clear()
            root = tree.root

            if not branches:
                root.label = "No shadow branches"
                diff_view.clear()
                diff_view.write("[dim]No shadow branches yet.[/]\n\nStart a task to track changes.")
                return

            root.label = f"Shadow Branches ({len(branches)})"
            for b in branches:
                label = f"[@click=app.navigate_branch('{b}')]{b}[/]"
                root.add_leaf(label)
            root.expand()

            # Show current diff in diff-view
            diff = await self.api.shadow_git.get_diff("shadow/current")
            diff_view.clear()
            if diff.strip():
                diff_view.write(diff)
            else:
                diff_view.write("[dim]No changes on current branch.[/]")
        except Exception as e:
            tree.clear()
            tree.root.label = "Error"
            diff_view.clear()
            diff_view.write(f"[red]Error loading branches:[/] {e}")

    def navigate_branch(self, branch_name: str) -> None:
        """Switch to a specific branch and update view."""
        self._log(f"Switching to branch: {branch_name}")
        cmd = self.query_one("#command-input")
        cmd.value = f"branch {branch_name}"
        cmd.display = True
        cmd.focus()

    def _update_tree(self, tree_type: str = "task") -> None:
        """Update the sidebar tree."""
        tree = self.query_one("#project-tree", ProjectTree)
        if tree_type == "task" and self._task_tree:
            tree.update_from_tasks(self._task_tree)
        else:
            tree.update_from_files(self.api, self._tree_root)
        self._update_breadcrumb()

    def _update_breadcrumb(self) -> None:
        """Update breadcrumb to reflect current tree root."""
        breadcrumb = self.query_one("#breadcrumb", Breadcrumb)
        breadcrumb.project_name = self.api.root.name
        if self._tree_root == self.api.root:
            breadcrumb.path_parts = []
        else:
            # Build path parts from project root to current
            parts = []
            current = self._tree_root
            while current != self.api.root and current != current.parent:
                parts.insert(0, (current.name, current))
                current = current.parent
            breadcrumb.path_parts = parts

    def action_cd_to(self, path: str) -> None:
        """Navigate to a specific directory."""
        target = Path(path)
        if target.is_dir():
            self._tree_root = target
            self._update_tree("file")
            self._log(f"Changed to: {target.name}")

    def action_cd_up(self) -> None:
        """Navigate up one directory."""
        if self._tree_root != self.api.root:
            self._tree_root = self._tree_root.parent
            self._update_tree("file")
            self._log(f"Changed to: {self._tree_root.name}")

    def action_cd_root(self) -> None:
        """Navigate back to project root."""
        if self._tree_root != self.api.root:
            self._tree_root = self.api.root
            self._update_tree("file")
            self._log("Changed to project root")

    def action_enter_dir(self) -> None:
        """Enter selected directory (navigate into it)."""
        if self._selected_type == "dir" and self._selected_path:
            self.action_cd_to(self._selected_path)

    def action_tree_expand(self) -> None:
        """Expand the current tree node."""
        tree = self.query_one("#project-tree", ProjectTree)
        if tree.cursor_node and tree.cursor_node.allow_expand:
            tree.cursor_node.expand()

    def action_tree_collapse(self) -> None:
        """Collapse the current tree node or go to parent."""
        tree = self.query_one("#project-tree", ProjectTree)
        if tree.cursor_node:
            if tree.cursor_node.is_expanded:
                tree.cursor_node.collapse()
            elif tree.cursor_node.parent:
                # If already collapsed, go to parent node
                tree.select_node(tree.cursor_node.parent)

    def action_goto_node(self) -> None:
        """Show goto input for fuzzy file navigation."""
        cmd_input = self.query_one("#command-input", Input)
        cmd_input.placeholder = "Goto: type path to fuzzy match..."
        cmd_input.value = "goto "
        cmd_input.display = True
        cmd_input.focus()
        # Move cursor to end
        cmd_input.cursor_position = len(cmd_input.value)

    def action_toggle_command(self) -> None:
        """Toggle command input visibility."""
        cmd_input = self.query_one("#command-input", Input)
        if cmd_input.display:
            cmd_input.display = False
            cmd_input.placeholder = "Enter command..."
            self.query_one("#project-tree").focus()
        else:
            cmd_input.display = True
            cmd_input.focus()

    def action_hide_command(self) -> None:
        """Hide command input (Escape)."""
        cmd_input = self.query_one("#command-input", Input)
        if cmd_input.display:
            cmd_input.display = False
            cmd_input.value = ""
            cmd_input.placeholder = "Enter command..."
            self.query_one("#project-tree").focus()

    def on_tree_node_highlighted(self, event: Tree.NodeHighlighted) -> None:
        """Handle node highlight (hover/selection movement)."""
        data = event.node.data
        if not data:
            return

        tooltip = self.query_one("#hover-tooltip", HoverTooltip)

        if data["type"] == "file":
            path = data["path"]
            self._selected_path = str(path)
            self._selected_type = "file"
            tooltip.file_path = path
            try:
                skeleton = self.api.skeleton.format(path)
                summary = "\n".join(skeleton.split("\n")[:15])
                if len(skeleton.split("\n")) > 15:
                    summary += "\n..."
                tooltip.content = summary
            except (OSError, ValueError):
                tooltip.content = f"File: {path.name}"
        elif data["type"] == "dir":
            path = data["path"]
            self._selected_path = str(path)
            self._selected_type = "dir"
            tooltip.file_path = None
            tooltip.content = f"Directory: {path.name}"
        elif data["type"] == "symbol":
            symbol = data["symbol"]
            path = data["path"]
            symbol_path = f"{path}/{symbol.name}"
            self._selected_path = symbol_path
            self._selected_type = "symbol"
            tooltip.file_path = path
            lines = [symbol.signature]
            if symbol.lineno:
                lines[0] += f"  # line {symbol.lineno}"
            if symbol.docstring:
                doc_lines = symbol.docstring.strip().split("\n")[:5]
                if len(doc_lines) < len(symbol.docstring.strip().split("\n")):
                    doc_lines.append("...")
                lines.append("")
                lines.extend(f'"""{line}' if i == 0 else line for i, line in enumerate(doc_lines))
                if not lines[-1].endswith('"""'):
                    lines.append('"""')
            tooltip.content = "\n".join(lines)
        elif data["type"] == "task":
            tooltip.file_path = None
            node = data["node"]
            tooltip.content = f"Goal: {node.goal}\nStatus: {node.status.name}"
            if node.summary:
                tooltip.content += f"\nSummary: {node.summary}"
        else:
            tooltip.file_path = None
            tooltip.content = ""

        # Auto-update preview on arrow navigation in Explore mode (throttled)
        if self.current_mode_name == "Explore" and data["type"] in ("file", "symbol"):
            import time

            now = time.time() * 1000  # ms
            if now - self._last_preview_update >= self._preview_throttle_ms:
                self._last_preview_update = now
                self.action_primitive_view()

    def on_tree_node_selected(self, event: Tree.NodeSelected) -> None:
        """Handle tree node selection (click/enter)."""
        data = event.node.data
        if not data:
            return

        if data["type"] == "file":
            path = data["path"]
            self._selected_path = str(path)
            self._selected_type = "file"
            # In Explore mode, double-click triggers view
            if self.current_mode_name == "Explore":
                self.action_primitive_view()
            else:
                self._log(f"Opened file: {path.name}")
                cmd = self.query_one("#command-input")
                cmd.value = f"view {path}"
                cmd.display = True
                cmd.focus()
        elif data["type"] == "dir":
            path = data["path"]
            path_str = str(path)
            self._selected_type = "dir"
            # Double-click detection: navigate on second click within 0.5s
            if self.current_mode_name == "Explore":
                import time

                now = time.time()
                if (
                    self._selected_path == path_str
                    and now - getattr(self, "_last_dir_click", 0) < 0.5
                ):
                    self.action_cd_to(path_str)
                self._last_dir_click = now
            self._selected_path = path_str
        elif data["type"] == "symbol":
            symbol = data["symbol"]
            path = data["path"]
            symbol_path = f"{path}/{symbol.name}"
            self._selected_path = symbol_path
            self._selected_type = "symbol"
            self._selected_symbol = symbol  # Keep symbol object for markdown headings
            self._selected_file = path
            if self.current_mode_name == "Explore":
                self.action_primitive_view()
            else:
                self._log(f"Symbol: {symbol.name} at {path.name}:{symbol.lineno}")
                self.query_one("#command-input").value = f"view {symbol_path}"
                self.query_one("#command-input").focus()

    def action_toggle_tooltip(self) -> None:
        """Toggle tooltip visibility."""
        tooltip = self.query_one("#hover-tooltip")
        tooltip.display = not tooltip.display

    def action_next_mode(self) -> None:
        """Switch to the next mode."""
        next_mode = self._mode_registry.next_mode(self.current_mode_name)
        self.current_mode_name = next_mode.name
        self._log(f"Switched to {self.current_mode_name} mode")

    def action_resume_task(self, task_id: str) -> None:
        """Resume a task by ID."""
        from moss.session import SessionManager

        manager = SessionManager(self.api.root / ".moss" / "sessions")
        task = manager.get(task_id)
        if task:
            self._log(f"Resuming task: {task.task[:50]}")
            self._log(f"Shadow branch: {task.shadow_branch}")
            # In a full implementation, this would:
            # 1. Checkout the shadow branch
            # 2. Load the task's context
            # 3. Resume the agent loop
        else:
            self._log(f"Task not found: {task_id}")

    def action_toggle_transparency(self) -> None:
        """Toggle transparent background for terminal opacity support."""
        self._transparent_bg = not getattr(self, "_transparent_bg", False)
        self._save_settings()
        self._apply_transparency()
        status = "enabled" if self._transparent_bg else "disabled"
        self._log(f"Transparent background {status}")

    def _apply_transparency(self) -> None:
        """Apply transparency setting to UI."""
        screen = self.screen
        if self._transparent_bg:
            screen.add_class("transparent")
        else:
            screen.remove_class("transparent")

    def get_system_commands(self, screen):
        """Add custom commands to the command palette."""
        from textual.app import SystemCommand

        yield from super().get_system_commands(screen)
        yield SystemCommand(
            "Goto File",
            "Fuzzy search and jump to a file (g)",
            self.action_goto_node,
        )
        yield SystemCommand(
            "Toggle Transparency",
            "Enable/disable transparent background for terminal opacity",
            self.action_toggle_transparency,
        )
        yield SystemCommand(
            "View Selected",
            "View the currently selected file or symbol (v)",
            self.action_primitive_view,
        )
        yield SystemCommand(
            "Analyze Selected",
            "Analyze the currently selected file (a)",
            self.action_primitive_analyze,
        )

    async def _update_git_view(self) -> None:
        """Fetch and display shadow git data."""
        diff_view = self.query_one("#diff-view")
        history = self.query_one("#history-tree")

        try:
            # Check if any shadow branches exist
            branches = await self.api.shadow_git.list_branches()
            if not branches:
                diff_view.clear()
                diff_view.write("[dim]No shadow branches yet.[/]\n\nStart a task to track changes.")
                history.clear()
                history.root.label = "No changes"
                return

            # Get current shadow branch diff
            diff = await self.api.shadow_git.get_diff("shadow/current")
            diff_view.clear()
            if diff.strip():
                diff_view.write(diff)
            else:
                diff_view.write("[dim]No changes on current branch.[/]")

            # Update history (hunks)
            hunks = await self.api.shadow_git.get_hunks("shadow/current")
            history.clear()
            root = history.root
            root.label = f"Current Hunks ({len(hunks)})"
            for hunk in hunks:
                symbol = hunk["symbol"] or "no symbol"
                path = hunk["file_path"]
                label = f"[@click=app.navigate('{path}')]{path}[/]:{hunk['new_start']} ({symbol})"
                root.add_leaf(label)
            root.expand()
        except Exception as e:
            diff_view.clear()
            diff_view.write(f"[red]Error loading diff:[/] {e}")
            history.clear()
            history.root.label = "Error"

    async def _update_task_view(self) -> None:
        """Fetch and display all tasks (unified: sessions, workflows, agents)."""
        try:
            from moss.session import SessionManager, SessionStatus

            manager = SessionManager(self.api.root / ".moss" / "sessions")
            root_tasks = manager.list_root_tasks()

            tree = self.query_one("#session-tree")
            tree.clear()
            root = tree.root
            root.label = f"Tasks ({len(root_tasks)})"

            # Status icons
            status_icon = {
                SessionStatus.CREATED: "○",
                SessionStatus.RUNNING: "●",
                SessionStatus.PAUSED: "◐",
                SessionStatus.COMPLETED: "✓",
                SessionStatus.FAILED: "✗",
                SessionStatus.CANCELLED: "⊘",
            }

            def add_task_node(parent_node: Any, task: Any, indent: int = 0) -> None:
                """Recursively add task and its children to tree."""
                icon = status_icon.get(task.status, "?")
                task_desc = task.task[:40] if task.task else "(no description)"
                if len(task.task) > 40:
                    task_desc += "..."

                # Color based on driver (cyan=user, others=magenta)
                color = "cyan" if task.driver == "user" else "magenta"
                click = f"[@click=app.resume_task('{task.id}')]"
                label = f"{icon} {click}[{color}]{task.id}[/][/]: {task_desc}"

                if task.children:
                    # Has children - add as expandable node
                    node = parent_node.add(label)
                    for child_id in task.children:
                        child = manager.get(child_id)
                        if child:
                            add_task_node(node, child, indent + 1)
                else:
                    # Leaf task
                    parent_node.add_leaf(label)

            for task in root_tasks:
                add_task_node(root, task)

            root.expand()
        except Exception as e:
            self._log(f"Failed to fetch task data: {e}")

    # Backwards compatibility alias
    _update_session_view = _update_task_view

    async def _update_swarm_view(self) -> None:
        """Fetch and display multi-agent swarm status."""
        try:
            # For this TUI we'll mock some swarm status if API doesn't provide it yet
            # In a real implementation, we'd query the Agent Manager
            tree = self.query_one("#swarm-tree")
            tree.clear()
            root = tree.root
            root.label = "Agent Swarm"

            # Placeholder for worker data
            workers = [
                {"id": "worker-1", "status": "IDLE", "task": "None"},
                {"id": "worker-2", "status": "WORKING", "task": "Analyze src/moss/api.py"},
            ]

            for w in workers:
                label = f"{w['id']}: [{w['status']}] {w['task']}"
                root.add_leaf(label)
            root.expand()
        except Exception as e:
            self._log(f"Failed to fetch swarm data: {e}")

    async def on_input_submitted(self, event: Input.Submitted) -> None:
        """Handle command input."""
        command = event.value.strip()
        cmd_input = self.query_one("#command-input")
        cmd_input.value = ""
        cmd_input.display = False
        self.query_one("#project-tree").focus()

        if not command:
            return

        if command == "exit":
            self.exit()
            return

        # In Explore mode, parse and route primitive commands
        if self.current_mode_name == "Explore":
            self._handle_explore_command(command)
        else:
            self._log(f"[{self.current_mode_name}] {command}")

    def _handle_explore_command(self, command: str) -> None:
        """Parse and execute explore mode commands."""
        import shlex

        try:
            parts = shlex.split(command)
        except ValueError:
            parts = command.split()

        if not parts:
            return

        cmd = parts[0].lower()
        args = parts[1:] if len(parts) > 1 else []

        # Support both explicit (view foo) and implicit (foo -> view foo)
        if cmd in ("view", "v"):
            target = args[0] if args else self._selected_path
            if target:
                self._execute_primitive("view", target)
            else:
                self._log("[dim]Usage: view <path>[/]")
        elif cmd in ("edit", "e"):
            if args:
                # Parse edit options: edit <target> --delete, --replace "...", etc.
                target = args[0]
                self._handle_edit_command(target, args[1:])
            else:
                self._log("[dim]Usage: edit <path> [--delete|--replace '...'][/]")
        elif cmd in ("analyze", "a"):
            target = args[0] if args else self._selected_path or "."
            # Parse analyze flags
            flags = {"health": False, "complexity": False, "security": False}
            for arg in args[1:]:
                if arg in ("--health", "-h"):
                    flags["health"] = True
                elif arg in ("--complexity", "-c"):
                    flags["complexity"] = True
                elif arg in ("--security", "-s"):
                    flags["security"] = True
            self._execute_primitive("analyze", target, **flags)
        elif cmd == "cd":
            if not args or args[0] == "..":
                self.action_cd_up()
            else:
                target_path = Path(args[0])
                if not target_path.is_absolute():
                    target_path = self._tree_root / args[0]
                if target_path.is_dir():
                    self.action_cd_to(str(target_path))
                else:
                    self._log(f"[red]Not a directory: {args[0]}[/]")
        elif cmd in ("goto", "g", "jump", "j"):
            # Fuzzy navigation to file
            pattern = args[0] if args else ""
            if pattern:
                self._goto_fuzzy(pattern)
            else:
                self._log("[dim]Usage: goto <pattern>[/]")
        else:
            # Try as implicit view (just a path)
            self._selected_path = command
            self._execute_primitive("view", command)

    def _handle_edit_command(self, target: str, args: list[str]) -> None:
        """Handle edit command with options."""
        from moss.core_api import EditAPI

        explore_detail = self.query_one("#explore-detail")
        explore_detail.clear()

        # Parse edit options
        delete = "--delete" in args or "-d" in args
        dry_run = "--dry-run" in args

        replace_content = None
        for i, arg in enumerate(args):
            if arg in ("--replace", "-r") and i + 1 < len(args):
                replace_content = args[i + 1]
                break

        try:
            api = EditAPI(self.api.root)
            if delete:
                result = api.delete(target, dry_run=dry_run)
            elif replace_content:
                result = api.replace(target, replace_content, dry_run=dry_run)
            else:
                explore_detail.write("[dim]Edit options: --delete, --replace '...'[/]\n")
                explore_detail.write(f"Target: {target}\n")
                return

            explore_detail.write(f"[b]EDIT: {result.target}[/] ({result.operation})\n\n")
            if dry_run:
                explore_detail.write("[yellow]DRY RUN[/]\n\n")
            if result.success:
                explore_detail.write(f"[green]SUCCESS:[/] {result.message}\n")
            else:
                explore_detail.write(f"[red]FAILED:[/] {result.message}\n")
            if result.diff:
                explore_detail.write("\n[b]Diff:[/]\n")
                explore_detail.write(result.diff)

        except Exception as e:
            explore_detail.write(f"[red]Error: {e}[/]")

    def navigate(self, target: str) -> None:
        """Navigate to a specific file or symbol."""
        self._log(f"Navigating to: {target}")
        self.query_one("#command-input").value = f"expand {target}"
        self.query_one("#command-input").focus()
        # In a full implementation, this would also highlight the node in the tree

    def _goto_fuzzy(self, pattern: str) -> None:
        """Fuzzy navigate to a file or symbol using Rust index."""
        from moss.rust_shim import rust_find_symbols

        tree = self.query_one("#project-tree", ProjectTree)

        # Use Rust index for symbol search (works without git)
        symbols = rust_find_symbols(pattern, fuzzy=True, limit=20, root=str(self.api.root))

        if symbols:
            # Group by file, pick best match
            # Prefer: exact name match > function/class > other
            def score_symbol(s: dict) -> tuple:
                name = s.get("name", "").lower()
                kind = s.get("kind", "")
                pat = pattern.lower()
                if name == pat:
                    name_score = 0
                elif name.startswith(pat):
                    name_score = 1
                else:
                    name_score = 2
                kind_score = 0 if kind in ("function", "class", "method") else 1
                return (name_score, kind_score, s.get("file", ""))

            symbols.sort(key=score_symbol)
            best = symbols[0]
            file_path = best.get("file", "")
            line = best.get("line", 1)

            if file_path:
                # Navigate to file and symbol
                full_path = Path(file_path)
                if full_path.is_absolute():
                    rel_path = str(full_path.relative_to(self.api.root))
                else:
                    rel_path = file_path
                    full_path = self.api.root / file_path

                # Expand tree to file
                self._expand_and_select_path(tree, rel_path)

                # Update selection
                self._selected_path = str(full_path)
                self._selected_type = "file"

                # View the symbol (file:line or file/symbol)
                view_target = f"{full_path}:{line}" if line else str(full_path)
                self._execute_primitive("view", view_target)

                # Show alternatives
                if len(symbols) > 1:
                    alts = [f"{s['name']} ({s['kind']})" for s in symbols[1:4]]
                    self._log(f"[dim]Also: {', '.join(alts)}[/]")
                return

        # Fallback: file-only search if no symbols found
        self._log(f"[yellow]No symbols matching '{pattern}'[/]")

    def _expand_and_select_path(self, tree: ProjectTree, rel_path: str) -> None:
        """Expand tree nodes along path and select the final node."""
        parts = rel_path.split("/")
        current_node = tree.root

        # Expand root first
        if not current_node.is_expanded:
            current_node.expand()

        # Traverse each path component
        for i, part in enumerate(parts):
            is_last = i == len(parts) - 1

            # Find child node matching this path part
            found = None
            for child in current_node.children:
                if child.data:
                    child_path = child.data.get("path")
                    if child_path and child_path.name == part:
                        found = child
                        break

            if not found:
                # Node not loaded yet - expand parent to trigger lazy load
                if not current_node.data or not current_node.data.get("loaded"):
                    current_node.expand()
                    # Try again after expansion
                    for child in current_node.children:
                        if child.data:
                            child_path = child.data.get("path")
                            if child_path and child_path.name == part:
                                found = child
                                break

            if found:
                if is_last:
                    # Select the final node
                    tree.select_node(found)
                    found.expand()  # Show symbols if it's a file
                else:
                    # Expand intermediate directory
                    if not found.is_expanded:
                        found.expand()
                    current_node = found
            else:
                self._log(f"[dim]Could not find node: {part}[/]")
                break

    def action_primitive_view(self) -> None:
        """View the currently selected node."""
        if not self._selected_path:
            self._log("[dim]No node selected[/]")
            return
        # Handle markdown headings specially (not supported by ViewAPI)
        if (
            self._selected_type == "symbol"
            and hasattr(self, "_selected_file")
            and str(self._selected_file).endswith(".md")
        ):
            self._view_markdown_section()
        else:
            self._execute_primitive("view", self._selected_path)

    def _view_markdown_section(self) -> None:
        """View a markdown section from heading to next heading."""
        explore_header = self.query_one("#explore-header", Static)
        explore_detail = self.query_one("#explore-detail")
        explore_detail.clear()

        symbol = getattr(self, "_selected_symbol", None)
        file_path = getattr(self, "_selected_file", None)
        if not symbol or not file_path:
            explore_detail.write("[dim](no section data)[/]")
            return

        # For headings with children, show tree structure
        children = getattr(symbol, "children", [])
        if children:
            explore_header.update(f"{symbol.signature} ({len(children)} subsections)")
            # Build tree view of children
            lines = [f"[b]{symbol.name}[/b]"]
            self._format_heading_tree(children, lines, indent=1)
            explore_detail.write("\n".join(lines))
            return

        try:
            content = file_path.read_text()
            lines = content.splitlines()
            start = symbol.lineno - 1  # 0-indexed

            # Use end_lineno from symbol (computed by tree-sitter in Rust)
            end_lineno = getattr(symbol, "end_lineno", None)
            if end_lineno is not None:
                end = end_lineno
            else:
                end = len(lines)

            section = "\n".join(lines[start:end])
            explore_header.update(f"{symbol.signature} ({end - start} lines)")
            # Render markdown properly (handles code blocks, lists, etc.)
            from rich.markdown import Markdown

            md = Markdown(section, code_theme=self._get_syntax_theme())
            explore_detail.write(md)
        except OSError as e:
            explore_detail.write(f"[red]Error: {e}[/]")

    def _format_heading_tree(self, symbols: list, lines: list[str], indent: int) -> None:
        """Format markdown headings as a tree structure."""
        prefix = "  " * indent
        for sym in symbols:
            lines.append(f"{prefix}├─ {sym.name}")
            children = getattr(sym, "children", [])
            if children:
                self._format_heading_tree(children, lines, indent + 1)

    def action_primitive_edit(self) -> None:
        """Edit the currently selected node."""
        if not self._selected_path:
            self._log("[dim]No node selected[/]")
            return
        # Pre-fill command for edit (user needs to specify operation)
        self.query_one("#command-input").value = f"edit {self._selected_path} "
        self.query_one("#command-input").focus()

    def action_primitive_analyze(self) -> None:
        """Analyze the currently selected node."""
        if not self._selected_path:
            self._log("[dim]No node selected[/]")
            return
        self._execute_primitive("analyze", self._selected_path)

    def _format_symbols_skeleton(self, symbols: list, indent: int = 0) -> str:
        """Format symbols list as a skeleton (like CLI output)."""
        lines = []
        prefix = "    " * indent
        for sym in symbols:
            sig = sym.get("signature", sym.get("name", "?"))
            children = sym.get("children", [])
            if children:
                lines.append(f"{prefix}{sig}:")
                lines.append(self._format_symbols_skeleton(children, indent + 1))
            else:
                lines.append(f"{prefix}{sig}")
        return "\n".join(lines)

    def _get_lexer_for_path(self, path: str) -> str | None:
        """Get pygments lexer name for a file path."""
        suffix_map = {
            ".py": "python",
            ".rs": "rust",
            ".js": "javascript",
            ".ts": "typescript",
            ".go": "go",
            ".rb": "ruby",
            ".java": "java",
            ".c": "c",
            ".cpp": "cpp",
            ".h": "c",
            ".hpp": "cpp",
            ".sh": "bash",
            ".yaml": "yaml",
            ".yml": "yaml",
            ".json": "json",
            ".toml": "toml",
            ".md": "markdown",
        }
        for suffix, lexer in suffix_map.items():
            if path.endswith(suffix):
                return lexer
        return None

    def _execute_primitive(self, primitive: str, target: str, **kwargs) -> None:
        """Execute a primitive (view/edit/analyze) and display results."""
        from moss.core_api import AnalyzeAPI, ViewAPI

        explore_header = self.query_one("#explore-header", Static)
        explore_detail = self.query_one("#explore-detail")
        explore_detail.clear()

        try:
            if primitive == "view":
                api = ViewAPI(self.api.root)
                result = api.view(target=target, depth=kwargs.get("depth", 1))

                # Handle failed resolution
                if result.kind == "unknown":
                    explore_header.update(f"Not found: {target}")
                    explore_detail.write(f"[yellow]Could not resolve: {target}[/]\n")
                    explore_detail.write("[dim]Check path or try 'view .'[/]")
                    return

                explore_header.update(f"VIEW: {result.target} ({result.kind})")

                # Format content based on kind
                if result.kind == "directory":
                    files = result.content.get("files", [])
                    explore_detail.write(f"Files: {len(files)}\n")
                    for f in files[:20]:
                        explore_detail.write(f"  {f}\n")
                    if len(files) > 20:
                        explore_detail.write(f"  ... and {len(files) - 20} more\n")
                elif result.kind == "file":
                    # Format symbols as skeleton with syntax highlighting
                    symbols = result.content.get("symbols", [])
                    line_count = result.content.get("line_count", "?")
                    explore_header.update(f"{result.target} ({line_count} lines)")
                    if symbols:
                        skeleton = self._format_symbols_skeleton(symbols)
                        lexer = self._get_lexer_for_path(target)
                        if lexer:
                            from rich.syntax import Syntax

                            syntax = Syntax(
                                skeleton,
                                lexer,
                                theme=self._get_syntax_theme(),
                                background_color=self._get_syntax_bg(),
                            )
                            explore_detail.write(syntax)
                        else:
                            explore_detail.write(skeleton)
                    else:
                        # No symbols - show the full file content (limited for large data files)
                        from pathlib import Path

                        # Data files and lockfiles can be huge - limit preview
                        data_exts = (".json", ".yaml", ".yml", ".toml", ".lock", ".lockb")
                        is_data_file = any(target.endswith(ext) for ext in data_exts)
                        max_lines = 50 if is_data_file else 500

                        try:
                            file_path = Path(target)
                            content = file_path.read_text(errors="replace")
                            lines = content.splitlines()
                            truncated = len(lines) > max_lines
                            if truncated:
                                content = "\n".join(lines[:max_lines])

                            lexer = self._get_lexer_for_path(target)
                            if lexer:
                                from rich.syntax import Syntax

                                syntax = Syntax(
                                    content,
                                    lexer,
                                    theme=self._get_syntax_theme(),
                                    background_color=self._get_syntax_bg(),
                                    line_numbers=True,
                                )
                                explore_detail.write(syntax)
                            else:
                                explore_detail.write(content)

                            if truncated:
                                explore_detail.write(
                                    f"\n[dim]... truncated ({len(lines) - max_lines} more lines)[/]"
                                )
                        except (OSError, UnicodeDecodeError):
                            explore_detail.write("[dim](unable to read file)[/]")
                else:  # symbol
                    source = result.content.get("source", "")
                    if source:
                        # Syntax highlight the source
                        # Get file path from target (e.g., src/foo.py/Bar -> src/foo.py)
                        file_path = target.rsplit("/", 1)[0] if "/" in target else target
                        lexer = self._get_lexer_for_path(file_path)
                        if lexer:
                            from rich.syntax import Syntax

                            syntax = Syntax(
                                source,
                                lexer,
                                theme=self._get_syntax_theme(),
                                background_color=self._get_syntax_bg(),
                            )
                            explore_detail.write(syntax)
                        else:
                            explore_detail.write(source)
                    else:
                        sig = result.content.get("signature", "")
                        if sig:
                            explore_detail.write(sig)
                        else:
                            explore_detail.write("[dim](no source available)[/]")

            elif primitive == "analyze":
                api = AnalyzeAPI(self.api.root)
                result = api.analyze(
                    target=target,
                    health=kwargs.get("health", False),
                    complexity=kwargs.get("complexity", False),
                    security=kwargs.get("security", False),
                )
                explore_header.update(f"ANALYZE: {result.target}")

                if result.health:
                    explore_detail.write("[b]Health:[/]\n")
                    for k, v in result.health.items():
                        explore_detail.write(f"  {k}: {v}\n")
                if result.complexity:
                    explore_detail.write("\n[b]Complexity:[/]\n")
                    funcs = result.complexity.get("functions", [])
                    for f in funcs[:10]:
                        name = f.get("name", "?")
                        score = f.get("complexity", 0)
                        explore_detail.write(f"  {name}: {score}\n")
                    if len(funcs) > 10:
                        explore_detail.write(f"  ... and {len(funcs) - 10} more\n")
                if result.security:
                    findings = result.security.get("findings", [])
                    explore_detail.write(f"\n[b]Security:[/] {len(findings)} findings\n")
                    for f in findings[:5]:
                        sev = f.get("severity", "?")
                        msg = f.get("message", "")
                        explore_detail.write(f"  [[{sev}]] {msg}\n")

        except RuntimeError as e:
            # Rust CLI not available or failed to start
            explore_header.update(f"Error: {primitive}")
            explore_detail.write(f"[red]Rust CLI error: {e}[/]\n")
            explore_detail.write("[dim]Ensure moss is built: cargo build --release[/]")
        except (OSError, FileNotFoundError) as e:
            explore_header.update(f"Error: {primitive}")
            explore_detail.write(f"[red]File error: {e}[/]")
        except Exception as e:
            explore_header.update(f"Error: {primitive}")
            explore_detail.write(f"[red]{type(e).__name__}: {e}[/]")

    def _log(self, message: str) -> None:
        """Add a message to the log view."""
        log_view = self.query_one("#log-view")
        # Simple heuristic to make paths clickable
        pattern = r"([a-zA-Z0-9_\-\./]+\.[a-z]{2,4}(?::\d+)?)"
        linked_message = re.sub(pattern, r"[@click=app.navigate('\1')]\1[/]", message)

        log_view.mount(Static(linked_message, classes="log-entry", markup=True))
        log_view.scroll_end()


def run_tui(api: MossAPI) -> None:
    """Run the Moss TUI."""
    try:
        from textual.app import App as _App  # noqa: F401
    except ImportError:
        print("Error: textual not installed. Install with: pip install 'moss[tui]'")
        return

    app = MossTUI(api)
    app.run()
