"""Control Flow Graph (CFG) view provider.

This module provides:
- CFGNode: Represents a basic block in the control flow graph
- CFGEdge: Represents an edge between basic blocks
- ControlFlowGraph: The complete CFG for a function
- CFGBuilder: Builds CFGs from Python AST
- CFGViewProvider: View provider for generating CFG views

Usage:
    from moss.cfg import CFGBuilder, CFGViewProvider

    # Build CFG from source
    builder = CFGBuilder()
    cfgs = builder.build_from_source(source, "my_function")

    # Or use the view provider
    from pathlib import Path
    provider = CFGViewProvider()
    view = provider.provide(Path("myfile.py"))
"""

from __future__ import annotations

import ast
from dataclasses import dataclass, field
from enum import Enum, auto
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from moss.plugins import PluginMetadata
    from .views import View, ViewOptions, ViewTarget


class EdgeType(Enum):
    """Type of control flow edge."""

    SEQUENTIAL = auto()  # Normal sequential flow
    CONDITIONAL_TRUE = auto()  # True branch of if/while
    CONDITIONAL_FALSE = auto()  # False branch of if/while
    LOOP_BACK = auto()  # Back edge for loops
    EXCEPTION = auto()  # Exception handling
    BREAK = auto()  # Break statement
    CONTINUE = auto()  # Continue statement
    RETURN = auto()  # Return statement


class NodeType(Enum):
    """Type of CFG node (basic block)."""

    ENTRY = auto()  # Function entry point
    EXIT = auto()  # Function exit point
    BASIC = auto()  # Regular basic block
    BRANCH = auto()  # Conditional branch (if/while)
    LOOP_HEADER = auto()  # Loop header
    EXCEPTION_HANDLER = auto()  # Exception handler
    FINALLY = auto()  # Finally block


@dataclass
class CFGNode:
    """Represents a basic block in the control flow graph."""

    id: str
    node_type: NodeType
    statements: list[str] = field(default_factory=list)
    line_start: int | None = None
    line_end: int | None = None
    ast_nodes: list[ast.AST] = field(default_factory=list)

    def __hash__(self) -> int:
        return hash(self.id)

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, CFGNode):
            return NotImplemented
        return self.id == other.id

    @property
    def label(self) -> str:
        """Label for display (first statement or node type)."""
        if self.statements:
            return self.statements[0]
        return self.node_type.name

    @property
    def lineno(self) -> int | None:
        """Alias for line_start for API compatibility."""
        return self.line_start


@dataclass
class CFGEdge:
    """Represents an edge between basic blocks."""

    source: str  # Source node ID
    target: str  # Target node ID
    edge_type: EdgeType
    label: str | None = None
    condition: str | None = None  # For conditional edges


