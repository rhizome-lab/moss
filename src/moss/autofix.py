"""Auto-fix system with safety classification and rollback support.

This module provides:
- Fix: Data model for code fixes with safety classification
- FixSafety: Enum for safe/unsafe/needs_review classifications
- FixEngine: Apply fixes with preview, validation, and rollback
- ConflictResolver: Handle overlapping fixes

Usage:
    engine = FixEngine(repo_path)

    # Create fixes
    fix1 = Fix(
        file_path="src/foo.py",
        old_text="def foo():",
        new_text="def foo() -> None:",
        description="Add return type annotation",
        safety=FixSafety.SAFE,
    )

    # Preview
    diff = await engine.preview([fix1])
    print(diff)

    # Apply with rollback support
    result = await engine.apply([fix1])
    if result.errors:
        await engine.rollback()
"""

from __future__ import annotations

import difflib
import re
from dataclasses import dataclass, field
from enum import Enum, auto
from pathlib import Path
from typing import TYPE_CHECKING, ClassVar

if TYPE_CHECKING:
    from moss.shadow_git import CommitHandle, ShadowBranch, ShadowGit


class FixSafety(Enum):
    """Safety classification for fixes."""

    SAFE = auto()  # Can be auto-applied (formatting, type hints, imports)
    NEEDS_REVIEW = auto()  # Should be previewed (refactoring, renames)
    UNSAFE = auto()  # Requires explicit approval (logic changes, deletions)


@dataclass
class Fix:
    """A single code fix to apply."""

    file_path: str
    old_text: str
    new_text: str
    description: str
    safety: FixSafety = FixSafety.NEEDS_REVIEW

    # Location hints (optional, for better error messages)
    line_start: int | None = None
    line_end: int | None = None

    # Metadata
    source: str = ""  # e.g., "ruff", "mypy", "manual"
    rule_id: str = ""  # e.g., "E501", "no-unused-vars"
    confidence: float = 1.0  # 0.0 to 1.0

    @property
    def is_safe(self) -> bool:
        """Check if fix can be auto-applied."""
        return self.safety == FixSafety.SAFE


@dataclass
class FixResult:
    """Result of applying fixes."""

    applied: list[Fix] = field(default_factory=list)
    skipped: list[tuple[Fix, str]] = field(default_factory=list)  # (fix, reason)
    errors: list[tuple[Fix, Exception]] = field(default_factory=list)
    commit: CommitHandle | None = None

    @property
    def success(self) -> bool:
        """Check if all fixes were applied successfully."""
        return len(self.errors) == 0 and len(self.applied) > 0


@dataclass
class Conflict:
    """A conflict between overlapping fixes."""

    fixes: list[Fix]
    file_path: str
    overlap_start: int
    overlap_end: int


# =============================================================================
# Safety Classifier
# =============================================================================


class SafetyClassifier:
    """Classify fix safety based on heuristics."""

    # Patterns that indicate safe fixes
    SAFE_PATTERNS: ClassVar[list[str]] = [
        # Type annotations
        r"^\s*def\s+\w+\([^)]*\)\s*:$",  # Missing return type
        r"^\s*\w+\s*=\s*",  # Variable type annotation
        # Import organization
        r"^import\s+",
        r"^from\s+\w+\s+import\s+",
        # Whitespace/formatting
        r"^\s*$",  # Empty lines
        # Trailing whitespace
        r"\s+$",
    ]

    # Patterns that indicate unsafe fixes
    UNSAFE_PATTERNS: ClassVar[list[str]] = [
        # Deletions
        r"^\s*#.*TODO",  # Removing TODOs
        r"^\s*raise\s+",  # Changing exception handling
        r"^\s*return\s+",  # Changing return statements
        # Security-sensitive
        r"password|secret|token|key|credential",
        r"eval\s*\(|exec\s*\(",
    ]

    def classify(self, fix: Fix) -> FixSafety:
        """Classify fix safety based on content analysis."""
        # If already classified, respect that
        if fix.safety != FixSafety.NEEDS_REVIEW:
            return fix.safety

        # Check for deletion
        if not fix.new_text.strip():
            return FixSafety.UNSAFE

        # Check for unsafe patterns
        combined = f"{fix.old_text}\n{fix.new_text}"
        for pattern in self.UNSAFE_PATTERNS:
            if re.search(pattern, combined, re.IGNORECASE):
                return FixSafety.UNSAFE

        # Check for safe patterns
        for pattern in self.SAFE_PATTERNS:
            if re.search(pattern, fix.old_text) or re.search(pattern, fix.new_text):
                return FixSafety.SAFE

        # Check change magnitude
        old_lines = fix.old_text.count("\n") + 1
        new_lines = fix.new_text.count("\n") + 1
        if abs(new_lines - old_lines) > 5:
            return FixSafety.NEEDS_REVIEW

        # Default to needs review
        return FixSafety.NEEDS_REVIEW

    def classify_batch(self, fixes: list[Fix]) -> list[Fix]:
        """Classify a batch of fixes, updating their safety field."""
        for fix in fixes:
            fix.safety = self.classify(fix)
        return fixes


