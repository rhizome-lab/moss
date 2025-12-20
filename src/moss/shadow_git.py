"""Shadow Git: Atomic commits on shadow branches with rollback support.

# See: docs/architecture/overview.md
"""

from __future__ import annotations

import asyncio
import re
import subprocess
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from uuid import UUID, uuid4


@dataclass(frozen=True)
class DiffHunk:
    """A single hunk from a git diff.

    Represents a contiguous block of changes in a file.
    """

    file_path: str
    old_start: int  # Starting line in old file
    old_count: int  # Number of lines in old file
    new_start: int  # Starting line in new file
    new_count: int  # Number of lines in new file
    content: str  # Raw hunk content including +/- prefixes
    header: str  # The @@ line
    symbol: str | None = None  # Containing AST symbol (set by map_hunks_to_symbols)

    @property
    def is_addition(self) -> bool:
        """True if this hunk only adds lines."""
        return self.old_count == 0

    @property
    def is_deletion(self) -> bool:
        """True if this hunk only removes lines."""
        return self.new_count == 0

    def lines_changed(self) -> tuple[list[str], list[str]]:
        """Get (removed_lines, added_lines) from the hunk."""
        removed = []
        added = []
        for line in self.content.split("\n"):
            if line.startswith("-") and not line.startswith("---"):
                removed.append(line[1:])
            elif line.startswith("+") and not line.startswith("+++"):
                added.append(line[1:])
        return removed, added


