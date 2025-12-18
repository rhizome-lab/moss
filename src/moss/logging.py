"""Structured logging for Moss.

This module provides consistent, structured logging across all Moss components.
It supports both human-readable and JSON output formats.
"""

import json
import logging
import sys
import time
from contextlib import contextmanager
from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class LogFormat(Enum):
    """Log output format."""

    TEXT = "text"
    JSON = "json"


@dataclass
class LogContext:
    """Context information for structured logging."""

    component: str = ""
    operation: str = ""
    request_id: str = ""
    extra: dict[str, Any] = field(default_factory=dict)

    def with_extra(self, **kwargs: Any) -> "LogContext":
        """Create new context with additional fields."""
        new_extra = {**self.extra, **kwargs}
        return LogContext(
            component=self.component,
            operation=self.operation,
            request_id=self.request_id,
            extra=new_extra,
        )


class StructuredFormatter(logging.Formatter):
    """Formatter that outputs structured JSON logs."""

    def format(self, record: logging.LogRecord) -> str:
        """Format log record as JSON."""
        log_data = {
            "timestamp": self.formatTime(record),
            "level": record.levelname,
            "logger": record.name,
            "message": record.getMessage(),
        }

        # Add exception info if present
        if record.exc_info:
            log_data["exception"] = self.formatException(record.exc_info)

        # Add context fields if present
        ctx = getattr(record, "context", None)
        if ctx is not None and isinstance(ctx, LogContext):
            if ctx.component:
                log_data["component"] = ctx.component
            if ctx.operation:
                log_data["operation"] = ctx.operation
            if ctx.request_id:
                log_data["request_id"] = ctx.request_id
            if ctx.extra:
                log_data.update(ctx.extra)

        # Add any extra fields passed directly
        extra_fields = getattr(record, "extra_fields", None)
        if extra_fields is not None:
            log_data.update(extra_fields)

        return json.dumps(log_data)


class TextFormatter(logging.Formatter):
    """Formatter that outputs human-readable logs with context."""

    def format(self, record: logging.LogRecord) -> str:
        """Format log record as human-readable text."""
        # Build prefix from context
        prefix_parts = []

        ctx = getattr(record, "context", None)
        if ctx is not None and isinstance(ctx, LogContext):
            if ctx.component:
                prefix_parts.append(f"[{ctx.component}]")
            if ctx.operation:
                prefix_parts.append(f"({ctx.operation})")
            if ctx.request_id:
                prefix_parts.append(f"req:{ctx.request_id[:8]}")

        prefix = " ".join(prefix_parts)
        if prefix:
            prefix = f"{prefix} "

        # Format base message
        base = super().format(record)

        # Add extra fields if present
        extra_str = ""
        if ctx is not None and isinstance(ctx, LogContext) and ctx.extra:
            extra_str = " " + " ".join(f"{k}={v}" for k, v in ctx.extra.items())

        return f"{prefix}{base}{extra_str}"