# =============================================================================
# Conflict Resolution
# =============================================================================


class ConflictResolver:
    """Detect and resolve conflicts between overlapping fixes."""

    def find_conflicts(self, fixes: list[Fix]) -> list[Conflict]:
        """Find all conflicts between fixes."""
        conflicts = []

        # Group fixes by file
        by_file: dict[str, list[Fix]] = {}
        for fix in fixes:
            by_file.setdefault(fix.file_path, []).append(fix)

        # Check each file for overlapping fixes
        for file_path, file_fixes in by_file.items():
            if len(file_fixes) < 2:
                continue

            # Sort by line number if available
            sorted_fixes = sorted(
                file_fixes,
                key=lambda f: f.line_start if f.line_start else 0,
            )

            # Check for overlaps
            for i, fix1 in enumerate(sorted_fixes):
                for fix2 in sorted_fixes[i + 1 :]:
                    if self._overlaps(fix1, fix2):
                        conflicts.append(
                            Conflict(
                                fixes=[fix1, fix2],
                                file_path=file_path,
                                overlap_start=fix1.line_start or 0,
                                overlap_end=fix2.line_end or 0,
                            )
                        )

        return conflicts

    def _overlaps(self, fix1: Fix, fix2: Fix) -> bool:
        """Check if two fixes overlap."""
        # If we don't have line info, check text overlap
        if fix1.line_start is None or fix2.line_start is None:
            return fix1.old_text in fix2.old_text or fix2.old_text in fix1.old_text

        # Check line range overlap
        end1 = fix1.line_end or fix1.line_start
        end2 = fix2.line_end or fix2.line_start

        return not (end1 < fix2.line_start or end2 < fix1.line_start)

    def resolve(
        self,
        conflict: Conflict,
        strategy: str = "first",
    ) -> list[Fix]:
        """Resolve a conflict using the specified strategy.

        Strategies:
        - "first": Keep the first fix
        - "last": Keep the last fix
        - "highest_confidence": Keep fix with highest confidence
        - "safest": Keep the safest fix
        """
        if not conflict.fixes:
            return []

        if strategy == "first":
            return [conflict.fixes[0]]
        elif strategy == "last":
            return [conflict.fixes[-1]]
        elif strategy == "highest_confidence":
            return [max(conflict.fixes, key=lambda f: f.confidence)]
        elif strategy == "safest":
            safety_order = {FixSafety.SAFE: 0, FixSafety.NEEDS_REVIEW: 1, FixSafety.UNSAFE: 2}
            return [min(conflict.fixes, key=lambda f: safety_order[f.safety])]
        else:
            raise ValueError(f"Unknown resolution strategy: {strategy}")


# =============================================================================
# Fix Engine
# =============================================================================


