"""Patch Application: AST-aware code edits with fallback."""

from __future__ import annotations

import ast
from dataclasses import dataclass
from enum import Enum, auto

from .anchors import (
    Anchor,
    AnchorMatch,
    AnchorNotFoundError,
    AnchorResolver,
)


class PatchType(Enum):
    """Types of patches."""

    REPLACE = auto()  # Replace entire anchor content
    INSERT_BEFORE = auto()  # Insert before anchor
    INSERT_AFTER = auto()  # Insert after anchor
    DELETE = auto()  # Delete anchor


@dataclass
class Patch:
    """A code patch to apply."""

    anchor: Anchor
    patch_type: PatchType
    content: str = ""  # New content (empty for DELETE)


@dataclass
class PatchResult:
    """Result of applying a patch."""

    success: bool
    original: str
    patched: str
    match: AnchorMatch | None = None
    error: str | None = None
    used_fallback: bool = False


class PatchError(Exception):
    """Patch application failed."""

    def __init__(self, message: str, original: str, partial: str | None = None):
        super().__init__(message)
        self.original = original
        self.partial = partial


def _get_indent(line: str) -> str:
    """Extract leading whitespace from a line."""
    return line[: len(line) - len(line.lstrip())]


def _reindent(content: str, indent: str) -> str:
    """Reindent content to match target indentation."""
    lines = content.splitlines()
    if not lines:
        return content

    # Find the minimum indentation in the content (ignoring empty lines)
    min_indent = float("inf")
    for line in lines:
        if line.strip():
            line_indent = len(line) - len(line.lstrip())
            min_indent = min(min_indent, line_indent)

    if min_indent == float("inf"):
        min_indent = 0

    # Reindent all lines
    result = []
    for line in lines:
        if line.strip():
            result.append(indent + line[int(min_indent) :])
        else:
            result.append("")

    return "\n".join(result)


def apply_patch(source: str, patch: Patch, min_score: float = 0.6) -> PatchResult:
    """Apply a patch to source code.

    Args:
        source: Original source code
        patch: The patch to apply
        min_score: Minimum anchor match score

    Returns:
        PatchResult with success status and patched code
    """
    resolver = AnchorResolver(source, min_score)

    try:
        match = resolver.resolve(patch.anchor)
    except AnchorNotFoundError as e:
        return PatchResult(
            success=False,
            original=source,
            patched=source,
            error=str(e),
        )

    lines = source.splitlines(keepends=True)

    # Get line indices (0-indexed)
    start_idx = match.lineno - 1
    end_idx = match.end_lineno  # Already correct for slicing

    # Get indentation from the first line of the match
    base_indent = _get_indent(lines[start_idx]) if lines else ""

    if patch.patch_type == PatchType.REPLACE:
        # Reindent the new content to match
        new_content = _reindent(patch.content, base_indent)
        if not new_content.endswith("\n"):
            new_content += "\n"

        # Replace the lines
        new_lines = [*lines[:start_idx], new_content, *lines[end_idx:]]

    elif patch.patch_type == PatchType.INSERT_BEFORE:
        new_content = _reindent(patch.content, base_indent)
        if not new_content.endswith("\n"):
            new_content += "\n"
        new_lines = [*lines[:start_idx], new_content, *lines[start_idx:]]

    elif patch.patch_type == PatchType.INSERT_AFTER:
        new_content = _reindent(patch.content, base_indent)
        if not new_content.endswith("\n"):
            new_content += "\n"
        new_lines = [*lines[:end_idx], new_content, *lines[end_idx:]]

    elif patch.patch_type == PatchType.DELETE:
        new_lines = lines[:start_idx] + lines[end_idx:]

    else:
        return PatchResult(
            success=False,
            original=source,
            patched=source,
            error=f"Unknown patch type: {patch.patch_type}",
        )

    patched = "".join(new_lines)

    # Validate the patched code parses
    try:
        ast.parse(patched)
    except SyntaxError as e:
        return PatchResult(
            success=False,
            original=source,
            patched=patched,
            match=match,
            error=f"Patch created invalid syntax: {e}",
        )

    return PatchResult(
        success=True,
        original=source,
        patched=patched,
        match=match,
    )


def apply_text_patch(
    source: str,
    search: str,
    replace: str,
    *,
    occurrence: int = 1,
) -> PatchResult:
    """Apply a text-based patch (fallback for broken AST).

    Args:
        source: Original source code
        search: Text to search for
        replace: Replacement text
        occurrence: Which occurrence to replace (1-indexed, 0 for all)

    Returns:
        PatchResult with success status and patched code
    """
    if search not in source:
        return PatchResult(
            success=False,
            original=source,
            patched=source,
            error=f"Search text not found: {search[:50]}...",
            used_fallback=True,
        )

    if occurrence == 0:
        # Replace all occurrences
        patched = source.replace(search, replace)
    else:
        # Replace specific occurrence
        parts = source.split(search)
        if len(parts) <= occurrence:
            return PatchResult(
                success=False,
                original=source,
                patched=source,
                error=f"Occurrence {occurrence} not found (only {len(parts) - 1} matches)",
                used_fallback=True,
            )

        patched = search.join(parts[:occurrence]) + replace + search.join(parts[occurrence:])

    return PatchResult(
        success=True,
        original=source,
        patched=patched,
        used_fallback=True,
    )


def apply_patch_with_fallback(
    source: str,
    patch: Patch,
    *,
    fallback_search: str | None = None,
    fallback_replace: str | None = None,
) -> PatchResult:
    """Try AST-based patch, fall back to text if AST is broken.

    Args:
        source: Original source code
        patch: The AST-based patch to try first
        fallback_search: Text to search for if AST fails
        fallback_replace: Replacement text if AST fails

    Returns:
        PatchResult with success status and patched code
    """
    # Try to parse the source first
    try:
        ast.parse(source)
    except SyntaxError:
        # Source has broken AST, use text fallback if provided
        if fallback_search and fallback_replace is not None:
            return apply_text_patch(source, fallback_search, fallback_replace)
        return PatchResult(
            success=False,
            original=source,
            patched=source,
            error="Source has syntax errors and no fallback provided",
            used_fallback=True,
        )

    # Try AST-based patch
    result = apply_patch(source, patch)

    # If AST patch fails and we have fallback, try text-based
    if not result.success and fallback_search and fallback_replace is not None:
        return apply_text_patch(source, fallback_search, fallback_replace)

    return result
