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
    """Protocol for TUI operating modes."""

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

    async def on_enter(self, app: MossTUI) -> None:
        """Called when entering this mode."""
        ...


class PlanMode:
    name = "PLAN"
    color = "blue"
    placeholder = "What is the plan? (e.g. breakdown...)"

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = True
        app.query_one("#git-view").display = False
        app.query_one("#content-header").update("Agent Log")
        app._update_tree("task")


class ReadMode:
    name = "READ"
    color = "green"
    placeholder = "Explore codebase... (e.g. skeleton, grep, expand)"

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = True
        app.query_one("#git-view").display = False
        app.query_one("#content-header").update("Agent Log")
        app._update_tree("file")


class WriteMode:
    name = "WRITE"
    color = "red"
    placeholder = "Modify code... (e.g. write, replace, insert)"

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = True
        app.query_one("#git-view").display = False
        app.query_one("#content-header").update("Agent Log")
        app._update_tree("file")


class DiffMode:
    name = "DIFF"
    color = "magenta"
    placeholder = "Review changes... (revert <file> <line> to undo)"

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = False
        app.query_one("#git-view").display = True
        app.query_one("#content-header").update("Shadow Git")
        await app._update_git_view()
        app._update_tree("task")


class SessionMode:
    name = "SESSION"
    color = "yellow"
    placeholder = "Manage sessions... (resume <id> to continue)"

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = False
        app.query_one("#git-view").display = False
        app.query_one("#session-view").display = True
        app.query_one("#content-header").update("Sessions")
        await app._update_session_view()


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
    name = "BRANCH"
    color = "cyan"
    placeholder = "Manage branches... (branch <name> to switch)"

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = False
        app.query_one("#git-view").display = True
        app.query_one("#session-view").display = False
        app.query_one("#content-header").update("Git Dashboard")
        await app._update_branch_view()


class SwarmMode:
    name = "SWARM"
    color = "white"
    placeholder = "Manage swarm... (wait for workers to complete)"

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = False
        app.query_one("#git-view").display = False
        app.query_one("#session-view").display = False
        app.query_one("#swarm-view").display = True
        app.query_one("#content-header").update("Swarm Dashboard")
        await app._update_swarm_view()


class CommitMode:
    name = "COMMIT"
    color = "green"
    placeholder = "Review commit actions... (select a hunk to view)"

    async def on_enter(self, app: MossTUI) -> None:
        app.query_one("#log-view").display = False
        app.query_one("#git-view").display = True
        app.query_one("#session-view").display = False
        app.query_one("#swarm-view").display = False
        app.query_one("#content-header").update("Commit Dashboard")
        await app._update_git_view()


class ExploreMode:
    """Unified exploration mode using tree + three primitives (view/edit/analyze)."""

    name = "EXPLORE"
    color = "cyan"
    placeholder = ""

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
        SessionMode,
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

    mode_name = reactive("PLAN")
    mode_color = reactive("blue")

    def render(self) -> str:
        return f"Mode: [{self.mode_color} b]{self.mode_name}[/]"