class FixEngine:
    """Engine for applying fixes with preview, validation, and rollback."""

    def __init__(
        self,
        repo_path: Path | str,
        shadow_git: ShadowGit | None = None,
    ) -> None:
        """Initialize fix engine.

        Args:
            repo_path: Repository root path
            shadow_git: Optional ShadowGit instance for rollback support
        """
        self.repo_path = Path(repo_path).resolve()
        self._shadow_git = shadow_git
        self._classifier = SafetyClassifier()
        self._resolver = ConflictResolver()
        self._current_branch: ShadowBranch | None = None
        self._last_commit: CommitHandle | None = None

    async def _ensure_shadow_git(self) -> ShadowGit:
        """Ensure ShadowGit is initialized."""
        if self._shadow_git is None:
            from moss.shadow_git import ShadowGit

            self._shadow_git = ShadowGit(self.repo_path)
        return self._shadow_git

    def preview(self, fixes: list[Fix]) -> str:
        """Generate unified diff preview of all fixes.

        Args:
            fixes: List of fixes to preview

        Returns:
            Unified diff string showing all changes
        """
        diffs = []

        # Group by file
        by_file: dict[str, list[Fix]] = {}
        for fix in fixes:
            by_file.setdefault(fix.file_path, []).append(fix)

        for file_path, file_fixes in sorted(by_file.items()):
            path = self.repo_path / file_path
            if not path.exists():
                continue

            original = path.read_text()
            modified = original

            # Apply fixes in reverse line order to preserve positions
            sorted_fixes = sorted(
                file_fixes,
                key=lambda f: f.line_start if f.line_start else 0,
                reverse=True,
            )

            for fix in sorted_fixes:
                if fix.old_text in modified:
                    modified = modified.replace(fix.old_text, fix.new_text, 1)

            # Generate diff
            diff = difflib.unified_diff(
                original.splitlines(keepends=True),
                modified.splitlines(keepends=True),
                fromfile=f"a/{file_path}",
                tofile=f"b/{file_path}",
            )
            diffs.extend(diff)

        return "".join(diffs)

    def preview_fix(self, fix: Fix) -> str:
        """Generate preview for a single fix."""
        return self.preview([fix])

    async def apply(
        self,
        fixes: list[Fix],
        *,
        auto_resolve_conflicts: bool = True,
        conflict_strategy: str = "first",
        use_shadow_branch: bool = True,
        commit_message: str | None = None,
    ) -> FixResult:
        """Apply fixes to the repository.

        Args:
            fixes: List of fixes to apply
            auto_resolve_conflicts: Automatically resolve conflicts
            conflict_strategy: Strategy for conflict resolution
            use_shadow_branch: Use shadow branch for rollback support
            commit_message: Optional commit message

        Returns:
            FixResult with details of applied/skipped/errored fixes
        """
        result = FixResult()

        if not fixes:
            return result

        # Classify fixes
        fixes = self._classifier.classify_batch(fixes)

        # Check for conflicts
        conflicts = self._resolver.find_conflicts(fixes)
        if conflicts:
            if auto_resolve_conflicts:
                # Resolve conflicts
                resolved_fixes: list[Fix] = []
                conflict_fixes: list[Fix] = [f for c in conflicts for f in c.fixes]

                for conflict in conflicts:
                    kept = self._resolver.resolve(conflict, conflict_strategy)
                    resolved_fixes.extend(kept)
                    for fix in conflict.fixes:
                        if fix not in kept:
                            result.skipped.append((fix, "Conflict resolved"))

                # Add non-conflicting fixes
                for fix in fixes:
                    if fix not in conflict_fixes:
                        resolved_fixes.append(fix)

                fixes = resolved_fixes
            else:
                # Skip conflicting fixes
                conflict_fixes = [f for c in conflicts for f in c.fixes]
                for fix in fixes:
                    if fix in conflict_fixes:
                        result.skipped.append((fix, "Conflicting fix"))
                fixes = [f for f in fixes if f not in conflict_fixes]

        # Create shadow branch if requested
        if use_shadow_branch:
            git = await self._ensure_shadow_git()
            self._current_branch = await git.create_shadow_branch()

        # Apply fixes by file
        by_file: dict[str, list[Fix]] = {}
        for fix in fixes:
            by_file.setdefault(fix.file_path, []).append(fix)

        for file_path, file_fixes in by_file.items():
            path = self.repo_path / file_path
            if not path.exists():
                for fix in file_fixes:
                    result.errors.append((fix, FileNotFoundError(f"File not found: {file_path}")))
                continue

            try:
                content = path.read_text()
                modified = content

                # Sort fixes by line number (reverse) to preserve positions
                sorted_fixes = sorted(
                    file_fixes,
                    key=lambda f: f.line_start if f.line_start else 0,
                    reverse=True,
                )

                for fix in sorted_fixes:
                    if fix.old_text not in modified:
                        result.skipped.append((fix, "Text not found in file"))
                        continue

                    modified = modified.replace(fix.old_text, fix.new_text, 1)
                    result.applied.append(fix)

                # Write modified content
                if modified != content:
                    path.write_text(modified)

            except (OSError, UnicodeDecodeError) as e:
                for fix in file_fixes:
                    result.errors.append((fix, e))

        # Commit if using shadow branch
        if use_shadow_branch and self._current_branch and result.applied:
            git = await self._ensure_shadow_git()
            msg = commit_message or f"Apply {len(result.applied)} fixes"
            try:
                result.commit = await git.commit(self._current_branch, msg)
                self._last_commit = result.commit
            except (OSError, ValueError) as e:
                # Rollback on commit failure
                await self.rollback()
                for fix in result.applied:
                    result.errors.append((fix, e))
                result.applied = []

        return result

    async def rollback(self) -> bool:
        """Rollback the last set of applied fixes.

        Returns:
            True if rollback succeeded, False if nothing to rollback
        """
        if self._current_branch is None:
            return False

        git = await self._ensure_shadow_git()

        try:
            await git.abort(self._current_branch)
            self._current_branch = None
            self._last_commit = None
            return True
        except (OSError, ValueError):
            return False

    async def finalize(self, message: str | None = None) -> CommitHandle | None:
        """Finalize fixes by squash-merging shadow branch to main.

        Args:
            message: Optional merge commit message

        Returns:
            CommitHandle for the merge commit, or None if no shadow branch
        """
        if self._current_branch is None:
            return None

        git = await self._ensure_shadow_git()

        try:
            commit = await git.squash_merge(self._current_branch, message)
            await git.abort(self._current_branch)  # Clean up shadow branch
            self._current_branch = None
            return commit
        except (OSError, ValueError):
            return None


