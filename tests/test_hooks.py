"""Tests for pre-commit hook integration."""

import subprocess
from pathlib import Path

import pytest

from moss.hooks import (
    HOOK_SCRIPT,
    check_hooks_installed,
    generate_hook_config,
    get_staged_files,
    install_hooks,
    uninstall_hooks,
)


@pytest.fixture
def git_repo(tmp_path: Path):
    """Create a temporary git repository."""
    subprocess.run(
        ["git", "init"],
        cwd=tmp_path,
        capture_output=True,
        check=True,
    )
    # Configure git for commits
    subprocess.run(
        ["git", "config", "user.email", "test@example.com"],
        cwd=tmp_path,
        capture_output=True,
        check=True,
    )
    subprocess.run(
        ["git", "config", "user.name", "Test User"],
        cwd=tmp_path,
        capture_output=True,
        check=True,
    )
    return tmp_path


class TestInstallHooks:
    """Tests for install_hooks."""

    def test_installs_hook(self, git_repo: Path):
        result = install_hooks(git_repo)

        assert result is True
        hook_path = git_repo / ".git" / "hooks" / "pre-commit"
        assert hook_path.exists()
        assert "Moss pre-commit hook" in hook_path.read_text()

    def test_hook_is_executable(self, git_repo: Path):
        install_hooks(git_repo)

        hook_path = git_repo / ".git" / "hooks" / "pre-commit"
        # Check execute permission
        assert hook_path.stat().st_mode & 0o111

    def test_raises_on_non_git_repo(self, tmp_path: Path):
        with pytest.raises(FileNotFoundError, match="Not a git repository"):
            install_hooks(tmp_path)

    def test_raises_on_existing_hook(self, git_repo: Path):
        # Install once
        install_hooks(git_repo)

        # Try to install again without force
        with pytest.raises(FileExistsError, match="already exists"):
            install_hooks(git_repo)

    def test_force_overwrites_existing(self, git_repo: Path):
        # Install once
        install_hooks(git_repo)

        # Install with force
        result = install_hooks(git_repo, force=True)

        assert result is True


class TestUninstallHooks:
    """Tests for uninstall_hooks."""

    def test_uninstalls_moss_hook(self, git_repo: Path):
        install_hooks(git_repo)

        result = uninstall_hooks(git_repo)

        assert result is True
        hook_path = git_repo / ".git" / "hooks" / "pre-commit"
        assert not hook_path.exists()

    def test_returns_false_if_no_hook(self, git_repo: Path):
        result = uninstall_hooks(git_repo)
        assert result is False

    def test_does_not_remove_non_moss_hook(self, git_repo: Path):
        hook_path = git_repo / ".git" / "hooks" / "pre-commit"
        hook_path.parent.mkdir(exist_ok=True)
        hook_path.write_text("#!/bin/sh\necho 'Not a moss hook'")

        result = uninstall_hooks(git_repo)

        assert result is False
        assert hook_path.exists()

    def test_returns_false_on_non_git_repo(self, tmp_path: Path):
        result = uninstall_hooks(tmp_path)
        assert result is False


class TestCheckHooksInstalled:
    """Tests for check_hooks_installed."""

    def test_returns_true_when_installed(self, git_repo: Path):
        install_hooks(git_repo)

        assert check_hooks_installed(git_repo) is True

    def test_returns_false_when_not_installed(self, git_repo: Path):
        assert check_hooks_installed(git_repo) is False

    def test_returns_false_for_non_moss_hook(self, git_repo: Path):
        hook_path = git_repo / ".git" / "hooks" / "pre-commit"
        hook_path.parent.mkdir(exist_ok=True)
        hook_path.write_text("#!/bin/sh\necho 'Not a moss hook'")

        assert check_hooks_installed(git_repo) is False

    def test_returns_false_for_non_git_repo(self, tmp_path: Path):
        assert check_hooks_installed(tmp_path) is False


class TestGenerateHookConfig:
    """Tests for generate_hook_config."""

    def test_returns_dict(self):
        config = generate_hook_config()

        assert isinstance(config, dict)
        assert "repos" in config

    def test_includes_moss_hooks(self):
        config = generate_hook_config()

        hooks = config["repos"][0]["hooks"]
        hook_ids = [h["id"] for h in hooks]

        assert "moss-skeleton" in hook_ids
        assert "moss-deps" in hook_ids


class TestGetStagedFiles:
    """Tests for get_staged_files."""

    def test_returns_empty_for_no_staged_files(self, git_repo: Path):
        result = get_staged_files(git_repo)
        assert result == []

    def test_returns_staged_python_files(self, git_repo: Path):
        # Create and stage a Python file
        py_file = git_repo / "test.py"
        py_file.write_text("x = 1")
        subprocess.run(
            ["git", "add", "test.py"],
            cwd=git_repo,
            capture_output=True,
            check=True,
        )

        result = get_staged_files(git_repo)

        assert len(result) == 1
        assert result[0].name == "test.py"

    def test_ignores_non_python_files(self, git_repo: Path):
        # Create and stage non-Python files
        txt_file = git_repo / "test.txt"
        txt_file.write_text("hello")
        subprocess.run(
            ["git", "add", "test.txt"],
            cwd=git_repo,
            capture_output=True,
            check=True,
        )

        result = get_staged_files(git_repo)

        assert result == []


class TestHookScript:
    """Tests for the hook script content."""

    def test_script_is_shell_script(self):
        assert HOOK_SCRIPT.startswith("#!/bin/sh")

    def test_script_checks_for_python_files(self):
        assert "grep -E '\\.py$'" in HOOK_SCRIPT

    def test_script_runs_moss_skeleton(self):
        assert "moss" in HOOK_SCRIPT
        assert "skeleton" in HOOK_SCRIPT
