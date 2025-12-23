"""CLI generator from MossAPI introspection.

This module generates argparse CLI commands from the MossAPI structure.
Each sub-API becomes a command group, and each method becomes a subcommand.

Example generated structure:
    moss skeleton extract <file>
    moss skeleton format <file> [--show-bodies]
    moss anchor find <file> <name> [--type]
    moss health check
    moss health summarize
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from moss_orchestration.gen.introspect import APIMethod, APIParameter, SubAPI, introspect_api


@dataclass
class CLICommand:
    """A generated CLI command.

    Attributes:
        name: Command name
        description: Help text
        arguments: Positional arguments
        options: Optional arguments (flags)
        api_path: Path to API method (e.g., "skeleton.extract")
    """

    name: str
    description: str = ""
    arguments: list[CLIArgument] = field(default_factory=list)
    options: list[CLIOption] = field(default_factory=list)
    api_path: str = ""


@dataclass
class CLIArgument:
    """A positional CLI argument.

    Attributes:
        name: Argument name
        type: Python type (str, int, etc.)
        description: Help text
        nargs: Number of arguments ("?", "*", "+", or None)
    """

    name: str
    type: str = "str"
    description: str = ""
    nargs: str | None = None


@dataclass
class CLIOption:
    """An optional CLI argument (flag).

    Attributes:
        name: Option name (without --)
        short: Short form (single char, without -)
        type: Python type
        default: Default value
        description: Help text
        is_flag: If True, this is a boolean flag
    """

    name: str
    short: str | None = None
    type: str = "str"
    default: Any = None
    description: str = ""
    is_flag: bool = False


@dataclass
class CLIGroup:
    """A group of CLI commands (subcommands).

    Attributes:
        name: Group name (e.g., "skeleton")
        description: Help text
        commands: List of commands in this group
    """

    name: str
    description: str = ""
    commands: list[CLICommand] = field(default_factory=list)


def _param_to_type(type_hint: str) -> str:
    """Convert API type hint to CLI type."""
    type_map = {
        "str": "str",
        "int": "int",
        "float": "float",
        "bool": "bool",
        "Path": "str",
        "Any": "str",
    }

    # Handle union types (str | Path -> str)
    if "|" in type_hint:
        parts = [p.strip() for p in type_hint.split("|")]
        # Prefer str if available
        if "str" in parts:
            return "str"
        return type_map.get(parts[0], "str")

    # Handle list types (list[str] -> str with nargs)
    if type_hint.startswith("list["):
        return "str"  # Will need nargs handling

    return type_map.get(type_hint, "str")


def _is_path_type(type_hint: str) -> bool:
    """Check if a type hint represents a path."""
    return "Path" in type_hint or "path" in type_hint.lower()


def _param_to_cli_arg(param: APIParameter) -> CLIArgument | CLIOption:
    """Convert an API parameter to a CLI argument or option."""
    cli_type = _param_to_type(param.type_hint)

    # Determine nargs for list types
    nargs = None
    if param.type_hint.startswith("list["):
        nargs = "*"
    elif not param.required:
        nargs = "?"

    # Boolean parameters become flags
    if cli_type == "bool":
        return CLIOption(
            name=param.name.replace("_", "-"),
            type=cli_type,
            default=param.default if param.default is not None else False,
            description=param.description,
            is_flag=True,
        )

    # Required parameters become positional arguments
    if param.required:
        return CLIArgument(
            name=param.name.replace("_", "-"),
            type=cli_type,
            description=param.description,
            nargs=nargs,
        )

    # Optional parameters become options
    return CLIOption(
        name=param.name.replace("_", "-"),
        type=cli_type,
        default=param.default,
        description=param.description,
    )


def method_to_command(method: APIMethod, api_name: str) -> CLICommand:
    """Convert an API method to a CLI command.

    Args:
        method: The API method to convert
        api_name: Name of the parent API (e.g., "skeleton")

    Returns:
        CLICommand representing the method
    """
    arguments: list[CLIArgument] = []
    options: list[CLIOption] = []

    for param in method.parameters:
        cli_param = _param_to_cli_arg(param)
        if isinstance(cli_param, CLIArgument):
            arguments.append(cli_param)
        else:
            options.append(cli_param)

    return CLICommand(
        name=method.name.replace("_", "-"),
        description=method.description,
        arguments=arguments,
        options=options,
        api_path=f"{api_name}.{method.name}",
    )


def subapi_to_group(subapi: SubAPI) -> CLIGroup:
    """Convert a sub-API to a CLI command group.

    Args:
        subapi: The sub-API to convert

    Returns:
        CLIGroup containing all methods as commands
    """
    commands = [method_to_command(m, subapi.name) for m in subapi.methods]

    return CLIGroup(
        name=subapi.name,
        description=subapi.description,
        commands=commands,
    )


class CLIGenerator:
    """Generator for argparse CLI from MossAPI.

    Usage:
        generator = CLIGenerator()
        parser = generator.generate_parser()

        # Or get the structure for customization
        groups = generator.generate_groups()
    """

    def __init__(self, prog: str = "moss"):
        """Initialize the generator.

        Args:
            prog: Program name for the CLI
        """
        self.prog = prog
        self._groups: list[CLIGroup] | None = None

    def generate_groups(self) -> list[CLIGroup]:
        """Generate CLI groups from MossAPI introspection.

        Returns:
            List of CLIGroup objects
        """
        if self._groups is None:
            sub_apis = introspect_api()
            self._groups = [subapi_to_group(api) for api in sub_apis]
        return self._groups

    def generate_parser(self) -> argparse.ArgumentParser:
        """Generate an argparse parser from MossAPI.

        Returns:
            Configured ArgumentParser with all subcommands
        """
        parser = argparse.ArgumentParser(
            prog=self.prog,
            description="Moss: Headless agent orchestration layer",
        )
        parser.add_argument(
            "--root",
            "-r",
            type=str,
            default=".",
            help="Project root directory",
        )
        parser.add_argument(
            "--json",
            action="store_true",
            help="Output as JSON",
        )

        subparsers = parser.add_subparsers(dest="command", help="Available commands")

        for group in self.generate_groups():
            group_parser = subparsers.add_parser(
                group.name,
                help=group.description,
            )
            group_subs = group_parser.add_subparsers(
                dest="subcommand",
                help=f"{group.name} commands",
            )

            for cmd in group.commands:
                cmd_parser = group_subs.add_parser(
                    cmd.name,
                    help=cmd.description,
                )

                # Add positional arguments
                for arg in cmd.arguments:
                    kwargs: dict[str, Any] = {
                        "help": arg.description,
                    }
                    if arg.nargs:
                        kwargs["nargs"] = arg.nargs
                    if arg.type == "int":
                        kwargs["type"] = int
                    elif arg.type == "float":
                        kwargs["type"] = float

                    cmd_parser.add_argument(arg.name, **kwargs)

                # Add optional arguments
                for opt in cmd.options:
                    flag = f"--{opt.name}"
                    kwargs = {
                        "help": opt.description,
                    }

                    if opt.is_flag:
                        kwargs["action"] = "store_true"
                        if opt.default:
                            kwargs["action"] = "store_false"
                    else:
                        kwargs["default"] = opt.default
                        if opt.type == "int":
                            kwargs["type"] = int
                        elif opt.type == "float":
                            kwargs["type"] = float

                    if opt.short:
                        cmd_parser.add_argument(f"-{opt.short}", flag, **kwargs)
                    else:
                        cmd_parser.add_argument(flag, **kwargs)

        return parser

    def generate_executor(self) -> CLIExecutor:
        """Generate a CLI executor that runs the parsed commands.

        Returns:
            CLIExecutor instance
        """
        return CLIExecutor(self.generate_groups())


class CLIExecutor:
    """Executor for generated CLI commands.

    Runs MossAPI methods based on parsed CLI arguments.
    """

    def __init__(self, groups: list[CLIGroup]):
        """Initialize the executor.

        Args:
            groups: CLI groups from the generator
        """
        self._groups = groups
        self._command_map: dict[str, CLICommand] = {}
        for group in groups:
            for cmd in group.commands:
                key = f"{group.name}.{cmd.name}"
                self._command_map[key] = cmd

    def execute(self, args: argparse.Namespace) -> Any:
        """Execute a command based on parsed arguments.

        Args:
            args: Parsed CLI arguments

        Returns:
            Result from the API call
        """
        from moss import MossAPI

        if not args.command or not args.subcommand:
            return None

        root = Path(args.root).resolve()
        api = MossAPI.for_project(root)

        # Get the sub-API
        subapi = getattr(api, args.command, None)
        if subapi is None:
            raise ValueError(f"Unknown command: {args.command}")

        # Get the method
        method_name = args.subcommand.replace("-", "_")
        method = getattr(subapi, method_name, None)
        if method is None:
            raise ValueError(f"Unknown subcommand: {args.subcommand}")

        # Build kwargs from args
        cmd_key = f"{args.command}.{args.subcommand}"
        cmd = self._command_map.get(cmd_key)
        if cmd is None:
            raise ValueError(f"Unknown command path: {cmd_key}")

        kwargs: dict[str, Any] = {}

        # Add positional arguments
        for arg in cmd.arguments:
            arg_name = arg.name.replace("-", "_")
            if hasattr(args, arg_name):
                value = getattr(args, arg_name)
                if value is not None:
                    kwargs[arg_name] = value

        # Add optional arguments
        for opt in cmd.options:
            opt_name = opt.name.replace("-", "_")
            if hasattr(args, opt_name):
                value = getattr(args, opt_name)
                if value is not None and value != opt.default:
                    kwargs[opt_name] = value

        # Call the method
        return method(**kwargs)


def generate_cli(prog: str = "moss") -> argparse.ArgumentParser:
    """Generate a CLI parser from MossAPI.

    This is a convenience function that creates a CLIGenerator
    and returns the parser.

    Args:
        prog: Program name for the CLI

    Returns:
        Configured ArgumentParser
    """
    generator = CLIGenerator(prog=prog)
    return generator.generate_parser()


__all__ = [
    "CLIArgument",
    "CLICommand",
    "CLIExecutor",
    "CLIGenerator",
    "CLIGroup",
    "CLIOption",
    "generate_cli",
    "method_to_command",
    "subapi_to_group",
]
