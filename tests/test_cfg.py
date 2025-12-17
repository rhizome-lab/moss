"""Tests for Control Flow Graph module."""

import pytest

from moss.cfg import (
    CFGBuilder,
    CFGEdge,
    CFGNode,
    CFGViewProvider,
    ControlFlowGraph,
    EdgeType,
    NodeType,
    build_cfg,
)


class TestCFGNode:
    """Tests for CFGNode."""

    def test_create_node(self):
        node = CFGNode(
            id="N1",
            node_type=NodeType.BASIC,
            statements=["x = 1"],
            line_start=5,
        )

        assert node.id == "N1"
        assert node.node_type == NodeType.BASIC
        assert node.statements == ["x = 1"]
        assert node.line_start == 5

    def test_node_equality(self):
        node1 = CFGNode(id="N1", node_type=NodeType.BASIC)
        node2 = CFGNode(id="N1", node_type=NodeType.ENTRY)  # Different type, same ID

        assert node1 == node2  # Equality by ID

    def test_node_hash(self):
        node1 = CFGNode(id="N1", node_type=NodeType.BASIC)
        node2 = CFGNode(id="N1", node_type=NodeType.BASIC)

        assert hash(node1) == hash(node2)
        assert len({node1, node2}) == 1  # Same in set


class TestCFGEdge:
    """Tests for CFGEdge."""

    def test_create_edge(self):
        edge = CFGEdge(
            source="N1",
            target="N2",
            edge_type=EdgeType.CONDITIONAL_TRUE,
            condition="x > 0",
        )

        assert edge.source == "N1"
        assert edge.target == "N2"
        assert edge.edge_type == EdgeType.CONDITIONAL_TRUE
        assert edge.condition == "x > 0"


class TestControlFlowGraph:
    """Tests for ControlFlowGraph."""

    @pytest.fixture
    def simple_cfg(self) -> ControlFlowGraph:
        cfg = ControlFlowGraph(name="test")
        cfg.add_node(CFGNode(id="ENTRY", node_type=NodeType.ENTRY))
        cfg.add_node(CFGNode(id="N1", node_type=NodeType.BASIC))
        cfg.add_node(CFGNode(id="EXIT", node_type=NodeType.EXIT))
        cfg.add_edge("ENTRY", "N1")
        cfg.add_edge("N1", "EXIT")
        cfg.entry_node = "ENTRY"
        cfg.exit_node = "EXIT"
        return cfg

    def test_add_node(self):
        cfg = ControlFlowGraph(name="test")
        node = CFGNode(id="N1", node_type=NodeType.BASIC)
        cfg.add_node(node)

        assert "N1" in cfg.nodes
        assert cfg.nodes["N1"] == node

    def test_add_edge(self):
        cfg = ControlFlowGraph(name="test")
        cfg.add_edge("N1", "N2", EdgeType.CONDITIONAL_TRUE)

        assert len(cfg.edges) == 1
        assert cfg.edges[0].source == "N1"
        assert cfg.edges[0].target == "N2"

    def test_get_successors(self, simple_cfg: ControlFlowGraph):
        succs = simple_cfg.get_successors("ENTRY")
        assert succs == ["N1"]

    def test_get_predecessors(self, simple_cfg: ControlFlowGraph):
        preds = simple_cfg.get_predecessors("EXIT")
        assert preds == ["N1"]

    def test_node_count(self, simple_cfg: ControlFlowGraph):
        assert simple_cfg.node_count == 3

    def test_edge_count(self, simple_cfg: ControlFlowGraph):
        assert simple_cfg.edge_count == 2

    def test_to_dot(self, simple_cfg: ControlFlowGraph):
        dot = simple_cfg.to_dot()

        assert 'digraph "test"' in dot
        assert "ENTRY" in dot
        assert "EXIT" in dot
        assert "->" in dot

    def test_to_text(self, simple_cfg: ControlFlowGraph):
        text = simple_cfg.to_text()

        assert "CFG for test:" in text
        assert "[ENTRY]" in text
        assert "[EXIT]" in text


