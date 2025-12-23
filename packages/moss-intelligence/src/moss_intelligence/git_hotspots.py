"""Git hot spots analysis.

Identifies frequently changed files in a git repository.
Hot spots indicate areas of high churn that may need attention.
"""

from __future__ import annotations

import subprocess
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class FileHotspot:
    """A file identified as a hot spot."""

    path: Path
    changes: int
    authors: int = 0
    last_changed: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "path": str(self.path),
            "changes": self.changes,
            "authors": self.authors,
            "last_changed": self.last_changed,
        }


@dataclass
class GitHotspotAnalysis:
    """Results of git hot spot analysis."""

    root: Path
    hotspots: list[FileHotspot] = field(default_factory=list)
    total_commits: int = 0
    days_analyzed: int = 0
    error: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "root": str(self.root),
            "total_commits": self.total_commits,
            "days_analyzed": self.days_analyzed,
            "hotspots": [h.to_dict() for h in self.hotspots],
            "error": self.error,
        }

    def to_compact(self) -> str:
        """Format as compact summary."""
        if self.error:
            return f"git-hotspots: error - {self.error}"

        if not self.hotspots:
            return "git-hotspots: no changes found"

        top = self.hotspots[:5]
        files = ", ".join(f"{h.path.name}:{h.changes}" for h in top)
        return f"git-hotspots: {files} (last {self.days_analyzed} days)"

    def to_markdown(self) -> str:
        """Format as markdown report."""
        lines = ["# Git Hot Spots", ""]

        if self.error:
            lines.append(f"**Error**: {self.error}")
            return "\n".join(lines)

        lines.append(f"Analysis of {self.total_commits} commits over {self.days_analyzed} days.")
        lines.append("")

        if not self.hotspots:
            lines.append("No file changes found in the specified period.")
            return "\n".join(lines)

        lines.append("## Most Frequently Changed Files")
        lines.append("")
        lines.append("| File | Changes | Authors | Last Changed |")
        lines.append("|------|---------|---------|--------------|")

        for h in self.hotspots[:20]:
            lines.append(f"| {h.path} | {h.changes} | {h.authors} | {h.last_changed} |")

        lines.append("")
        lines.append("*High churn files may indicate complex code that needs refactoring.*")

        return "\n".join(lines)


class GitHotspotAnalyzer:
    """Analyze git history for hot spots."""

    def __init__(self, root: Path, days: int = 90):
        self.root = Path(root).resolve()
        self.days = days

    def analyze(self) -> GitHotspotAnalysis:
        """Run git log analysis to find hot spots."""
        result = GitHotspotAnalysis(root=self.root, days_analyzed=self.days)

        # Check if this is a git repo
        if not (self.root / ".git").exists():
            result.error = "Not a git repository"
            return result

        try:
            # Get file changes from git log
            file_changes = self._get_file_changes()
            result.total_commits = self._count_commits()

            # Count changes per file
            change_counts: Counter[str] = Counter()
            for path in file_changes:
                change_counts[path] += 1

            # Get additional info for top files
            hotspots = []
            for path, count in change_counts.most_common(50):
                # Only include files that still exist
                full_path = self.root / path
                if full_path.exists():
                    authors = self._count_authors(path)
                    last_changed = self._get_last_changed(path)
                    hotspots.append(
                        FileHotspot(
                            path=Path(path),
                            changes=count,
                            authors=authors,
                            last_changed=last_changed,
                        )
                    )

            result.hotspots = hotspots

        except (OSError, subprocess.SubprocessError) as e:
            result.error = str(e)

        return result

    def _get_file_changes(self) -> list[str]:
        """Get list of all file changes in the period."""
        cmd = [
            "git",
            "log",
            f"--since={self.days} days ago",
            "--name-only",
            "--pretty=format:",
        ]
        try:
            output = subprocess.run(
                cmd,
                cwd=self.root,
                capture_output=True,
                text=True,
                check=True,
            )
            # Filter empty lines and return file paths
            return [line.strip() for line in output.stdout.splitlines() if line.strip()]
        except subprocess.CalledProcessError:
            return []

    def _count_commits(self) -> int:
        """Count total commits in the period."""
        cmd = [
            "git",
            "rev-list",
            "--count",
            f"--since={self.days} days ago",
            "HEAD",
        ]
        try:
            output = subprocess.run(
                cmd,
                cwd=self.root,
                capture_output=True,
                text=True,
                check=True,
            )
            return int(output.stdout.strip())
        except (subprocess.CalledProcessError, ValueError):
            return 0

    def _count_authors(self, path: str) -> int:
        """Count unique authors for a file."""
        cmd = [
            "git",
            "log",
            f"--since={self.days} days ago",
            "--format=%an",
            "--",
            path,
        ]
        try:
            output = subprocess.run(
                cmd,
                cwd=self.root,
                capture_output=True,
                text=True,
                check=True,
            )
            authors = set(line.strip() for line in output.stdout.splitlines() if line.strip())
            return len(authors)
        except subprocess.CalledProcessError:
            return 0

    def _get_last_changed(self, path: str) -> str:
        """Get the date of last change for a file."""
        cmd = [
            "git",
            "log",
            "-1",
            "--format=%cr",
            "--",
            path,
        ]
        try:
            output = subprocess.run(
                cmd,
                cwd=self.root,
                capture_output=True,
                text=True,
                check=True,
            )
            return output.stdout.strip()
        except subprocess.CalledProcessError:
            return ""


def analyze_hotspots(root: str | Path, days: int = 90) -> GitHotspotAnalysis:
    """Convenience function to analyze git hot spots.

    Args:
        root: Path to the git repository
        days: Number of days to analyze (default: 90)

    Returns:
        GitHotspotAnalysis with hot spot data
    """
    analyzer = GitHotspotAnalyzer(Path(root), days=days)
    return analyzer.analyze()