# =============================================================================
# Factory Functions
# =============================================================================


def create_fix(
    file_path: str,
    old_text: str,
    new_text: str,
    description: str,
    *,
    source: str = "",
    rule_id: str = "",
    line_start: int | None = None,
) -> Fix:
    """Create a fix with automatic safety classification.

    Args:
        file_path: Path to the file to modify
        old_text: Text to replace
        new_text: Replacement text
        description: Human-readable description
        source: Tool that generated the fix
        rule_id: Rule or check that triggered the fix
        line_start: Starting line number

    Returns:
        Fix instance with safety classified
    """
    fix = Fix(
        file_path=file_path,
        old_text=old_text,
        new_text=new_text,
        description=description,
        source=source,
        rule_id=rule_id,
        line_start=line_start,
    )
    classifier = SafetyClassifier()
    fix.safety = classifier.classify(fix)
    return fix


def parse_ruff_output(output: str, repo_path: Path) -> list[Fix]:
    """Parse ruff check output and create fixes.

    Args:
        output: Output from `ruff check --output-format=json`
        repo_path: Repository root path

    Returns:
        List of Fix objects
    """
    import json

    fixes = []

    try:
        diagnostics = json.loads(output)
    except json.JSONDecodeError:
        return fixes

    for diag in diagnostics:
        if not diag.get("fix"):
            continue

        fix_data = diag["fix"]
        edits = fix_data.get("edits", [])

        for edit in edits:
            file_path = diag.get("filename", "")
            if file_path.startswith(str(repo_path)):
                file_path = str(Path(file_path).relative_to(repo_path))

            fix = Fix(
                file_path=file_path,
                old_text=edit.get("content", ""),
                new_text=edit.get("replacement", ""),
                description=diag.get("message", ""),
                source="ruff",
                rule_id=diag.get("code", ""),
                line_start=diag.get("location", {}).get("row"),
                safety=FixSafety.SAFE,  # Ruff fixes are generally safe
            )
            fixes.append(fix)

    return fixes
