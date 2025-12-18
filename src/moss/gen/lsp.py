"""LSP handler generator from MossAPI introspection.

This module generates LSP workspace/executeCommand handlers from the MossAPI.
Each API method becomes an executable command that can be invoked from editors.

This complements the document-centric features in moss.lsp_server (hover,
diagnostics, symbols) with project-wide API access.

Usage:
    from moss.gen.lsp import LSPGenerator

    # Get command definitions for client capabilities
    generator = LSPGenerator()
    commands = generator.generate_commands()

    # Register handlers on an existing server
    generator.register_on_server(server)

Commands are named as: moss.{api}.{method}
Example: moss.skeleton.extract, moss.health.check
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from moss.gen.introspect import APIMethod, SubAPI, introspect_api
from moss.gen.serialize import serialize


@dataclass
class LSPCommand:
    """An LSP command generated from an API method.

    Attributes:
        command: Full command ID (e.g., "moss.skeleton.extract")
        title: Human-readable title for command palette
        description: Command description
        api_path: Path to underlying API method
        parameters: Parameter definitions
    """

    command: str
    title: str
    description: str
    api_path: str
    parameters: list[LSPParameter] = field(default_factory=list)


@dataclass
class LSPParameter:
    """A parameter for an LSP command.

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


def method_to_command(method: APIMethod, api_name: str) -> LSPCommand:
    """Convert an API method to an LSP command definition."""
    parameters = [
        LSPParameter(
            name=p.name,
            type_hint=p.type_hint,
            required=p.required,
            default=p.default,
            description=p.description,
        )
        for p in method.parameters
    ]

    # Create human-readable title
    title = method.name.replace("_", " ").title()
    if api_name:
        title = f"{api_name.title()}: {title}"

    return LSPCommand(
        command=f"moss.{api_name}.{method.name}",
        title=title,
        description=method.description,
        api_path=f"{api_name}.{method.name}",
        parameters=parameters,
    )


def subapi_to_commands(subapi: SubAPI) -> list[LSPCommand]:
    """Convert a sub-API to LSP commands."""
    return [method_to_command(m, subapi.name) for m in subapi.methods]


class LSPExecutor:
    """Executor for LSP commands.

    Handles command execution with parameter parsing and result serialization.
    """

    def __init__(self, root: str | Path = "."):
        """Initialize the executor.

        Args:
            root: Project root directory
        """
        self._root = Path(root).resolve()
        self._api = None

    @property
    def api(self):
        """Lazy-initialize and cache the MossAPI instance."""
        if self._api is None:
            from moss import MossAPI

            self._api = MossAPI.for_project(self._root)
        return self._api

    def execute(self, command: str, arguments: list[Any] | None = None) -> Any:
        """Execute a command and return serialized result.

        Args:
            command: Command ID (e.g., "moss.skeleton.extract")
            arguments: Command arguments as list (positional or single dict)

        Returns:
            Serialized result (JSON-compatible)

        Raises:
            ValueError: If command is invalid
        """
        # Parse command ID
        if not command.startswith("moss."):
            raise ValueError(f"Invalid command: {command}. Expected 'moss.*'")

        parts = command[5:].split(".")  # Strip "moss." prefix
        if len(parts) != 2:
            raise ValueError(f"Invalid command: {command}. Expected 'moss.api.method'")

        subapi_name, method_name = parts

        # Get API and method
        subapi = getattr(self.api, subapi_name, None)
        if subapi is None:
            raise ValueError(f"Unknown API: {subapi_name}")

        method = getattr(subapi, method_name, None)
        if method is None:
            raise ValueError(f"Unknown method: {subapi_name}.{method_name}")

        # Parse arguments
        kwargs = {}
        if arguments:
            if len(arguments) == 1 and isinstance(arguments[0], dict):
                # Single dict argument
                kwargs = arguments[0]
            else:
                # Positional arguments - would need introspection to map
                # For now, assume dict-style
                pass

        # Execute and serialize
        result = method(**kwargs)
        return serialize(result)


class LSPGenerator:
    """Generator for LSP handlers from MossAPI.

    Usage:
        generator = LSPGenerator()

        # Get command list for capabilities
        commands = generator.generate_commands()

        # Register on pygls server
        generator.register_on_server(server)
    """

    def __init__(self, root: str | Path = "."):
        """Initialize the generator.

        Args:
            root: Project root directory
        """
        self._root = Path(root).resolve()
        self._commands: list[LSPCommand] | None = None

    def generate_commands(self) -> list[LSPCommand]:
        """Generate LSP commands from MossAPI introspection."""
        if self._commands is None:
            sub_apis = introspect_api()
            self._commands = []
            for api in sub_apis:
                self._commands.extend(subapi_to_commands(api))
        return self._commands

    def generate_command_list(self) -> list[str]:
        """Get list of command IDs for server capabilities."""
        return [cmd.command for cmd in self.generate_commands()]

    def register_on_server(self, server: Any) -> None:
        """Register command handlers on a pygls LanguageServer.

        Args:
            server: A pygls LanguageServer instance
        """
        try:
            from lsprotocol import types as lsp
        except ImportError as e:
            raise ImportError(
                "LSP dependencies required. Install with: pip install 'moss[lsp]'"
            ) from e

        executor = LSPExecutor(self._root)
        commands = self.generate_commands()

        # Create command -> info mapping
        command_info = {cmd.command: cmd for cmd in commands}

        @server.feature(lsp.WORKSPACE_EXECUTE_COMMAND)
        def execute_command(params: lsp.ExecuteCommandParams) -> Any:
            """Handle workspace/executeCommand requests."""
            command = params.command
            arguments = params.arguments or []

            if command not in command_info:
                return {"error": f"Unknown command: {command}"}

            try:
                result = executor.execute(command, arguments)
                return {"success": True, "result": result}
            except Exception as e:
                return {"success": False, "error": str(e)}

        # Update server capabilities
        if hasattr(server, "server_capabilities"):
            caps = server.server_capabilities
            if hasattr(caps, "execute_command_provider"):
                if caps.execute_command_provider is None:
                    caps.execute_command_provider = lsp.ExecuteCommandOptions(
                        commands=self.generate_command_list()
                    )
                else:
                    existing = caps.execute_command_provider.commands or []
                    caps.execute_command_provider.commands = existing + self.generate_command_list()


def generate_lsp_commands() -> list[LSPCommand]:
    """Generate LSP commands from MossAPI.

    Convenience function.
    """
    generator = LSPGenerator()
    return generator.generate_commands()


def get_command_list() -> list[str]:
    """Get list of all available command IDs.

    Convenience function for server capabilities.
    """
    generator = LSPGenerator()
    return generator.generate_command_list()


__all__ = [
    "LSPCommand",
    "LSPExecutor",
    "LSPGenerator",
    "LSPParameter",
    "generate_lsp_commands",
    "get_command_list",
    "method_to_command",
    "subapi_to_commands",
]