class KeybindBar(Static):
    """Footer showing keybindings with underlined hotkey letters."""

    DEFAULT_CSS = """
    KeybindBar {
        dock: bottom;
        width: 100%;
        height: 1;
        background: $surface-darken-1;
    }
    """

    def render(self) -> str:
        binds = [
            "[u]Q[/u]uit",
            "[u]T[/u]heme",
            "[u]V[/u]iew",
            "[u]E[/u]dit",
            "[u]A[/u]nalyze",
            "[dim]-[/dim]Up",
            "[dim]/[/dim]Cmd",
        ]
        return "  ".join(binds)


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

    def update_from_files(self, api: MossAPI, tree_root: Path | None = None) -> None:
        """Update tree from filesystem.

        Args:
            api: The MossAPI instance
            tree_root: Root directory to show (defaults to api.root)
        """
        self._api = api
        self.clear()
        root = self.root
        tree_root = tree_root or api.root
        root.label = f"[b]Files: {tree_root.name}[/b]"

        import os

        def add_dir(tree_node: TreeNode[Any], path: Path):
            try:
                entries = sorted(os.listdir(path))
                for entry in entries:
                    if entry.startswith(".") and entry != ".moss":
                        continue

                    full_path = path / entry
                    if full_path.is_dir():
                        if entry in ("__pycache__", "node_modules", "target", ".git"):
                            continue
                        node = tree_node.add(f"📁 {entry}", data={"type": "dir", "path": full_path})
                        add_dir(node, full_path)
                    else:
                        file_data = {"type": "file", "path": full_path, "loaded": False}
                        # Python and markdown files are expandable (symbols loaded on expand)
                        if entry.endswith(".py") or entry.endswith(".md"):
                            tree_node.add(f"📄 {entry}", data=file_data)
                        else:
                            # Add padding to align with expandable items (▶ is 2 chars)
                            tree_node.add_leaf(f"  📄 {entry}", data=file_data)
            except OSError:
                pass

        add_dir(root, tree_root)
        root.expand()

    def on_tree_node_expanded(self, event: Tree.NodeExpanded) -> None:
        """Load symbols lazily when a file node is expanded."""
        data = event.node.data
        if not data or data.get("type") != "file" or data.get("loaded"):
            return

        path = data["path"]
        data["loaded"] = True

        # Symbol kind icons
        kind_icons = {
            "class": "📦",
            "function": "⚡",
            "method": "🔧",
            "variable": "📌",
            "heading": "📑",
        }

        def add_symbols(tree_node: TreeNode[Any], symbols: list, path: Path) -> None:
            for symbol in symbols:
                icon = kind_icons.get(symbol.kind, "•")
                label = f"{icon} {symbol.name}"
                sym_data = {"type": "symbol", "symbol": symbol, "path": path}
                if symbol.children:
                    sym_node = tree_node.add(label, data=sym_data)
                    add_symbols(sym_node, symbol.children, path)
                else:
                    # Add padding to align with expandable items
                    tree_node.add_leaf(f"  {label}", data=sym_data)

        if self._api and str(path).endswith(".py"):
            try:
                symbols = self._api.skeleton.extract(path)
                if symbols:
                    add_symbols(event.node, symbols, path)
            except (OSError, ValueError, SyntaxError):
                pass
        elif str(path).endswith(".md"):
            # Extract markdown headings
            try:
                headings = self._extract_markdown_headings(path)
                add_symbols(event.node, headings, path)
            except OSError:
                pass

    def _extract_markdown_headings(self, path: Path) -> list:
        """Extract headings from markdown file as pseudo-symbols."""
        import re
        from dataclasses import dataclass

        @dataclass
        class HeadingSymbol:
            name: str
            kind: str = "heading"
            signature: str = ""
            docstring: str | None = None
            lineno: int = 0
            children: list = None

            def __post_init__(self):
                if self.children is None:
                    self.children = []

        headings = []
        content = path.read_text()
        for i, line in enumerate(content.splitlines(), 1):
            match = re.match(r"^(#{1,6})\s+(.+)$", line)
            if match:
                level = len(match.group(1))
                title = match.group(2).strip()
                headings.append(
                    HeadingSymbol(
                        name=title,
                        signature=f"{'#' * level} {title}",
                        lineno=i,
                    )
                )
        return headings


