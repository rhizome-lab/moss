"""Markdown structure extraction plugin.

Extracts structural information from Markdown files:
- Heading hierarchy
- Code blocks with language tags
- Links (internal and external)
- Front matter (YAML)
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from moss_intelligence.views import View, ViewOptions, ViewTarget

    from moss_orchestration.plugins import PluginMetadata


@dataclass
class MarkdownHeading:
    """A heading in a Markdown document."""

    level: int
    text: str
    line: int


@dataclass
class MarkdownCodeBlock:
    """A fenced code block."""

    language: str | None
    content: str
    line_start: int
    line_end: int


@dataclass
class MarkdownLink:
    """A link in the document."""

    text: str
    url: str
    line: int
    is_internal: bool  # True for relative/anchor links


@dataclass
class MarkdownStructure:
    """Extracted structure from a Markdown file."""

    headings: list[MarkdownHeading] = field(default_factory=list)
    code_blocks: list[MarkdownCodeBlock] = field(default_factory=list)
    links: list[MarkdownLink] = field(default_factory=list)
    front_matter: dict | None = None


def extract_markdown_structure(source: str) -> MarkdownStructure:
    """Extract structural information from Markdown source.

    Args:
        source: Markdown source text

    Returns:
        MarkdownStructure with headings, code blocks, and links
    """
    lines = source.splitlines()
    structure = MarkdownStructure()

    # Check for YAML front matter
    if lines and lines[0] == "---":
        end_idx = None
        for i, line in enumerate(lines[1:], 1):
            if line == "---":
                end_idx = i
                break
        if end_idx:
            try:
                import yaml

                front_matter_text = "\n".join(lines[1:end_idx])
                structure.front_matter = yaml.safe_load(front_matter_text)
            except (ImportError, Exception):
                # YAML not available or parse error
                structure.front_matter = {"_raw": "\n".join(lines[1:end_idx])}

    # Extract headings (ATX style: # Heading)
    heading_pattern = re.compile(r"^(#{1,6})\s+(.+)$")
    in_code_block = False
    code_block_start = 0
    code_block_lang = None
    code_block_lines: list[str] = []

    for i, line in enumerate(lines):
        line_num = i + 1

        # Handle fenced code blocks
        if line.startswith("```"):
            if not in_code_block:
                in_code_block = True
                code_block_start = line_num
                code_block_lang = line[3:].strip() or None
                code_block_lines = []
            else:
                structure.code_blocks.append(
                    MarkdownCodeBlock(
                        language=code_block_lang,
                        content="\n".join(code_block_lines),
                        line_start=code_block_start,
                        line_end=line_num,
                    )
                )
                in_code_block = False
            continue

        if in_code_block:
            code_block_lines.append(line)
            continue

        # Extract headings
        heading_match = heading_pattern.match(line)
        if heading_match:
            level = len(heading_match.group(1))
            text = heading_match.group(2).strip()
            structure.headings.append(MarkdownHeading(level=level, text=text, line=line_num))

        # Extract links [text](url)
        link_pattern = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
        for match in link_pattern.finditer(line):
            url = match.group(2)
            is_internal = not url.startswith(("http://", "https://", "//"))
            structure.links.append(
                MarkdownLink(
                    text=match.group(1),
                    url=url,
                    line=line_num,
                    is_internal=is_internal,
                )
            )

    return structure


def format_markdown_structure(structure: MarkdownStructure) -> str:
    """Format extracted structure as readable text.

    Args:
        structure: Extracted Markdown structure

    Returns:
        Formatted string representation
    """
    lines = []

    # Front matter
    if structure.front_matter:
        lines.append("Front Matter:")
        if "_raw" in structure.front_matter:
            lines.append("  (unparsed YAML)")
        else:
            for key, value in structure.front_matter.items():
                lines.append(f"  {key}: {value}")
        lines.append("")

    # Table of contents from headings
    if structure.headings:
        lines.append("Structure:")
        for heading in structure.headings:
            indent = "  " * (heading.level - 1)
            lines.append(f"{indent}[L{heading.line}] {heading.text}")
        lines.append("")

    # Code blocks summary
    if structure.code_blocks:
        lines.append("Code Blocks:")
        for block in structure.code_blocks:
            lang = block.language or "plain"
            line_count = block.line_end - block.line_start - 1
            lines.append(f"  [{lang}] L{block.line_start}-{block.line_end} ({line_count} lines)")
        lines.append("")

    # Links
    if structure.links:
        internal = [lnk for lnk in structure.links if lnk.is_internal]
        external = [lnk for lnk in structure.links if not lnk.is_internal]

        if internal:
            lines.append("Internal Links:")
            for link in internal:
                lines.append(f"  L{link.line}: [{link.text}]({link.url})")
            lines.append("")

        if external:
            lines.append("External Links:")
            for link in external:
                lines.append(f"  L{link.line}: [{link.text}]({link.url})")
            lines.append("")

    return "\n".join(lines).strip()


class MarkdownStructurePlugin:
    """Plugin for extracting structure from Markdown files."""

    @property
    def metadata(self) -> PluginMetadata:
        from moss_orchestration.plugins import PluginMetadata

        return PluginMetadata(
            name="markdown-structure",
            view_type="skeleton",  # Reuse skeleton view type for structure
            languages=frozenset(["markdown"]),
            priority=5,
            version="0.1.0",
            description="Markdown structure extraction (headings, code blocks, links)",
        )

    def supports(self, target: ViewTarget) -> bool:
        """Check if this plugin can handle the target."""
        from moss_orchestration.plugins import detect_language

        if not target.path.exists():
            return False
        return detect_language(target.path) == "markdown"

    async def render(
        self,
        target: ViewTarget,
        options: ViewOptions | None = None,
    ) -> View:
        """Render a structure view for a Markdown file."""
        from moss_intelligence.views import View, ViewType

        source = target.path.read_text()
        structure = extract_markdown_structure(source)
        content = format_markdown_structure(structure)

        return View(
            target=target,
            view_type=ViewType.SKELETON,
            content=content,
            metadata={
                "heading_count": len(structure.headings),
                "code_block_count": len(structure.code_blocks),
                "link_count": len(structure.links),
                "has_front_matter": structure.front_matter is not None,
                "headings": [
                    {"level": h.level, "text": h.text, "line": h.line} for h in structure.headings
                ],
                "code_blocks": [
                    {
                        "language": b.language,
                        "line_start": b.line_start,
                        "line_end": b.line_end,
                    }
                    for b in structure.code_blocks
                ],
                "links": [
                    {
                        "text": lnk.text,
                        "url": lnk.url,
                        "line": lnk.line,
                        "is_internal": lnk.is_internal,
                    }
                    for lnk in structure.links
                ],
                "language": "markdown",
            },
        )
