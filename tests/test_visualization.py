"""Tests for CFG visualization module."""

from pathlib import Path

import pytest

from moss_intelligence.cfg import CFGBuilder, ControlFlowGraph
from moss_intelligence.visualization import (
    CFGVisualizer,
    is_graphviz_available,
    render_mermaid_to_html,
    visualize_cfgs,
)


@pytest.fixture
def simple_source() -> str:
    return """
def simple():
    x = 1
    return x
"""


@pytest.fixture
def branching_source() -> str:
    return """
def branching(x):
    if x > 0:
        return "positive"
    else:
        return "non-positive"
"""


@pytest.fixture
def loop_source() -> str:
    return """
def with_loop(n):
    total = 0
    for i in range(n):
        total += i
    return total
"""


@pytest.fixture
def simple_cfg(simple_source: str) -> ControlFlowGraph:
    builder = CFGBuilder()
    cfgs = builder.build_from_source(simple_source, "simple")
    return cfgs[0]


@pytest.fixture
def branching_cfg(branching_source: str) -> ControlFlowGraph:
    builder = CFGBuilder()
    cfgs = builder.build_from_source(branching_source, "branching")
    return cfgs[0]


class TestMermaidOutput:
    """Tests for Mermaid diagram generation."""

    def test_to_mermaid_simple(self, simple_cfg: ControlFlowGraph):
        mermaid = simple_cfg.to_mermaid()

        assert "flowchart TD" in mermaid
        assert "ENTRY" in mermaid or "N" in mermaid

    def test_to_mermaid_branching(self, branching_cfg: ControlFlowGraph):
        mermaid = branching_cfg.to_mermaid()

        assert "flowchart TD" in mermaid
        # Should have conditional edges
        assert "|True|" in mermaid or "|False|" in mermaid

    def test_mermaid_escapes_quotes(self, simple_source: str):
        # Source with quotes
        source = """
def with_quotes():
    x = "hello"
    return x
"""
        builder = CFGBuilder()
        cfgs = builder.build_from_source(source, "with_quotes")
        cfg = cfgs[0]
        mermaid = cfg.to_mermaid()

        # Should not have unescaped double quotes in labels
        assert '""' not in mermaid


class TestRenderMermaidToHtml:
    """Tests for Mermaid to HTML rendering."""

    def test_basic_html(self):
        mermaid = "flowchart TD\n    A --> B"
        html = render_mermaid_to_html(mermaid, title="Test")

        assert "<!DOCTYPE html>" in html
        assert "<title>Test</title>" in html
        assert "mermaid" in html
        assert "A --> B" in html

    def test_html_with_info(self):
        mermaid = "flowchart TD\n    A --> B"
        html = render_mermaid_to_html(
            mermaid,
            title="Test",
            include_info=True,
            node_count=2,
            edge_count=1,
            complexity=1,
        )

        assert "Nodes: 2" in html
        assert "Edges: 1" in html
        assert "Cyclomatic Complexity: 1" in html

    def test_html_without_info(self):
        mermaid = "flowchart TD\n    A --> B"
        html = render_mermaid_to_html(mermaid, include_info=False)

        assert "Graph Information" not in html


class TestCFGVisualizer:
    """Tests for CFGVisualizer."""

    @pytest.fixture
    def visualizer(self) -> CFGVisualizer:
        return CFGVisualizer()

    def test_render_html_mermaid(self, visualizer: CFGVisualizer, simple_cfg: ControlFlowGraph):
        html = visualizer.render_html(simple_cfg, format="mermaid")

        assert "<!DOCTYPE html>" in html
        assert "flowchart TD" in html

    @pytest.mark.skipif(not is_graphviz_available(), reason="Graphviz not installed")
    def test_render_svg(self, visualizer: CFGVisualizer, simple_cfg: ControlFlowGraph):
        svg = visualizer.render_svg(simple_cfg)

        assert "<svg" in svg
        assert "</svg>" in svg

    @pytest.mark.skipif(not is_graphviz_available(), reason="Graphviz not installed")
    def test_render_png(self, visualizer: CFGVisualizer, simple_cfg: ControlFlowGraph):
        png = visualizer.render_png(simple_cfg)

        # PNG magic bytes
        assert png[:8] == b"\x89PNG\r\n\x1a\n"

    def test_save_mermaid(
        self, visualizer: CFGVisualizer, simple_cfg: ControlFlowGraph, tmp_path: Path
    ):
        output = tmp_path / "cfg.mermaid"
        visualizer.save(simple_cfg, output)

        content = output.read_text()
        assert "flowchart TD" in content

    def test_save_dot(
        self, visualizer: CFGVisualizer, simple_cfg: ControlFlowGraph, tmp_path: Path
    ):
        output = tmp_path / "cfg.dot"
        visualizer.save(simple_cfg, output)

        content = output.read_text()
        assert "digraph" in content

    def test_save_html(
        self, visualizer: CFGVisualizer, simple_cfg: ControlFlowGraph, tmp_path: Path
    ):
        output = tmp_path / "cfg.html"
        visualizer.save(simple_cfg, output)

        content = output.read_text()
        assert "<!DOCTYPE html>" in content

    def test_save_markdown(
        self, visualizer: CFGVisualizer, simple_cfg: ControlFlowGraph, tmp_path: Path
    ):
        output = tmp_path / "cfg.md"
        visualizer.save(simple_cfg, output)

        content = output.read_text()
        assert "```mermaid" in content
        assert "flowchart TD" in content


class TestVisualizeCfgs:
    """Tests for visualize_cfgs helper."""

    def test_visualize_multiple_cfgs(self, simple_source: str, branching_source: str):
        builder = CFGBuilder()
        cfg1 = builder.build_from_source(simple_source, "simple")[0]
        cfg2 = builder.build_from_source(branching_source, "branching")[0]

        content = visualize_cfgs([cfg1, cfg2], format="mermaid")

        assert "flowchart TD" in content

    def test_visualize_to_html(self, simple_source: str):
        builder = CFGBuilder()
        cfg = builder.build_from_source(simple_source, "simple")[0]

        html = visualize_cfgs([cfg], format="html")

        assert "<!DOCTYPE html>" in html

    def test_visualize_empty_list(self):
        content = visualize_cfgs([], format="mermaid")
        assert content == ""

    def test_visualize_to_file(self, simple_source: str, tmp_path: Path):
        builder = CFGBuilder()
        cfg = builder.build_from_source(simple_source, "simple")[0]
        output = tmp_path / "output.mermaid"

        visualize_cfgs([cfg], output_path=output, format="mermaid")

        assert output.exists()
        assert "flowchart TD" in output.read_text()


class TestIsGraphvizAvailable:
    """Tests for graphviz availability check."""

    def test_returns_bool(self):
        result = is_graphviz_available()
        assert isinstance(result, bool)
