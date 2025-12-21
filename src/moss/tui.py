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
    from textual.containers import Container, Horizontal, Vertical
    from textual.reactive import reactive
    from textual.widgets import Footer, Header, Input, Static, Tree
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


class ModeRegistry:
    """Registry for extensible TUI modes."""

    def __init__(self):
        self._modes: dict[str, TUIMode] = {
            "PLAN": PlanMode(),
            "READ": ReadMode(),
            "WRITE": WriteMode(),
            "DIFF": DiffMode(),
            "SESSION": SessionMode(),
            "BRANCH": BranchMode(),
            "SWARM": SwarmMode(),
            "COMMIT": CommitMode(),
        }
        self._order: list[str] = [
            "PLAN",
            "READ",
            "WRITE",
            "DIFF",
            "SESSION",
            "BRANCH",
            "SWARM",
            "COMMIT",
        ]

    def get_mode(self, name: str) -> TUIMode | None:
        return self._modes.get(name)

    def next_mode(self, current_name: str) -> TUIMode:
        idx = self._order.index(current_name)
        next_idx = (idx + 1) % len(self._order)
        return self._modes[self._order[next_idx]]

    def register_mode(self, mode: TUIMode) -> None:
        self._modes[mode.name] = mode
        if mode.name not in self._order:
            self._order.append(mode.name)


