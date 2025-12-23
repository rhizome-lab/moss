"""Roadmap visualization from TODO.md.

This module parses TODO.md and provides visualization of project progress,
including what's done, in progress, and what's next.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import TextIO


class PhaseStatus(Enum):
    """Status of a roadmap phase."""

    COMPLETE = "complete"
    IN_PROGRESS = "in_progress"
    FUTURE = "future"


@dataclass
class TaskItem:
    """A single task item within a phase."""

    description: str
    completed: bool
    indent: int = 0


@dataclass
class Phase:
    """A phase in the roadmap."""

    id: str  # e.g., "29", "29a", "D"
    title: str
    status: PhaseStatus
    description: str = ""
    tasks: list[TaskItem] = field(default_factory=list)
    subphases: list[Phase] = field(default_factory=list)

    @property
    def progress(self) -> tuple[int, int]:
        """Return (completed, total) task counts."""
        completed = sum(1 for t in self.tasks if t.completed)
        total = len(self.tasks)
        # Include subphase tasks
        for sub in self.subphases:
            sub_completed, sub_total = sub.progress
            completed += sub_completed
            total += sub_total
        return completed, total

    @property
    def progress_percent(self) -> float:
        """Return progress as percentage."""
        completed, total = self.progress
        return (completed / total * 100) if total > 0 else 100.0


@dataclass
class Roadmap:
    """Parsed roadmap from TODO.md."""

    in_progress: list[Phase] = field(default_factory=list)
    future: list[Phase] = field(default_factory=list)
    completed: list[Phase] = field(default_factory=list)

    @property
    def all_phases(self) -> list[Phase]:
        """All phases in order."""
        return self.in_progress + self.future + self.completed


def is_truly_incomplete(phase: Phase) -> bool:
    """Check if a phase is truly incomplete.

    A phase is incomplete if:
    - Not marked as COMPLETE status
    - Has tasks remaining (progress < 100%)
    """
    if phase.status == PhaseStatus.COMPLETE:
        return False
    completed, total = phase.progress
    return total == 0 or completed < total


def parse_todo_md(path: Path) -> Roadmap:
    """Parse TODO.md into a Roadmap structure.

    Args:
        path: Path to TODO.md file

    Returns:
        Parsed Roadmap
    """
    content = path.read_text()
    roadmap = Roadmap()

    current_section: str | None = None
    current_phase: Phase | None = None
    current_subphase: Phase | None = None

    lines = content.split("\n")
    i = 0

    while i < len(lines):
        line = lines[i]

        # Section headers
        if line.startswith("## In Progress") or line.startswith("## Next Up"):
            current_section = "in_progress"
            i += 1
            continue
        elif line.startswith("## Future") or line.startswith("## Backlog"):
            current_section = "future"
            i += 1
            continue
        elif line.startswith("## "):
            # Unknown section - reset current_section and continue looking
            current_section = None
            i += 1
            continue

        # Phase headers - handle multiple formats:
        # - "### Phase X: Title" -> id=X, title=Title
        # - "### Title" -> id="", title=Title
        # - "### Title ✅" -> id="", title=Title, complete
        # - "**Title:**" -> id="", title=Title (backlog format)
        header_content = None
        is_complete = False

        if line.startswith("### "):
            header_content = line[4:].strip()
            is_complete = header_content.endswith("✅")
            if is_complete:
                header_content = header_content[:-1].strip()
        elif current_section and line.startswith("**") and line.rstrip().endswith(":**"):
            # Backlog format: **Title:**
            header_content = line.strip()[2:-3]  # Remove ** and :**

        if header_content:
            # Check for "Phase X:" format
            phase_id_match = re.match(r"^Phase (\w+):\s*(.+)$", header_content)
            if phase_id_match:
                phase_id = phase_id_match.group(1)
                title = phase_id_match.group(2).strip()
            else:
                # No phase ID - use full header as title
                phase_id = ""
                title = header_content

            if is_complete:
                status = PhaseStatus.COMPLETE
            elif current_section == "in_progress":
                status = PhaseStatus.IN_PROGRESS
            else:
                status = PhaseStatus.FUTURE

            current_phase = Phase(id=phase_id, title=title, status=status)
            current_subphase = None

            if current_section == "in_progress":
                roadmap.in_progress.append(current_phase)
            elif current_section == "future":
                roadmap.future.append(current_phase)

            i += 1
            continue

        # Subphase headers (#### 29a: Title)
        subphase_match = re.match(r"^#### (\w+):?\s*(.+?)(\s*✅)?$", line)
        if subphase_match and current_phase:
            sub_id = subphase_match.group(1)
            title = subphase_match.group(2).strip()
            is_complete = subphase_match.group(3) is not None

            if is_complete:
                status = PhaseStatus.COMPLETE
            elif current_phase.status == PhaseStatus.IN_PROGRESS:
                status = PhaseStatus.IN_PROGRESS
            else:
                status = PhaseStatus.FUTURE

            current_subphase = Phase(id=sub_id, title=title, status=status)
            current_phase.subphases.append(current_subphase)
            i += 1
            continue

        # Task items (- [x] or - [ ])
        task_match = re.match(r"^(\s*)- \[([ xX])\] (.+)$", line)
        if task_match:
            indent = len(task_match.group(1))
            completed = task_match.group(2).lower() == "x"
            description = task_match.group(3).strip()

            task = TaskItem(description=description, completed=completed, indent=indent)

            if current_subphase:
                current_subphase.tasks.append(task)
            elif current_phase:
                current_phase.tasks.append(task)

            i += 1
            continue

        # Description paragraph (non-empty, non-header line after phase)
        if current_phase and line.strip() and not line.startswith("#") and not line.startswith("-"):
            if not current_subphase and not current_phase.description:
                current_phase.description = line.strip()

        i += 1

    return roadmap


# =============================================================================
# Plain Text Display
# =============================================================================


def format_plain(roadmap: Roadmap, show_completed: bool = False, max_items: int = 0) -> str:
    """Format roadmap as plain text.

    Args:
        roadmap: Parsed roadmap
        show_completed: Include completed phases
        max_items: Max items per section (0 = unlimited)

    Returns:
        Formatted string
    """
    lines: list[str] = []

    # In Progress section - only incomplete phases (not marked complete and not 100%)
    active_phases = [p for p in roadmap.in_progress if is_truly_incomplete(p)]
    if active_phases:
        lines.append("In Progress")
        lines.append("=" * 50)
        for phase in active_phases:
            lines.extend(_format_phase_plain(phase))
        lines.append("")

    # Future section - show what's next (only incomplete)
    pending_future = [p for p in roadmap.future if is_truly_incomplete(p)]
    if pending_future:
        lines.append("Next Up")
        lines.append("=" * 50)
        display_future = pending_future if max_items == 0 else pending_future[:max_items]
        for phase in display_future:
            lines.extend(_format_phase_plain(phase, brief=True))
        remaining = len(pending_future) - len(display_future)
        if remaining > 0:
            lines.append(f"  ... and {remaining} more phases")
        lines.append("")

    # Recently Completed section - phases still in TODO but done (marked or 100%)
    all_phases = roadmap.in_progress + roadmap.future
    recently_completed = [p for p in all_phases if not is_truly_incomplete(p)]
    if recently_completed:
        lines.append("Recently Completed")
        lines.append("=" * 50)
        display_completed = recently_completed if max_items == 0 else recently_completed[:max_items]
        for phase in display_completed:
            phase_prefix = f"Phase {phase.id}: " if phase.id else ""
            lines.append(f"  ✓ {phase_prefix}{phase.title}")
        remaining = len(recently_completed) - len(display_completed)
        if remaining > 0:
            lines.append(f"  ... and {remaining} more")
        lines.append("")

    # Archived completed section (optional)
    if show_completed and roadmap.completed:
        lines.append("ARCHIVED")
        lines.append("=" * 50)
        for phase in roadmap.completed:
            lines.append(f"  ✓ Phase {phase.id}: {phase.title}")

    return "\n".join(lines)


def _get_status_icon(status: PhaseStatus) -> str:
    """Get status icon for a phase."""
    if status == PhaseStatus.COMPLETE:
        return "✓"
    elif status == PhaseStatus.IN_PROGRESS:
        return "→"
    return "○"


def _format_phase_plain(phase: Phase, brief: bool = False) -> list[str]:
    """Format a single phase as plain text."""
    lines: list[str] = []

    # Phase header with progress
    completed, total = phase.progress
    # Format phase header - hide "Phase X:" if ID is empty
    phase_prefix = f"Phase {phase.id}: " if phase.id else ""

    if total > 0:
        pct = phase.progress_percent
        status_icon = _get_status_icon(phase.status)
        progress_str = f"[{completed}/{total}] {pct:.0f}%"
        lines.append(f"  {status_icon} {phase_prefix}{phase.title} {progress_str}")
    else:
        status_icon = _get_status_icon(phase.status)
        lines.append(f"  {status_icon} {phase_prefix}{phase.title}")

    if brief:
        return lines

    # Description
    if phase.description:
        lines.append(f"    {phase.description[:70]}...")

    # Subphases
    for sub in phase.subphases:
        sub_completed, sub_total = sub.progress
        sub_icon = _get_status_icon(sub.status)
        if sub_total > 0:
            lines.append(f"      {sub_icon} {sub.id}: {sub.title} [{sub_completed}/{sub_total}]")
        else:
            lines.append(f"      {sub_icon} {sub.id}: {sub.title}")

    return lines


# =============================================================================
# TUI Display (Box Drawing)
# =============================================================================


# Box drawing characters
BOX_CHARS = {
    "tl": "┌",  # top-left
    "tr": "┐",  # top-right
    "bl": "└",  # bottom-left
    "br": "┘",  # bottom-right
    "h": "─",  # horizontal
    "v": "│",  # vertical
    "cross": "┼",
    "t_down": "┬",
    "t_up": "┴",
    "t_right": "├",
    "t_left": "┤",
}

# Progress bar characters
PROGRESS_CHARS = {
    "full": "█",
    "half": "▓",
    "quarter": "▒",
    "empty": "░",
}


def format_tui(
    roadmap: Roadmap,
    width: int = 80,
    show_completed: bool = False,
    use_color: bool = True,
    max_items: int = 0,
) -> str:
    """Format roadmap as TUI with box drawing.

    Args:
        roadmap: Parsed roadmap
        width: Terminal width
        show_completed: Include completed phases
        use_color: Use ANSI colors
        max_items: Max items per section (0 = unlimited)

    Returns:
        Formatted string with box drawing
    """
    lines: list[str] = []

    # Colors
    if use_color:
        RESET = "\033[0m"
        BOLD = "\033[1m"
        DIM = "\033[2m"
        GREEN = "\033[32m"
        YELLOW = "\033[33m"
        BLUE = "\033[34m"
        CYAN = "\033[36m"
    else:
        RESET = BOLD = DIM = GREEN = YELLOW = BLUE = CYAN = ""

    inner_width = width - 4  # Account for box borders

    def box_line(content: str, pad: str = " ") -> str:
        """Create a line inside a box."""
        content = content[:inner_width]
        padding = inner_width - len(_strip_ansi(content))
        return f"{BOX_CHARS['v']} {content}{pad * padding} {BOX_CHARS['v']}"

    def box_top() -> str:
        return BOX_CHARS["tl"] + BOX_CHARS["h"] * (width - 2) + BOX_CHARS["tr"]

    def box_bottom() -> str:
        return BOX_CHARS["bl"] + BOX_CHARS["h"] * (width - 2) + BOX_CHARS["br"]

    def box_separator() -> str:
        return BOX_CHARS["t_right"] + BOX_CHARS["h"] * (width - 2) + BOX_CHARS["t_left"]

    def progress_bar(percent: float, bar_width: int = 20) -> str:
        """Create a progress bar."""
        filled = int(percent / 100 * bar_width)
        empty = bar_width - filled
        bar = PROGRESS_CHARS["full"] * filled + PROGRESS_CHARS["empty"] * empty
        if use_color:
            if percent >= 100:
                return f"{GREEN}{bar}{RESET}"
            elif percent >= 50:
                return f"{YELLOW}{bar}{RESET}"
            else:
                return f"{BLUE}{bar}{RESET}"
        return bar

    # Title
    lines.append(box_top())
    title = f"{BOLD}Moss Roadmap{RESET}" if use_color else "Moss Roadmap"
    lines.append(box_line(title.center(inner_width + (len(title) - len(_strip_ansi(title))))))
    lines.append(box_separator())

    # In Progress section - show only incomplete phases (not marked complete and not 100%)
    active_phases = [p for p in roadmap.in_progress if is_truly_incomplete(p)]
    if active_phases:
        section_title = f"{CYAN}▶ In Progress{RESET}" if use_color else "▶ In Progress"
        lines.append(box_line(section_title))
        lines.append(box_line(""))

        for phase in active_phases:
            lines.extend(_format_phase_tui(phase, inner_width, use_color))
            lines.append(box_line(""))

        lines.append(box_separator())

    # Next Up section - show only incomplete future phases
    pending_future = [p for p in roadmap.future if is_truly_incomplete(p)]
    if pending_future:
        section_title = f"{BLUE}○ Next Up{RESET}" if use_color else "○ Next Up"
        lines.append(box_line(section_title))
        lines.append(box_line(""))

        display_future = pending_future if max_items == 0 else pending_future[:max_items]

        # Calculate max name length for alignment
        def phase_name(p: Phase) -> str:
            prefix = f"Phase {p.id}: " if p.id else ""
            return f"{prefix}{p.title}"

        max_name_len = max(len(phase_name(p)) for p in display_future)
        bar_width = 10  # Mini progress bar width

        for phase in display_future:
            name = phase_name(phase)
            padded_name = name.ljust(max_name_len)
            completed, total = phase.progress

            if total > 0:
                pct = int(100 * completed / total)
                filled = int(bar_width * completed / total)
                empty = bar_width - filled
                bar = PROGRESS_CHARS["full"] * filled + PROGRESS_CHARS["empty"] * empty
                progress_str = f"{bar} [{completed}/{total}] {pct}%"
                if use_color:
                    progress_str = f"{DIM}{progress_str}{RESET}"
                phase_line = f"  ○ {padded_name} {progress_str}"
            else:
                phase_line = f"  ○ {padded_name}"
            lines.append(box_line(phase_line))

        remaining = len(pending_future) - len(display_future)
        if remaining > 0:
            plain_more = f"... and {remaining} more"
            more_text = f"{DIM}{plain_more}{RESET}" if use_color else plain_more
            lines.append(box_line(f"    {more_text}"))

        lines.append(box_separator())

    # Recently Completed section
    all_phases = roadmap.in_progress + roadmap.future
    recently_completed = [p for p in all_phases if not is_truly_incomplete(p)]
    if recently_completed:
        completed_text = "✓ Recently Completed"
        section_title = f"{GREEN}{completed_text}{RESET}" if use_color else completed_text
        lines.append(box_line(section_title))
        lines.append(box_line(""))

        display_completed = recently_completed if max_items == 0 else recently_completed[:max_items]
        for phase in display_completed:
            check = f"{GREEN}✓{RESET}" if use_color else "✓"
            phase_prefix = f"Phase {phase.id}: " if phase.id else ""
            lines.append(box_line(f"  {check} {phase_prefix}{phase.title}"))

        remaining = len(recently_completed) - len(display_completed)
        if remaining > 0:
            plain_more = f"... and {remaining} more"
            more_text = f"{DIM}{plain_more}{RESET}" if use_color else plain_more
            lines.append(box_line(f"    {more_text}"))

        lines.append(box_separator())

    # Summary stats
    total_phases = len(roadmap.in_progress) + len(roadmap.future)
    complete_count = len(recently_completed)

    # Calculate overall progress
    all_tasks_completed = 0
    all_tasks_total = 0
    for phase in roadmap.in_progress + roadmap.future:
        c, t = phase.progress
        all_tasks_completed += c
        all_tasks_total += t

    overall_pct = (all_tasks_completed / all_tasks_total * 100) if all_tasks_total > 0 else 0

    stats_line = f"Phases: {complete_count}/{total_phases} complete"
    tasks_line = f"Tasks:  {all_tasks_completed}/{all_tasks_total}"
    bar = progress_bar(overall_pct, 30)

    lines.append(box_line(f"{stats_line}"))
    lines.append(box_line(f"{tasks_line} {bar} {overall_pct:.0f}%"))

    lines.append(box_bottom())

    return "\n".join(lines)


def _format_phase_tui(phase: Phase, width: int, use_color: bool) -> list[str]:
    """Format a phase for TUI display."""
    lines: list[str] = []

    if use_color:
        RESET = "\033[0m"
        BOLD = "\033[1m"
        GREEN = "\033[32m"
        YELLOW = "\033[33m"
    else:
        RESET = BOLD = GREEN = YELLOW = ""

    # Phase header
    completed, total = phase.progress
    pct = phase.progress_percent

    if phase.status == PhaseStatus.COMPLETE:
        icon = f"{GREEN}✓{RESET}" if use_color else "✓"
    elif phase.status == PhaseStatus.IN_PROGRESS:
        icon = f"{YELLOW}→{RESET}" if use_color else "→"
    else:
        icon = "○"

    # Format phase header - hide "Phase X:" if ID is empty
    phase_prefix = f"Phase {phase.id}: " if phase.id else ""
    phase_text = f"{phase_prefix}{phase.title}"
    title = f"{BOLD}{phase_text}{RESET}" if use_color else phase_text

    # Progress bar
    bar_width = 15
    filled = int(pct / 100 * bar_width)
    empty = bar_width - filled
    bar = PROGRESS_CHARS["full"] * filled + PROGRESS_CHARS["empty"] * empty

    if use_color:
        if pct >= 100:
            bar = f"{GREEN}{bar}{RESET}"
        elif pct >= 50:
            bar = f"{YELLOW}{bar}{RESET}"

    progress_str = f"[{completed}/{total}]" if total > 0 else ""
    lines.append(_box_content(f"  {icon} {title} {progress_str} {bar} {pct:.0f}%", width))

    # Subphases (compact)
    for sub in phase.subphases:
        sub_completed, sub_total = sub.progress
        if sub.status == PhaseStatus.COMPLETE:
            sub_icon = f"{GREEN}✓{RESET}" if use_color else "✓"
        elif sub.status == PhaseStatus.IN_PROGRESS:
            sub_icon = f"{YELLOW}→{RESET}" if use_color else "→"
        else:
            sub_icon = "○"

        sub_progress = f"[{sub_completed}/{sub_total}]" if sub_total > 0 else ""
        lines.append(_box_content(f"      {sub_icon} {sub.id}: {sub.title} {sub_progress}", width))

    return lines


def _box_content(content: str, width: int) -> str:
    """Create content for inside a box."""
    visible_len = len(_strip_ansi(content))
    padding = width - visible_len
    if padding < 0:
        # Truncate if too long
        content = content[: width - 3] + "..."
        padding = 0
    return f"{BOX_CHARS['v']} {content}{' ' * padding} {BOX_CHARS['v']}"


def _strip_ansi(text: str) -> str:
    """Remove ANSI escape codes from text."""
    return re.sub(r"\033\[[0-9;]*m", "", text)


# =============================================================================
# CLI Integration
# =============================================================================


def find_todo_md(start: Path | None = None) -> Path | None:
    """Find TODO.md file, searching up from start directory.

    Args:
        start: Starting directory (defaults to cwd)

    Returns:
        Path to TODO.md or None if not found
    """
    if start is None:
        start = Path.cwd()

    current = start.resolve()
    while current != current.parent:
        todo_path = current / "TODO.md"
        if todo_path.exists():
            return todo_path
        current = current.parent

    return None


def display_roadmap(
    path: Path | None = None,
    tui: bool = False,
    show_completed: bool = False,
    use_color: bool = True,
    width: int = 80,
    output: TextIO | None = None,
    max_items: int = 0,
) -> int:
    """Display the roadmap.

    Args:
        path: Path to TODO.md (auto-detected if None)
        tui: Use TUI display mode
        show_completed: Show completed phases
        use_color: Use ANSI colors
        width: Terminal width for TUI mode
        output: Output stream (defaults to stdout)
        max_items: Max items per section (0 = unlimited)

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    import sys

    if output is None:
        output = sys.stdout

    # Find TODO.md
    if path is None:
        path = find_todo_md()

    if path is None or not path.exists():
        print("ERROR: TODO.md not found", file=sys.stderr)
        return 1

    # Parse
    roadmap = parse_todo_md(path)

    # Format and display
    if tui:
        formatted = format_tui(
            roadmap,
            width=width,
            show_completed=show_completed,
            use_color=use_color,
            max_items=max_items,
        )
    else:
        formatted = format_plain(roadmap, show_completed=show_completed, max_items=max_items)

    print(formatted, file=output)
    return 0


__all__ = [
    "Phase",
    "PhaseStatus",
    "Roadmap",
    "TaskItem",
    "display_roadmap",
    "find_todo_md",
    "format_plain",
    "format_tui",
    "parse_todo_md",
]
