"""Tests for CLI interface."""

import subprocess
from pathlib import Path

import pytest

from moss.cli import (
    cmd_anchors,
    cmd_cfg,
    cmd_config,
    cmd_context,
    cmd_deps,
    cmd_distros,
    cmd_init,
    cmd_query,
    cmd_run,
    cmd_skeleton,
    cmd_status,
    create_parser,
    main,
)


class TestCreateParser:
    """Tests for create_parser."""

    def test_creates_parser(self):
        parser = create_parser()
        assert parser is not None
        assert parser.prog == "moss"

    def test_has_version(self):
        parser = create_parser()
        # Version action exists
        assert any(action.option_strings == ["--version"] for action in parser._actions)

    def test_has_subcommands(self):
        parser = create_parser()
        # Check subparsers exist
        subparsers_action = next((a for a in parser._actions if hasattr(a, "_parser_class")), None)
        assert subparsers_action is not None
        assert "init" in subparsers_action.choices
        assert "run" in subparsers_action.choices
        assert "status" in subparsers_action.choices
        assert "config" in subparsers_action.choices
        assert "distros" in subparsers_action.choices
        # New introspection commands
        assert "skeleton" in subparsers_action.choices
        assert "anchors" in subparsers_action.choices
        assert "query" in subparsers_action.choices
        assert "cfg" in subparsers_action.choices
        assert "deps" in subparsers_action.choices
        assert "context" in subparsers_action.choices
        assert "mcp-server" in subparsers_action.choices


class TestMain:
    """Tests for main entry point."""

    def test_no_command_shows_help(self, capsys):
        result = main([])
        assert result == 0
        captured = capsys.readouterr()
        assert "usage:" in captured.out.lower()

    def test_version(self):
        with pytest.raises(SystemExit) as exc:
            main(["--version"])
        assert exc.value.code == 0


class TestCmdInit:
    """Tests for init command."""

    def test_creates_config_file(self, tmp_path: Path):
        args = create_parser().parse_args(["init", str(tmp_path)])
        result = cmd_init(args)

        assert result == 0
        config_file = tmp_path / "moss_config.py"
        assert config_file.exists()

        content = config_file.read_text()
        assert "MossConfig" in content
        assert "with_project" in content

    def test_creates_moss_directory(self, tmp_path: Path):
        args = create_parser().parse_args(["init", str(tmp_path)])
        cmd_init(args)

        moss_dir = tmp_path / ".moss"
        assert moss_dir.exists()
        assert (moss_dir / ".gitignore").exists()

    def test_refuses_to_overwrite(self, tmp_path: Path):
        config_file = tmp_path / "moss_config.py"
        config_file.write_text("existing")

        args = create_parser().parse_args(["init", str(tmp_path)])
        result = cmd_init(args)

        assert result == 1

    def test_force_overwrites(self, tmp_path: Path):
        config_file = tmp_path / "moss_config.py"
        config_file.write_text("existing")

        args = create_parser().parse_args(["init", str(tmp_path), "--force"])
        result = cmd_init(args)

        assert result == 0
        assert "MossConfig" in config_file.read_text()

    def test_custom_distro(self, tmp_path: Path):
        args = create_parser().parse_args(["init", str(tmp_path), "--distro", "strict"])
        cmd_init(args)

        config_file = tmp_path / "moss_config.py"
        content = config_file.read_text()
        assert '"strict"' in content

    def test_nonexistent_directory(self, tmp_path: Path):
        args = create_parser().parse_args(["init", str(tmp_path / "nonexistent")])
        result = cmd_init(args)

        assert result == 1