def parse_diff(diff_output: str) -> list[DiffHunk]:
    """Parse git diff output into individual hunks.

    Args:
        diff_output: Raw output from git diff

    Returns:
        List of DiffHunk objects
    """
    hunks: list[DiffHunk] = []
    current_file: str | None = None

    # Pattern for diff header
    file_pattern = re.compile(r"^diff --git a/(.*) b/(.*)$")
    # Pattern for hunk header: @@ -old_start,old_count +new_start,new_count @@
    hunk_pattern = re.compile(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$")

    lines = diff_output.split("\n")
    i = 0

    while i < len(lines):
        line = lines[i]

        # Check for new file
        file_match = file_pattern.match(line)
        if file_match:
            current_file = file_match.group(2)
            i += 1
            continue

        # Check for hunk
        hunk_match = hunk_pattern.match(line)
        if hunk_match and current_file:
            old_start = int(hunk_match.group(1))
            old_count = int(hunk_match.group(2) or "1")
            new_start = int(hunk_match.group(3))
            new_count = int(hunk_match.group(4) or "1")
            header = line

            # Collect hunk content
            content_lines = []
            i += 1
            while i < len(lines):
                next_line = lines[i]
                # Stop at next hunk, file, or end
                if (
                    next_line.startswith("@@")
                    or next_line.startswith("diff --git")
                    or (not next_line and i + 1 < len(lines) and lines[i + 1].startswith("diff"))
                ):
                    break
                content_lines.append(next_line)
                i += 1

            hunks.append(
                DiffHunk(
                    file_path=current_file,
                    old_start=old_start,
                    old_count=old_count,
                    new_start=new_start,
                    new_count=new_count,
                    content="\n".join(content_lines),
                    header=header,
                )
            )
            continue

        i += 1

    return hunks


class GitError(Exception):
    """Git operation failed."""

    def __init__(self, message: str, returncode: int, stderr: str):
        super().__init__(message)
        self.returncode = returncode
        self.stderr = stderr


@dataclass(frozen=True)
class CommitHandle:
    """Reference to a shadow commit."""

    sha: str
    message: str
    timestamp: datetime
    branch: str


@dataclass
class ShadowBranch:
    """A shadow branch for isolated agent work."""

    name: str
    base_branch: str
    repo_path: Path
    commits: list[CommitHandle] = field(default_factory=list)
    _id: UUID = field(default_factory=uuid4)

    @property
    def id(self) -> UUID:
        return self._id


class ShadowGit:
    """Manages shadow branches for atomic, reversible agent operations."""

    def __init__(self, repo_path: Path | str):
        self.repo_path = Path(repo_path).resolve()
        self._branches: dict[str, ShadowBranch] = {}

    async def _run_git(self, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        """Run a git command asynchronously."""
        proc = await asyncio.create_subprocess_exec(
            "git",
            *args,
            cwd=self.repo_path,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await proc.communicate()
        stdout_str = stdout.decode().strip()
        stderr_str = stderr.decode().strip()

        if check and proc.returncode != 0:
            raise GitError(
                f"git {' '.join(args)} failed: {stderr_str}",
                proc.returncode or 1,
                stderr_str,
            )

        return subprocess.CompletedProcess(
            args=["git", *args],
            returncode=proc.returncode or 0,
            stdout=stdout_str,
            stderr=stderr_str,
        )

    async def _get_current_branch(self) -> str:
        """Get the current branch name."""
        result = await self._run_git("rev-parse", "--abbrev-ref", "HEAD")
        return result.stdout

    async def _get_head_sha(self) -> str:
        """Get the current HEAD commit SHA."""
        result = await self._run_git("rev-parse", "HEAD")
        return result.stdout

    async def create_shadow_branch(self, name: str | None = None) -> ShadowBranch:
        """Create a new shadow branch from current HEAD."""
        base_branch = await self._get_current_branch()
        branch_name = name or f"shadow/{uuid4().hex[:8]}"

        await self._run_git("checkout", "-b", branch_name)

        branch = ShadowBranch(
            name=branch_name,
            base_branch=base_branch,
            repo_path=self.repo_path,
        )
        self._branches[branch_name] = branch
        return branch

    async def checkout_shadow_branch(self, branch: ShadowBranch) -> None:
        """Switch to a shadow branch."""
        await self._run_git("checkout", branch.name)

    async def commit(
        self,
        branch: ShadowBranch,
        message: str,
        *,
        allow_empty: bool = False,
    ) -> CommitHandle:
        """Create an atomic commit on the shadow branch."""
        # Ensure we're on the right branch
        current = await self._get_current_branch()
        if current != branch.name:
            await self._run_git("checkout", branch.name)

        # Stage all changes
        await self._run_git("add", "-A")

        # Check if there are changes to commit
        status = await self._run_git("status", "--porcelain", check=False)
        if not status.stdout and not allow_empty:
            raise GitError("Nothing to commit", 1, "No changes staged")

        # Create commit
        cmd = ["commit", "-m", message]
        if allow_empty:
            cmd.append("--allow-empty")
        await self._run_git(*cmd)

        sha = await self._get_head_sha()
        handle = CommitHandle(
            sha=sha,
            message=message,
            timestamp=datetime.now(UTC),
            branch=branch.name,
        )
        branch.commits.append(handle)
        return handle

    async def rollback(self, branch: ShadowBranch, steps: int = 1) -> None:
        """Rollback the shadow branch by N commits."""
        if steps < 1:
            raise ValueError("steps must be at least 1")
        if steps > len(branch.commits):
            raise ValueError(f"Cannot rollback {steps} commits; only {len(branch.commits)} exist")

        current = await self._get_current_branch()
        if current != branch.name:
            await self._run_git("checkout", branch.name)

        await self._run_git("reset", "--hard", f"HEAD~{steps}")

        # Update commit list
        branch.commits = branch.commits[:-steps]

    async def rollback_to(self, branch: ShadowBranch, commit: CommitHandle) -> None:
        """Rollback to a specific commit."""
        if commit not in branch.commits:
            raise ValueError("Commit not found in branch history")

        idx = branch.commits.index(commit)
        steps = len(branch.commits) - idx - 1
        if steps > 0:
            await self.rollback(branch, steps)

    async def squash_merge(
        self,
        branch: ShadowBranch,
        message: str | None = None,
    ) -> CommitHandle:
        """Squash merge shadow branch into base branch."""
        if not branch.commits:
            raise GitError("No commits to merge", 1, "Shadow branch has no commits")

        base = branch.base_branch
        merge_msg = message or f"Merge shadow branch {branch.name}"

        # Checkout base branch
        await self._run_git("checkout", base)

        # Squash merge
        await self._run_git("merge", "--squash", branch.name)
        await self._run_git("commit", "-m", merge_msg)

        sha = await self._get_head_sha()
        handle = CommitHandle(
            sha=sha,
            message=merge_msg,
            timestamp=datetime.now(UTC),
            branch=base,
        )

        return handle

    async def abort(self, branch: ShadowBranch) -> None:
        """Abort and delete the shadow branch."""
        base = branch.base_branch

        # Checkout base first
        await self._run_git("checkout", base)

        # Delete shadow branch
        await self._run_git("branch", "-D", branch.name)

        # Remove from tracking
        self._branches.pop(branch.name, None)

    async def diff(self, branch: ShadowBranch) -> str:
        """Get diff of all changes on shadow branch vs base."""
        result = await self._run_git("diff", f"{branch.base_branch}...{branch.name}")
        return result.stdout

    async def diff_stat(self, branch: ShadowBranch) -> str:
        """Get diff stat of changes on shadow branch."""
        result = await self._run_git("diff", "--stat", f"{branch.base_branch}...{branch.name}")
        return result.stdout

    async def get_hunks(self, branch: ShadowBranch) -> list[DiffHunk]:
        """Get all diff hunks for the shadow branch.

        Parses the diff into individual hunks for fine-grained analysis.
        """
        diff_output = await self.diff(branch)
        return parse_diff(diff_output)

    async def map_hunks_to_symbols(self, hunks: list[DiffHunk]) -> list[DiffHunk]:
        """Map each hunk to its containing AST symbol.

        Uses tree-sitter to find the function/class containing each hunk.

        Returns:
            New list of DiffHunk objects with symbol field populated.
        """
        from dataclasses import replace

        try:
            from moss.tree_sitter import get_symbols_at_line
        except ImportError:
            # tree-sitter not available, return hunks unchanged
            return hunks

        result = []
        for hunk in hunks:
            file_path = self.repo_path / hunk.file_path
            if not file_path.exists():
                result.append(hunk)
                continue

            # Find symbol at the start of the hunk (new file line numbers)
            try:
                symbols = get_symbols_at_line(file_path, hunk.new_start)
                symbol_name = symbols[0] if symbols else None
            except Exception:
                symbol_name = None

            result.append(replace(hunk, symbol=symbol_name))

        return result

    async def rollback_hunks(
        self,
        branch: ShadowBranch,
        hunks_to_revert: list[DiffHunk],
    ) -> int:
        """Selectively revert specific hunks while keeping others.

        This is more surgical than commit-level rollback - keeps passing
        changes while reverting only the problematic hunks.

        Args:
            branch: The shadow branch to modify
            hunks_to_revert: List of hunks to revert (must be from get_hunks)

        Returns:
            Number of hunks actually reverted
        """
        if not hunks_to_revert:
            return 0

        current = await self._get_current_branch()
        if current != branch.name:
            await self._run_git("checkout", branch.name)

        reverted = 0

        # Group hunks by file for efficiency
        by_file: dict[str, list[DiffHunk]] = {}
        for hunk in hunks_to_revert:
            by_file.setdefault(hunk.file_path, []).append(hunk)

        for file_path, file_hunks in by_file.items():
            full_path = self.repo_path / file_path

            if not full_path.exists():
                continue

            # Read current file content
            content = full_path.read_text()
            lines = content.split("\n")

            # Sort hunks by line number descending (revert from bottom up)
            file_hunks.sort(key=lambda h: h.new_start, reverse=True)

            for hunk in file_hunks:
                removed, _added = hunk.lines_changed()

                # Calculate the range to replace
                # new_start is 1-indexed, convert to 0-indexed
                start_idx = hunk.new_start - 1
                end_idx = start_idx + hunk.new_count

                # Replace added lines with removed lines
                lines[start_idx:end_idx] = removed
                reverted += 1

            # Write back
            full_path.write_text("\n".join(lines))

        return reverted

    def get_branch(self, name: str) -> ShadowBranch | None:
        """Get a tracked shadow branch by name."""
        return self._branches.get(name)

    @property
    def active_branches(self) -> list[ShadowBranch]:
        """List all active shadow branches."""
        return list(self._branches.values())
