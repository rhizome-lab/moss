"""Configurable output verbosity for CLI.

This module provides a unified output system with configurable verbosity levels,
supporting different output formats and styles.

Usage:
    from moss.output import Output, Verbosity

    output = Output(verbosity=Verbosity.VERBOSE)
    output.info("Processing files...")
    output.debug("Detailed information")
    output.success("Done!")

    # Or use the global output
    from moss.output import output
    output.info("Message")
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from enum import IntEnum
from typing import IO, Any

# =============================================================================
# Verbosity Levels
# =============================================================================


class Verbosity(IntEnum):
    """Output verbosity levels."""

    QUIET = 0  # Only errors
    NORMAL = 1  # Normal output
    VERBOSE = 2  # Additional details
    DEBUG = 3  # Everything


# =============================================================================
# Output Styles
# =============================================================================


@dataclass
class OutputStyle:
    """Styling configuration for output."""

    use_colors: bool = True
    use_emoji: bool = True
    indent_size: int = 2
    max_width: int = 100

    # Color codes
    colors: dict[str, str] = field(
        default_factory=lambda: {
            "reset": "\033[0m",
            "bold": "\033[1m",
            "dim": "\033[2m",
            "red": "\033[31m",
            "green": "\033[32m",
            "yellow": "\033[33m",
            "blue": "\033[34m",
            "magenta": "\033[35m",
            "cyan": "\033[36m",
            "white": "\033[37m",
        }
    )

    # Emoji prefixes
    emoji: dict[str, str] = field(
        default_factory=lambda: {
            "error": "X",
            "warning": "!",
            "success": "+",
            "info": "*",
            "debug": "#",
            "step": ">",
        }
    )


# =============================================================================
# Output Formatters
# =============================================================================


class OutputFormatter:
    """Base class for output formatters."""

    def format_message(self, level: str, message: str, style: OutputStyle, use_tty: bool) -> str:
        """Format a message for output."""
        return message

    def format_data(self, data: Any, style: OutputStyle) -> str:
        """Format data for output."""
        return str(data)


class TextFormatter(OutputFormatter):
    """Plain text formatter with optional colors."""

    def format_message(self, level: str, message: str, style: OutputStyle, use_tty: bool) -> str:
        prefix = ""

        if style.use_emoji:
            emoji = style.emoji.get(level, "")
            if emoji:
                prefix = f"[{emoji}] "

        if style.use_colors and use_tty:
            color = self._level_color(level, style)
            reset = style.colors["reset"]
            return f"{color}{prefix}{message}{reset}"

        return f"{prefix}{message}"

    def _level_color(self, level: str, style: OutputStyle) -> str:
        color_map = {
            "error": style.colors["red"],
            "warning": style.colors["yellow"],
            "success": style.colors["green"],
            "info": style.colors["blue"],
            "debug": style.colors["dim"],
            "step": style.colors["cyan"],
        }
        return color_map.get(level, "")

    def format_data(self, data: Any, style: OutputStyle) -> str:
        if isinstance(data, dict):
            return self._format_dict(data, style)
        elif isinstance(data, list):
            return self._format_list(data, style)
        return str(data)

    def _format_dict(self, data: dict, style: OutputStyle, indent: int = 0) -> str:
        lines = []
        prefix = " " * (indent * style.indent_size)
        for key, value in data.items():
            if isinstance(value, dict):
                lines.append(f"{prefix}{key}:")
                lines.append(self._format_dict(value, style, indent + 1))
            elif isinstance(value, list):
                lines.append(f"{prefix}{key}:")
                lines.append(self._format_list(value, style, indent + 1))
            else:
                lines.append(f"{prefix}{key}: {value}")
        return "\n".join(lines)

    def _format_list(self, data: list, style: OutputStyle, indent: int = 0) -> str:
        lines = []
        prefix = " " * (indent * style.indent_size)
        for item in data:
            if isinstance(item, dict):
                lines.append(f"{prefix}-")
                lines.append(self._format_dict(item, style, indent + 1))
            else:
                lines.append(f"{prefix}- {item}")
        return "\n".join(lines)


class JSONFormatter(OutputFormatter):
    """JSON output formatter."""

    def format_message(self, level: str, message: str, style: OutputStyle, use_tty: bool) -> str:
        return json.dumps({"level": level, "message": message})

    def format_data(self, data: Any, style: OutputStyle) -> str:
        return json.dumps(data, indent=2, default=str)


class CompactFormatter(OutputFormatter):
    """Compact single-line formatter."""

    def format_message(self, level: str, message: str, style: OutputStyle, use_tty: bool) -> str:
        level_char = level[0].upper() if level else " "
        return f"[{level_char}] {message}"

    def format_data(self, data: Any, style: OutputStyle) -> str:
        if isinstance(data, (dict, list)):
            return json.dumps(data, separators=(",", ":"))
        return str(data)


# =============================================================================
# Output Class
# =============================================================================


class Output:
    """Configurable output for CLI commands."""

    def __init__(
        self,
        verbosity: Verbosity = Verbosity.NORMAL,
        style: OutputStyle | None = None,
        formatter: OutputFormatter | None = None,
        stdout: IO[str] | None = None,
        stderr: IO[str] | None = None,
    ) -> None:
        self.verbosity = verbosity
        self.style = style or OutputStyle()
        self.formatter = formatter or TextFormatter()
        self.stdout = stdout or sys.stdout
        self.stderr = stderr or sys.stderr
        self._indent_level = 0
        self._jq_expr: str | None = None

    # -------------------------------------------------------------------------
    # Configuration
    # -------------------------------------------------------------------------

    def set_verbosity(self, verbosity: Verbosity) -> None:
        """Set verbosity level."""
        self.verbosity = verbosity

    def set_quiet(self) -> None:
        """Set quiet mode (errors only)."""
        self.verbosity = Verbosity.QUIET

    def set_verbose(self) -> None:
        """Enable verbose output."""
        self.verbosity = Verbosity.VERBOSE

    def set_debug(self) -> None:
        """Enable debug output."""
        self.verbosity = Verbosity.DEBUG

    def use_json(self) -> None:
        """Switch to JSON output format."""
        self.formatter = JSONFormatter()
        self.style.use_colors = False
        self.style.use_emoji = False

    def use_compact(self) -> None:
        """Switch to compact output format."""
        self.formatter = CompactFormatter()

    def set_jq(self, expr: str) -> None:
        """Set jq expression for filtering JSON output.

        Args:
            expr: jq filter expression (e.g., '.stats', '.dependencies[0].name')
        """
        self._jq_expr = expr

    # -------------------------------------------------------------------------
    # Indentation
    # -------------------------------------------------------------------------

    def indent(self) -> Output:
        """Increase indentation level."""
        self._indent_level += 1
        return self

    def dedent(self) -> Output:
        """Decrease indentation level."""
        self._indent_level = max(0, self._indent_level - 1)
        return self

    def _get_indent(self) -> str:
        return " " * (self._indent_level * self.style.indent_size)

    # -------------------------------------------------------------------------
    # Output Methods
    # -------------------------------------------------------------------------

    def write(
        self,
        message: str,
        level: str = "info",
        min_verbosity: Verbosity = Verbosity.NORMAL,
    ) -> None:
        """Write a message if verbosity allows."""
        if self.verbosity < min_verbosity:
            return

        stream = self.stderr if level == "error" else self.stdout
        use_tty = stream.isatty()

        formatted = self.formatter.format_message(level, message, self.style, use_tty)
        indent = self._get_indent()

        stream.write(f"{indent}{formatted}\n")
        stream.flush()

    def error(self, message: str) -> None:
        """Output an error message (always shown unless quiet)."""
        self.write(message, "error", Verbosity.QUIET)

    def warning(self, message: str) -> None:
        """Output a warning message."""
        self.write(message, "warning", Verbosity.NORMAL)

    def success(self, message: str) -> None:
        """Output a success message."""
        self.write(message, "success", Verbosity.NORMAL)

    def info(self, message: str) -> None:
        """Output an info message."""
        self.write(message, "info", Verbosity.NORMAL)

    def verbose(self, message: str) -> None:
        """Output a verbose message (only in verbose mode)."""
        self.write(message, "info", Verbosity.VERBOSE)

    def debug(self, message: str) -> None:
        """Output a debug message (only in debug mode)."""
        self.write(message, "debug", Verbosity.DEBUG)

    def debug_traceback(self) -> None:
        """Output current exception traceback in debug mode."""
        import traceback

        if self.verbosity >= Verbosity.DEBUG:
            self.write(traceback.format_exc(), "debug", Verbosity.DEBUG)

    def step(self, message: str) -> None:
        """Output a step/progress message."""
        self.write(message, "step", Verbosity.NORMAL)

    def print(self, message: str) -> None:
        """Print raw message (no formatting)."""
        if self.verbosity >= Verbosity.NORMAL:
            self.stdout.write(f"{self._get_indent()}{message}\n")
            self.stdout.flush()

    def data(self, data: Any, min_verbosity: Verbosity = Verbosity.NORMAL) -> None:
        """Output structured data."""
        if self.verbosity < min_verbosity:
            return

        # Apply jq filter if set
        if self._jq_expr and isinstance(data, (dict, list)):
            result = self._apply_jq(data)
            if result is not None:
                self.stdout.write(f"{result}\n")
                self.stdout.flush()
                return

        formatted = self.formatter.format_data(data, self.style)
        self.stdout.write(f"{formatted}\n")
        self.stdout.flush()

    def _apply_jq(self, data: Any) -> str | None:
        """Apply jq filter to data.

        Returns filtered output string, or None if jq failed.
        """
        import shutil
        import subprocess

        if not shutil.which("jq"):
            self.error("jq not found - install jq or omit --jq flag")
            return None

        if self._jq_expr is None:
            return None

        try:
            json_input = json.dumps(data)
            result = subprocess.run(
                ["jq", "-c", self._jq_expr],
                input=json_input,
                capture_output=True,
                text=True,
                timeout=10,
            )
            if result.returncode != 0:
                self.error(f"jq error: {result.stderr.strip()}")
                return None
            return result.stdout.strip()
        except subprocess.TimeoutExpired:
            self.error("jq timed out")
            return None
        except Exception as e:
            self.error(f"jq failed: {e}")
            return None

    # -------------------------------------------------------------------------
    # Convenience Methods
    # -------------------------------------------------------------------------

    def header(self, text: str) -> None:
        """Output a section header."""
        if self.verbosity < Verbosity.NORMAL:
            return

        use_tty = self.stdout.isatty()
        if self.style.use_colors and use_tty:
            bold = self.style.colors["bold"]
            reset = self.style.colors["reset"]
            self.stdout.write(f"\n{bold}{text}{reset}\n")
        else:
            self.stdout.write(f"\n{text}\n")
        self.stdout.write("-" * min(len(text), self.style.max_width) + "\n")
        self.stdout.flush()

    def blank(self) -> None:
        """Output a blank line."""
        if self.verbosity >= Verbosity.NORMAL:
            self.stdout.write("\n")
            self.stdout.flush()

    def separator(self, char: str = "-") -> None:
        """Output a separator line."""
        if self.verbosity >= Verbosity.NORMAL:
            self.stdout.write(char * self.style.max_width + "\n")
            self.stdout.flush()


# =============================================================================
# Global Output Instance
# =============================================================================


# Global output instance for convenience
_output: Output | None = None


def get_output() -> Output:
    """Get the global output instance."""
    global _output
    if _output is None:
        _output = Output()
    return _output


def set_output(output: Output) -> None:
    """Set the global output instance."""
    global _output
    _output = output


def reset_output() -> None:
    """Reset the global output instance.

    This clears the cached output so a fresh one will be created
    on the next get_output() call. Useful in tests to ensure
    clean stdout/stderr references.
    """
    global _output
    _output = None


def configure_output(
    verbosity: Verbosity | None = None,
    json_format: bool = False,
    compact: bool = False,
    no_color: bool = False,
    jq_expr: str | None = None,
) -> Output:
    """Configure the global output instance.

    Args:
        verbosity: Verbosity level
        json_format: Use JSON output format
        compact: Use compact output format (token-efficient for AI agents)
        no_color: Disable colors
        jq_expr: jq filter expression for JSON output

    Returns:
        Configured Output instance
    """
    output = get_output()

    if verbosity is not None:
        output.set_verbosity(verbosity)

    if compact:
        output.use_compact()
    elif json_format:
        output.use_json()

    if no_color:
        output.style.use_colors = False

    if jq_expr:
        output.set_jq(jq_expr)
        # jq implies JSON mode
        output.use_json()

    return output


# Convenience exports for direct use
def error(msg: str) -> None:
    get_output().error(msg)


def warning(msg: str) -> None:
    get_output().warning(msg)


def success(msg: str) -> None:
    get_output().success(msg)


def info(msg: str) -> None:
    get_output().info(msg)


def verbose(msg: str) -> None:
    get_output().verbose(msg)


def debug(msg: str) -> None:
    get_output().debug(msg)


def data(d: Any) -> None:
    get_output().data(d)
