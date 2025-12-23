"""Visualization utilities for CFG and other graphs.

This module provides:
- render_dot_to_svg: Convert DOT to SVG using graphviz
- render_mermaid_to_html: Create HTML with embedded Mermaid diagram
- CFGVisualizer: High-level visualization interface

Usage:
    from moss_intelligence.cfg import CFGBuilder
    from moss_orchestration.visualization import CFGVisualizer

    builder = CFGBuilder()
    cfg = builder.build_from_source(source, "my_function")

    viz = CFGVisualizer()
    svg = viz.render_svg(cfg)
    html = viz.render_html(cfg, format="mermaid")
"""

from __future__ import annotations

import subprocess
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from moss_intelligence.cfg import ControlFlowGraph


# =============================================================================
# DOT/Graphviz Rendering
# =============================================================================


def render_dot_to_svg(dot: str) -> str:
    """Render DOT format to SVG using graphviz.

    Args:
        dot: DOT format graph

    Returns:
        SVG string

    Raises:
        RuntimeError: If graphviz is not installed
    """
    try:
        result = subprocess.run(
            ["dot", "-Tsvg"],
            input=dot,
            capture_output=True,
            text=True,
            check=True,
            timeout=30,
        )
        return result.stdout
    except FileNotFoundError as e:
        raise RuntimeError(
            "Graphviz not installed. Install with: apt install graphviz (Linux), "
            "brew install graphviz (macOS), or choco install graphviz (Windows)"
        ) from e
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"Graphviz error: {e.stderr}") from e
    except subprocess.TimeoutExpired as e:
        raise RuntimeError("Graphviz timed out") from e


def render_dot_to_png(dot: str) -> bytes:
    """Render DOT format to PNG using graphviz.

    Args:
        dot: DOT format graph

    Returns:
        PNG bytes

    Raises:
        RuntimeError: If graphviz is not installed
    """
    try:
        result = subprocess.run(
            ["dot", "-Tpng"],
            input=dot.encode(),
            capture_output=True,
            check=True,
            timeout=30,
        )
        return result.stdout
    except FileNotFoundError as e:
        raise RuntimeError("Graphviz not installed") from e
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"Graphviz error: {e.stderr.decode()}") from e
    except subprocess.TimeoutExpired as e:
        raise RuntimeError("Graphviz timed out") from e


def is_graphviz_available() -> bool:
    """Check if graphviz is available."""
    try:
        subprocess.run(
            ["dot", "-V"],
            capture_output=True,
            check=True,
            timeout=5,
        )
        return True
    except (FileNotFoundError, subprocess.CalledProcessError, subprocess.TimeoutExpired):
        return False


# =============================================================================
# Mermaid Rendering
# =============================================================================


MERMAID_HTML_TEMPLATE = """<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>{title}</title>
    <script src="https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js"></script>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
            background: #f5f5f5;
        }}
        h1 {{
            color: #333;
        }}
        .mermaid {{
            background: white;
            padding: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .info {{
            margin-top: 20px;
            padding: 15px;
            background: white;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .info h3 {{
            margin-top: 0;
        }}
        pre {{
            background: #f0f0f0;
            padding: 10px;
            border-radius: 4px;
            overflow-x: auto;
        }}
    </style>
</head>
<body>
    <h1>{title}</h1>
    <div class="mermaid">
{diagram}
    </div>
    {info_section}
    <script>
        mermaid.initialize({{ startOnLoad: true, theme: 'default' }});
    </script>
</body>
</html>
"""

INFO_SECTION_TEMPLATE = """
    <div class="info">
        <h3>Graph Information</h3>
        <ul>
            <li>Nodes: {node_count}</li>
            <li>Edges: {edge_count}</li>
            <li>Cyclomatic Complexity: {complexity}</li>
        </ul>
        <h3>Source (Mermaid)</h3>
        <pre>{source}</pre>
    </div>
"""


def render_mermaid_to_html(
    mermaid: str,
    title: str = "Control Flow Graph",
    include_info: bool = True,
    node_count: int = 0,
    edge_count: int = 0,
    complexity: int = 0,
) -> str:
    """Render Mermaid diagram to standalone HTML.

    Args:
        mermaid: Mermaid diagram code
        title: HTML page title
        include_info: Include graph info section
        node_count: Number of nodes (for info)
        edge_count: Number of edges (for info)
        complexity: Cyclomatic complexity (for info)

    Returns:
        HTML string with embedded Mermaid diagram
    """
    info_section = ""
    if include_info:
        # Escape HTML in source
        escaped_source = mermaid.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        info_section = INFO_SECTION_TEMPLATE.format(
            node_count=node_count,
            edge_count=edge_count,
            complexity=complexity,
            source=escaped_source,
        )

    return MERMAID_HTML_TEMPLATE.format(
        title=title,
        diagram=mermaid,
        info_section=info_section,
    )


# =============================================================================
# CFG Visualizer
# =============================================================================


