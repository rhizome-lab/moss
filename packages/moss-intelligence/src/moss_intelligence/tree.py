"""Git-aware tree visualization.

Shows project structure with awareness of git tracking status.
Pure Python implementation that doesn't require external `tree` command.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class TreeNode:
    """A node in the file tree."""

    name: str
    is_dir: bool
    children: dict[str, TreeNode] = field(default_factory=dict)

    def add_path(self, parts: list[str]) -> None:
        """Add a path to the tree."""
        if not parts:
            return

        name = parts[0]
        is_dir = len(parts) > 1

        if name not in self.children:
            self.children[name] = TreeNode(name=name, is_dir=is_dir)
        elif is_dir:
            # Upgrade to directory if we see it as a dir
            self.children[name].is_dir = True

        if len(parts) > 1:
            self.children[name].add_path(parts[1:])


def get_git_tracked_files(root: Path) -> list[str]:
    """Get list of git-tracked files relative to root."""
    try:
        result = subprocess.run(
            ["git", "ls-tree", "-r", "--name-only", "HEAD"],
            cwd=root,
            capture_output=True,
            text=True,
            check=True,
        )
        return [line for line in result.stdout.strip().split("\n") if line]
    except subprocess.CalledProcessError:
        return []


def get_all_files(root: Path, gitignore: bool = True) -> list[str]:
    """Get all files, optionally respecting .gitignore."""
    if gitignore:
        try:
            # Use git ls-files to get files respecting .gitignore
            result = subprocess.run(
                ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
                cwd=root,
                capture_output=True,
                text=True,
                check=True,
            )
            return [line for line in result.stdout.strip().split("\n") if line]
        except subprocess.CalledProcessError:
            pass

    # Fall back to walking directory
    files = []
    for path in root.rglob("*"):
        if path.is_file() and not any(p.startswith(".") for p in path.relative_to(root).parts):
            files.append(str(path.relative_to(root)))
    return sorted(files)


def build_tree(files: list[str]) -> TreeNode:
    """Build a tree structure from a list of file paths."""
    root = TreeNode(name=".", is_dir=True)

    for file_path in files:
        parts = file_path.split("/")
        root.add_path(parts)

    return root


def render_tree(
    node: TreeNode,
    prefix: str = "",
    is_last: bool = True,
    is_root: bool = True,
) -> list[str]:
    """Render tree to lines of text."""
    lines = []

    if is_root:
        lines.append(node.name)
    else:
        connector = "└── " if is_last else "├── "
        lines.append(f"{prefix}{connector}{node.name}")

    # Sort children: directories first, then alphabetically
    children = sorted(
        node.children.values(),
        key=lambda n: (not n.is_dir, n.name.lower()),
    )

    for i, child in enumerate(children):
        is_child_last = i == len(children) - 1
        if is_root:
            child_prefix = ""
        else:
            child_prefix = prefix + ("    " if is_last else "│   ")

        lines.extend(render_tree(child, child_prefix, is_child_last, is_root=False))

    return lines


@dataclass
class TreeResult:
    """Result of tree generation."""

    root: Path
    files: list[str]
    tree_lines: list[str]
    file_count: int
    dir_count: int

    def to_text(self) -> str:
        """Render as text."""
        lines = self.tree_lines.copy()
        lines.append("")
        lines.append(f"{self.dir_count} directories, {self.file_count} files")
        return "\n".join(lines)

    def to_compact(self) -> str:
        """Token-efficient format showing just structure."""
        # Show up to 30 lines, then truncate
        lines = self.tree_lines[:30]
        if len(self.tree_lines) > 30:
            lines.append(f"... (+{len(self.tree_lines) - 30} more)")
        lines.append(f"({self.dir_count} dirs, {self.file_count} files)")
        return "\n".join(lines)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON output."""
        return {
            "root": str(self.root),
            "file_count": self.file_count,
            "dir_count": self.dir_count,
            "files": self.files,
        }


def generate_tree(
    root: Path,
    tracked_only: bool = False,
    gitignore: bool = True,
) -> TreeResult:
    """Generate a tree visualization of a directory.

    Args:
        root: Root directory to visualize
        tracked_only: If True, only show git-tracked files
        gitignore: If True, respect .gitignore when showing all files

    Returns:
        TreeResult with tree visualization and statistics
    """
    root = root.resolve()

    if tracked_only:
        files = get_git_tracked_files(root)
    else:
        files = get_all_files(root, gitignore=gitignore)

    if not files:
        return TreeResult(
            root=root,
            files=[],
            tree_lines=[str(root.name), "(empty)"],
            file_count=0,
            dir_count=0,
        )

    tree = build_tree(files)
    tree.name = root.name
    tree_lines = render_tree(tree)

    # Count directories and files
    dirs = set()
    for f in files:
        parts = f.split("/")
        for i in range(len(parts) - 1):
            dirs.add("/".join(parts[: i + 1]))

    return TreeResult(
        root=root,
        files=files,
        tree_lines=tree_lines,
        file_count=len(files),
        dir_count=len(dirs),
    )