@dataclass
class ControlFlowGraph:
    """Complete control flow graph for a function."""

    name: str
    nodes: dict[str, CFGNode] = field(default_factory=dict)
    edges: list[CFGEdge] = field(default_factory=list)
    entry_node: str | None = None
    exit_node: str | None = None

    def add_node(self, node: CFGNode) -> None:
        """Add a node to the graph."""
        self.nodes[node.id] = node

    def add_edge(
        self,
        source: str,
        target: str,
        edge_type: EdgeType = EdgeType.SEQUENTIAL,
        label: str | None = None,
        condition: str | None = None,
    ) -> None:
        """Add an edge to the graph."""
        self.edges.append(
            CFGEdge(
                source=source,
                target=target,
                edge_type=edge_type,
                label=label,
                condition=condition,
            )
        )

    def get_successors(self, node_id: str) -> list[str]:
        """Get successor node IDs for a node."""
        return [e.target for e in self.edges if e.source == node_id]

    def get_predecessors(self, node_id: str) -> list[str]:
        """Get predecessor node IDs for a node."""
        return [e.source for e in self.edges if e.target == node_id]

    def get_edges_from(self, node_id: str) -> list[CFGEdge]:
        """Get all edges from a node."""
        return [e for e in self.edges if e.source == node_id]

    def get_edges_to(self, node_id: str) -> list[CFGEdge]:
        """Get all edges to a node."""
        return [e for e in self.edges if e.target == node_id]

    @property
    def node_count(self) -> int:
        """Number of nodes in the graph."""
        return len(self.nodes)

    @property
    def edge_count(self) -> int:
        """Number of edges in the graph."""
        return len(self.edges)

    @property
    def cyclomatic_complexity(self) -> int:
        """McCabe cyclomatic complexity: E - N + 2.

        Measures the number of linearly independent paths through the code.
        Higher values indicate more complex control flow.
        """
        return self.edge_count - self.node_count + 2

    @property
    def entry(self) -> str | None:
        """Alias for entry_node for API compatibility."""
        return self.entry_node

    @property
    def exit(self) -> str | None:
        """Alias for exit_node for API compatibility."""
        return self.exit_node

    def to_dot(self) -> str:
        """Convert CFG to DOT format for visualization."""
        lines = [f'digraph "{self.name}" {{']
        lines.append("  rankdir=TB;")
        lines.append("  node [shape=box];")

        # Nodes
        for node in self.nodes.values():
            label = f"{node.id}"
            if node.statements:
                stmt_text = "\\n".join(node.statements[:3])
                if len(node.statements) > 3:
                    stmt_text += "\\n..."
                label = f"{node.id}\\n{stmt_text}"

            shape = "box"
            if node.node_type == NodeType.ENTRY:
                shape = "ellipse"
            elif node.node_type == NodeType.EXIT:
                shape = "ellipse"
            elif node.node_type == NodeType.BRANCH:
                shape = "diamond"

            lines.append(f'  "{node.id}" [label="{label}", shape={shape}];')

        # Edges
        for edge in self.edges:
            style = ""
            label = ""
            if edge.edge_type == EdgeType.CONDITIONAL_TRUE:
                label = "True"
                style = "color=green"
            elif edge.edge_type == EdgeType.CONDITIONAL_FALSE:
                label = "False"
                style = "color=red"
            elif edge.edge_type == EdgeType.LOOP_BACK:
                style = "style=dashed"
            elif edge.edge_type == EdgeType.EXCEPTION:
                style = "color=orange"

            attrs = []
            if label:
                attrs.append(f'label="{label}"')
            if style:
                attrs.append(style)
            attr_str = f" [{', '.join(attrs)}]" if attrs else ""
            lines.append(f'  "{edge.source}" -> "{edge.target}"{attr_str};')

        lines.append("}")
        return "\n".join(lines)

    def to_mermaid(self) -> str:
        """Convert CFG to Mermaid flowchart format."""
        lines = ["flowchart TD"]

        # Nodes
        for node in self.nodes.values():
            label = node.id
            if node.statements:
                stmt_text = "<br/>".join(node.statements[:3])
                if len(node.statements) > 3:
                    stmt_text += "<br/>..."
                # Escape special characters
                stmt_text = stmt_text.replace('"', "'")
                label = f"{node.id}<br/>{stmt_text}"

            # Different shapes for different node types
            if node.node_type == NodeType.ENTRY:
                lines.append(f'    {node.id}(["{label}"])')
            elif node.node_type == NodeType.EXIT:
                lines.append(f'    {node.id}(["{label}"])')
            elif node.node_type == NodeType.BRANCH:
                lines.append(f'    {node.id}{{"{label}"}}')
            else:
                lines.append(f'    {node.id}["{label}"]')

        # Edges
        for edge in self.edges:
            arrow = "-->"
            label = ""

            if edge.edge_type == EdgeType.CONDITIONAL_TRUE:
                label = "|True|"
            elif edge.edge_type == EdgeType.CONDITIONAL_FALSE:
                label = "|False|"
            elif edge.edge_type == EdgeType.LOOP_BACK:
                arrow = "-.->>"
            elif edge.edge_type == EdgeType.EXCEPTION:
                label = "|exception|"

            lines.append(f"    {edge.source} {arrow}{label} {edge.target}")

        return "\n".join(lines)

    def to_text(self) -> str:
        """Convert CFG to human-readable text format."""
        lines = [f"CFG for {self.name}:"]
        lines.append(
            f"  Nodes: {self.node_count}, Edges: {self.edge_count}, "
            f"Complexity: {self.cyclomatic_complexity}"
        )
        lines.append("")

        for node in self.nodes.values():
            lines.append(f"[{node.id}] ({node.node_type.name})")
            if node.line_start:
                lines.append(f"  Lines: {node.line_start}-{node.line_end or node.line_start}")
            if node.statements:
                for stmt in node.statements:
                    lines.append(f"  | {stmt}")

            succs = self.get_successors(node.id)
            if succs:
                edges = self.get_edges_from(node.id)
                for edge in edges:
                    edge_info = f"  -> {edge.target}"
                    if edge.edge_type != EdgeType.SEQUENTIAL:
                        edge_info += f" ({edge.edge_type.name})"
                    if edge.condition:
                        edge_info += f" [{edge.condition}]"
                    lines.append(edge_info)
            lines.append("")

        return "\n".join(lines)


