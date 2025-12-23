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
    experiment_id: str | None = None  # Link to parent experiment
    metadata: dict = field(default_factory=dict)  # Approach, params, metrics

    @property
    def id(self) -> UUID:
        return self._id


@dataclass
class ExperimentComparison:
    """Result of comparing branches in an experiment."""

    experiment_id: str
    branches: list[str]
    common_files: list[str]  # Files modified in all branches
    unique_files: dict[str, list[str]]  # Branch -> files only modified there
    metrics: dict[str, dict]  # Branch -> metrics


@dataclass
class Experiment:
    """Groups multiple concurrent branches testing the same problem."""

    id: str
    description: str
    base_branch: str
    branches: dict[str, ShadowBranch] = field(default_factory=dict)
    metadata: dict = field(default_factory=dict)
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    _uuid: UUID = field(default_factory=uuid4)

    @property
    def branch_count(self) -> int:
        return len(self.branches)

    @property
    def total_commits(self) -> int:
        return sum(len(b.commits) for b in self.branches.values())


class ShadowGit:
    """Manages shadow branches for atomic, reversible agent operations."""

    def __init__(self, repo_path: Path | str):
        self.repo_path = Path(repo_path).resolve()
        self._branches: dict[str, ShadowBranch] = {}
        self._experiments: dict[str, Experiment] = {}
        self._multi_commit_mode: bool = False
        self._staged_messages: list[str] = []

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

    async def begin_multi_commit(self) -> None:
        """Enter multi-commit mode where multiple actions are grouped."""
        self._multi_commit_mode = True
        self._staged_messages = []

    async def finish_multi_commit(
        self, branch: ShadowBranch, message: str | None = None
    ) -> CommitHandle:
        """Commit all changes staged during multi-commit mode."""
        self._multi_commit_mode = False
        final_message = message or " / ".join(self._staged_messages) or "Multi-action commit"
        handle = await self.commit(branch, final_message)
        self._staged_messages = []
        return handle

    async def commit(
        self,
        branch: ShadowBranch,
        message: str,
        *,
        allow_empty: bool = False,
    ) -> CommitHandle:
        """Create an atomic commit on the shadow branch."""
        # If in multi-commit mode, just stage the message and return placeholder
        if self._multi_commit_mode:
            self._staged_messages.append(message)
            # We still add to git index
            await self._run_git("add", "-A")
            return CommitHandle(
                sha="staged", message=message, timestamp=datetime.now(UTC), branch=branch.name
            )

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

    async def smart_merge(
        self,
        branch: ShadowBranch,
        message: str | None = None,
    ) -> CommitHandle:
        """Merge shadow branch with automated conflict resolution.

        Favors changes from the shadow branch for simple text conflicts.
        If complex conflicts occur, falls back to standard merge (which may fail).
        """
        base = branch.base_branch
        merge_msg = message or f"Smart merge shadow branch {branch.name}"

        # Checkout base branch
        await self._run_git("checkout", base)

        try:
            # Try normal merge first
            await self._run_git("merge", branch.name, "-m", merge_msg)
        except GitError:
            # Conflict detected - try to resolve automatically
            # This is a simplified 'favor-theirs' strategy for now
            await self._run_git("checkout", "--theirs", ".")
            await self._run_git("add", "-A")
            await self._run_git("commit", "-m", f"{merge_msg} (resolved conflicts)")

        sha = await self._get_head_sha()
        return CommitHandle(
            sha=sha,
            message=merge_msg,
            timestamp=datetime.now(UTC),
            branch=base,
        )

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
            from moss_intelligence.tree_sitter import get_symbols_at_line
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
            except (OSError, ValueError, IndexError):
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

    # =========================================================================
    # Experiment API: Multiple concurrent branches for parallel exploration
    # =========================================================================

    async def create_experiment(
        self,
        name: str,
        description: str = "",
        metadata: dict | None = None,
    ) -> Experiment:
        """Create a new experiment to group multiple parallel branches.

        An experiment represents a problem to solve with multiple approaches.
        Each approach gets its own branch that can be compared and merged.

        Args:
            name: Unique experiment identifier (e.g., "optimize-search")
            description: What the experiment is testing
            metadata: Optional approach parameters, config

        Returns:
            Experiment object for tracking branches
        """
        base_branch = await self._get_current_branch()

        experiment = Experiment(
            id=name,
            description=description,
            base_branch=base_branch,
            metadata=metadata or {},
        )
        self._experiments[name] = experiment
        return experiment

    def get_experiment(self, name: str) -> Experiment | None:
        """Get an experiment by name."""
        return self._experiments.get(name)

    @property
    def active_experiments(self) -> list[Experiment]:
        """List all active experiments."""
        return list(self._experiments.values())

    async def create_experiment_branch(
        self,
        experiment: Experiment,
        approach_name: str,
        metadata: dict | None = None,
    ) -> ShadowBranch:
        """Create a new branch within an experiment.

        Args:
            experiment: Parent experiment
            approach_name: Name for this approach (e.g., "vectorize", "cache")
            metadata: Optional approach-specific parameters

        Returns:
            ShadowBranch linked to the experiment
        """
        branch_name = f"experiment/{experiment.id}/{approach_name}"

        # Ensure we're on base branch before creating
        await self._run_git("checkout", experiment.base_branch)
        await self._run_git("checkout", "-b", branch_name)

        branch = ShadowBranch(
            name=branch_name,
            base_branch=experiment.base_branch,
            repo_path=self.repo_path,
            experiment_id=experiment.id,
            metadata=metadata or {},
        )

        experiment.branches[approach_name] = branch
        self._branches[branch_name] = branch
        return branch

    async def record_metrics(
        self,
        branch: ShadowBranch,
        metrics: dict,
    ) -> None:
        """Record metrics for a branch (e.g., test results, performance).

        Args:
            branch: The branch to record metrics for
            metrics: Key-value pairs of metrics
        """
        branch.metadata.setdefault("metrics", {}).update(metrics)

    async def compare_experiment_branches(
        self,
        experiment: Experiment,
    ) -> ExperimentComparison:
        """Compare all branches in an experiment.

        Analyzes which files each branch modified and collects metrics
        for comparison.

        Args:
            experiment: Experiment to compare

        Returns:
            ExperimentComparison with file overlap and metrics
        """
        files_by_branch: dict[str, set[str]] = {}

        for name, branch in experiment.branches.items():
            # Get files changed in this branch
            result = await self._run_git(
                "diff",
                "--name-only",
                f"{experiment.base_branch}...{branch.name}",
                check=False,
            )
            files = set(result.stdout.split("\n")) if result.stdout else set()
            files.discard("")
            files_by_branch[name] = files

        # Find common files (modified in all branches)
        if files_by_branch:
            common = set.intersection(*files_by_branch.values())
        else:
            common = set()

        # Find unique files per branch
        unique: dict[str, list[str]] = {}
        for name, files in files_by_branch.items():
            other_files = set()
            for other_name, other in files_by_branch.items():
                if other_name != name:
                    other_files |= other
            unique[name] = sorted(files - other_files)

        # Collect metrics
        metrics = {
            name: branch.metadata.get("metrics", {}) for name, branch in experiment.branches.items()
        }

        return ExperimentComparison(
            experiment_id=experiment.id,
            branches=list(experiment.branches.keys()),
            common_files=sorted(common),
            unique_files=unique,
            metrics=metrics,
        )

    async def select_winner(
        self,
        experiment: Experiment,
        winner_name: str,
        merge_message: str | None = None,
    ) -> CommitHandle:
        """Select winning approach and merge to base branch.

        Args:
            experiment: The experiment
            winner_name: Name of the winning approach
            merge_message: Optional merge commit message

        Returns:
            CommitHandle for the merge commit
        """
        if winner_name not in experiment.branches:
            raise ValueError(f"No branch named '{winner_name}' in experiment")

        winner = experiment.branches[winner_name]
        message = merge_message or f"Experiment '{experiment.id}': selected {winner_name}"

        return await self.squash_merge(winner, message)

    async def abort_experiment(self, experiment: Experiment) -> None:
        """Abort experiment and delete all branches.

        Args:
            experiment: Experiment to abort
        """
        for branch in list(experiment.branches.values()):
            await self.abort(branch)

        self._experiments.pop(experiment.id, None)