class TestCmdStatus:
    """Tests for status command."""

    @pytest.fixture
    def git_repo(self, tmp_path: Path):
        """Create a minimal git repo."""
        repo = tmp_path / "repo"
        repo.mkdir()

        subprocess.run(["git", "init"], cwd=repo, capture_output=True, check=True)
        subprocess.run(["git", "config", "user.email", "test@test.com"], cwd=repo, check=True)
        subprocess.run(["git", "config", "user.name", "Test User"], cwd=repo, check=True)
        (repo / "README.md").write_text("# Test")
        subprocess.run(["git", "add", "-A"], cwd=repo, check=True)
        subprocess.run(
            ["git", "commit", "-m", "Initial"], cwd=repo, capture_output=True, check=True
        )

        return repo

    def test_shows_status(self, git_repo: Path, capsys):
        args = create_parser().parse_args(["status", "-C", str(git_repo)])
        result = cmd_status(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "Moss Status" in captured.out
        assert "Active requests:" in captured.out
        assert "Active workers:" in captured.out


class TestCmdConfig:
    """Tests for config command."""

    def test_list_distros(self, capsys):
        args = create_parser().parse_args(["config", "--list-distros"])
        result = cmd_config(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "python" in captured.out

    def test_no_config_file(self, tmp_path: Path, capsys):
        args = create_parser().parse_args(["config", "-C", str(tmp_path)])
        result = cmd_config(args)

        assert result == 1
        captured = capsys.readouterr()
        assert "No config file" in captured.out

    def test_shows_config(self, tmp_path: Path, capsys):
        # Create a config
        config_file = tmp_path / "moss_config.py"
        config_file.write_text("""
from pathlib import Path
from moss.config import MossConfig

config = MossConfig().with_project(Path(__file__).parent, "test-project")
""")

        args = create_parser().parse_args(["config", "-C", str(tmp_path)])
        result = cmd_config(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "Configuration" in captured.out
        assert "test-project" in captured.out

    def test_validate_config(self, tmp_path: Path, capsys):
        # Create a valid config
        config_file = tmp_path / "moss_config.py"
        config_file.write_text("""
from pathlib import Path
from moss.config import MossConfig

config = MossConfig().with_project(Path(__file__).parent, "test-project")
""")

        args = create_parser().parse_args(["config", "-C", str(tmp_path), "--validate"])
        result = cmd_config(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "valid" in captured.out.lower()


class TestCmdDistros:
    """Tests for distros command."""

    def test_lists_distros(self, capsys):
        args = create_parser().parse_args(["distros"])
        result = cmd_distros(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "Available Distros" in captured.out
        assert "python" in captured.out
        assert "strict" in captured.out
        assert "lenient" in captured.out
        assert "fast" in captured.out


class TestCmdRun:
    """Tests for run command."""

    @pytest.fixture
    def git_repo(self, tmp_path: Path):
        """Create a minimal git repo."""
        repo = tmp_path / "repo"
        repo.mkdir()

        subprocess.run(["git", "init"], cwd=repo, capture_output=True, check=True)
        subprocess.run(["git", "config", "user.email", "test@test.com"], cwd=repo, check=True)
        subprocess.run(["git", "config", "user.name", "Test User"], cwd=repo, check=True)
        (repo / "README.md").write_text("# Test")
        subprocess.run(["git", "add", "-A"], cwd=repo, check=True)
        subprocess.run(
            ["git", "commit", "-m", "Initial"], cwd=repo, capture_output=True, check=True
        )

        return repo

    def test_creates_task(self, git_repo: Path, capsys):
        args = create_parser().parse_args(
            [
                "run",
                "Test task",
                "-C",
                str(git_repo),
            ]
        )
        result = cmd_run(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "Task created:" in captured.out
        assert "Ticket:" in captured.out

    def test_with_priority(self, git_repo: Path, capsys):
        args = create_parser().parse_args(
            [
                "run",
                "High priority task",
                "-C",
                str(git_repo),
                "--priority",
                "high",
            ]
        )
        result = cmd_run(args)

        assert result == 0

    def test_with_constraints(self, git_repo: Path, capsys):
        args = create_parser().parse_args(
            [
                "run",
                "Constrained task",
                "-C",
                str(git_repo),
                "-c",
                "no-tests",
                "-c",
                "dry-run",
            ]
        )
        result = cmd_run(args)

        assert result == 0


class TestCmdSkeleton:
    """Tests for skeleton command."""

    @pytest.fixture
    def python_file(self, tmp_path: Path):
        """Create a Python file for testing."""
        py_file = tmp_path / "sample.py"
        py_file.write_text('''
"""Module docstring."""

class Foo:
    """A class."""
    def bar(self, x: int) -> str:
        """A method."""
        return str(x)

def baz():
    """A function."""
    pass
''')
        return py_file

    def test_extracts_skeleton(self, python_file: Path, capsys):
        args = create_parser().parse_args(["skeleton", str(python_file)])
        result = cmd_skeleton(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "class Foo" in captured.out
        assert "def bar" in captured.out
        assert "def baz" in captured.out

    def test_json_output(self, python_file: Path, capsys):
        args = create_parser().parse_args(["--json", "skeleton", str(python_file)])
        result = cmd_skeleton(args)

        assert result == 0
        captured = capsys.readouterr()
        # Should be valid JSON
        import json

        data = json.loads(captured.out)
        assert "file" in data
        assert "symbols" in data
        assert any(s["name"] == "Foo" for s in data["symbols"])

    def test_handles_syntax_error(self, tmp_path: Path, capsys):
        bad_file = tmp_path / "bad.py"
        bad_file.write_text("def broken(")

        args = create_parser().parse_args(["skeleton", str(bad_file)])
        result = cmd_skeleton(args)

        assert result == 0  # Continues despite error
        captured = capsys.readouterr()
        assert "Error in" in captured.err  # Error reported via plugin system

    def test_directory_with_pattern(self, tmp_path: Path, capsys):
        (tmp_path / "a.py").write_text("def foo(): pass")
        (tmp_path / "b.py").write_text("def bar(): pass")
        (tmp_path / "c.txt").write_text("not python")

        args = create_parser().parse_args(["skeleton", str(tmp_path), "-p", "*.py"])
        result = cmd_skeleton(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "def foo" in captured.out
        assert "def bar" in captured.out


class TestCmdAnchors:
    """Tests for anchors command."""

    @pytest.fixture
    def python_file(self, tmp_path: Path):
        """Create a Python file for testing."""
        py_file = tmp_path / "sample.py"
        py_file.write_text("""
class MyClass:
    def method(self): pass

def my_function():
    pass
""")
        return py_file

    def test_finds_all_anchors(self, python_file: Path, capsys):
        args = create_parser().parse_args(["anchors", str(python_file)])
        result = cmd_anchors(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "MyClass" in captured.out
        assert "method" in captured.out
        assert "my_function" in captured.out

    def test_filter_by_type(self, python_file: Path, capsys):
        args = create_parser().parse_args(["anchors", str(python_file), "-t", "class"])
        result = cmd_anchors(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "MyClass" in captured.out
        assert "my_function" not in captured.out

    def test_json_output(self, python_file: Path, capsys):
        args = create_parser().parse_args(["--json", "anchors", str(python_file)])
        result = cmd_anchors(args)

        assert result == 0
        captured = capsys.readouterr()
        import json

        data = json.loads(captured.out)
        assert isinstance(data, list)
        assert len(data) > 0
        assert "name" in data[0]
        assert "type" in data[0]


class TestCmdCfg:
    """Tests for cfg command."""

    @pytest.fixture
    def python_file(self, tmp_path: Path):
        """Create a Python file with control flow."""
        py_file = tmp_path / "sample.py"
        py_file.write_text("""
def check(x):
    if x > 0:
        return "positive"
    else:
        return "non-positive"
""")
        return py_file

    def test_builds_cfg(self, python_file: Path, capsys):
        args = create_parser().parse_args(["cfg", str(python_file)])
        result = cmd_cfg(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "CFG for check" in captured.out
        assert "ENTRY" in captured.out
        assert "EXIT" in captured.out

    def test_specific_function(self, python_file: Path, capsys):
        args = create_parser().parse_args(["cfg", str(python_file), "check"])
        result = cmd_cfg(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "CFG for check" in captured.out

    def test_json_output(self, python_file: Path, capsys):
        args = create_parser().parse_args(["--json", "cfg", str(python_file)])
        result = cmd_cfg(args)

        assert result == 0
        captured = capsys.readouterr()
        import json

        data = json.loads(captured.out)
        assert isinstance(data, list)
        assert len(data) > 0
        assert "name" in data[0]
        assert "nodes" in data[0]

    def test_dot_output(self, python_file: Path, capsys):
        args = create_parser().parse_args(["cfg", str(python_file), "--dot"])
        result = cmd_cfg(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "digraph" in captured.out
        assert "->" in captured.out


class TestCmdDeps:
    """Tests for deps command."""

    @pytest.fixture
    def python_file(self, tmp_path: Path):
        """Create a Python file with imports."""
        py_file = tmp_path / "sample.py"
        py_file.write_text("""
import os
from pathlib import Path

def public_func():
    pass

class PublicClass:
    pass
""")
        return py_file

    def test_extracts_deps(self, python_file: Path, capsys):
        args = create_parser().parse_args(["deps", str(python_file)])
        result = cmd_deps(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "os" in captured.out
        assert "pathlib" in captured.out or "Path" in captured.out
        assert "public_func" in captured.out
        assert "PublicClass" in captured.out

    def test_json_output(self, python_file: Path, capsys):
        args = create_parser().parse_args(["--json", "deps", str(python_file)])
        result = cmd_deps(args)

        assert result == 0
        captured = capsys.readouterr()
        import json

        data = json.loads(captured.out)
        assert "file" in data
        assert "imports" in data
        assert "exports" in data


class TestCmdContext:
    """Tests for context command."""

    @pytest.fixture
    def python_file(self, tmp_path: Path):
        """Create a Python file for testing."""
        py_file = tmp_path / "sample.py"
        py_file.write_text('''
"""A sample module."""

import os

class Foo:
    """A class."""
    def bar(self): pass

def baz():
    """A function."""
    pass
''')
        return py_file

    def test_shows_context(self, python_file: Path, capsys):
        args = create_parser().parse_args(["context", str(python_file)])
        result = cmd_context(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "Lines:" in captured.out
        assert "Classes:" in captured.out
        assert "Imports:" in captured.out
        assert "Skeleton" in captured.out
        assert "Foo" in captured.out

    def test_json_output(self, python_file: Path, capsys):
        args = create_parser().parse_args(["--json", "context", str(python_file)])
        result = cmd_context(args)

        assert result == 0
        captured = capsys.readouterr()
        import json

        data = json.loads(captured.out)
        assert "file" in data
        assert "summary" in data
        assert "symbols" in data
        assert "imports" in data
        assert "exports" in data
        assert data["summary"]["classes"] >= 1
        assert data["summary"]["functions"] >= 1


class TestCmdQuery:
    """Tests for query command."""

    @pytest.fixture
    def python_file(self, tmp_path: Path):
        """Create a Python file for testing."""
        py_file = tmp_path / "sample.py"
        py_file.write_text("""
class BaseClass:
    '''A base class.'''
    pass

class ChildClass(BaseClass):
    '''A child class.'''
    def method(self): pass

def my_function(x: int) -> str:
    '''A function.'''
    return str(x)

def other_function():
    pass
""")
        return py_file

    def test_finds_by_name(self, python_file: Path, capsys):
        args = create_parser().parse_args(["query", str(python_file), "--name", "my_.*"])
        result = cmd_query(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "my_function" in captured.out
        assert "other_function" not in captured.out

    def test_finds_by_type(self, python_file: Path, capsys):
        args = create_parser().parse_args(["query", str(python_file), "--type", "class"])
        result = cmd_query(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "BaseClass" in captured.out
        assert "ChildClass" in captured.out
        assert "my_function" not in captured.out

    def test_finds_by_inheritance(self, python_file: Path, capsys):
        args = create_parser().parse_args(
            ["--json", "query", str(python_file), "--inherits", "BaseClass", "--type", "class"]
        )
        result = cmd_query(args)

        assert result == 0
        captured = capsys.readouterr()
        import json

        data = json.loads(captured.out)
        names = [r["name"] for r in data]
        assert "ChildClass" in names
        # BaseClass doesn't inherit from BaseClass, so it shouldn't be in results
        assert "BaseClass" not in names

    def test_finds_by_signature(self, python_file: Path, capsys):
        args = create_parser().parse_args(["query", str(python_file), "--signature", r"x:\s*int"])
        result = cmd_query(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "my_function" in captured.out

    def test_json_output(self, python_file: Path, capsys):
        args = create_parser().parse_args(
            ["--json", "query", str(python_file), "--type", "function"]
        )
        result = cmd_query(args)

        assert result == 0
        captured = capsys.readouterr()
        import json

        data = json.loads(captured.out)
        assert isinstance(data, list)
        names = [r["name"] for r in data]
        assert "my_function" in names
        assert "other_function" in names

    def test_no_matches(self, python_file: Path, capsys):
        args = create_parser().parse_args(["query", str(python_file), "--name", "nonexistent"])
        result = cmd_query(args)

        assert result == 0
        captured = capsys.readouterr()
        assert "No matches found" in captured.out