class CFGBuilder:
    """Builds control flow graphs from Python AST."""

    def __init__(self) -> None:
        self._node_counter = 0
        self._current_cfg: ControlFlowGraph | None = None

    def _new_node_id(self) -> str:
        """Generate a new unique node ID."""
        self._node_counter += 1
        return f"N{self._node_counter}"

    def build_from_source(
        self, source: str, function_name: str | None = None
    ) -> list[ControlFlowGraph]:
        """Build CFGs from Python source code.

        Args:
            source: Python source code
            function_name: Specific function to analyze (None for all)

        Returns:
            List of ControlFlowGraph objects
        """
        tree = ast.parse(source)
        return self.build_from_ast(tree, function_name)

    def build_from_file(
        self, path: Path, function_name: str | None = None
    ) -> list[ControlFlowGraph]:
        """Build CFGs from a Python file.

        Args:
            path: Path to Python file
            function_name: Specific function to analyze (None for all)

        Returns:
            List of ControlFlowGraph objects
        """
        source = path.read_text()
        return self.build_from_source(source, function_name)

    def build_from_ast(
        self, tree: ast.AST, function_name: str | None = None
    ) -> list[ControlFlowGraph]:
        """Build CFGs from AST.

        Args:
            tree: AST tree
            function_name: Specific function to analyze (None for all)

        Returns:
            List of ControlFlowGraph objects
        """
        cfgs = []

        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                if function_name is None or node.name == function_name:
                    cfg = self._build_function_cfg(node)
                    cfgs.append(cfg)

        return cfgs

    def _build_function_cfg(self, func: ast.FunctionDef | ast.AsyncFunctionDef) -> ControlFlowGraph:
        """Build CFG for a single function."""
        self._node_counter = 0
        cfg = ControlFlowGraph(name=func.name)

        # Create entry and exit nodes
        entry = CFGNode(id="ENTRY", node_type=NodeType.ENTRY, line_start=func.lineno)
        exit_node = CFGNode(id="EXIT", node_type=NodeType.EXIT)
        cfg.add_node(entry)
        cfg.add_node(exit_node)
        cfg.entry_node = "ENTRY"
        cfg.exit_node = "EXIT"

        # Process function body
        last_nodes = self._process_body(cfg, func.body, "ENTRY")

        # Connect remaining nodes to exit
        for node_id in last_nodes:
            cfg.add_edge(node_id, "EXIT")

        return cfg

    def _process_body(
        self, cfg: ControlFlowGraph, body: list[ast.stmt], prev_node: str
    ) -> list[str]:
        """Process a list of statements and return list of exit node IDs."""
        current_nodes = [prev_node]

        for stmt in body:
            if not current_nodes:
                # No exits (e.g., return), stop processing
                break

            if len(current_nodes) == 1:
                # Single predecessor - process normally
                exits = self._process_statement(cfg, stmt, current_nodes[0])
            else:
                # Multiple predecessors - create merge node to avoid exponential blowup
                # Process statement from first predecessor
                exits = self._process_statement(cfg, stmt, current_nodes[0])

                # Connect remaining predecessors to the first node created
                # (which is the entry point for this statement)
                if exits:
                    # Find the first node connected to current_nodes[0] for this statement
                    first_created = None
                    for edge in cfg.edges:
                        if edge.source == current_nodes[0]:
                            # This is likely the entry node for this statement
                            first_created = edge.target
                            break

                    if first_created:
                        for extra_node in current_nodes[1:]:
                            cfg.add_edge(extra_node, first_created)

            current_nodes = exits

        return current_nodes

    def _process_statement(
        self, cfg: ControlFlowGraph, stmt: ast.stmt, prev_node: str
    ) -> list[str]:
        """Process a single statement, return list of exit node IDs."""
        if isinstance(stmt, ast.If):
            return self._process_if(cfg, stmt, prev_node)
        elif isinstance(stmt, (ast.While, ast.For)):
            return self._process_loop(cfg, stmt, prev_node)
        elif isinstance(stmt, ast.Try):
            return self._process_try(cfg, stmt, prev_node)
        elif isinstance(stmt, ast.Return):
            return self._process_return(cfg, stmt, prev_node)
        elif isinstance(stmt, ast.Break):
            # Break handled by loop processing
            return []
        elif isinstance(stmt, ast.Continue):
            # Continue handled by loop processing
            return []
        else:
            return self._process_simple(cfg, stmt, prev_node)

    def _process_if(self, cfg: ControlFlowGraph, stmt: ast.If, prev_node: str) -> list[str]:
        """Process if statement."""
        # Create branch node
        branch_id = self._new_node_id()
        branch = CFGNode(
            id=branch_id,
            node_type=NodeType.BRANCH,
            statements=[f"if {ast.unparse(stmt.test)}:"],
            line_start=stmt.lineno,
            ast_nodes=[stmt],
        )
        cfg.add_node(branch)
        cfg.add_edge(prev_node, branch_id)

        exits = []

        # True branch
        if stmt.body:
            true_exits = self._process_body(cfg, stmt.body, branch_id)
            # Add condition to first edge
            for edge in cfg.edges:
                if edge.source == branch_id and edge.target in [branch_id]:
                    edge.edge_type = EdgeType.CONDITIONAL_TRUE
            exits.extend(true_exits)

        # False branch (else/elif)
        if stmt.orelse:
            false_exits = self._process_body(cfg, stmt.orelse, branch_id)
            exits.extend(false_exits)
        else:
            exits.append(branch_id)

        # Update edge types
        true_targets = set()
        if stmt.body:
            for s in stmt.body[:1]:
                true_targets.add(self._get_first_node_for_stmt(cfg, s))

        for edge in cfg.edges:
            if edge.source == branch_id:
                if edge.target in true_targets:
                    edge.edge_type = EdgeType.CONDITIONAL_TRUE
                elif edge.target != branch_id:
                    edge.edge_type = EdgeType.CONDITIONAL_FALSE

        return exits

    def _get_first_node_for_stmt(self, cfg: ControlFlowGraph, stmt: ast.stmt) -> str | None:
        """Get the first node ID for a statement (approximation)."""
        for node in cfg.nodes.values():
            if node.line_start == getattr(stmt, "lineno", None):
                return node.id
        return None

    def _process_loop(
        self, cfg: ControlFlowGraph, stmt: ast.While | ast.For, prev_node: str
    ) -> list[str]:
        """Process while/for loop."""
        # Create loop header
        header_id = self._new_node_id()
        if isinstance(stmt, ast.While):
            condition = f"while {ast.unparse(stmt.test)}:"
        else:
            condition = f"for {ast.unparse(stmt.target)} in {ast.unparse(stmt.iter)}:"

        header = CFGNode(
            id=header_id,
            node_type=NodeType.LOOP_HEADER,
            statements=[condition],
            line_start=stmt.lineno,
            ast_nodes=[stmt],
        )
        cfg.add_node(header)
        cfg.add_edge(prev_node, header_id)

        # Process body
        body_exits = self._process_body(cfg, stmt.body, header_id)

        # Add back edges
        for exit_id in body_exits:
            cfg.add_edge(exit_id, header_id, EdgeType.LOOP_BACK)

        exits = [header_id]  # Loop can exit after condition fails

        # Process else clause (runs if loop completes normally)
        if stmt.orelse:
            else_exits = self._process_body(cfg, stmt.orelse, header_id)
            exits.extend(else_exits)

        return exits

    def _process_try(self, cfg: ControlFlowGraph, stmt: ast.Try, prev_node: str) -> list[str]:
        """Process try/except/finally."""
        exits = []

        # Process try body
        try_exits = self._process_body(cfg, stmt.body, prev_node)
        exits.extend(try_exits)

        # Process exception handlers
        for handler in stmt.handlers:
            handler_id = self._new_node_id()
            exc_type = ast.unparse(handler.type) if handler.type else "Exception"
            handler_node = CFGNode(
                id=handler_id,
                node_type=NodeType.EXCEPTION_HANDLER,
                statements=[f"except {exc_type}:"],
                line_start=handler.lineno,
                ast_nodes=[handler],
            )
            cfg.add_node(handler_node)
            cfg.add_edge(prev_node, handler_id, EdgeType.EXCEPTION)

            handler_exits = self._process_body(cfg, handler.body, handler_id)
            exits.extend(handler_exits)

        # Process finally
        if stmt.finalbody:
            finally_id = self._new_node_id()
            finally_node = CFGNode(
                id=finally_id,
                node_type=NodeType.FINALLY,
                statements=["finally:"],
                line_start=stmt.finalbody[0].lineno if stmt.finalbody else None,
            )
            cfg.add_node(finally_node)

            # Connect all exits to finally
            for exit_id in exits:
                cfg.add_edge(exit_id, finally_id)

            finally_exits = self._process_body(cfg, stmt.finalbody, finally_id)
            exits = finally_exits

        return exits

    def _process_return(self, cfg: ControlFlowGraph, stmt: ast.Return, prev_node: str) -> list[str]:
        """Process return statement."""
        node_id = self._new_node_id()
        value = ast.unparse(stmt.value) if stmt.value else ""
        node = CFGNode(
            id=node_id,
            node_type=NodeType.BASIC,
            statements=[f"return {value}" if value else "return"],
            line_start=stmt.lineno,
            ast_nodes=[stmt],
        )
        cfg.add_node(node)
        cfg.add_edge(prev_node, node_id)
        cfg.add_edge(node_id, "EXIT", EdgeType.RETURN)
        return []  # Return terminates this path

    def _process_simple(self, cfg: ControlFlowGraph, stmt: ast.stmt, prev_node: str) -> list[str]:
        """Process simple statement (assignment, expression, etc.)."""
        node_id = self._new_node_id()
        try:
            stmt_text = ast.unparse(stmt)
        except ValueError:
            stmt_text = f"<{stmt.__class__.__name__}>"

        node = CFGNode(
            id=node_id,
            node_type=NodeType.BASIC,
            statements=[stmt_text],
            line_start=stmt.lineno,
            line_end=getattr(stmt, "end_lineno", stmt.lineno),
            ast_nodes=[stmt],
        )
        cfg.add_node(node)
        cfg.add_edge(prev_node, node_id)
        return [node_id]


