"""Documentation freshness checker.

Compares codebase structure against documentation to find:
- Stale references (docs mention things that don't exist)
- Missing documentation (code not mentioned in docs)
- Outdated descriptions (docstrings don't match docs)
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, ClassVar

from moss.summarize import ProjectSummary, Summarizer


@dataclass
class DocIssue:
    """A documentation issue found."""

    severity: str  # "error", "warning", "info"
    category: str  # "stale", "missing", "outdated"
    message: str
    file: Path | None = None
    line: int | None = None
    suggestion: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "severity": self.severity,
            "category": self.category,
            "message": self.message,
            "file": str(self.file) if self.file else None,
            "line": self.line,
            "suggestion": self.suggestion,
        }


@dataclass
class DocCheckResult:
    """Result of documentation check."""

    issues: list[DocIssue] = field(default_factory=list)
    docs_checked: int = 0
    modules_found: int = 0
    modules_documented: int = 0

    @property
    def has_errors(self) -> bool:
        return any(i.severity == "error" for i in self.issues)

    @property
    def has_warnings(self) -> bool:
        return any(i.severity == "warning" for i in self.issues)

    @property
    def error_count(self) -> int:
        return sum(1 for i in self.issues if i.severity == "error")

    @property
    def warning_count(self) -> int:
        return sum(1 for i in self.issues if i.severity == "warning")

    @property
    def coverage(self) -> float:
        if self.modules_found == 0:
            return 0.0
        return self.modules_documented / self.modules_found

    def to_dict(self) -> dict[str, Any]:
        return {
            "issues": [i.to_dict() for i in self.issues],
            "stats": {
                "docs_checked": self.docs_checked,
                "modules_found": self.modules_found,
                "modules_documented": self.modules_documented,
                "coverage": self.coverage,
                "errors": self.error_count,
                "warnings": self.warning_count,
            },
        }

    def to_compact(self) -> str:
        """Format as compact single-line summary (token-efficient).

        Example: docs: 85% coverage | 12/14 modules | 2 errors, 1 warning
        """
        doc, found = self.modules_documented, self.modules_found
        parts = [f"docs: {self.coverage:.0%} coverage"]
        parts.append(f"{doc}/{found} modules")
        if self.error_count or self.warning_count:
            issue_parts = []
            if self.error_count:
                issue_parts.append(f"{self.error_count} errors")
            if self.warning_count:
                issue_parts.append(f"{self.warning_count} warnings")
            parts.append(", ".join(issue_parts))
        return " | ".join(parts)

    def to_markdown(self) -> str:
        lines = ["# Documentation Check Results", ""]

        # Stats
        doc, found = self.modules_documented, self.modules_found
        lines.append(f"**Coverage:** {self.coverage:.0%} ({doc}/{found} modules)")
        lines.append(f"**Docs checked:** {self.docs_checked}")
        lines.append(f"**Issues:** {self.error_count} errors, {self.warning_count} warnings")
        lines.append("")

        if not self.issues:
            lines.append("No issues found.")
            return "\n".join(lines)

        # Group by category
        by_category: dict[str, list[DocIssue]] = {}
        for issue in self.issues:
            by_category.setdefault(issue.category, []).append(issue)

        for category, issues in sorted(by_category.items()):
            lines.append(f"## {category.title()}")
            lines.append("")
            for issue in issues:
                if issue.severity == "error":
                    icon = "[!]"
                elif issue.severity == "warning":
                    icon = "[?]"
                else:
                    icon = "[i]"
                lines.append(f"- {icon} {issue.message}")
                if issue.file:
                    loc = f"`{issue.file}`:{issue.line}" if issue.line else f"`{issue.file}`"
                    lines.append(f"  - File: {loc}")
                if issue.suggestion:
                    lines.append(f"  - Suggestion: {issue.suggestion}")
            lines.append("")

        return "\n".join(lines)


class DocChecker:
    """Check documentation freshness against codebase."""

    # Pattern for markdown links [text](url)
    LINK_PATTERN = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")

    def __init__(self, root: Path, *, check_links: bool = False):
        self.root = root.resolve()
        self.summarizer = Summarizer(include_private=False, include_tests=False)
        self.check_links = check_links

    def check(self) -> DocCheckResult:
        """Run all documentation checks."""
        result = DocCheckResult()

        # Get codebase summary
        summary = self.summarizer.summarize_project(self.root)
        all_modules = self._get_all_modules(summary)
        result.modules_found = len(all_modules)

        # Find documentation files
        doc_files = self._find_doc_files()
        result.docs_checked = len(doc_files)

        # Extract module references from docs
        doc_references: dict[str, list[tuple[Path, int]]] = {}
        for doc_file in doc_files:
            refs = self._extract_references(doc_file)
            for ref, line in refs:
                doc_references.setdefault(ref, []).append((doc_file, line))

        # Check for stale references (mentioned in docs but don't exist)
        for ref, locations in doc_references.items():
            if not self._module_exists(ref, all_modules):
                for doc_file, line in locations:
                    result.issues.append(
                        DocIssue(
                            severity="warning",
                            category="stale",
                            message=f"Reference to `{ref}` not found in codebase",
                            file=doc_file,
                            line=line,
                            suggestion=f"Remove or update reference to `{ref}`",
                        )
                    )

        # Check for missing documentation
        documented_modules = set()
        for ref in doc_references:
            # Normalize reference
            normalized = self._normalize_module_name(ref)
            if normalized:
                documented_modules.add(normalized)

        result.modules_documented = len(documented_modules & all_modules)

        # Find undocumented modules (only warn for public, significant modules)
        for module in all_modules:
            if module not in documented_modules:
                # Skip __init__ and very small modules
                if module.endswith("__init__"):
                    continue
                result.issues.append(
                    DocIssue(
                        severity="info",
                        category="missing",
                        message=f"Module `{module}` not mentioned in documentation",
                        suggestion=f"Add documentation for `{module}`",
                    )
                )

        # Check README specifically
        readme_issues = self._check_readme(summary)
        result.issues.extend(readme_issues)

        # Check links if enabled
        if self.check_links:
            for doc_file in doc_files:
                link_issues = self._check_links(doc_file)
                result.issues.extend(link_issues)

        return result

    def _get_all_modules(self, summary: ProjectSummary) -> set[str]:
        """Get all module names from summary."""
        modules = set()

        def add_from_package(pkg, prefix: str = ""):
            pkg_prefix = f"{prefix}{pkg.name}." if prefix else f"{pkg.name}."
            for f in pkg.files:
                modules.add(f"{prefix}{pkg.name}.{f.module_name}")
            for sub in pkg.subpackages:
                add_from_package(sub, pkg_prefix)

        for pkg in summary.packages:
            add_from_package(pkg)

        for f in summary.standalone_files:
            modules.add(f.module_name)

        return modules

    def _find_doc_files(self) -> list[Path]:
        """Find all documentation files."""
        files = []

        # README
        for name in ["README.md", "README.rst", "README.txt", "README"]:
            readme = self.root / name
            if readme.exists():
                files.append(readme)

        # docs/ directory
        docs_dir = self.root / "docs"
        if docs_dir.exists():
            for f in docs_dir.rglob("*.md"):
                files.append(f)

        # CLAUDE.md, CONTRIBUTING.md, etc.
        for name in ["CLAUDE.md", "CONTRIBUTING.md", "CHANGELOG.md"]:
            f = self.root / name
            if f.exists():
                files.append(f)

        return files

    def _extract_references(self, doc_file: Path) -> list[tuple[str, int]]:
        """Extract code/module references from a documentation file."""
        refs = []
        try:
            content = doc_file.read_text()
        except Exception:
            return refs

        lines = content.splitlines()
        for i, line in enumerate(lines, 1):
            # Match backtick references like `moss.cli` or `cli.py`
            for match in re.finditer(r"`([a-zA-Z_][a-zA-Z0-9_./]*)`", line):
                ref = match.group(1)
                if self._looks_like_module(ref):
                    refs.append((ref, i))

            # Match code block language hints
            if line.strip().startswith("```python"):
                # Could extract imports from code blocks
                pass

        return refs

    def _looks_like_module(self, ref: str) -> bool:
        """Check if a reference looks like a module name."""
        # Skip common non-module patterns
        if ref in {"true", "false", "null", "None", "True", "False"}:
            return False
        if ref.startswith("--") or ref.startswith("-"):  # CLI flags
            return False
        if "/" in ref and not ref.endswith(".py"):  # Paths that aren't Python
            return False
        if ref.startswith("http"):
            return False

        # Looks like a module if it has dots or ends with .py
        return "." in ref or ref.endswith(".py") or ref.startswith("moss")

    # Files that are generated, not in source tree
    GENERATED_FILES: ClassVar[set[str]] = {"moss_config.py", "moss.toml"}

    # Entry point group names (not modules)
    ENTRY_POINT_GROUPS: ClassVar[set[str]] = {
        "moss.plugins",
        "moss.synthesis.generators",
        "moss.synthesis.validators",
        "moss.synthesis.strategies",
        "moss.synthesis.libraries",
    }

    def _module_exists(self, ref: str, all_modules: set[str]) -> bool:
        """Check if a reference matches an existing module or file."""
        # Skip known generated files
        if ref in self.GENERATED_FILES:
            return True

        # Skip entry point group names
        if ref in self.ENTRY_POINT_GROUPS:
            return True

        # Check if it's a file path that exists
        if "/" in ref or ref.endswith(".py") or ref.endswith(".md"):
            file_path = self.root / ref
            if file_path.exists():
                return True

        normalized = self._normalize_module_name(ref)
        if not normalized:
            return True  # Can't verify, assume it's fine

        # Direct match
        if normalized in all_modules:
            return True

        # Partial match (e.g., "cli" matches "moss.cli")
        for module in all_modules:
            if module.endswith(f".{normalized}") or module == normalized:
                return True

        return False

    def _normalize_module_name(self, ref: str) -> str | None:
        """Normalize a reference to a module name."""
        # Remove .py extension
        if ref.endswith(".py"):
            ref = ref[:-3]

        # Convert path to module
        ref = ref.replace("/", ".")

        # Remove src. prefix
        if ref.startswith("src."):
            ref = ref[4:]

        return ref if ref else None

    def _check_readme(self, summary: ProjectSummary) -> list[DocIssue]:
        """Check README for common issues."""
        issues = []

        readme = self.root / "README.md"
        if not readme.exists():
            issues.append(
                DocIssue(
                    severity="error",
                    category="missing",
                    message="No README.md found",
                    suggestion="Create a README.md with project overview",
                )
            )
            return issues

        try:
            content = readme.read_text()
        except Exception:
            return issues

        # Check for project structure section
        if "## Project Structure" in content or "## Structure" in content:
            # Verify it mentions key packages
            for pkg in summary.packages:
                if pkg.name not in content:
                    issues.append(
                        DocIssue(
                            severity="info",
                            category="outdated",
                            message=f"Package `{pkg.name}` not in README project structure",
                            file=readme,
                            suggestion=f"Add `{pkg.name}/` to project structure section",
                        )
                    )

        # Check for outdated statistics
        lines_match = re.search(r"(\d+)\s*lines", content, re.IGNORECASE)
        if lines_match:
            documented_lines = int(lines_match.group(1))
            actual_lines = summary.total_lines
            # Allow 20% variance before warning
            if abs(documented_lines - actual_lines) > actual_lines * 0.2:
                msg = f"README says {documented_lines} lines but codebase has {actual_lines}"
                issues.append(
                    DocIssue(
                        severity="warning",
                        category="outdated",
                        message=msg,
                        file=readme,
                        suggestion="Update line count statistics",
                    )
                )

        return issues

    def _check_links(self, doc_file: Path) -> list[DocIssue]:
        """Check links in a documentation file."""
        issues: list[DocIssue] = []

        try:
            content = doc_file.read_text()
        except Exception:
            return issues

        lines = content.splitlines()
        for i, line in enumerate(lines, 1):
            for match in self.LINK_PATTERN.finditer(line):
                link_text = match.group(1)
                link_url = match.group(2)

                # Skip external URLs
                if link_url.startswith(("http://", "https://", "mailto:")):
                    continue

                # Skip anchor-only links
                if link_url.startswith("#"):
                    continue

                # Handle relative links
                if link_url.startswith("/"):
                    # Absolute path from root
                    target = self.root / link_url[1:]
                else:
                    # Relative to current file
                    target = doc_file.parent / link_url

                # Remove anchor from path
                if "#" in str(target):
                    target = Path(str(target).split("#")[0])

                # Check if target exists
                if not target.exists():
                    issues.append(
                        DocIssue(
                            severity="warning",
                            category="broken_link",
                            message=f"Broken link: [{link_text}]({link_url})",
                            file=doc_file,
                            line=i,
                            suggestion=f"Fix or remove link to `{link_url}`",
                        )
                    )

        return issues
