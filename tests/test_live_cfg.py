"""Tests for live CFG rendering module."""

from pathlib import Path

import pytest


@pytest.fixture
def simple_python_file(tmp_path: Path) -> Path:
    """Create a simple Python file for testing."""
    file = tmp_path / "test.py"
    file.write_text("""
def simple():
    x = 1
    return x


def complex_func(a, b):
    if a > 0:
        if b > 0:
            return a + b
        return a
    return b
""")
    return file


class TestLiveCFGConfig:
    """Tests for LiveCFGConfig."""

    def test_default_config(self):
        from moss.live_cfg import LiveCFGConfig

        config = LiveCFGConfig()

        assert config.host == "127.0.0.1"
        assert config.port == 8765
        assert config.auto_open is True
        assert config.debounce_ms == 500

    def test_custom_config(self):
        from moss.live_cfg import LiveCFGConfig

        config = LiveCFGConfig(
            host="0.0.0.0",
            port=9000,
            auto_open=False,
            debounce_ms=200,
        )

        assert config.host == "0.0.0.0"
        assert config.port == 9000
        assert config.auto_open is False
        assert config.debounce_ms == 200


class TestCFGState:
    """Tests for CFGState."""

    def test_create_state(self, simple_python_file: Path):
        from moss.live_cfg import CFGState

        state = CFGState(path=simple_python_file)

        assert state.path == simple_python_file
        assert state.function_name is None
        assert state.mermaid == ""
        assert state.cfgs == []

    def test_update_state(self, simple_python_file: Path):
        from moss.live_cfg import CFGState

        state = CFGState(path=simple_python_file)
        state.update()

        assert len(state.cfgs) > 0
        assert state.mermaid != ""
        assert state.error is None
        assert state.last_updated > 0

        # Check we found the functions
        names = [cfg["name"] for cfg in state.cfgs]
        assert "simple" in names
        assert "complex_func" in names

    def test_update_with_function_filter(self, simple_python_file: Path):
        from moss.live_cfg import CFGState

        state = CFGState(path=simple_python_file, function_name="simple")
        state.update()

        assert len(state.cfgs) == 1
        assert state.cfgs[0]["name"] == "simple"

    def test_update_with_invalid_file(self, tmp_path: Path):
        from moss.live_cfg import CFGState

        # File with syntax error
        bad_file = tmp_path / "bad.py"
        bad_file.write_text("def broken(:\n    pass")

        state = CFGState(path=bad_file)
        state.update()

        assert state.error is not None

    def test_to_json(self, simple_python_file: Path):
        from moss.live_cfg import CFGState

        state = CFGState(path=simple_python_file)
        state.update()

        data = state.to_json()

        assert "path" in data
        assert "mermaid" in data
        assert "cfgs" in data
        assert "last_updated" in data
        assert "error" in data


class TestLiveCFGServer:
    """Tests for LiveCFGServer."""

    def test_create_server(self, simple_python_file: Path):
        from moss.live_cfg import LiveCFGConfig, LiveCFGServer

        config = LiveCFGConfig(auto_open=False)
        server = LiveCFGServer(simple_python_file, config=config)

        assert server.path == simple_python_file
        assert server.config.auto_open is False

    def test_initial_state_update(self, simple_python_file: Path):
        from moss.live_cfg import LiveCFGConfig, LiveCFGServer

        config = LiveCFGConfig(auto_open=False)
        server = LiveCFGServer(simple_python_file, config=config)

        # Manually trigger initial update
        server.state.update()

        assert len(server.state.cfgs) > 0


class TestLiveCFGHandler:
    """Tests for HTTP handler."""

    def test_html_template_formatting(self):
        from moss.live_cfg import LIVE_HTML_TEMPLATE

        # Template should be valid HTML
        html = LIVE_HTML_TEMPLATE.format(path="test.py")

        assert "<!DOCTYPE html>" in html
        assert "Live CFG" in html
        assert "mermaid" in html
        assert "test.py" in html


class TestStartLiveCfg:
    """Tests for CLI integration function."""

    def test_function_exists(self):
        from moss.live_cfg import start_live_cfg

        assert callable(start_live_cfg)