class CFGViewProvider:
    """View provider that generates Control Flow Graph views.

    Note: This is a legacy interface. For the plugin-based architecture,
    use PythonCFGPlugin instead.
    """

    def __init__(self) -> None:
        self._builder = CFGBuilder()

    @property
    def name(self) -> str:
        return "cfg"

    def provide(self, path: Path, options: ViewOptions | None = None) -> View:
        """Provide a CFG view for a file.

        Args:
            path: Path to the source file
            options: View options (may include "function" in extra dict)

        Returns:
            View containing CFG information
        """
        from .views import View, ViewTarget, ViewType

        opts = options or ViewOptions()
        content = path.read_text()

        function_name = opts.extra.get("function") if opts.extra else None
        cfgs = self._builder.build_from_source(content, function_name)

        # Format output
        output_lines = []
        for cfg in cfgs:
            output_lines.append(cfg.to_text())
            output_lines.append("")

        return View(
            target=ViewTarget(path=path),
            view_type=ViewType.CFG,
            content="\n".join(output_lines),
            metadata={
                "function_count": len(cfgs),
                "functions": [c.name for c in cfgs],
            },
        )

    def provide_dot(self, path: Path, function_name: str | None = None) -> str:
        """Generate DOT format CFG for visualization.

        Args:
            path: Path to the source file
            function_name: Specific function (None for all)

        Returns:
            DOT format string (can be rendered with Graphviz)
        """
        content = path.read_text()

        cfgs = self._builder.build_from_source(content, function_name)

        dots = []
        for cfg in cfgs:
            dots.append(cfg.to_dot())

        return "\n\n".join(dots)