class ModeIndicator(Static):
    """Widget to display the current agent mode."""

    mode_name = reactive("PLAN")
    mode_color = reactive("blue")

    def render(self) -> str:
        return f"Mode: [{self.mode_color} b]{self.mode_name}[/]"


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

    def update_from_files(self, api: MossAPI) -> None:
        self.clear()
        root = self.root
        root.label = f"[b]Files: {api.root.name}[/b]"

        # Symbol kind icons
        kind_icons = {
            "class": "📦",
            "function": "⚡",
            "method": "🔧",
            "variable": "📌",
        }

        def add_symbols(tree_node: TreeNode[Any], symbols: list, path: Path) -> None:
            """Add symbol nodes as children of a file node."""
            for symbol in symbols:
                icon = kind_icons.get(symbol.kind, "•")
                label = f"{icon} {symbol.name}"
                sym_node = tree_node.add(
                    label,
                    data={"type": "symbol", "symbol": symbol, "path": path},
                )
                # Add nested symbols (class methods, nested functions)
                if symbol.children:
                    add_symbols(sym_node, symbol.children, path)

        # Simple recursive file tree
        import os

        def add_dir(tree_node: TreeNode[Any], path: Path):
            try:
                # Limit depth/count for performance
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
                        file_node = tree_node.add(
                            f"📄 {entry}", data={"type": "file", "path": full_path}
                        )
                        # Add symbols for Python files
                        if entry.endswith(".py"):
                            try:
                                symbols = api.skeleton.extract(full_path)
                                add_symbols(file_node, symbols, full_path)
                            except (OSError, ValueError, SyntaxError):
                                pass
            except OSError:
                pass

        add_dir(root, api.root)
        root.expand()


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

    #content-area {
        width: 70%;
        height: 1fr;
        padding: 1;
    }

    #command-input {
        dock: bottom;
        margin: 1;
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

    #diff-view {
        height: 1fr;
        border: solid $secondary;
    }

    #history-tree {
        height: 30%;
        border: solid $secondary;
    }

    ModeIndicator {
        background: $surface-lighten-1;
        padding: 0 1;
        text-align: center;
        border: round $primary;
        margin: 0 1;
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
    """

    BINDINGS: ClassVar[list[tuple[str, str, str]]] = [
        ("ctrl+c", "handle_ctrl_c", "Quit"),
        ("d", "toggle_dark", "Toggle Dark Mode"),
        ("shift+tab", "next_mode", "Next Mode"),
        ("h", "toggle_tooltip", "Toggle Tooltip"),
    ]

    current_mode_name = reactive("PLAN")

    def __init__(self, api: MossAPI):
        super().__init__()
        self.api = api
        self._task_tree: TaskTree | None = None
        self._registry = ModeRegistry()
        self._last_ctrl_c: float = 0

    def action_handle_ctrl_c(self) -> None:
        """Handle Ctrl+C with double-tap to exit."""
        import time

        now = time.time()
        if now - self._last_ctrl_c < 0.5:
            self.exit()
        else:
            self._last_ctrl_c = now
            self._log("Press Ctrl+C again to exit")

    def compose(self) -> ComposeResult:
        """Create child widgets for the app."""
        from textual.widgets import RichLog

        yield Header(show_clock=True)
        yield Horizontal(ModeIndicator(id="mode-indicator"), id="header-bar", height="auto")
        yield Container(
            Horizontal(
                Vertical(
                    Static("Navigation", id="sidebar-header"),
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
                    id="content-area",
                ),
                id="main-container",
            ),
            Input(placeholder="Enter command...", id="command-input"),
            HoverTooltip(id="hover-tooltip"),
        )
        yield Footer()

    def on_mount(self) -> None:
        """Called when the app is mounted."""
        self.title = "Moss TUI"
        self.sub_title = f"Project: {self.api.root.name}"
        self.query_one("#command-input").focus()
        # Initialize first mode
        self.current_mode_name = "PLAN"

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
                bd_str = f" [[dim]{', '.join(bd_parts)}[/]]"

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
        mode = self._registry.get_mode(name)
        if not mode:
            return

        indicator = self.query_one("#mode-indicator")
        indicator.mode_name = mode.name
        indicator.mode_color = mode.color

        self.query_one("#command-input").placeholder = mode.placeholder

        # Reset all views before entering new mode
        self.query_one("#log-view").display = False
        self.query_one("#git-view").display = False
        self.query_one("#session-view").display = False
        self.query_one("#swarm-view").display = False

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
        self.query_one("#command-input").value = f"branch {branch_name}"
        self.query_one("#command-input").focus()

    def _update_tree(self, tree_type: str = "task") -> None:
        """Update the sidebar tree."""
        tree = self.query_one("#project-tree", ProjectTree)
        if tree_type == "task" and self._task_tree:
            tree.update_from_tasks(self._task_tree)
        else:
            tree.update_from_files(self.api)

    def on_tree_node_highlighted(self, event: Tree.NodeHighlighted) -> None:
        """Handle node highlight (hover/selection movement)."""
        data = event.node.data
        if not data:
            return

        tooltip = self.query_one("#hover-tooltip", HoverTooltip)

        if data["type"] == "file":
            path = data["path"]
            tooltip.file_path = path  # Enable syntax highlighting
            # Show file skeleton summary in tooltip
            try:
                skeleton = self.api.skeleton.format(path)
                # Take first few lines of skeleton
                summary = "\n".join(skeleton.split("\n")[:15])
                if len(skeleton.split("\n")) > 15:
                    summary += "\n..."
                tooltip.content = summary
            except (OSError, ValueError):
                tooltip.content = f"File: {path.name}"
        elif data["type"] == "symbol":
            symbol = data["symbol"]
            path = data["path"]
            tooltip.file_path = path  # Enable syntax highlighting for signature
            # Build symbol info display
            lines = [symbol.signature]
            if symbol.lineno:
                lines[0] += f"  # line {symbol.lineno}"
            if symbol.docstring:
                # Show first few lines of docstring
                doc_lines = symbol.docstring.strip().split("\n")[:5]
                if len(doc_lines) < len(symbol.docstring.strip().split("\n")):
                    doc_lines.append("...")
                lines.append("")
                lines.extend(f'"""{line}' if i == 0 else line for i, line in enumerate(doc_lines))
                if not lines[-1].endswith('"""'):
                    lines.append('"""')
            tooltip.content = "\n".join(lines)
        elif data["type"] == "task":
            tooltip.file_path = None  # No syntax highlighting for tasks
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
            self._log(f"Opened file: {path.name}")
            # Automatically run skeleton command for selected file
            self.query_one("#command-input").value = f"skeleton {path}"
            # Focus input so user can press enter
            self.query_one("#command-input").focus()
        elif data["type"] == "symbol":
            symbol = data["symbol"]
            path = data["path"]
            self._log(f"Symbol: {symbol.name} at {path.name}:{symbol.lineno}")
            # Pre-fill with file:line for editor navigation
            self.query_one("#command-input").value = f"{path}:{symbol.lineno}"
            self.query_one("#command-input").focus()

    def action_toggle_tooltip(self) -> None:
        """Toggle tooltip visibility."""
        tooltip = self.query_one("#hover-tooltip")
        tooltip.display = not tooltip.display

    def action_next_mode(self) -> None:
        """Switch to the next mode."""
        next_mode = self._registry.next_mode(self.current_mode_name)
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
        if not command:
            return

        self.query_one("#command-input").value = ""
        self._log(f"[{self.current_mode_name}] {command}")

        # TODO: Integrate with AgentLoop or DWIM
        if command == "exit":
            self.exit()

    def navigate(self, target: str) -> None:
        """Navigate to a specific file or symbol."""
        self._log(f"Navigating to: {target}")
        self.query_one("#command-input").value = f"expand {target}"
        self.query_one("#command-input").focus()
        # In a full implementation, this would also highlight the node in the tree

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
