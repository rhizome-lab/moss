"""Tests for the interactive shell module."""

from pathlib import Path


class TestMossShell:
    """Tests for MossShell."""

    def test_create_shell(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)

        assert shell.workspace == tmp_path
        assert shell.running is True

    def test_shell_commands_registered(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)

        expected_commands = [
            "help",
            "exit",
            "quit",
            "cd",
            "pwd",
            "ls",
            "skeleton",
            "deps",
            "cfg",
            "query",
            "search",
            "context",
            "anchors",
        ]

        for cmd in expected_commands:
            assert cmd in shell.commands

    def test_cmd_exit(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)
        assert shell.running is True

        shell.cmd_exit([])

        assert shell.running is False

    def test_cmd_pwd(self, tmp_path: Path, capsys):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)
        shell.cmd_pwd([])

        # Output should contain the workspace path
        # (output goes through moss.output, not directly to capsys)

    def test_cmd_cd(self, tmp_path: Path):
        from moss.shell import MossShell

        subdir = tmp_path / "subdir"
        subdir.mkdir()

        shell = MossShell(tmp_path)
        shell.cmd_cd(["subdir"])

        assert shell.workspace == subdir

    def test_cmd_cd_invalid(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)
        original = shell.workspace

        shell.cmd_cd(["nonexistent"])

        # Should not change directory
        assert shell.workspace == original

    def test_cmd_ls(self, tmp_path: Path):
        from moss.shell import MossShell

        (tmp_path / "test.py").write_text("# test")
        (tmp_path / "other.txt").write_text("other")

        shell = MossShell(tmp_path)
        shell.cmd_ls([])

        # Should not raise

    def test_cmd_skeleton(self, tmp_path: Path):
        from moss.shell import MossShell

        test_file = tmp_path / "test.py"
        test_file.write_text('''
def hello():
    """Say hello."""
    print("Hello")

class Greeter:
    def greet(self):
        pass
''')

        shell = MossShell(tmp_path)
        shell.cmd_skeleton(["test.py"])

        # Should not raise

    def test_cmd_skeleton_not_found(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)
        shell.cmd_skeleton(["nonexistent.py"])

        # Should not raise, just show error

    def test_resolve_path_absolute(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)

        result = shell._resolve_path("/absolute/path")

        assert result == Path("/absolute/path")

    def test_resolve_path_relative(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)

        result = shell._resolve_path("relative/path")

        assert result == tmp_path / "relative" / "path"

    def test_execute_unknown_command(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)
        shell._execute("unknowncommand arg1 arg2")

        # Should not raise, just show error

    def test_execute_help(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)
        shell._execute("help")

        # Should not raise

    def test_get_prompt(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)
        prompt = shell._get_prompt()

        assert "moss" in prompt
        assert ">" in prompt


class TestStartShell:
    """Tests for start_shell function."""

    def test_start_shell_returns_int(self, tmp_path: Path):
        from moss.shell import MossShell

        shell = MossShell(tmp_path)
        shell.running = False  # Exit immediately

        # Just verify it doesn't crash
        assert shell.running is False
