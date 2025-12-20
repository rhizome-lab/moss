"""TUI Interface: Interactive terminal UI for Moss.

Uses Textual for a modern, reactive terminal experience.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, ClassVar

try:
    from textual.app import App, ComposeResult
    from textual.containers import Container, Horizontal, Vertical
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


class TaskTreeWidget(Tree[str]):
    """Widget for visualizing the task tree."""

    def update_from_tree(self, task_tree: TaskTree) -> None:
        """Update the widget content from a TaskTree instance."""
        self.clear()
        root = self.root
        root.label = task_tree.root.goal
        self._add_node(root, task_tree.root)
        root.expand()

    def _add_node(self, tree_node: TreeNode[str], task_node: TaskNode) -> None:
        """Recursively add nodes to the tree widget."""
        for child in task_node.children:
            status_icon = "✓" if child.status.name == "DONE" else "→"
            label = f"{status_icon} {child.goal}"
            if child.summary:
                label += f" ({child.summary})"

            new_node = tree_node.add(label, expand=True)
            self._add_node(new_node, child)


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
    """

    BINDINGS: ClassVar[list[tuple[str, str, str]]] = [
        ("q", "quit", "Quit"),
        ("d", "toggle_dark", "Toggle Dark Mode"),
    ]

    def __init__(self, api: MossAPI):
        super().__init__()
        self.api = api
        self._task_tree: TaskTree | None = None

    def compose(self) -> ComposeResult:
        """Create child widgets for the app."""
        yield Header(show_clock=True)
        yield Container(
            Horizontal(
                Vertical(
                    Static("Task Tree", classes="sidebar-header"),
                    TaskTreeWidget("Tasks", id="task-tree"),
                    id="sidebar",
                ),
                Vertical(
                    Static("Agent Log", classes="content-header"),
                    Container(id="log-view"),
                    id="content-area",
                ),
                id="main-container",
            ),
            Input(placeholder="Enter command...", id="command-input"),
        )
        yield Footer()

    def on_mount(self) -> None:
        """Called when the app is mounted."""
        self.title = "Moss TUI"
        self.sub_title = f"Project: {self.api.root.name}"
        self.query_one("#command-input").focus()

    async def on_input_submitted(self, event: Input.Submitted) -> None:
        """Handle command input."""
        command = event.value.strip()
        if not command:
            return

        self.query_one("#command-input").value = ""
        self._log(f"Executing: {command}")

        # TODO: Integrate with AgentLoop or DWIM
        if command == "exit":
            self.exit()

    def _log(self, message: str) -> None:
        """Add a message to the log view."""
        log_view = self.query_one("#log-view")
        log_view.mount(Static(message, classes="log-entry"))
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
