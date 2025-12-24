//! Control Flow Graph (CFG) analysis.
//!
//! Builds a simplified control flow graph for functions.

use moss_core::{tree_sitter, Language, Parsers};
use moss_languages::{get_support, LanguageSupport};
use std::path::Path;

/// Type of control flow edge
#[derive(Debug, Clone, Copy)]
pub enum EdgeType {
    Sequential,
    ConditionalTrue,
    ConditionalFalse,
    LoopBack,
    Return,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::Sequential => "seq",
            EdgeType::ConditionalTrue => "true",
            EdgeType::ConditionalFalse => "false",
            EdgeType::LoopBack => "loop",
            EdgeType::Return => "return",
        }
    }
}

/// Type of CFG node
#[derive(Debug, Clone, Copy)]
pub enum NodeType {
    Entry,
    Exit,
    Basic,
    Branch,
    LoopHeader,
}

impl NodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeType::Entry => "entry",
            NodeType::Exit => "exit",
            NodeType::Basic => "basic",
            NodeType::Branch => "branch",
            NodeType::LoopHeader => "loop",
        }
    }
}

/// A node in the control flow graph
#[derive(Debug, Clone)]
pub struct CfgNode {
    pub id: usize,
    pub node_type: NodeType,
    pub statement: Option<String>,
    pub start_line: Option<usize>,
}

/// An edge in the control flow graph
#[derive(Debug, Clone)]
pub struct CfgEdge {
    pub source: usize,
    pub target: usize,
    pub edge_type: EdgeType,
}

/// Control flow graph for a function
pub struct ControlFlowGraph {
    pub name: String,
    pub nodes: Vec<CfgNode>,
    pub edges: Vec<CfgEdge>,
    pub start_line: usize,
    pub end_line: usize,
}

impl ControlFlowGraph {
    pub fn cyclomatic_complexity(&self) -> usize {
        // E - N + 2
        self.edges.len().saturating_sub(self.nodes.len()) + 2
    }

    pub fn format_text(&self) -> String {
        let mut lines = vec![
            format!(
                "CFG: {} (lines {}-{})",
                self.name, self.start_line, self.end_line
            ),
            format!(
                "  Nodes: {}, Edges: {}, Complexity: {}",
                self.nodes.len(),
                self.edges.len(),
                self.cyclomatic_complexity()
            ),
            String::new(),
        ];

        // Format nodes
        for node in &self.nodes {
            let stmt = node
                .statement
                .as_ref()
                .map(|s| {
                    let truncated = if s.len() > 50 {
                        format!("{}...", &s[..47])
                    } else {
                        s.clone()
                    };
                    format!(": {}", truncated)
                })
                .unwrap_or_default();

            let line_info = node
                .start_line
                .map(|l| format!(" @{}", l))
                .unwrap_or_default();

            lines.push(format!(
                "  [{}] {}{}{}",
                node.id,
                node.node_type.as_str(),
                line_info,
                stmt
            ));

            // Show outgoing edges
            let outgoing: Vec<_> = self.edges.iter().filter(|e| e.source == node.id).collect();
            for edge in outgoing {
                lines.push(format!(
                    "    -> [{}] ({})",
                    edge.target,
                    edge.edge_type.as_str()
                ));
            }
        }

        lines.join("\n")
    }
}

/// CFG result for a file
#[allow(dead_code)] // file_path provides context for the result
pub struct CfgResult {
    pub graphs: Vec<ControlFlowGraph>,
    pub file_path: String,
}

pub struct CfgBuilder {
    parsers: Parsers,
    node_counter: usize,
}

impl CfgBuilder {
    pub fn new() -> Self {
        Self {
            parsers: Parsers::new(),
            node_counter: 0,
        }
    }

    pub fn build(&mut self, path: &Path, content: &str, function_name: Option<&str>) -> CfgResult {
        let lang = Language::from_path(path);
        let graphs = match lang {
            Some(Language::Python) => self.build_python(content, function_name),
            Some(Language::Rust) => self.build_rust(content, function_name),
            _ => Vec::new(),
        };

        CfgResult {
            graphs,
            file_path: path.to_string_lossy().to_string(),
        }
    }

