"""Bidirectional reference checking between code and documentation.

Validates that:
- Code files reference their documentation (e.g., ``# See: docs/...``)
- Documentation references implementation files (e.g., backtick-quoted paths)
- References are not stale (code modified after docs)

Uses liberal pattern matching to find path-like substrings.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any, ClassVar


@dataclass
class CodeReference:
    """A reference from code to documentation."""

    source_file: Path
    source_line: int
    target_doc: Path
    raw_text: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_file": str(self.source_file),
            "source_line": self.source_line,
            "target_doc": str(self.target_doc),
            "raw_text": self.raw_text,
        }


@dataclass
class DocReference:
    """A reference from documentation to code."""

    source_doc: Path
    source_line: int
    target_file: Path
    raw_text: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_doc": str(self.source_doc),
            "source_line": self.source_line,
            "target_file": str(self.target_file),
            "raw_text": self.raw_text,
        }


@dataclass
class StaleReference:
    """A reference where target was modified after source."""

    source_path: Path
    target_path: Path
    source_mtime: datetime
    target_mtime: datetime
    reference_line: int
    is_code_to_doc: bool  # True if code->doc, False if doc->code

    @property
    def staleness_days(self) -> int:
        """Days since the source became potentially stale."""
        delta = self.target_mtime - self.source_mtime
        return max(0, delta.days)

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_path": str(self.source_path),
            "target_path": str(self.target_path),
            "source_mtime": self.source_mtime.isoformat(),
            "target_mtime": self.target_mtime.isoformat(),
            "staleness_days": self.staleness_days,
            "reference_line": self.reference_line,
            "direction": "code_to_doc" if self.is_code_to_doc else "doc_to_code",
        }


@dataclass
class RefCheckResult:
    """Result of bidirectional reference check."""

    # Code -> Docs references
    code_to_docs: list[CodeReference] = field(default_factory=list)
    code_to_docs_broken: list[CodeReference] = field(default_factory=list)

    # Docs -> Code references
    docs_to_code: list[DocReference] = field(default_factory=list)
    docs_to_code_broken: list[DocReference] = field(default_factory=list)

    # Staleness
    stale_references: list[StaleReference] = field(default_factory=list)

    # Stats
    code_files_checked: int = 0
    doc_files_checked: int = 0

    @property
    def has_errors(self) -> bool:
        return bool(self.code_to_docs_broken or self.docs_to_code_broken)

    @property
    def has_warnings(self) -> bool:
        return bool(self.stale_references)

    @property
    def error_count(self) -> int:
        return len(self.code_to_docs_broken) + len(self.docs_to_code_broken)

    @property
    def warning_count(self) -> int:
        return len(self.stale_references)

    def to_dict(self) -> dict[str, Any]:
        return {
            "stats": {
                "code_files_checked": self.code_files_checked,
                "doc_files_checked": self.doc_files_checked,
                "code_to_docs_valid": len(self.code_to_docs),
                "code_to_docs_broken": len(self.code_to_docs_broken),
                "docs_to_code_valid": len(self.docs_to_code),
                "docs_to_code_broken": len(self.docs_to_code_broken),
                "stale_references": len(self.stale_references),
                "errors": self.error_count,
                "warnings": self.warning_count,
            },
            "broken_code_to_docs": [r.to_dict() for r in self.code_to_docs_broken],
            "broken_docs_to_code": [r.to_dict() for r in self.docs_to_code_broken],
            "stale": [s.to_dict() for s in self.stale_references],
        }

    def to_compact(self) -> str:
        """Format as compact single-line summary (token-efficient).

        Example: refs: ok | 12 valid | 0 broken | 2 stale
        """
        valid = len(self.code_to_docs) + len(self.docs_to_code)
        broken = self.error_count
        status = "FAIL" if broken else "ok"
        parts = [f"refs: {status}"]
        parts.append(f"{valid} valid")
        if broken:
            parts.append(f"{broken} broken")
        if self.stale_references:
            parts.append(f"{len(self.stale_references)} stale")
        return " | ".join(parts)

    def to_markdown(self) -> str:
        lines = ["# Reference Check Results", ""]

        # Stats
        lines.append(f"**Code files:** {self.code_files_checked} checked")
        lines.append(f"**Doc files:** {self.doc_files_checked} checked")
        lines.append(
            f"**References:** {len(self.code_to_docs)} code->doc, "
            f"{len(self.docs_to_code)} doc->code"
        )
        lines.append(f"**Issues:** {self.error_count} errors, {self.warning_count} warnings")
        lines.append("")

        if not self.has_errors and not self.has_warnings:
            lines.append("All references are valid and up-to-date.")
            return "\n".join(lines)

        # Broken references
        if self.code_to_docs_broken:
            lines.append("## Broken Code -> Doc References")
            lines.append("")
            for ref in self.code_to_docs_broken:
                lines.append(f"- `{ref.source_file}:{ref.source_line}` -> `{ref.target_doc}`")
                lines.append(f"  - Not found: `{ref.raw_text}`")
            lines.append("")

        if self.docs_to_code_broken:
            lines.append("## Broken Doc -> Code References")
            lines.append("")
            for ref in self.docs_to_code_broken:
                lines.append(f"- `{ref.source_doc}:{ref.source_line}` -> `{ref.target_file}`")
                lines.append(f"  - Not found: `{ref.raw_text}`")
            lines.append("")

        # Stale references
        if self.stale_references:
            lines.append("## Stale References")
            lines.append("")
            lines.append("These references may need updating (target modified after source):")
            lines.append("")
            lines.append("| Source | Target | Stale Days |")
            lines.append("|--------|--------|------------|")
            for stale in sorted(self.stale_references, key=lambda s: -s.staleness_days):
                lines.append(
                    f"| {stale.source_path} | {stale.target_path} | {stale.staleness_days} |"
                )
            lines.append("")

        return "\n".join(lines)


class RefChecker:
    """Check bidirectional references between code and documentation.

    Uses liberal pattern matching to find path-like substrings.
    """

    # Liberal patterns for code -> docs references
    # These match comments/docstrings with doc paths
    CODE_TO_DOC_PATTERNS: ClassVar[list[str]] = [
        # Explicit markers: # See: docs/*.md or // See: docs/*.md
        r"(?:#|//)\s*[Ss]ee:?\s*(docs/\S+\.md)",
        r"(?:#|//)\s*[Rr]ef:?\s*(docs/\S+\.md)",
        r"(?:#|//)\s*[Dd]ocs?:?\s*(docs/\S+\.md)",
        r"(?:#|//)\s*[Dd]ocumentation:?\s*(docs/\S+\.md)",
        r"(?:#|//)\s*[Rr]elated:?\s*(docs/\S+\.md)",
        # Informal: "see docs/*.md"
        r"[Ss]ee\s+(docs/\S+\.md)",
        # Any docs/*.md path in a string or comment
        r"['\"`](docs/[^'\"` ]+\.md)['\"`]",
    ]

    # Liberal patterns for docs -> code references
    DOC_TO_CODE_PATTERNS: ClassVar[list[str]] = [
        # HTML comments: <!-- Implementation: src/*.py -->
        r"<!--\s*[Ii]mplementation:?\s*((?:src|crates)/\S+\.\w+|Cargo\.toml|pyproject\.toml)\s*-->",
        r"<!--\s*[Cc]ode:?\s*((?:src|crates)/\S+\.\w+|Cargo\.toml|pyproject\.toml)\s*-->",
        r"<!--\s*[Ss]ource:?\s*((?:src|crates)/\S+\.\w+|Cargo\.toml|pyproject\.toml)\s*-->",
        # Backtick code references: `src/*.py`
        r"`((?:src|crates)/[^`]+\.\w+|Cargo\.toml|pyproject\.toml)`",
        # Markdown links to source
        r"\]\(((?:src|crates)/[^)]+\.\w+|Cargo\.toml|pyproject\.toml)\)",
        # Bare paths
        r"((?:src|crates)/\S+\.\w+|Cargo\.toml|pyproject\.toml)",
    ]

    def __init__(
        self,
        root: Path,
        *,
        staleness_days: int = 30,
        code_patterns: list[str] | None = None,
        doc_patterns: list[str] | None = None,
    ):
        """Initialize reference checker.

        Args:
            root: Project root directory
            staleness_days: Warn if code modified more than N days after docs
            code_patterns: Custom patterns for code->doc references
            doc_patterns: Custom patterns for doc->code references
        """
        self.root = root.resolve()
        self.staleness_days = staleness_days
        self.code_patterns = [re.compile(p) for p in (code_patterns or self.CODE_TO_DOC_PATTERNS)]
        self.doc_patterns = [re.compile(p) for p in (doc_patterns or self.DOC_TO_CODE_PATTERNS)]

    def check(self) -> RefCheckResult:
        """Run bidirectional reference check."""
        result = RefCheckResult()

        # Scan code files for doc references
        code_files = self._find_code_files()
        result.code_files_checked = len(code_files)

        for code_file in code_files:
            refs = self._extract_code_to_doc_refs(code_file)
            for ref in refs:
                target_path = self.root / ref.target_doc
                if target_path.exists():
                    result.code_to_docs.append(ref)
                    # Check staleness
                    stale = self._check_staleness(
                        code_file, target_path, ref.source_line, is_code_to_doc=True
                    )
                    if stale:
                        result.stale_references.append(stale)
                else:
                    result.code_to_docs_broken.append(ref)

        # Scan doc files for code references
        doc_files = self._find_doc_files()
        result.doc_files_checked = len(doc_files)

        for doc_file in doc_files:
            refs = self._extract_doc_to_code_refs(doc_file)
            for ref in refs:
                target_path = self.root / ref.target_file
                if target_path.exists():
                    result.docs_to_code.append(ref)
                    # Check staleness (code changed after doc)
                    stale = self._check_staleness(
                        doc_file, target_path, ref.source_line, is_code_to_doc=False
                    )
                    if stale:
                        result.stale_references.append(stale)
                else:
                    result.docs_to_code_broken.append(ref)

        return result

    def _find_code_files(self) -> list[Path]:
        """Find source files (Python, Rust)."""
        files = []
        # Python source
        src_dir = self.root / "src"
        if src_dir.exists():
            files.extend(src_dir.rglob("*.py"))
            files.extend(src_dir.rglob("*.rs"))

        # Rust crates
        crates_dir = self.root / "crates"
        if crates_dir.exists():
            files.extend(crates_dir.rglob("*.rs"))

        # Config files
        for name in ["Cargo.toml", "pyproject.toml"]:
            path = self.root / name
            if path.exists():
                files.append(path)

        # Root level .py/.rs files
        files.extend(self.root.glob("*.py"))
        files.extend(self.root.glob("*.rs"))

        return sorted(set(files))

    def _find_doc_files(self) -> list[Path]:
        """Find documentation files."""
        files = []
        docs_dir = self.root / "docs"
        if docs_dir.exists():
            files.extend(docs_dir.rglob("*.md"))
        # Also check root level markdown
        for name in ["README.md", "CONTRIBUTING.md", "CHANGELOG.md", "CLAUDE.md"]:
            path = self.root / name
            if path.exists():
                files.append(path)
        return sorted(set(files))

    def _extract_code_to_doc_refs(self, code_file: Path) -> list[CodeReference]:
        """Extract doc references from a code file."""
        refs = []
        try:
            content = code_file.read_text()
            for i, line in enumerate(content.splitlines(), 1):
                for pattern in self.code_patterns:
                    for match in pattern.finditer(line):
                        doc_path = match.group(1)
                        # Skip if it looks like a false positive
                        if self._is_valid_doc_path(doc_path):
                            refs.append(
                                CodeReference(
                                    source_file=code_file.relative_to(self.root),
                                    source_line=i,
                                    target_doc=Path(doc_path),
                                    raw_text=match.group(0),
                                )
                            )
        except (OSError, UnicodeDecodeError):
            pass
        return refs

    def _extract_doc_to_code_refs(self, doc_file: Path) -> list[DocReference]:
        """Extract code references from a doc file."""
        refs = []
        seen = set()
        try:
            content = doc_file.read_text()
            for i, line in enumerate(content.splitlines(), 1):
                for pattern in self.doc_patterns:
                    for match in pattern.finditer(line):
                        code_path = match.group(1)
                        # Skip if it looks like a false positive
                        if self._is_valid_code_path(code_path):
                            ref = DocReference(
                                source_doc=doc_file.relative_to(self.root),
                                source_line=i,
                                target_file=Path(code_path),
                                raw_text=match.group(0),
                            )
                            # Simple deduplication per line/pattern
                            key = (i, code_path)
                            if key not in seen:
                                refs.append(ref)
                                seen.add(key)
        except (OSError, UnicodeDecodeError):
            pass
        return refs

    def _is_valid_doc_path(self, path: str) -> bool:
        """Check if a path looks like a valid doc reference."""
        if not path:
            return False
        # Must be in docs/ and end with .md
        if not path.startswith("docs/"):
            return False
        if not path.endswith(".md"):
            return False
        # No weird characters
        if any(c in path for c in ["<", ">", "|", "*", "?"]):
            return False
        return True

    def _is_valid_code_path(self, path: str) -> bool:
        """Check if a path looks like a valid code reference."""
        if not path:
            return False

        # Reject paths with invalid filename characters
        if any(c in path for c in "<>|\"'"):
            return False

        # Valid files/directories
        if path in ("Cargo.toml", "pyproject.toml", "Architecture.md"):
            return True

        if path.startswith("src/") or path.startswith("crates/"):
            # Valid extensions if it has one
            if "." in path:
                return any(path.endswith(ext) for ext in [".py", ".rs", ".toml"])
            return True  # Directory reference

        return False

    def _check_staleness(
        self,
        source_path: Path,
        target_path: Path,
        reference_line: int,
        is_code_to_doc: bool,
    ) -> StaleReference | None:
        """Check if a reference is stale (target modified after source)."""
        try:
            source_mtime = datetime.fromtimestamp(source_path.stat().st_mtime)
            target_mtime = datetime.fromtimestamp(target_path.stat().st_mtime)

            # For code->doc: doc modified after code suggests code might need updating
            # For doc->code: code modified after doc suggests doc might need updating
            if target_mtime > source_mtime:
                stale = StaleReference(
                    source_path=source_path.relative_to(self.root),
                    target_path=target_path.relative_to(self.root),
                    source_mtime=source_mtime,
                    target_mtime=target_mtime,
                    reference_line=reference_line,
                    is_code_to_doc=is_code_to_doc,
                )
                if stale.staleness_days >= self.staleness_days:
                    return stale
        except (OSError, ValueError):
            pass
        return None


def create_ref_checker(root: Path | None = None, **kwargs: Any) -> RefChecker:
    """Factory function to create a RefChecker.

    Args:
        root: Project root (default: current directory)
        **kwargs: Additional arguments passed to RefChecker

    Returns:
        Configured RefChecker instance
    """
    if root is None:
        root = Path.cwd()
    return RefChecker(root, **kwargs)