def build_cfg(source: str, function_name: str | None = None) -> list[ControlFlowGraph]:
    """Convenience function to build CFGs from source.

    Args:
        source: Python source code
        function_name: Specific function (None for all)

    Returns:
        List of ControlFlowGraph objects
    """
    builder = CFGBuilder()
    return builder.build_from_source(source, function_name)


# =============================================================================
# Plugin Wrapper
# =============================================================================


class PythonCFGPlugin:
    """Plugin wrapper for CFG generation.

    This provides a ViewPlugin interface for CFG generation,
    adapting from the Handle-based CFGViewProvider to ViewTarget.
    """

    def __init__(self) -> None:
        self._builder = CFGBuilder()

    @property
    def metadata(self) -> PluginMetadata:
        from moss.plugins import PluginMetadata

        return PluginMetadata(
            name="python-cfg",
            view_type="cfg",
            languages=frozenset(["python"]),
            priority=5,
            version="0.1.0",
            description="Python control flow graph generation",
        )

    def supports(self, target: ViewTarget) -> bool:
        """Check if this plugin can handle the target."""
        from moss.plugins import detect_language

        if not target.path.exists():
            return False
        return detect_language(target.path) == "python"

    async def render(
        self,
        target: ViewTarget,
        options: ViewOptions | None = None,
    ) -> View:
        """Render a CFG view for the target."""
        from .views import View, ViewOptions, ViewType

        opts = options or ViewOptions()
        source = target.path.read_text()

        # Get function name from options or target
        function_name = target.symbol or opts.extra.get("function_name")

        try:
            cfgs = self._builder.build_from_source(source, function_name)
        except SyntaxError as e:
            return View(
                target=target,
                view_type=ViewType.CFG,
                content=f"# Parse error: {e}",
                metadata={"error": str(e)},
            )

        # Format output
        output_lines = []
        dot_lines = []
        cfg_data = []

        for cfg in cfgs:
            output_lines.append(cfg.to_text())
            output_lines.append("")
            dot_lines.append(cfg.to_dot())

            # Serialize CFG data for JSON output
            cfg_data.append(
                {
                    "name": cfg.name,
                    "node_count": cfg.node_count,
                    "edge_count": cfg.edge_count,
                    "cyclomatic_complexity": cfg.cyclomatic_complexity,
                    "entry": cfg.entry_node,
                    "exit": cfg.exit_node,
                    "nodes": {
                        nid: {
                            "type": n.node_type.value,
                            "statements": n.statements,
                            "line_start": n.line_start,
                        }
                        for nid, n in cfg.nodes.items()
                    },
                    "edges": [
                        {
                            "source": e.source,
                            "target": e.target,
                            "type": e.edge_type.value,
                            "condition": e.condition,
                        }
                        for e in cfg.edges
                    ],
                }
            )

        return View(
            target=target,
            view_type=ViewType.CFG,
            content="\n".join(output_lines),
            metadata={
                "function_count": len(cfgs),
                "cfgs": cfg_data,
                "dot": "\n".join(dot_lines),
                "language": "python",
            },
        )