class TestCFGBuilder:
    """Tests for CFGBuilder."""

    def test_simple_function(self):
        source = """
def foo():
    x = 1
    y = 2
    return x + y
"""
        builder = CFGBuilder()
        cfgs = builder.build_from_source(source)

        assert len(cfgs) == 1
        cfg = cfgs[0]
        assert cfg.name == "foo"
        assert cfg.entry_node == "ENTRY"
        assert cfg.exit_node == "EXIT"

    def test_if_statement(self):
        source = """
def foo(x):
    if x > 0:
        return 1
    else:
        return -1
"""
        builder = CFGBuilder()
        cfgs = builder.build_from_source(source)

        cfg = cfgs[0]
        # Should have branch node
        branch_nodes = [n for n in cfg.nodes.values() if n.node_type == NodeType.BRANCH]
        assert len(branch_nodes) == 1

    def test_while_loop(self):
        source = """
def foo():
    x = 0
    while x < 10:
        x += 1
    return x
"""
        builder = CFGBuilder()
        cfgs = builder.build_from_source(source)

        cfg = cfgs[0]
        # Should have loop header
        loop_nodes = [n for n in cfg.nodes.values() if n.node_type == NodeType.LOOP_HEADER]
        assert len(loop_nodes) == 1
        # Should have back edge
        back_edges = [e for e in cfg.edges if e.edge_type == EdgeType.LOOP_BACK]
        assert len(back_edges) >= 1

    def test_try_except(self):
        source = """
def foo():
    try:
        risky()
    except ValueError:
        handle_error()
"""
        builder = CFGBuilder()
        cfgs = builder.build_from_source(source)

        cfg = cfgs[0]
        # Should have exception handler
        handler_nodes = [n for n in cfg.nodes.values() if n.node_type == NodeType.EXCEPTION_HANDLER]
        assert len(handler_nodes) == 1

    def test_multiple_functions(self):
        source = """
def foo():
    pass

def bar():
    pass

def baz():
    pass
"""
        builder = CFGBuilder()
        cfgs = builder.build_from_source(source)

        assert len(cfgs) == 3
        names = [c.name for c in cfgs]
        assert "foo" in names
        assert "bar" in names
        assert "baz" in names

    def test_specific_function(self):
        source = """
def foo():
    pass

def bar():
    pass
"""
        builder = CFGBuilder()
        cfgs = builder.build_from_source(source, function_name="bar")

        assert len(cfgs) == 1
        assert cfgs[0].name == "bar"

    def test_nested_if(self):
        source = """
def foo(x, y):
    if x > 0:
        if y > 0:
            return 1
        else:
            return 2
    return 0
"""
        builder = CFGBuilder()
        cfgs = builder.build_from_source(source)

        cfg = cfgs[0]
        branch_nodes = [n for n in cfg.nodes.values() if n.node_type == NodeType.BRANCH]
        assert len(branch_nodes) == 2


class TestBuildCFG:
    """Tests for build_cfg convenience function."""

    def test_build_cfg(self):
        source = """
def hello():
    print("Hello")
"""
        cfgs = build_cfg(source)

        assert len(cfgs) == 1
        assert cfgs[0].name == "hello"


class TestCFGViewProvider:
    """Tests for CFGViewProvider."""

    def test_provide_view(self, tmp_path):
        source = """
def example():
    x = 1
    return x
"""
        test_file = tmp_path / "test.py"
        test_file.write_text(source)

        provider = CFGViewProvider()
        cfgs = provider._builder.build_from_file(test_file)

        assert len(cfgs) == 1
        assert cfgs[0].name == "example"
        text = cfgs[0].to_text()
        assert "CFG for example:" in text

    def test_provide_dot(self, tmp_path):
        source = """
def example():
    x = 1
    return x
"""
        test_file = tmp_path / "test.py"
        test_file.write_text(source)

        provider = CFGViewProvider()
        cfgs = provider._builder.build_from_file(test_file)

        assert len(cfgs) == 1
        dot = cfgs[0].to_dot()

        assert "digraph" in dot
        assert "example" in dot
