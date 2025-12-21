"""TUI Interface: Interactive terminal UI for Moss.

Uses Textual for a modern, reactive terminal experience.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, ClassVar, Protocol, runtime_checkable

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


class ModeRegistry:
    """Registry for extensible TUI modes."""

    def __init__(self):
        self._modes: dict[str, TUIMode] = {
            "PLAN": PlanMode(),
            "READ": ReadMode(),
            "WRITE": WriteMode(),
            "DIFF": DiffMode(),
        }
        self._order: list[str] = ["PLAN", "READ", "WRITE", "DIFF"]

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

    def render(self) -> str:
        if not self.content:
            return ""
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

        # Simple recursive file tree
        import os
        from pathlib import Path

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
                        tree_node.add_leaf(f"📄 {entry}", data={"type": "file", "path": full_path})
            except Exception:
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
        ("q", "quit", "Quit"),
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

    async def watch_current_mode_name(self, name: str) -> None:
        """React to mode changes."""
        mode = self._registry.get_mode(name)
        if not mode:
            return

        indicator = self.query_one("#mode-indicator")
        indicator.mode_name = mode.name
        indicator.mode_color = mode.color

        self.query_one("#command-input").placeholder = mode.placeholder

        await mode.on_enter(self)

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
            # Show file skeleton summary in tooltip
            try:
                skeleton = self.api.skeleton.format(path)
                # Take first few lines of skeleton
                summary = "\n".join(skeleton.split("\n")[:15])
                if len(skeleton.split("\n")) > 15:
                    summary += "\n..."
                tooltip.content = summary
            except Exception:
                tooltip.content = f"File: {path.name}"
        elif data["type"] == "task":
            node = data["node"]
            tooltip.content = f"Goal: {node.goal}\nStatus: {node.status.name}"
            if node.summary:
                tooltip.content += f"\nSummary: {node.summary}"
        else:
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
        import re

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