    /// Check if a node kind is a control flow node using the trait
    #[allow(dead_code)]
    fn is_control_flow(&self, lang: Language, kind: &str) -> bool {
        if let Some(support) = get_support(lang) {
            return support.control_flow_kinds().contains(&kind);
        }
        false
    }

    /// Check if a node kind is a function using the trait
    #[allow(dead_code)]
    fn is_function(&self, lang: Language, kind: &str) -> bool {
        if let Some(support) = get_support(lang) {
            return support.function_kinds().contains(&kind);
        }
        false
    }

    fn new_node(
        &mut self,
        node_type: NodeType,
        statement: Option<String>,
        start_line: Option<usize>,
    ) -> CfgNode {
        let id = self.node_counter;
        self.node_counter += 1;
        CfgNode {
            id,
            node_type,
            statement,
            start_line,
        }
    }

    fn build_python(&mut self, content: &str, filter_name: Option<&str>) -> Vec<ControlFlowGraph> {
        let tree = match self.parsers.parse_lang(Language::Python, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut graphs = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_python_functions(&mut cursor, content, &mut graphs, filter_name);
        graphs
    }

    fn collect_python_functions(
        &mut self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        graphs: &mut Vec<ControlFlowGraph>,
        filter_name: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            if kind == "function_definition" || kind == "async_function_definition" {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = content[name_node.byte_range()].to_string();

                    if filter_name.is_none() || filter_name == Some(&name) {
                        if let Some(cfg) = self.build_python_function(&node, content, &name) {
                            graphs.push(cfg);
                        }
                    }
                }
            }

            // Recurse
            if cursor.goto_first_child() {
                self.collect_python_functions(cursor, content, graphs, filter_name);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn build_python_function(
        &mut self,
        func_node: &tree_sitter::Node,
        content: &str,
        name: &str,
    ) -> Option<ControlFlowGraph> {
        self.node_counter = 0;

        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // Entry node
        let entry = self.new_node(
            NodeType::Entry,
            Some("ENTRY".to_string()),
            Some(func_node.start_position().row + 1),
        );
        nodes.push(entry);

        // Exit node
        let exit = self.new_node(NodeType::Exit, Some("EXIT".to_string()), None);
        let exit_id = exit.id;
        nodes.push(exit);

        // Process body
        if let Some(body) = func_node.child_by_field_name("body") {
            let exit_nodes = self.process_python_body(&body, content, 0, &mut nodes, &mut edges);

            // Connect remaining nodes to exit
            for id in exit_nodes {
                edges.push(CfgEdge {
                    source: id,
                    target: exit_id,
                    edge_type: EdgeType::Sequential,
                });
            }
        } else {
            // Empty function
            edges.push(CfgEdge {
                source: 0,
                target: exit_id,
                edge_type: EdgeType::Sequential,
            });
        }

        Some(ControlFlowGraph {
            name: name.to_string(),
            nodes,
            edges,
            start_line: func_node.start_position().row + 1,
            end_line: func_node.end_position().row + 1,
        })
    }

    fn process_python_body(
        &mut self,
        body: &tree_sitter::Node,
        content: &str,
        prev_id: usize,
        nodes: &mut Vec<CfgNode>,
        edges: &mut Vec<CfgEdge>,
    ) -> Vec<usize> {
        let mut current_ids = vec![prev_id];
        let mut cursor = body.walk();

        if !cursor.goto_first_child() {
            return current_ids;
        }

        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "if_statement" => {
                    // Branch node
                    let condition = node
                        .child_by_field_name("condition")
                        .map(|c| content[c.byte_range()].to_string())
                        .unwrap_or_else(|| "if".to_string());

                    let branch = self.new_node(
                        NodeType::Branch,
                        Some(format!("if {}", condition)),
                        Some(node.start_position().row + 1),
                    );
                    let branch_id = branch.id;
                    nodes.push(branch);

                    // Connect from current nodes
                    for id in &current_ids {
                        edges.push(CfgEdge {
                            source: *id,
                            target: branch_id,
                            edge_type: EdgeType::Sequential,
                        });
                    }

                    let mut exit_ids = Vec::new();

                    // True branch (consequence)
                    if let Some(consequence) = node.child_by_field_name("consequence") {
                        let true_exits = self.process_python_body(
                            &consequence,
                            content,
                            branch_id,
                            nodes,
                            edges,
                        );

                        // Mark first edge as true branch
                        for edge in edges.iter_mut().rev() {
                            if edge.source == branch_id
                                && edge.edge_type as u8 == EdgeType::Sequential as u8
                            {
                                edge.edge_type = EdgeType::ConditionalTrue;
                                break;
                            }
                        }

                        exit_ids.extend(true_exits);
                    }

                    // False branch (alternative - else/elif)
                    if let Some(alternative) = node.child_by_field_name("alternative") {
                        let false_exits = self.process_python_body(
                            &alternative,
                            content,
                            branch_id,
                            nodes,
                            edges,
                        );

                        // Mark edge as false branch
                        for edge in edges.iter_mut().rev() {
                            if edge.source == branch_id
                                && edge.edge_type as u8 == EdgeType::Sequential as u8
                            {
                                edge.edge_type = EdgeType::ConditionalFalse;
                                break;
                            }
                        }

                        exit_ids.extend(false_exits);
                    } else {
                        // No else - branch can fall through
                        exit_ids.push(branch_id);
                    }

                    current_ids = exit_ids;
                }
                "for_statement" | "while_statement" => {
                    let is_for = kind == "for_statement";
                    let header_text = if is_for {
                        let target = node
                            .child_by_field_name("left")
                            .map(|c| content[c.byte_range()].to_string())
                            .unwrap_or_default();
                        let iter = node
                            .child_by_field_name("right")
                            .map(|c| content[c.byte_range()].to_string())
                            .unwrap_or_default();
                        format!("for {} in {}", target, iter)
                    } else {
                        let condition = node
                            .child_by_field_name("condition")
                            .map(|c| content[c.byte_range()].to_string())
                            .unwrap_or_else(|| "while".to_string());
                        format!("while {}", condition)
                    };

                    let loop_header = self.new_node(
                        NodeType::LoopHeader,
                        Some(header_text),
                        Some(node.start_position().row + 1),
                    );
                    let header_id = loop_header.id;
                    nodes.push(loop_header);

                    // Connect from current nodes
                    for id in &current_ids {
                        edges.push(CfgEdge {
                            source: *id,
                            target: header_id,
                            edge_type: EdgeType::Sequential,
                        });
                    }

                    // Loop body
                    if let Some(body_node) = node.child_by_field_name("body") {
                        let body_exits =
                            self.process_python_body(&body_node, content, header_id, nodes, edges);

                        // Back edge to header
                        for id in body_exits {
                            edges.push(CfgEdge {
                                source: id,
                                target: header_id,
                                edge_type: EdgeType::LoopBack,
                            });
                        }
                    }

                    // Loop can exit after condition fails
                    current_ids = vec![header_id];
                }
                "return_statement" => {
                    let value = node
                        .child(1)
                        .map(|c| content[c.byte_range()].to_string())
                        .unwrap_or_default();

                    let ret = self.new_node(
                        NodeType::Basic,
                        Some(format!("return {}", value)),
                        Some(node.start_position().row + 1),
                    );
                    let ret_id = ret.id;
                    nodes.push(ret);

                    for id in &current_ids {
                        edges.push(CfgEdge {
                            source: *id,
                            target: ret_id,
                            edge_type: EdgeType::Sequential,
                        });
                    }

                    // Connect to exit (id 1)
                    edges.push(CfgEdge {
                        source: ret_id,
                        target: 1,
                        edge_type: EdgeType::Return,
                    });

                    current_ids = vec![]; // Path terminates
                }
                _ if !kind.ends_with("_clause") && !kind.ends_with("_keyword") => {
                    // Simple statement
                    let stmt_text = content[node.byte_range()].trim().to_string();
                    if !stmt_text.is_empty() && !stmt_text.starts_with('#') {
                        let stmt = self.new_node(
                            NodeType::Basic,
                            Some(stmt_text),
                            Some(node.start_position().row + 1),
                        );
                        let stmt_id = stmt.id;
                        nodes.push(stmt);

                        for id in &current_ids {
                            edges.push(CfgEdge {
                                source: *id,
                                target: stmt_id,
                                edge_type: EdgeType::Sequential,
                            });
                        }

                        current_ids = vec![stmt_id];
                    }
                }
                _ => {}
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }

        current_ids
    }

    fn build_rust(&mut self, content: &str, filter_name: Option<&str>) -> Vec<ControlFlowGraph> {
        let tree = match self.parsers.parse_lang(Language::Rust, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut graphs = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_rust_functions(&mut cursor, content, &mut graphs, filter_name);
        graphs
    }

    fn collect_rust_functions(
        &mut self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        graphs: &mut Vec<ControlFlowGraph>,
        filter_name: Option<&str>,
    ) {
        loop {
            let node = cursor.node();

            if node.kind() == "function_item" {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = content[name_node.byte_range()].to_string();

                    if filter_name.is_none() || filter_name == Some(&name) {
                        if let Some(cfg) = self.build_rust_function(&node, content, &name) {
                            graphs.push(cfg);
                        }
                    }
                }
            }

            // Recurse
            if cursor.goto_first_child() {
                self.collect_rust_functions(cursor, content, graphs, filter_name);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn build_rust_function(
        &mut self,
        func_node: &tree_sitter::Node,
        content: &str,
        name: &str,
    ) -> Option<ControlFlowGraph> {
        self.node_counter = 0;

        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // Entry node
        let entry = self.new_node(
            NodeType::Entry,
            Some("ENTRY".to_string()),
            Some(func_node.start_position().row + 1),
        );
        nodes.push(entry);

        // Exit node
        let exit = self.new_node(NodeType::Exit, Some("EXIT".to_string()), None);
        let exit_id = exit.id;
        nodes.push(exit);

        // Process body
        if let Some(body) = func_node.child_by_field_name("body") {
            let exit_nodes = self.process_rust_body(&body, content, 0, &mut nodes, &mut edges);

            // Connect remaining nodes to exit
            for id in exit_nodes {
                edges.push(CfgEdge {
                    source: id,
                    target: exit_id,
                    edge_type: EdgeType::Sequential,
                });
            }
        } else {
            edges.push(CfgEdge {
                source: 0,
                target: exit_id,
                edge_type: EdgeType::Sequential,
            });
        }

        Some(ControlFlowGraph {
            name: name.to_string(),
            nodes,
            edges,
            start_line: func_node.start_position().row + 1,
            end_line: func_node.end_position().row + 1,
        })
    }

    fn process_rust_body(
        &mut self,
        body: &tree_sitter::Node,
        content: &str,
        prev_id: usize,
        nodes: &mut Vec<CfgNode>,
        edges: &mut Vec<CfgEdge>,
    ) -> Vec<usize> {
        let mut current_ids = vec![prev_id];
        let mut cursor = body.walk();

        if !cursor.goto_first_child() {
            return current_ids;
        }

        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "if_expression" => {
                    let condition = node
                        .child_by_field_name("condition")
                        .map(|c| content[c.byte_range()].to_string())
                        .unwrap_or_else(|| "if".to_string());

                    let branch = self.new_node(
                        NodeType::Branch,
                        Some(format!("if {}", condition)),
                        Some(node.start_position().row + 1),
                    );
                    let branch_id = branch.id;
                    nodes.push(branch);

                    for id in &current_ids {
                        edges.push(CfgEdge {
                            source: *id,
                            target: branch_id,
                            edge_type: EdgeType::Sequential,
                        });
                    }

                    let mut exit_ids = Vec::new();

                    // True branch
                    if let Some(consequence) = node.child_by_field_name("consequence") {
                        let true_exits =
                            self.process_rust_body(&consequence, content, branch_id, nodes, edges);
                        for edge in edges.iter_mut().rev() {
                            if edge.source == branch_id
                                && edge.edge_type as u8 == EdgeType::Sequential as u8
                            {
                                edge.edge_type = EdgeType::ConditionalTrue;
                                break;
                            }
                        }
                        exit_ids.extend(true_exits);
                    }

                    // False branch (else)
                    if let Some(alternative) = node.child_by_field_name("alternative") {
                        let false_exits =
                            self.process_rust_body(&alternative, content, branch_id, nodes, edges);
                        for edge in edges.iter_mut().rev() {
                            if edge.source == branch_id
                                && edge.edge_type as u8 == EdgeType::Sequential as u8
                            {
                                edge.edge_type = EdgeType::ConditionalFalse;
                                break;
                            }
                        }
                        exit_ids.extend(false_exits);
                    } else {
                        exit_ids.push(branch_id);
                    }

                    current_ids = exit_ids;
                }
                "for_expression" | "while_expression" | "loop_expression" => {
                    let header_text = &content[node.byte_range()];
                    let first_line = header_text.lines().next().unwrap_or("loop");

                    let loop_header = self.new_node(
                        NodeType::LoopHeader,
                        Some(first_line.to_string()),
                        Some(node.start_position().row + 1),
                    );
                    let header_id = loop_header.id;
                    nodes.push(loop_header);

                    for id in &current_ids {
                        edges.push(CfgEdge {
                            source: *id,
                            target: header_id,
                            edge_type: EdgeType::Sequential,
                        });
                    }

                    if let Some(body_node) = node.child_by_field_name("body") {
                        let body_exits =
                            self.process_rust_body(&body_node, content, header_id, nodes, edges);
                        for id in body_exits {
                            edges.push(CfgEdge {
                                source: id,
                                target: header_id,
                                edge_type: EdgeType::LoopBack,
                            });
                        }
                    }

                    current_ids = vec![header_id];
                }
                "return_expression" => {
                    let stmt_text = content[node.byte_range()].trim().to_string();
                    let ret = self.new_node(
                        NodeType::Basic,
                        Some(stmt_text),
                        Some(node.start_position().row + 1),
                    );
                    let ret_id = ret.id;
                    nodes.push(ret);

                    for id in &current_ids {
                        edges.push(CfgEdge {
                            source: *id,
                            target: ret_id,
                            edge_type: EdgeType::Sequential,
                        });
                    }

                    edges.push(CfgEdge {
                        source: ret_id,
                        target: 1,
                        edge_type: EdgeType::Return,
                    });

                    current_ids = vec![];
                }
                _ => {}
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }

        current_ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_python_cfg() {
        let mut builder = CfgBuilder::new();
        let content = r#"
def simple():
    x = 1
    return x

def with_if(x):
    if x > 0:
        return x
    return -x
"#;
        let result = builder.build(&PathBuf::from("test.py"), content, None);
        assert_eq!(result.graphs.len(), 2);

        let simple = result.graphs.iter().find(|g| g.name == "simple").unwrap();
        assert!(simple.nodes.len() >= 2); // At least entry and exit
        assert!(simple.edges.len() >= 1);
    }

    #[test]
    fn test_rust_cfg() {
        let mut builder = CfgBuilder::new();
        let content = r#"
fn simple() -> i32 {
    let x = 1;
    x
}

fn with_if(x: i32) -> i32 {
    if x > 0 {
        return x;
    }
    -x
}
"#;
        let result = builder.build(&PathBuf::from("test.rs"), content, None);
        assert!(result.graphs.len() >= 2);
    }
}
