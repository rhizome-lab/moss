"""Code editing with intelligent routing.

This module provides intelligent code editing that routes tasks to
appropriate handlers based on complexity:

- Simple: Direct refactoring (rename, move, fix typo)
- Medium: Multi-agent decomposition
- Complex/Novel: Synthesis fallback

Usage:
    from moss.edit import edit, EditContext

    context = EditContext(project_root=Path("."))
    result = await edit("Add a retry decorator with exponential backoff", context)
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from moss.synthesis import Specification


# =============================================================================
# Types
# =============================================================================


class TaskComplexity(Enum):
    """Complexity level of an edit task."""

    SIMPLE = "simple"  # Direct edits (fix typo, rename variable)
    MEDIUM = "medium"  # Localized changes (add function, refactor class)
    COMPLEX = "complex"  # Multi-file changes, new features
    NOVEL = "novel"  # No clear pattern, requires design


@dataclass
class EditContext:
    """Context for edit operations."""

    project_root: Path
    target_file: Path | None = None
    target_symbol: str | None = None
    language: str = "python"
    constraints: list[str] = field(default_factory=list)
    tests_file: Path | None = None

    # Analysis results (populated during complexity analysis)
    affected_files: list[Path] = field(default_factory=list)
    related_symbols: list[str] = field(default_factory=list)


@dataclass
class EditResult:
    """Result of an edit operation."""

    success: bool
    changes: list[FileChange] = field(default_factory=list)
    method: str = ""  # "structural", "multi_agent", "synthesis"
    iterations: int = 0
    error: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class FileChange:
    """A change to a single file."""

    path: Path
    original: str
    modified: str
    description: str = ""

    @property
    def has_changes(self) -> bool:
        """Check if content actually changed."""
        return self.original != self.modified


# =============================================================================
# Complexity Analysis
# =============================================================================

# Patterns that suggest simple edits
SIMPLE_PATTERNS = [
    r"\bfix\s+typo\b",
    r"\brename\s+\w+\s+to\s+\w+\b",
    r"\bremove\s+(unused|dead)\b",
    r"\bupdate\s+(comment|docstring)\b",
    r"\bchange\s+\w+\s+from\s+\w+\s+to\s+\w+\b",
    r"\bdelete\s+(function|method|class)\s+\w+\b",
    r"\badd\s+(import|type\s+hint)\b",
]

# Patterns that suggest medium complexity
MEDIUM_PATTERNS = [
    r"\badd\s+(function|method|class)\b",
    r"\brefactor\s+\w+\b",
    r"\bextract\s+(function|method|class)\b",
    r"\binline\s+(function|variable)\b",
    r"\bmove\s+\w+\s+to\s+\w+\b",
    r"\bsplit\s+(class|module)\b",
    r"\badd\s+(logging|error\s+handling)\b",
]

# Patterns that suggest complex edits
COMPLEX_PATTERNS = [
    r"\bimplement\s+\w+",
    r"\badd\s+(feature|functionality)\b",
    r"\bcreate\s+(api|endpoint|service)\b",
    r"\bintegrate\s+\w+\b",
    r"\bmigrate\s+\w+\b",
    r"\brewrite\s+\w+\b",
    r"\boptimize\s+\w+\b",
]

# Patterns that suggest novel/design work
NOVEL_PATTERNS = [
    r"\bdesign\s+\w+\b",
    r"\barchitect\s+\w+\b",
    r"\bfrom\s+scratch\b",
    r"\bnew\s+(system|architecture)\b",
    r"\bunknown\s+\w+\b",
]


def analyze_complexity(task: str, context: EditContext) -> TaskComplexity:
    """Determine task complexity based on description and context.

    Args:
        task: Task description
        context: Edit context with project info

    Returns:
        TaskComplexity level
    """
    task_lower = task.lower()

    # Check novel patterns first (highest priority)
    for pattern in NOVEL_PATTERNS:
        if re.search(pattern, task_lower):
            return TaskComplexity.NOVEL

    # Check for multi-file indicators early (these override simpler patterns)
    multi_file_keywords = ["across", "all files", "entire", "project-wide", "codebase", "workspace"]
    if any(kw in task_lower for kw in multi_file_keywords):
        return TaskComplexity.COMPLEX

    # Check complex patterns
    for pattern in COMPLEX_PATTERNS:
        if re.search(pattern, task_lower):
            return TaskComplexity.COMPLEX

    # Check simple patterns
    for pattern in SIMPLE_PATTERNS:
        if re.search(pattern, task_lower):
            return TaskComplexity.SIMPLE

    # Check medium patterns
    for pattern in MEDIUM_PATTERNS:
        if re.search(pattern, task_lower):
            return TaskComplexity.MEDIUM

    # Heuristics based on task length and keywords
    word_count = len(task.split())

    # Very short tasks are usually simple
    if word_count <= 5:
        return TaskComplexity.SIMPLE

    # Long, detailed tasks are often complex
    if word_count > 30:
        return TaskComplexity.COMPLEX

    # Default to medium for unclear cases
    return TaskComplexity.MEDIUM


def is_direct_edit(task: str) -> bool:
    """Check if task is a direct edit (simple)."""
    return analyze_complexity(task, EditContext(project_root=Path("."))) == TaskComplexity.SIMPLE


def is_localized_change(task: str) -> bool:
    """Check if task is a localized change (medium)."""
    return analyze_complexity(task, EditContext(project_root=Path("."))) == TaskComplexity.MEDIUM


def is_multi_file_change(task: str) -> bool:
    """Check if task affects multiple files."""
    multi_file_keywords = ["across", "all files", "entire", "project-wide", "codebase", "workspace"]
    return any(kw in task.lower() for kw in multi_file_keywords)


def is_new_feature(task: str) -> bool:
    """Check if task is a new feature."""
    feature_keywords = ["implement", "add feature", "create", "build", "new"]
    return any(kw in task.lower() for kw in feature_keywords)


def is_novel_problem(task: str) -> bool:
    """Check if task is a novel problem requiring design."""
    return analyze_complexity(task, EditContext(project_root=Path("."))) == TaskComplexity.NOVEL


# =============================================================================
# Edit Handlers
# =============================================================================


async def structural_edit(task: str, context: EditContext) -> EditResult:
    """Handle simple structural edits using refactoring tools.

    Simple edits include:
    - Renaming symbols
    - Fixing typos
    - Updating comments/docstrings
    - Removing unused code
    - Adding imports

    Args:
        task: Task description
        context: Edit context

    Returns:
        EditResult with changes
    """
    from moss.refactoring import Refactorer, RefactoringScope, RenameRefactoring

    # Parse task to determine refactoring type
    task_lower = task.lower()

    # Handle rename
    rename_match = re.search(r"rename\s+(\w+)\s+to\s+(\w+)", task_lower)
    if rename_match:
        old_name, new_name = rename_match.groups()

        refactorer = Refactorer(context.project_root)
        refactoring = RenameRefactoring(
            old_name=old_name,
            new_name=new_name,
            scope=RefactoringScope.FILE if context.target_file else RefactoringScope.WORKSPACE,
        )

        result = await refactorer.apply(refactoring)

        return EditResult(
            success=result.success,
            changes=[
                FileChange(
                    path=c.path,
                    original=c.original_content,
                    modified=c.new_content,
                    description=c.description,
                )
                for c in result.changes
            ],
            method="structural",
            error=result.errors[0] if result.errors else None,
        )

    # For other simple edits, return placeholder
    # TODO: Implement more structural edit types
    return EditResult(
        success=False,
        method="structural",
        error="Structural edit type not yet implemented for this task",
        metadata={"task": task, "parsed_type": "unknown"},
    )


async def multi_agent_edit(task: str, context: EditContext) -> EditResult:
    """Handle medium complexity edits using multi-agent decomposition.

    Medium edits include:
    - Adding new functions/methods/classes
    - Refactoring existing code
    - Extracting or inlining code
    - Adding logging/error handling

    Args:
        task: Task description
        context: Edit context

    Returns:
        EditResult with changes
    """
    # TODO: Implement multi-agent decomposition
    # This would use the agent orchestration system to:
    # 1. Decompose the task into subtasks
    # 2. Assign subtasks to specialized agents
    # 3. Coordinate and merge results

    return EditResult(
        success=False,
        method="multi_agent",
        error="Multi-agent editing not yet implemented",
        metadata={"task": task, "complexity": "medium"},
    )


async def synthesize_edit(task: str, context: EditContext) -> EditResult:
    """Handle complex edits using synthesis.

    Complex/novel edits include:
    - Implementing new features
    - Creating APIs/services
    - Rewriting/optimizing code
    - Novel designs

    Args:
        task: Task description
        context: Edit context

    Returns:
        EditResult with synthesized code
    """
    from moss.synthesis import Context as SynthesisContext
    from moss.synthesis import SynthesisFramework
    from moss.synthesis.framework import SynthesisConfig
    from moss.synthesis.strategies import (
        PatternBasedDecomposition,
        TestDrivenDecomposition,
        TypeDrivenDecomposition,
    )

    # Extract specification from task
    spec = extract_specification(task, context)

    # Set up synthesis framework
    strategies = [
        TypeDrivenDecomposition(),
        TestDrivenDecomposition(),
        PatternBasedDecomposition(),
    ]

    config = SynthesisConfig(
        max_depth=10,
        max_iterations=50,
        parallel_subproblems=True,
    )

    framework = SynthesisFramework(
        strategies=strategies,
        config=config,
    )

    # Run synthesis
    synth_context = SynthesisContext()
    result = await framework.synthesize(spec, synth_context)

    if result.success and result.solution:
        # Create file change for the target file
        target = context.target_file or context.project_root / "generated.py"

        # Read existing content if file exists
        original = ""
        if target.exists():
            original = target.read_text()

        return EditResult(
            success=True,
            changes=[
                FileChange(
                    path=target,
                    original=original,
                    modified=result.solution,
                    description=f"Synthesized: {task}",
                )
            ],
            method="synthesis",
            iterations=result.iterations,
            metadata={
                "strategy": result.strategy_used,
                "subproblems_solved": result.subproblems_solved,
            },
        )

    return EditResult(
        success=False,
        method="synthesis",
        error=result.error or "Synthesis did not produce a result",
        iterations=result.iterations,
    )


def extract_specification(task: str, context: EditContext) -> Specification:
    """Extract a synthesis specification from task description.

    Args:
        task: Task description
        context: Edit context

    Returns:
        Specification for synthesis
    """
    from moss.synthesis import Specification

    # Build description
    description = task

    # Add context if available
    if context.target_symbol:
        description = f"{task} (in {context.target_symbol})"

    # Extract type signature if mentioned
    type_signature = None
    type_match = re.search(r"type[:\s]+([^,\n]+)", task, re.IGNORECASE)
    if type_match:
        type_signature = type_match.group(1).strip()

    # Extract constraints
    constraints = list(context.constraints)

    # Look for "must" or "should" clauses
    must_matches = re.findall(r"(?:must|should)\s+([^,.]+)", task, re.IGNORECASE)
    constraints.extend(must_matches)

    return Specification(
        description=description,
        type_signature=type_signature,
        constraints=tuple(constraints),
    )


# =============================================================================
# Main Edit Function
# =============================================================================


async def edit(task: str, context: EditContext) -> EditResult:
    """Edit code with intelligent routing based on complexity.

    Routes to appropriate handler:
    - Simple tasks → structural editing (refactoring)
    - Medium tasks → multi-agent decomposition
    - Complex/novel tasks → synthesis

    Args:
        task: Task description (what to do)
        context: Edit context (where to do it)

    Returns:
        EditResult with changes and metadata
    """
    # Analyze complexity
    complexity = analyze_complexity(task, context)

    if complexity == TaskComplexity.SIMPLE:
        return await structural_edit(task, context)

    elif complexity == TaskComplexity.MEDIUM:
        # Try structural first, fall back to multi-agent
        result = await structural_edit(task, context)
        if result.success:
            return result
        return await multi_agent_edit(task, context)

    elif complexity == TaskComplexity.COMPLEX:
        # Try multi-agent first, fall back to synthesis
        result = await multi_agent_edit(task, context)
        if result.success:
            return result
        return await synthesize_edit(task, context)

    else:  # NOVEL
        # Go directly to synthesis for novel problems
        return await synthesize_edit(task, context)


__all__ = [
    "EditAPI",
    "EditContext",
    "EditResult",
    "FileChange",
    "SimpleEditResult",
    "TaskComplexity",
    "analyze_complexity",
    "edit",
    "extract_specification",
    "is_direct_edit",
    "is_localized_change",
    "is_multi_file_change",
    "is_new_feature",
    "is_novel_problem",
    "multi_agent_edit",
    "structural_edit",
    "synthesize_edit",
]


@dataclass
class SimpleEditResult:
    """Result of a direct file edit operation."""

    success: bool
    file_path: str
    message: str
    original_size: int = 0
    new_size: int = 0
    error: str | None = None


class EditAPI:
    """API for direct file modifications."""

    def __init__(self, root: Path):
        self.root = root

    def _resolve_path(self, file_path: str | Path) -> Path:
        """Resolve path relative to root."""
        path = Path(file_path)
        if not path.is_absolute():
            path = self.root / path
        return path

    def write_file(self, file_path: str | Path, content: str) -> SimpleEditResult:
        """Overwrite or create a file with new content."""
        path = self._resolve_path(file_path)
        original_size = path.stat().st_size if path.exists() else 0

        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
            new_size = len(content)
            return SimpleEditResult(
                success=True,
                file_path=str(file_path),
                message=f"Successfully wrote {new_size} bytes to {file_path}",
                original_size=original_size,
                new_size=new_size,
            )
        except OSError as e:
            return SimpleEditResult(
                success=False,
                file_path=str(file_path),
                message=f"Failed to write file: {e}",
                error=str(e),
            )

    def replace_text(
        self, file_path: str | Path, search: str, replace: str, occurrence: int = 0
    ) -> SimpleEditResult:
        """Replace text in a file."""
        path = self._resolve_path(file_path)
        if not path.exists():
            return SimpleEditResult(False, str(file_path), "File not found", error="FileNotFound")

        content = path.read_text()
        original_size = len(content)

        if search not in content:
            return SimpleEditResult(
                False, str(file_path), f"Search string not found: {search[:50]}..."
            )

        if occurrence == 0:
            new_content = content.replace(search, replace)
            count = content.count(search)
            msg = f"Replaced {count} occurrences"
        else:
            parts = content.split(search)
            if len(parts) <= occurrence:
                return SimpleEditResult(False, str(file_path), f"Occurrence {occurrence} not found")

            new_content = (
                search.join(parts[:occurrence]) + replace + search.join(parts[occurrence:])
            )
            msg = f"Replaced occurrence {occurrence}"

        path.write_text(new_content)
        return SimpleEditResult(
            success=True,
            file_path=str(file_path),
            message=msg,
            original_size=original_size,
            new_size=len(new_content),
        )

    def insert_line(
        self,
        file_path: str | Path,
        line_content: str,
        at_line: int | None = None,
        after_pattern: str | None = None,
    ) -> SimpleEditResult:
        """Insert a line into a file at a specific position."""
        path = self._resolve_path(file_path)
        if not path.exists():
            return SimpleEditResult(False, str(file_path), "File not found", error="FileNotFound")

        lines = path.read_text().splitlines(keepends=True)
        original_size = sum(len(line) for line in lines)

        new_line = line_content + "\n" if not line_content.endswith("\n") else line_content

        if at_line is not None:
            # 1-indexed to 0-indexed
            idx = max(0, min(at_line - 1, len(lines)))
            # Ensure the line before has a newline if we're not at the start
            if idx > 0 and not lines[idx - 1].endswith("\n"):
                lines[idx - 1] += "\n"
            lines.insert(idx, new_line)
            msg = f"Inserted line at {at_line}"
        elif after_pattern:
            found = False
            for i, line in enumerate(lines):
                if re.search(after_pattern, line):
                    # Ensure the matched line has a newline
                    if not lines[i].endswith("\n"):
                        lines[i] += "\n"
                    lines.insert(i + 1, new_line)
                    msg = f"Inserted line after pattern: {after_pattern}"
                    found = True
                    break
            if not found:
                return SimpleEditResult(
                    False, str(file_path), f"Pattern not found: {after_pattern}"
                )
        else:
            # Append to end
            # Ensure last line has newline
            if lines and not lines[-1].endswith("\n"):
                lines[-1] += "\n"
            lines.append(new_line)
            msg = "Appended line to end of file"

        new_content = "".join(lines)
        path.write_text(new_content)
        return SimpleEditResult(
            success=True,
            file_path=str(file_path),
            message=msg,
            original_size=original_size,
            new_size=len(new_content),
        )
