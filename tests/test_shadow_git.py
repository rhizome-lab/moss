"""Tests for Shadow Git wrapper."""

import asyncio
from pathlib import Path

import pytest

from moss.shadow_git import CommitHandle, DiffHunk, GitError, ShadowGit, parse_diff


@pytest.fixture
async def git_repo(tmp_path: Path):
    """Create a temporary git repository."""
    repo = tmp_path / "repo"
    repo.mkdir()

    # Initialize repo
    proc = await asyncio.create_subprocess_exec(
        "git",
        "init",
        cwd=repo,
        stdout=asyncio.subprocess.DEVNULL,
        stderr=asyncio.subprocess.DEVNULL,
    )
    await proc.wait()

    # Configure git user
    proc = await asyncio.create_subprocess_exec(
        "git",
        "config",
        "user.email",
        "test@test.com",
        cwd=repo,
    )
    await proc.wait()
    proc = await asyncio.create_subprocess_exec(
        "git",
        "config",
        "user.name",
        "Test User",
        cwd=repo,
    )
    await proc.wait()

    # Create initial commit
    (repo / "README.md").write_text("# Test Repo")
    proc = await asyncio.create_subprocess_exec("git", "add", "-A", cwd=repo)
    await proc.wait()
    proc = await asyncio.create_subprocess_exec(
        "git",
        "commit",
        "-m",
        "Initial commit",
        cwd=repo,
    )
    await proc.wait()

    return repo


@pytest.fixture
def shadow_git(git_repo: Path):
    """Create ShadowGit instance."""
    return ShadowGit(git_repo)