class CFGVisualizer:
    """High-level interface for CFG visualization."""

    def render_svg(self, cfg: ControlFlowGraph) -> str:
        """Render CFG to SVG using graphviz.

        Args:
            cfg: Control flow graph

        Returns:
            SVG string

        Raises:
            RuntimeError: If graphviz is not available
        """
        dot = cfg.to_dot()
        return render_dot_to_svg(dot)

    def render_png(self, cfg: ControlFlowGraph) -> bytes:
        """Render CFG to PNG using graphviz.

        Args:
            cfg: Control flow graph

        Returns:
            PNG bytes

        Raises:
            RuntimeError: If graphviz is not available
        """
        dot = cfg.to_dot()
        return render_dot_to_png(dot)

    def render_html(
        self,
        cfg: ControlFlowGraph,
        format: str = "mermaid",
        include_info: bool = True,
    ) -> str:
        """Render CFG to HTML.

        Args:
            cfg: Control flow graph
            format: "mermaid" or "svg" (requires graphviz)
            include_info: Include graph information

        Returns:
            HTML string
        """
        if format == "svg":
            svg = self.render_svg(cfg)
            return self._svg_to_html(svg, cfg.name, include_info, cfg)
        else:
            mermaid = cfg.to_mermaid()
            return render_mermaid_to_html(
                mermaid,
                title=f"CFG: {cfg.name}",
                include_info=include_info,
                node_count=cfg.node_count,
                edge_count=cfg.edge_count,
                complexity=cfg.cyclomatic_complexity,
            )

    def _svg_to_html(
        self,
        svg: str,
        title: str,
        include_info: bool,
        cfg: ControlFlowGraph,
    ) -> str:
        """Wrap SVG in HTML."""
        info = ""
        if include_info:
            info = f"""
    <div class="info">
        <h3>Graph Information</h3>
        <ul>
            <li>Nodes: {cfg.node_count}</li>
            <li>Edges: {cfg.edge_count}</li>
            <li>Cyclomatic Complexity: {cfg.cyclomatic_complexity}</li>
        </ul>
    </div>
"""

        return f"""<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>CFG: {title}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
            background: #f5f5f5;
        }}
        h1 {{ color: #333; }}
        .graph {{
            background: white;
            padding: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
            overflow-x: auto;
        }}
        .info {{
            margin-top: 20px;
            padding: 15px;
            background: white;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .info h3 {{ margin-top: 0; }}
    </style>
</head>
<body>
    <h1>CFG: {title}</h1>
    <div class="graph">
        {svg}
    </div>
    {info}
</body>
</html>
"""

    def save(
        self,
        cfg: ControlFlowGraph,
        path: Path | str,
        format: str | None = None,
    ) -> None:
        """Save CFG visualization to file.

        Args:
            cfg: Control flow graph
            path: Output file path
            format: Output format (auto-detected from extension if None)
                   Supported: svg, png, html, dot, mermaid, md
        """
        path = Path(path)
        if format is None:
            format = path.suffix.lstrip(".")

        if format == "svg":
            content = self.render_svg(cfg)
            path.write_text(content)
        elif format == "png":
            content = self.render_png(cfg)
            path.write_bytes(content)
        elif format == "html":
            content = self.render_html(cfg, format="mermaid")
            path.write_text(content)
        elif format == "dot":
            content = cfg.to_dot()
            path.write_text(content)
        elif format in ("mermaid", "md"):
            content = cfg.to_mermaid()
            if format == "md":
                content = f"```mermaid\n{content}\n```"
            path.write_text(content)
        else:
            raise ValueError(f"Unsupported format: {format}")


# =============================================================================
# CLI Integration Helper
# =============================================================================


def visualize_cfgs(
    cfgs: list[ControlFlowGraph],
    output_path: Path | str | None = None,
    format: str = "mermaid",
    open_browser: bool = False,
) -> str:
    """Visualize one or more CFGs.

    Args:
        cfgs: List of control flow graphs
        output_path: Optional output file path
        format: Output format (mermaid, svg, dot, html)
        open_browser: Open HTML output in browser

    Returns:
        Visualization content
    """
    if not cfgs:
        return ""

    viz = CFGVisualizer()

    if format == "html":
        # Combine all CFGs into single HTML
        sections = []
        for cfg in cfgs:
            mermaid = cfg.to_mermaid()
            complexity = cfg.cyclomatic_complexity
            info = f"""
        <div class="cfg-section">
            <h2>{cfg.name}</h2>
            <p>Nodes: {cfg.node_count}, Edges: {cfg.edge_count}, Complexity: {complexity}</p>
            <div class="mermaid">
{mermaid}
            </div>
        </div>
"""
            sections.append(info)

        content = MERMAID_HTML_TEMPLATE.format(
            title="Control Flow Graphs",
            diagram="",
            info_section="\n".join(sections),
        ).replace(
            '<div class="mermaid">\n\n    </div>',
            "",
        )

    elif format == "mermaid":
        content = "\n\n".join(cfg.to_mermaid() for cfg in cfgs)

    elif format == "dot":
        content = "\n\n".join(cfg.to_dot() for cfg in cfgs)

    elif format == "svg":
        if len(cfgs) == 1:
            content = viz.render_svg(cfgs[0])
        else:
            # Multiple SVGs joined
            content = "\n\n".join(viz.render_svg(cfg) for cfg in cfgs)

    else:
        raise ValueError(f"Unsupported format: {format}")

    if output_path:
        Path(output_path).write_text(content)

        if open_browser and format == "html":
            import webbrowser

            webbrowser.open(f"file://{Path(output_path).resolve()}")

    return content