class MossTUI(App):
    """The main Moss TUI application."""

    CSS = """
    Screen {
        background: $surface;
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

    #breadcrumb {
        height: auto;
        padding: 0 1;
        background: $surface-darken-2;
    }

    #content-area {
        width: 70%;
        height: 1fr;
        padding: 1;
    }

    #command-input {
        dock: bottom;
        display: none;
        height: 1;
        margin: 0 1;
        padding: 0;
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
    """

    BINDINGS: ClassVar[list[Binding]] = [
        Binding("q", "quit", "Quit", show=False),
        Binding("ctrl+c", "handle_ctrl_c", "Interrupt", show=False),
        Binding("t", "app.toggle_dark", "Theme", show=False),
        Binding("tab", "next_mode", "Mode", show=False),
        Binding("v", "primitive_view", "View", show=False),
        Binding("e", "primitive_edit", "Edit", show=False),
        Binding("a", "primitive_analyze", "Analyze", show=False),
        Binding("minus", "cd_up", "Up", show=False),
        Binding("enter", "enter_dir", "Enter", show=False),
        Binding("slash", "toggle_command", "Cmd", show=False),
        Binding("escape", "hide_command", show=False),
    ]

    current_mode_name = reactive("EXPLORE")

    def __init__(self, api: MossAPI):
        super().__init__()
        self.api = api
        self._task_tree: TaskTree | None = None
        self._mode_registry = ModeRegistry()
        self._last_ctrl_c: float = 0
        self._tree_root: Path = api.root  # Current root for file tree
        self._transparent_bg: bool = False  # For terminal opacity support

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
        """Get syntax background color (None for transparent)."""
        if getattr(self, "_transparent_bg", False):
            return None
        return "default"

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
            Input(placeholder="Enter command...", id="command-input"),
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
        # Load theme preference
        self._load_settings()

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

    async def _update_branch_view(self) -> None:
        """Fetch and display all shadow branches."""
        try:
            branches = await self.api.shadow_git.list_branches()
            tree = self.query_one("#history-tree")
            tree.clear()
            root = tree.root
            root.label = f"Shadow Branches ({len(branches)})"

            for b in branches:
                label = f"[@click=app.navigate_branch('{b}')]{b}[/]"
                root.add_leaf(label)
            root.expand()

            # Show current diff in diff-view
            diff = await self.api.shadow_git.get_diff("shadow/current")
            self.query_one("#diff-view").clear()
            self.query_one("#diff-view").write(diff)
        except Exception as e:
            self._log(f"Failed to fetch branch data: {e}")

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

    def cd_to(self, path: str) -> None:
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

    def cd_root(self) -> None:
        """Navigate back to project root."""
        if self._tree_root != self.api.root:
            self._tree_root = self.api.root
            self._update_tree("file")
            self._log("Changed to project root")

    def action_enter_dir(self) -> None:
        """Enter selected directory (navigate into it)."""
        if self._selected_type == "dir" and self._selected_path:
            self.cd_to(self._selected_path)

    def action_toggle_command(self) -> None:
        """Toggle command input visibility."""
        cmd_input = self.query_one("#command-input")
        if cmd_input.display:
            cmd_input.display = False
            self.query_one("#project-tree").focus()
        else:
            cmd_input.display = True
            cmd_input.focus()

    def action_hide_command(self) -> None:
        """Hide command input (Escape)."""
        cmd_input = self.query_one("#command-input")
        if cmd_input.display:
            cmd_input.display = False
            cmd_input.value = ""
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

    def on_tree_node_selected(self, event: Tree.NodeSelected) -> None:
        """Handle tree node selection (click/enter)."""
        data = event.node.data
        if not data:
            return

        if data["type"] == "file":
            path = data["path"]
            self._selected_path = str(path)
            self._selected_type = "file"
            # In EXPLORE mode, double-click triggers view
            if self.current_mode_name == "EXPLORE":
                self.action_primitive_view()
            else:
                self._log(f"Opened file: {path.name}")
                cmd = self.query_one("#command-input")
                cmd.value = f"view {path}"
                cmd.display = True
                cmd.focus()
        elif data["type"] == "dir":
            path = data["path"]
            self._selected_path = str(path)
            self._selected_type = "dir"
            # Just select - use Enter again or cd command to navigate
            if self.current_mode_name == "EXPLORE":
                self.action_primitive_view()
        elif data["type"] == "symbol":
            symbol = data["symbol"]
            path = data["path"]
            symbol_path = f"{path}/{symbol.name}"
            self._selected_path = symbol_path
            self._selected_type = "symbol"
            if self.current_mode_name == "EXPLORE":
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

    async def _update_git_view(self) -> None:
        """Fetch and display shadow git data."""
        try:
            # Get current shadow branch diff
            # In a real TUI we'd track the current branch
            diff = await self.api.shadow_git.get_diff("shadow/current")
            diff_view = self.query_one("#diff-view")
            diff_view.clear()
            diff_view.write(diff)

            # Update history (hunks)
            hunks = await self.api.shadow_git.get_hunks("shadow/current")
            history = self.query_one("#history-tree")
            history.clear()
            root = history.root
            root.label = "Current Hunks"
            for hunk in hunks:
                symbol = hunk["symbol"] or "no symbol"
                path = hunk["file_path"]
                label = f"[@click=app.navigate('{path}')]{path}[/]:{hunk['new_start']} ({symbol})"
                root.add_leaf(label)
            root.expand()
        except Exception as e:
            self._log(f"Failed to fetch git data: {e}")

    async def _update_session_view(self) -> None:
        """Fetch and display past sessions."""
        try:
            # For this TUI we want the full list, so let's use SessionManager directly
            from moss.session import SessionManager

            manager = SessionManager(self.api.root / ".moss" / "sessions")
            sessions = manager.list_sessions()

            tree = self.query_one("#session-tree")
            tree.clear()
            root = tree.root
            root.label = f"Sessions ({len(sessions)})"

            for s in sessions:
                label = f"[@click=app.navigate('{s.id}')]{s.id}[/]: {s.task[:50]}"
                if len(s.task) > 50:
                    label += "..."
                root.add_leaf(label)
            root.expand()
        except Exception as e:
            self._log(f"Failed to fetch session data: {e}")

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

        # In EXPLORE mode, parse and route primitive commands
        if self.current_mode_name == "EXPLORE":
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
                    self.cd_to(str(target_path))
                else:
                    self._log(f"[red]Not a directory: {args[0]}[/]")
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

    def action_primitive_view(self) -> None:
        """View the currently selected node."""
        if not self._selected_path:
            self._log("[dim]No node selected[/]")
            return
        self._execute_primitive("view", self._selected_path)

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
                        explore_detail.write("[dim](no symbols)[/]")
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

        except Exception as e:
            explore_header.update(f"Error: {primitive}")
            explore_detail.write(f"[red]{e}[/]")

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