class TestShadowGit:
    """Tests for ShadowGit."""

    async def test_create_shadow_branch(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        assert branch.name == "shadow/test"
        assert branch.base_branch == "master"
        assert branch.repo_path == git_repo
        assert branch.commits == []

    async def test_create_shadow_branch_auto_name(self, shadow_git: ShadowGit):
        branch = await shadow_git.create_shadow_branch()

        assert branch.name.startswith("shadow/")

    async def test_commit_creates_handle(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        # Make a change
        (git_repo / "file.txt").write_text("hello")

        handle = await shadow_git.commit(branch, "Add file")

        assert handle.sha is not None
        assert handle.message == "Add file"
        assert handle.branch == "shadow/test"
        assert handle in branch.commits

    async def test_commit_no_changes_raises(self, shadow_git: ShadowGit):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        with pytest.raises(GitError, match="Nothing to commit"):
            await shadow_git.commit(branch, "Empty commit")

    async def test_commit_allow_empty(self, shadow_git: ShadowGit):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        handle = await shadow_git.commit(branch, "Empty commit", allow_empty=True)

        assert handle.sha is not None

    async def test_multiple_commits(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        (git_repo / "file1.txt").write_text("one")
        await shadow_git.commit(branch, "First")

        (git_repo / "file2.txt").write_text("two")
        await shadow_git.commit(branch, "Second")

        assert len(branch.commits) == 2
        assert branch.commits[0].message == "First"
        assert branch.commits[1].message == "Second"

    async def test_rollback(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        (git_repo / "file1.txt").write_text("one")
        await shadow_git.commit(branch, "First")

        (git_repo / "file2.txt").write_text("two")
        await shadow_git.commit(branch, "Second")

        await shadow_git.rollback(branch, steps=1)

        assert len(branch.commits) == 1
        assert not (git_repo / "file2.txt").exists()

    async def test_rollback_multiple(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        for i in range(3):
            (git_repo / f"file{i}.txt").write_text(str(i))
            await shadow_git.commit(branch, f"Commit {i}")

        await shadow_git.rollback(branch, steps=2)

        assert len(branch.commits) == 1
        assert (git_repo / "file0.txt").exists()
        assert not (git_repo / "file1.txt").exists()
        assert not (git_repo / "file2.txt").exists()

    async def test_rollback_too_many_raises(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        (git_repo / "file.txt").write_text("one")
        await shadow_git.commit(branch, "First")

        with pytest.raises(ValueError, match="Cannot rollback 5 commits"):
            await shadow_git.rollback(branch, steps=5)

    async def test_rollback_to_specific_commit(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        (git_repo / "file1.txt").write_text("one")
        first = await shadow_git.commit(branch, "First")

        (git_repo / "file2.txt").write_text("two")
        await shadow_git.commit(branch, "Second")

        (git_repo / "file3.txt").write_text("three")
        await shadow_git.commit(branch, "Third")

        await shadow_git.rollback_to(branch, first)

        assert len(branch.commits) == 1
        assert (git_repo / "file1.txt").exists()
        assert not (git_repo / "file2.txt").exists()

    async def test_squash_merge(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        (git_repo / "file1.txt").write_text("one")
        await shadow_git.commit(branch, "First")

        (git_repo / "file2.txt").write_text("two")
        await shadow_git.commit(branch, "Second")

        handle = await shadow_git.squash_merge(branch, "Merged feature")

        assert handle.branch == "master"
        assert (git_repo / "file1.txt").exists()
        assert (git_repo / "file2.txt").exists()

    async def test_squash_merge_no_commits_raises(self, shadow_git: ShadowGit):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        with pytest.raises(GitError, match="No commits to merge"):
            await shadow_git.squash_merge(branch, "Empty merge")

    async def test_abort_branch(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        (git_repo / "file.txt").write_text("one")
        await shadow_git.commit(branch, "First")

        await shadow_git.abort(branch)

        # Branch should be deleted
        assert shadow_git.get_branch("shadow/test") is None
        # Should be back on master
        proc = await asyncio.create_subprocess_exec(
            "git",
            "rev-parse",
            "--abbrev-ref",
            "HEAD",
            cwd=git_repo,
            stdout=asyncio.subprocess.PIPE,
        )
        stdout, _ = await proc.communicate()
        assert stdout.decode().strip() == "master"

    async def test_diff(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        (git_repo / "file.txt").write_text("hello world")
        await shadow_git.commit(branch, "Add file")

        diff = await shadow_git.diff(branch)

        assert "+hello world" in diff

    async def test_diff_stat(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        (git_repo / "file.txt").write_text("hello world")
        await shadow_git.commit(branch, "Add file")

        stat = await shadow_git.diff_stat(branch)

        assert "file.txt" in stat

    async def test_active_branches(self, shadow_git: ShadowGit):
        branch1 = await shadow_git.create_shadow_branch("shadow/one")
        # Need to go back to master to create another branch
        await shadow_git._run_git("checkout", "master")
        branch2 = await shadow_git.create_shadow_branch("shadow/two")

        branches = shadow_git.active_branches

        assert len(branches) == 2
        assert branch1 in branches
        assert branch2 in branches


class TestCommitHandle:
    """Tests for CommitHandle."""

    def test_commit_handle_frozen(self):
        handle = CommitHandle(
            sha="abc123",
            message="test",
            timestamp=None,  # type: ignore[arg-type]
            branch="main",
        )
        with pytest.raises(AttributeError):
            handle.sha = "def456"  # type: ignore[misc]


class TestDiffHunk:
    """Tests for DiffHunk dataclass."""

    def test_is_addition(self):
        hunk = DiffHunk(
            file_path="test.py",
            old_start=0,
            old_count=0,
            new_start=1,
            new_count=3,
            content="+line1\n+line2\n+line3",
            header="@@ -0,0 +1,3 @@",
        )
        assert hunk.is_addition
        assert not hunk.is_deletion

    def test_is_deletion(self):
        hunk = DiffHunk(
            file_path="test.py",
            old_start=1,
            old_count=3,
            new_start=0,
            new_count=0,
            content="-line1\n-line2\n-line3",
            header="@@ -1,3 +0,0 @@",
        )
        assert hunk.is_deletion
        assert not hunk.is_addition

    def test_lines_changed(self):
        hunk = DiffHunk(
            file_path="test.py",
            old_start=1,
            old_count=2,
            new_start=1,
            new_count=2,
            content="-old line\n+new line\n context",
            header="@@ -1,2 +1,2 @@",
        )
        removed, added = hunk.lines_changed()
        assert removed == ["old line"]
        assert added == ["new line"]


class TestParseDiff:
    """Tests for parse_diff function."""

    def test_parse_simple_diff(self):
        diff = """diff --git a/file.txt b/file.txt
index abc123..def456 100644
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,4 @@
 line1
+new line
 line2
 line3"""
        hunks = parse_diff(diff)
        assert len(hunks) == 1
        assert hunks[0].file_path == "file.txt"
        assert hunks[0].old_start == 1
        assert hunks[0].new_count == 4

    def test_parse_multiple_hunks(self):
        diff = """diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,4 @@
 line1
+added
 line2
@@ -10,2 +11,3 @@
 line10
+another
 line11"""
        hunks = parse_diff(diff)
        assert len(hunks) == 2
        assert hunks[0].old_start == 1
        assert hunks[1].old_start == 10

    def test_parse_multiple_files(self):
        diff = """diff --git a/file1.txt b/file1.txt
--- a/file1.txt
+++ b/file1.txt
@@ -1 +1,2 @@
 line1
+added
diff --git a/file2.txt b/file2.txt
--- a/file2.txt
+++ b/file2.txt
@@ -1 +1 @@
-old
+new"""
        hunks = parse_diff(diff)
        assert len(hunks) == 2
        assert hunks[0].file_path == "file1.txt"
        assert hunks[1].file_path == "file2.txt"


class TestHunkLevelRollback:
    """Tests for hunk-level rollback functionality."""

    async def test_get_hunks(self, shadow_git: ShadowGit, git_repo: Path):
        branch = await shadow_git.create_shadow_branch("shadow/test")

        # Create a file with multiple lines
        (git_repo / "code.py").write_text("line1\nline2\nline3\n")
        await shadow_git.commit(branch, "Add code")

        hunks = await shadow_git.get_hunks(branch)

        assert len(hunks) >= 1
        assert hunks[0].file_path == "code.py"

    async def test_rollback_hunks_single(self, shadow_git: ShadowGit, git_repo: Path):
        # Start on master, create file
        (git_repo / "code.py").write_text("line1\nline2\nline3\n")
        await shadow_git._run_git("add", "-A")
        await shadow_git._run_git("commit", "-m", "Add code")

        branch = await shadow_git.create_shadow_branch("shadow/test")

        # Modify two separate parts
        (git_repo / "code.py").write_text("NEW1\nline2\nNEW3\n")
        await shadow_git.commit(branch, "Modify code")

        hunks = await shadow_git.get_hunks(branch)

        # Revert only the first hunk
        if hunks:
            reverted = await shadow_git.rollback_hunks(branch, [hunks[0]])
            assert reverted >= 1

    async def test_rollback_hunks_preserves_others(self, shadow_git: ShadowGit, git_repo: Path):
        # Create two files on master
        (git_repo / "keep.txt").write_text("keep this\n")
        (git_repo / "revert.txt").write_text("revert this\n")
        await shadow_git._run_git("add", "-A")
        await shadow_git._run_git("commit", "-m", "Add files")

        branch = await shadow_git.create_shadow_branch("shadow/test")

        # Modify both files
        (git_repo / "keep.txt").write_text("modified keep\n")
        (git_repo / "revert.txt").write_text("modified revert\n")
        await shadow_git.commit(branch, "Modify both")

        hunks = await shadow_git.get_hunks(branch)

        # Find and revert only the revert.txt hunk
        revert_hunks = [h for h in hunks if h.file_path == "revert.txt"]
        if revert_hunks:
            await shadow_git.rollback_hunks(branch, revert_hunks)

            # keep.txt should still be modified
            assert "modified keep" in (git_repo / "keep.txt").read_text()
            # revert.txt should be reverted
            assert "revert this" in (git_repo / "revert.txt").read_text()

    async def test_rollback_on_verification_failure(self, shadow_git: ShadowGit, git_repo: Path):
        """Integration test: roll back hunks that cause verification failure.

        Scenario:
        1. Agent makes changes to two separate files
        2. Validation (syntax check) fails for one file
        3. Only the failing file's hunks are rolled back
        4. The passing changes are preserved
        """
        # Create two Python files on master
        (git_repo / "good.py").write_text("x = 42\n")
        (git_repo / "bad.py").write_text('msg = "hello"\n')
        await shadow_git._run_git("add", "-A")
        await shadow_git._run_git("commit", "-m", "Add files")

        branch = await shadow_git.create_shadow_branch("shadow/test")

        # Agent modifies both - one valid, one with syntax error
        (git_repo / "good.py").write_text("x = 42 * 2  # Valid change\n")
        (git_repo / "bad.py").write_text('msg = "hello  # Syntax error\n')
        await shadow_git.commit(branch, "Modify files")

        # Simulate verification: compile each file
        bad_path = git_repo / "bad.py"
        try:
            compile(bad_path.read_text(), bad_path, "exec")
            syntax_ok = True
        except SyntaxError:
            syntax_ok = False

        assert not syntax_ok, "Expected syntax error"

        # Get hunks and find the failing file's hunks
        hunks = await shadow_git.get_hunks(branch)
        assert len(hunks) >= 2, f"Expected 2 hunks, got {len(hunks)}"

        failing_hunks = [h for h in hunks if h.file_path == "bad.py"]
        assert len(failing_hunks) >= 1

        # Roll back only the failing hunks
        reverted = await shadow_git.rollback_hunks(branch, failing_hunks)
        assert reverted >= 1

        # Verify: bad.py should be reverted to original
        assert 'msg = "hello"' in (git_repo / "bad.py").read_text()

        # good.py should still have the valid change
        assert "42 * 2" in (git_repo / "good.py").read_text()

    async def test_map_hunks_to_symbols(self, shadow_git: ShadowGit, git_repo: Path):
        """Test that hunks can be mapped to AST symbols."""
        # Create Python file with function
        (git_repo / "code.py").write_text("def my_func():\n    pass\n")
        await shadow_git._run_git("add", "-A")
        await shadow_git._run_git("commit", "-m", "Add function")

        branch = await shadow_git.create_shadow_branch("shadow/test")

        # Modify function body
        (git_repo / "code.py").write_text("def my_func():\n    return 42\n")
        await shadow_git.commit(branch, "Modify function")

        hunks = await shadow_git.get_hunks(branch)
        mapped = await shadow_git.map_hunks_to_symbols(hunks)

        # Should have symbol info if tree-sitter is available
        if mapped and mapped[0].symbol:
            assert "my_func" in mapped[0].symbol
