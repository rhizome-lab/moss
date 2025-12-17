"""Tests for structured logging module."""

import logging

from moss.logging import (
    LogContext,
    LogFormat,
    MossLogger,
    configure_logging,
    get_logger,
    log_event,
)


class TestLogContext:
    """Tests for LogContext."""

    def test_default_context(self):
        ctx = LogContext()
        assert ctx.component == ""
        assert ctx.operation == ""
        assert ctx.request_id == ""
        assert ctx.extra == {}

    def test_with_extra(self):
        ctx = LogContext(component="test")
        new_ctx = ctx.with_extra(key="value", num=42)

        assert new_ctx.component == "test"
        assert new_ctx.extra["key"] == "value"
        assert new_ctx.extra["num"] == 42
        # Original unchanged
        assert ctx.extra == {}


class TestMossLogger:
    """Tests for MossLogger."""

    def test_create_logger(self):
        logger = MossLogger("test_component")
        assert logger._context.component == "test_component"

    def test_with_context(self):
        logger = MossLogger("test")
        new_logger = logger.with_context(user="alice", action="edit")

        assert new_logger._context.extra["user"] == "alice"
        assert new_logger._context.extra["action"] == "edit"

    def test_with_operation(self):
        logger = MossLogger("test")
        new_logger = logger.with_operation("validate")

        assert new_logger._context.operation == "validate"
        assert new_logger._context.component == "test"

    def test_with_request_id(self):
        logger = MossLogger("test")
        new_logger = logger.with_request_id("req-123")

        assert new_logger._context.request_id == "req-123"

    def test_logging_levels(self, caplog):
        # Use caplog for pytest's log capture
        with caplog.at_level(logging.DEBUG, logger="moss.test_levels"):
            logger = MossLogger("test_levels", level=logging.DEBUG)

            logger.debug("debug message")
            logger.info("info message")
            logger.warning("warning message")
            logger.error("error message")

        assert "debug message" in caplog.text
        assert "info message" in caplog.text
        assert "warning message" in caplog.text
        assert "error message" in caplog.text

    def test_json_format(self):
        # Test that JSON format logger can be created and used
        logger = MossLogger("test_json", log_format=LogFormat.JSON)
        # Just verify it doesn't crash
        logger.info("test message")
        assert logger._log_format == LogFormat.JSON

    def test_timed_context_manager(self, caplog):
        with caplog.at_level(logging.INFO, logger="moss.test_timed"):
            logger = MossLogger("test_timed", level=logging.INFO)

            with logger.timed("test_operation") as result:
                # Do some work
                _sum = sum(range(100))

            assert "elapsed_ms" in result
            assert result["elapsed_ms"] >= 0

        assert "test_operation completed" in caplog.text


class TestGetLogger:
    """Tests for get_logger function."""

    def test_get_logger(self):
        logger = get_logger("my_component")
        assert logger._context.component == "my_component"

    def test_get_logger_cached(self):
        logger1 = get_logger("cached_component")
        logger2 = get_logger("cached_component")
        assert logger1._logger is logger2._logger


class TestConfigureLogging:
    """Tests for configure_logging function."""

    def test_configure_text_format(self):
        configure_logging(level=logging.DEBUG, log_format=LogFormat.TEXT)
        root = logging.getLogger("moss")
        assert root.level == logging.DEBUG

    def test_configure_json_format(self):
        configure_logging(level=logging.INFO, log_format=LogFormat.JSON)
        root = logging.getLogger("moss")
        assert root.level == logging.INFO


class TestLogEvent:
    """Tests for log_event function."""

    def test_log_event(self, caplog):
        with caplog.at_level(logging.INFO, logger="moss.test_event"):
            log_event("test_event", "something happened", level=logging.INFO)

        assert "something happened" in caplog.text
