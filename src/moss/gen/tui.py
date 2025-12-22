"""TUI generator from MossAPI introspection.

This module generates a Textual-based terminal UI from the MossAPI structure.
The TUI provides an interactive interface to explore and execute API methods.

Usage:
    # Generate and run the TUI
    from moss.gen.tui import run_tui
    run_tui()

    # Or via CLI
    moss tui

Features:
    - Tree navigation of all sub-APIs and methods
    - Parameter input forms for method execution
    - Result display with syntax highlighting
    - Command palette for quick access
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from moss.gen.introspect import APIMethod, SubAPI, introspect_api


@dataclass
class TUIMethod:
    """A method displayed in the TUI.

    Attributes:
        name: Method name
        description: Method description
        api_path: Full path (e.g., "skeleton.extract")
        parameters: List of parameter definitions
    """

    name: str
    description: str
    api_path: str
    parameters: list[TUIParameter] = field(default_factory=list)


@dataclass
class TUIParameter:
    """A parameter input field in the TUI.

    Attributes:
        name: Parameter name
        type_hint: Type string
        required: Whether required
        default: Default value
        description: Help text
    """

    name: str
    type_hint: str
    required: bool = True
    default: Any = None
    description: str = ""


@dataclass
class TUIGroup:
    """A group of methods (sub-API) in the TUI.

    Attributes:
        name: Group name (e.g., "skeleton")
        description: Group description
        methods: List of methods
    """

    name: str
    description: str
    methods: list[TUIMethod] = field(default_factory=list)


def method_to_tui(method: APIMethod, api_name: str) -> TUIMethod:
    """Convert an API method to a TUI method definition."""
    parameters = [
        TUIParameter(
            name=p.name,
            type_hint=p.type_hint,
            required=p.required,
            default=p.default,
            description=p.description,
        )
        for p in method.parameters
    ]

    return TUIMethod(
        name=method.name,
        description=method.description,
        api_path=f"{api_name}.{method.name}",
        parameters=parameters,
    )


def subapi_to_group(subapi: SubAPI) -> TUIGroup:
    """Convert a sub-API to a TUI group."""
    methods = [method_to_tui(m, subapi.name) for m in subapi.methods]

    return TUIGroup(
        name=subapi.name,
        description=subapi.description,
        methods=methods,
    )


class TUIGenerator:
    """Generator for Textual TUI from MossAPI.

    Usage:
        generator = TUIGenerator()
        groups = generator.generate_groups()
        app = generator.generate_app()
        app.run()
    """

    def __init__(self):
        """Initialize the generator."""
        self._groups: list[TUIGroup] | None = None

    def generate_groups(self) -> list[TUIGroup]:
        """Generate TUI groups from MossAPI introspection."""
        if self._groups is None:
            sub_apis = introspect_api()
            self._groups = [subapi_to_group(api) for api in sub_apis]
        return self._groups

    def generate_app(self, root: str | Path = ".") -> Any:
        """Generate a Textual application.

        Args:
            root: Project root directory

        Returns:
            Textual App instance

        Raises:
            ImportError: If textual is not installed
        """
        try:
            from textual.app import App, ComposeResult
            from textual.binding import Binding
            from textual.containers import Container, Horizontal, Vertical
            from textual.widgets import (
                Button,
                Footer,
                Header,
                Input,
                Label,
                RichLog,
                Static,
                Tree,
            )
        except ImportError as e:
            raise ImportError(
                "Textual is required for TUI. Install with: pip install 'moss[tui]'"
            ) from e

        import json

        from moss.gen.http import HTTPExecutor

        groups = self.generate_groups()
        root_path = Path(root).resolve()
        executor = HTTPExecutor(root_path)

        from typing import ClassVar

        class MossApp(App):
            """Moss TUI Application."""

            CSS: ClassVar[str] = """
            #main-container {
                layout: horizontal;
            }

            #sidebar {
                width: 30;
                border: solid $primary;
                padding: 1;
            }

            #content {
                width: 1fr;
            }

            #method-info {
                height: auto;
                max-height: 10;
                border: solid $secondary;
                padding: 1;
                margin-bottom: 1;
            }

            #params-container {
                height: auto;
                border: solid $secondary;
                padding: 1;
                margin-bottom: 1;
            }

            #result-container {
                height: 1fr;
                border: solid $success;
                padding: 1;
            }

            .param-row {
                layout: horizontal;
                height: 3;
                margin-bottom: 1;
            }

            .param-label {
                width: 20;
                padding-top: 1;
            }

            .param-input {
                width: 1fr;
            }

            #execute-btn {
                margin-top: 1;
                width: 100%;
            }

            Tree {
                scrollbar-gutter: stable;
            }
            """

            BINDINGS: ClassVar[list[Binding]] = [
                Binding("q", "quit", "Quit"),
                Binding("ctrl+x", "execute", "Execute"),
                Binding("ctrl+c", "clear", "Clear"),
            ]

            def __init__(self):
                super().__init__()
                self.selected_method: TUIMethod | None = None
                self.param_inputs: dict[str, Input] = {}

            def compose(self) -> ComposeResult:
                yield Header()
                with Horizontal(id="main-container"):
                    with Vertical(id="sidebar"):
                        yield Static("API Explorer", classes="title")
                        tree: Tree[TUIMethod] = Tree("MossAPI")
                        tree.root.expand()
                        for group in groups:
                            branch = tree.root.add(group.name, expand=True)
                            for method in group.methods:
                                branch.add_leaf(method.name, data=method)
                        yield tree
                    with Vertical(id="content"):
                        yield Static("Select a method from the sidebar", id="method-info")
                        yield Container(id="params-container")
                        with Container(id="result-container"):
                            yield RichLog(id="result-log", highlight=True, markup=True)
                yield Footer()

            def on_tree_node_selected(self, event: Tree.NodeSelected) -> None:
                """Handle tree node selection."""
                if event.node.data is not None:
                    self.selected_method = event.node.data
                    self._update_method_display()

            def _update_method_display(self) -> None:
                """Update the method info and parameter inputs."""
                if self.selected_method is None:
                    return

                method = self.selected_method

                # Update method info
                info = self.query_one("#method-info", Static)
                info.update(
                    f"[bold]{method.api_path}[/bold]\n{method.description or 'No description'}"
                )

                # Clear and rebuild params container
                params_container = self.query_one("#params-container", Container)
                params_container.remove_children()
                self.param_inputs.clear()

                if method.parameters:
                    for param in method.parameters:
                        label_text = f"{param.name}"
                        if param.required:
                            label_text += " *"

                        row = Horizontal(classes="param-row")
                        label = Label(label_text, classes="param-label")

                        placeholder = param.type_hint
                        if param.default is not None:
                            placeholder = f"{param.type_hint} (default: {param.default})"

                        input_widget = Input(
                            placeholder=placeholder,
                            id=f"param-{param.name}",
                            classes="param-input",
                        )
                        if param.default is not None and param.default != "":
                            input_widget.value = str(param.default)

                        self.param_inputs[param.name] = input_widget
                        row.compose_add_child(label)
                        row.compose_add_child(input_widget)
                        params_container.mount(row)

                else:
                    params_container.mount(Static("No parameters required"))

                # Add execute button (outside if/else to avoid duplicate IDs)
                btn = Button("Execute (Ctrl+X)", id="execute-btn", variant="primary")
                params_container.mount(btn)

            def on_button_pressed(self, event: Button.Pressed) -> None:
                """Handle button press."""
                if event.button.id == "execute-btn":
                    self.action_execute()

            def action_execute(self) -> None:
                """Execute the selected method."""
                if self.selected_method is None:
                    return

                log = self.query_one("#result-log", RichLog)

                # Gather parameters
                args = {}
                for param in self.selected_method.parameters:
                    if param.name in self.param_inputs:
                        value = self.param_inputs[param.name].value
                        if value:
                            # Try to convert to appropriate type
                            if param.type_hint == "int":
                                try:
                                    value = int(value)
                                except ValueError:
                                    pass
                            elif param.type_hint == "float":
                                try:
                                    value = float(value)
                                except ValueError:
                                    pass
                            elif param.type_hint == "bool":
                                value = value.lower() in ("true", "1", "yes")
                            args[param.name] = value
                        elif param.required and param.default is None:
                            log.write(f"[red]Error: {param.name} is required[/red]")
                            return

                log.write(f"[cyan]>>> {self.selected_method.api_path}({args})[/cyan]")

                try:
                    result = executor.execute(self.selected_method.api_path, args)
                    formatted = json.dumps(result, indent=2, default=str)
                    log.write(formatted)
                except Exception as e:
                    log.write(f"[red]Error: {e}[/red]")

            def action_clear(self) -> None:
                """Clear the result log."""
                log = self.query_one("#result-log", RichLog)
                log.clear()

        return MossApp()


def generate_tui_groups() -> list[TUIGroup]:
    """Generate TUI groups from MossAPI.

    Convenience function.
    """
    generator = TUIGenerator()
    return generator.generate_groups()


def run_tui(root: str | Path = ".") -> None:
    """Run the Moss TUI application.

    Args:
        root: Project root directory
    """
    generator = TUIGenerator()
    app = generator.generate_app(root)
    app.run()


__all__ = [
    "TUIGenerator",
    "TUIGroup",
    "TUIMethod",
    "TUIParameter",
    "generate_tui_groups",
    "method_to_tui",
    "run_tui",
    "subapi_to_group",
]
