"""Shared utilities for CLI commands.

This module contains helper functions used across multiple CLI command modules.
"""

from __future__ import annotations

import sys
from typing import TYPE_CHECKING, Any

from moss.output import Output, Verbosity, configure_output, get_output

if TYPE_CHECKING:
    from argparse import Namespace


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
