"""Tests for single-tool MCP server."""

from moss.mcp_server import _execute_command, create_server


class TestExecuteCommand:
    """Tests for _execute_command function."""

    def test_executes_help(self):
        """Can execute help command."""
        result = _execute_command("help")
        assert result["exit_code"] == 0
        assert "output" in result
        assert "moss" in result["output"].lower()

    def test_executes_skeleton(self, tmp_path):
        """Can execute skeleton command."""
        py_file = tmp_path / "sample.py"
        py_file.write_text("def foo(): pass\n")

        result = _execute_command(f"skeleton {py_file}")
        assert result["exit_code"] == 0
        assert "foo" in result.get("output", "")

    def test_handles_invalid_command(self):
        """Reports error for invalid command."""
        result = _execute_command("nonexistent-command-xyz")
        assert result["exit_code"] != 0

    def test_handles_empty_command(self):
        """Reports error for empty command."""
        result = _execute_command("")
        assert result["exit_code"] == 1
        assert "error" in result

    def test_handles_malformed_quotes(self):
        """Reports error for malformed quotes."""
        result = _execute_command('skeleton "unclosed')
        assert result["exit_code"] == 1
        assert "error" in result


class TestCreateServer:
    """Tests for server creation."""

    def test_creates_server(self):
        """Can create MCP server."""
        server = create_server()
        assert server is not None
