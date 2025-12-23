"""PR review helper for analyzing and summarizing changes.

This module provides tools to analyze git changes and generate
PR-friendly summaries, including:
- Change summaries organized by category
- Potential issue detection (large files, missing tests, etc.)
- Impact assessment

Usage:
    from moss_orchestration.pr_review import analyze_pr, generate_pr_summary

    # Analyze changes between branches
    review = analyze_pr(Path("."), "main", "feature-branch")
    print(review.summary)

    # Or analyze staged changes
    review = analyze_pr(Path("."), staged=True)
    print(review.summary)
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from moss_orchestration.diff_analysis import (
    DiffAnalysis,
    analyze_diff,
    get_commit_diff,
    get_staged_diff,
)


@dataclass
class Issue:
    """A potential issue detected during review."""

    severity: str  # info, warning, error
    category: str  # size, tests, security, style, complexity
    message: str
    file_path: str | None = None
    suggestion: str | None = None


@dataclass
class ChangeCategory:
    """A category of changes (e.g., features, fixes, refactoring)."""

    name: str
    description: str
    files: list[str] = field(default_factory=list)
    symbols: list[str] = field(default_factory=list)


@dataclass
class PRReview:
    """Complete PR review analysis."""

    # Basic stats
    diff_analysis: DiffAnalysis
    title_suggestion: str = ""
    summary: str = ""

    # Categorized changes
    categories: list[ChangeCategory] = field(default_factory=list)

    # Issues detected
    issues: list[Issue] = field(default_factory=list)

    # Impact assessment
    impact_level: str = "low"  # low, medium, high
    impact_areas: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "title_suggestion": self.title_suggestion,
            "summary": self.summary,
            "stats": {
                "files_changed": self.diff_analysis.files_changed,
                "files_added": self.diff_analysis.files_added,
                "files_deleted": self.diff_analysis.files_deleted,
                "additions": self.diff_analysis.total_additions,
                "deletions": self.diff_analysis.total_deletions,
            },
            "categories": [
                {
                    "name": c.name,
                    "description": c.description,
                    "files": c.files,
                    "symbols": c.symbols,
                }
                for c in self.categories
            ],
            "issues": [
                {
                    "severity": i.severity,
                    "category": i.category,
                    "message": i.message,
                    "file": i.file_path,
                    "suggestion": i.suggestion,
                }
                for i in self.issues
            ],
            "impact": {
                "level": self.impact_level,
                "areas": self.impact_areas,
            },
        }


# Size thresholds for issue detection
LARGE_FILE_LINES = 500
LARGE_CHANGE_LINES = 300
MANY_FILES_THRESHOLD = 20


def detect_issues(analysis: DiffAnalysis) -> list[Issue]:
    """Detect potential issues in the changes.

    Args:
        analysis: Diff analysis to check

    Returns:
        List of detected issues
    """
    issues: list[Issue] = []

    # Check for large changes
    if analysis.total_additions + analysis.total_deletions > LARGE_CHANGE_LINES:
        issues.append(
            Issue(
                severity="warning",
                category="size",
                message=(
                    f"Large change: {analysis.total_additions + analysis.total_deletions} "
                    f"lines modified"
                ),
                suggestion="Consider breaking into smaller, focused PRs",
            )
        )

    # Check for too many files
    if analysis.files_changed > MANY_FILES_THRESHOLD:
        issues.append(
            Issue(
                severity="warning",
                category="size",
                message=f"Many files changed: {analysis.files_changed} files",
                suggestion="Review if all changes belong in this PR",
            )
        )

    # Check for large individual files
    for file_diff in analysis.file_diffs:
        if file_diff.additions > LARGE_FILE_LINES:
            issues.append(
                Issue(
                    severity="info",
                    category="size",
                    message=f"Large file addition: +{file_diff.additions} lines",
                    file_path=str(file_diff.path),
                    suggestion="Consider if this can be split into modules",
                )
            )

    # Check for test coverage
    has_source_changes = False
    has_test_changes = False

    for file_diff in analysis.file_diffs:
        path_str = str(file_diff.path)
        if path_str.endswith(".py"):
            if "test" in path_str.lower() or path_str.startswith("tests/"):
                has_test_changes = True
            else:
                has_source_changes = True

    if has_source_changes and not has_test_changes:
        issues.append(
            Issue(
                severity="info",
                category="tests",
                message="Source code changed without test changes",
                suggestion="Consider adding tests for new functionality",
            )
        )

    # Check for potentially sensitive files
    sensitive_patterns = [
        ".env",
        "secret",
        "credential",
        "password",
        "token",
        "key",
        "config.json",
        "settings.json",
    ]

    for file_diff in analysis.file_diffs:
        path_lower = str(file_diff.path).lower()
        for pattern in sensitive_patterns:
            if pattern in path_lower:
                issues.append(
                    Issue(
                        severity="warning",
                        category="security",
                        message="Potentially sensitive file modified",
                        file_path=str(file_diff.path),
                        suggestion="Ensure no secrets are being committed",
                    )
                )
                break

    return issues


def categorize_changes(analysis: DiffAnalysis) -> list[ChangeCategory]:
    """Categorize changes by type.

    Args:
        analysis: Diff analysis to categorize

    Returns:
        List of change categories
    """
    categories: list[ChangeCategory] = []

    # Group by file type/purpose
    test_files: list[str] = []
    docs_files: list[str] = []
    config_files: list[str] = []
    source_files: list[str] = []

    for file_diff in analysis.file_diffs:
        path_str = str(file_diff.path)
        path_lower = path_str.lower()

        if "test" in path_lower or path_str.startswith("tests/"):
            test_files.append(path_str)
        elif path_lower.endswith((".md", ".rst", ".txt")):
            docs_files.append(path_str)
        elif path_lower.endswith((".toml", ".yaml", ".yml", ".json", ".ini", ".cfg")):
            config_files.append(path_str)
        else:
            source_files.append(path_str)

    # Create categories for non-empty groups
    if source_files:
        # Determine category based on symbol changes
        added_symbols = [s for s in analysis.symbol_changes if s.change_type == "added"]
        modified_symbols = [s for s in analysis.symbol_changes if s.change_type == "modified"]
        deleted_symbols = [s for s in analysis.symbol_changes if s.change_type == "deleted"]

        if added_symbols and not modified_symbols and not deleted_symbols:
            cat = ChangeCategory(
                name="New Features",
                description="New functionality added",
                files=source_files,
                symbols=[f"{s.kind} {s.name}" for s in added_symbols[:10]],
            )
        elif deleted_symbols and not added_symbols:
            cat = ChangeCategory(
                name="Removals",
                description="Code removed",
                files=source_files,
                symbols=[f"{s.kind} {s.name}" for s in deleted_symbols[:10]],
            )
        elif modified_symbols:
            cat = ChangeCategory(
                name="Changes",
                description="Existing code modified",
                files=source_files,
                symbols=[f"{s.kind} {s.name}" for s in modified_symbols[:10]],
            )
        else:
            cat = ChangeCategory(
                name="Source Changes",
                description="Source code modifications",
                files=source_files,
            )
        categories.append(cat)

    if test_files:
        categories.append(
            ChangeCategory(
                name="Tests",
                description="Test files modified",
                files=test_files,
            )
        )

    if docs_files:
        categories.append(
            ChangeCategory(
                name="Documentation",
                description="Documentation updated",
                files=docs_files,
            )
        )

    if config_files:
        categories.append(
            ChangeCategory(
                name="Configuration",
                description="Configuration files modified",
                files=config_files,
            )
        )

    return categories


def assess_impact(analysis: DiffAnalysis) -> tuple[str, list[str]]:
    """Assess the impact level and affected areas.

    Args:
        analysis: Diff analysis to assess

    Returns:
        Tuple of (impact_level, list of affected areas)
    """
    areas: list[str] = []

    # Determine affected areas from file paths
    path_prefixes: dict[str, str] = {}
    for file_diff in analysis.file_diffs:
        parts = Path(file_diff.path).parts
        if len(parts) > 1:
            prefix = parts[0]
            if parts[0] == "src" and len(parts) > 2:
                prefix = f"{parts[0]}/{parts[1]}"
            path_prefixes[prefix] = prefix

    areas = list(path_prefixes.keys())

    # Determine impact level
    total_changes = analysis.total_additions + analysis.total_deletions
    symbol_changes = len(analysis.symbol_changes)

    if total_changes > 500 or symbol_changes > 20 or analysis.files_changed > 15:
        level = "high"
    elif total_changes > 100 or symbol_changes > 5 or analysis.files_changed > 5:
        level = "medium"
    else:
        level = "low"

    return level, areas


def suggest_title(analysis: DiffAnalysis, categories: list[ChangeCategory]) -> str:
    """Suggest a PR title based on changes.

    Args:
        analysis: Diff analysis
        categories: Categorized changes

    Returns:
        Suggested PR title
    """
    if not categories:
        return "Update files"

    primary = categories[0]

    # Get main action
    if "New Features" in primary.name:
        prefix = "feat"
    elif "Removals" in primary.name:
        prefix = "refactor"
    elif "Tests" in primary.name:
        prefix = "test"
    elif "Documentation" in primary.name:
        prefix = "docs"
    elif "Configuration" in primary.name:
        prefix = "chore"
    else:
        prefix = "chore"

    # Get scope from files
    files = primary.files
    if files:
        # Find common path component
        paths = [Path(f) for f in files]
        if len(paths) == 1:
            scope = paths[0].stem
        else:
            # Find common parent
            parents = [set(p.parts[:-1]) for p in paths]
            common = set.intersection(*parents) if parents else set()
            if common:
                scope = list(common)[-1]
            else:
                scope = paths[0].parts[0] if paths[0].parts else ""
    else:
        scope = ""

    # Build title
    if primary.symbols:
        main_symbol = primary.symbols[0].split()[-1]
        desc = f"add {main_symbol}" if "New" in primary.name else f"update {main_symbol}"
    else:
        desc = primary.description.lower()

    if scope:
        return f"{prefix}({scope}): {desc}"
    return f"{prefix}: {desc}"


def generate_summary(review: PRReview) -> str:
    """Generate a human-readable PR summary.

    Args:
        review: PR review to summarize

    Returns:
        Formatted summary string
    """
    lines: list[str] = []

    # Overview
    analysis = review.diff_analysis
    lines.append("## Summary")
    lines.append("")
    lines.append(
        f"**{analysis.files_changed}** files changed, "
        f"**+{analysis.total_additions}** additions, "
        f"**-{analysis.total_deletions}** deletions"
    )
    lines.append("")

    # Categories
    if review.categories:
        lines.append("## Changes")
        lines.append("")
        for cat in review.categories:
            lines.append(f"### {cat.name}")
            lines.append(cat.description)
            lines.append("")
            if cat.files:
                for f in cat.files[:5]:
                    lines.append(f"- `{f}`")
                if len(cat.files) > 5:
                    lines.append(f"- ... and {len(cat.files) - 5} more files")
            if cat.symbols:
                lines.append("")
                lines.append("Key changes:")
                for s in cat.symbols[:5]:
                    lines.append(f"- {s}")
                if len(cat.symbols) > 5:
                    lines.append(f"- ... and {len(cat.symbols) - 5} more")
            lines.append("")

    # Issues
    if review.issues:
        lines.append("## Review Notes")
        lines.append("")
        warnings = [i for i in review.issues if i.severity == "warning"]
        infos = [i for i in review.issues if i.severity == "info"]

        if warnings:
            lines.append("### Warnings")
            for issue in warnings:
                lines.append(f"- ⚠️ {issue.message}")
                if issue.file_path:
                    lines.append(f"  - File: `{issue.file_path}`")
                if issue.suggestion:
                    lines.append(f"  - Suggestion: {issue.suggestion}")
            lines.append("")

        if infos:
            lines.append("### Notes")
            for issue in infos:
                lines.append(f"- [info] {issue.message}")
                if issue.suggestion:
                    lines.append(f"  - {issue.suggestion}")
            lines.append("")

    # Impact
    lines.append("## Impact")
    lines.append("")
    lines.append(f"**Level:** {review.impact_level.capitalize()}")
    if review.impact_areas:
        lines.append(f"**Areas:** {', '.join(review.impact_areas)}")
    lines.append("")

    return "\n".join(lines)


def analyze_pr(
    repo_path: Path,
    from_ref: str = "main",
    to_ref: str = "HEAD",
    staged: bool = False,
) -> PRReview:
    """Analyze changes for PR review.

    Args:
        repo_path: Path to git repository
        from_ref: Base branch/commit
        to_ref: Head branch/commit
        staged: If True, analyze staged changes instead

    Returns:
        PRReview with complete analysis
    """
    # Get diff
    if staged:
        diff_output = get_staged_diff(repo_path)
    else:
        diff_output = get_commit_diff(repo_path, from_ref, to_ref)

    # Analyze diff
    analysis = analyze_diff(diff_output)

    # Categorize changes
    categories = categorize_changes(analysis)

    # Detect issues
    issues = detect_issues(analysis)

    # Assess impact
    impact_level, impact_areas = assess_impact(analysis)

    # Suggest title
    title = suggest_title(analysis, categories)

    # Create review
    review = PRReview(
        diff_analysis=analysis,
        title_suggestion=title,
        categories=categories,
        issues=issues,
        impact_level=impact_level,
        impact_areas=impact_areas,
    )

    # Generate summary
    review.summary = generate_summary(review)

    return review