class MossLogger:
    """Structured logger for Moss components."""

    def __init__(
        self,
        name: str,
        level: int = logging.INFO,
        log_format: LogFormat = LogFormat.TEXT,
    ):
        """Initialize the logger.

        Args:
            name: Logger name (typically component name)
            level: Logging level
            log_format: Output format (TEXT or JSON)
        """
        self._logger = logging.getLogger(f"moss.{name}")
        self._logger.setLevel(level)
        self._context = LogContext(component=name)
        self._log_format = log_format

        # Set up handler if not already configured
        if not self._logger.handlers:
            handler = logging.StreamHandler(sys.stderr)
            handler.setLevel(level)

            if log_format == LogFormat.JSON:
                handler.setFormatter(StructuredFormatter())
            else:
                handler.setFormatter(TextFormatter("%(asctime)s %(levelname)s %(message)s"))

            self._logger.addHandler(handler)

    def with_context(self, **kwargs: Any) -> "MossLogger":
        """Create a new logger with additional context.

        Args:
            **kwargs: Context fields to add

        Returns:
            New logger with updated context
        """
        new_logger = MossLogger.__new__(MossLogger)
        new_logger._logger = self._logger
        new_logger._context = self._context.with_extra(**kwargs)
        new_logger._log_format = self._log_format
        return new_logger

    def with_operation(self, operation: str) -> "MossLogger":
        """Create a new logger for a specific operation.

        Args:
            operation: Operation name

        Returns:
            New logger with operation context
        """
        new_logger = MossLogger.__new__(MossLogger)
        new_logger._logger = self._logger
        new_logger._context = LogContext(
            component=self._context.component,
            operation=operation,
            request_id=self._context.request_id,
            extra=self._context.extra,
        )
        new_logger._log_format = self._log_format
        return new_logger

    def with_request_id(self, request_id: str) -> "MossLogger":
        """Create a new logger with a request ID.

        Args:
            request_id: Request identifier

        Returns:
            New logger with request ID context
        """
        new_logger = MossLogger.__new__(MossLogger)
        new_logger._logger = self._logger
        new_logger._context = LogContext(
            component=self._context.component,
            operation=self._context.operation,
            request_id=request_id,
            extra=self._context.extra,
        )
        new_logger._log_format = self._log_format
        return new_logger

    def _log(self, level: int, msg: str, **kwargs: Any) -> None:
        """Log a message with context.

        Args:
            level: Logging level
            msg: Log message
            **kwargs: Additional context fields
        """
        record = self._logger.makeRecord(
            self._logger.name,
            level,
            "(unknown file)",
            0,
            msg,
            (),
            None,
        )

        # Add context with any extra kwargs
        if kwargs:
            record.context = self._context.with_extra(**kwargs)
        else:
            record.context = self._context

        self._logger.handle(record)

    def debug(self, msg: str, **kwargs: Any) -> None:
        """Log a debug message."""
        self._log(logging.DEBUG, msg, **kwargs)

    def info(self, msg: str, **kwargs: Any) -> None:
        """Log an info message."""
        self._log(logging.INFO, msg, **kwargs)

    def warning(self, msg: str, **kwargs: Any) -> None:
        """Log a warning message."""
        self._log(logging.WARNING, msg, **kwargs)

    def error(self, msg: str, **kwargs: Any) -> None:
        """Log an error message."""
        self._log(logging.ERROR, msg, **kwargs)

    def exception(self, msg: str, **kwargs: Any) -> None:
        """Log an exception with traceback."""
        self._log(logging.ERROR, msg, **kwargs)
        # Note: In production, you'd want to capture exc_info here

    @contextmanager
    def timed(self, operation: str, **kwargs: Any):
        """Context manager for timing operations.

        Args:
            operation: Name of the operation being timed
            **kwargs: Additional context fields

        Yields:
            Dict where 'elapsed_ms' will be set after completion
        """
        start = time.perf_counter()
        result: dict[str, Any] = {}
        try:
            yield result
        finally:
            elapsed_ms = (time.perf_counter() - start) * 1000
            result["elapsed_ms"] = elapsed_ms
            self.info(
                f"{operation} completed",
                operation=operation,
                elapsed_ms=f"{elapsed_ms:.2f}",
                **kwargs,
            )


# Global loggers for common components
_loggers: dict[str, MossLogger] = {}


def get_logger(
    name: str,
    level: int = logging.INFO,
    log_format: LogFormat = LogFormat.TEXT,
) -> MossLogger:
    """Get or create a logger for a component.

    Args:
        name: Component name
        level: Logging level
        log_format: Output format

    Returns:
        MossLogger instance
    """
    if name not in _loggers:
        _loggers[name] = MossLogger(name, level, log_format)
    return _loggers[name]


def configure_logging(
    level: int = logging.INFO,
    log_format: LogFormat = LogFormat.TEXT,
) -> None:
    """Configure global logging settings.

    Args:
        level: Default logging level
        log_format: Default output format
    """
    # Configure root moss logger
    root = logging.getLogger("moss")
    root.setLevel(level)

    # Clear existing handlers
    root.handlers.clear()

    # Add new handler
    handler = logging.StreamHandler(sys.stderr)
    handler.setLevel(level)

    if log_format == LogFormat.JSON:
        handler.setFormatter(StructuredFormatter())
    else:
        handler.setFormatter(TextFormatter("%(asctime)s %(levelname)s %(message)s"))

    root.addHandler(handler)


# Convenience function for quick logging
def log_event(
    component: str,
    event: str,
    level: int = logging.INFO,
    **kwargs: Any,
) -> None:
    """Log a single event quickly.

    Args:
        component: Component name
        event: Event message
        level: Logging level
        **kwargs: Additional context fields
    """
    logger = get_logger(component)
    logger._log(level, event, **kwargs)
