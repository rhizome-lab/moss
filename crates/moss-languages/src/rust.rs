//! Rust language support.

use crate::{Export, Import, LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct RustSupport;

impl LanguageSupport for RustSupport {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn grammar_name(&self) -> &'static str {
        "rust"
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["impl_item", "trait_item"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_item"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["struct_item", "enum_item", "type_item", "trait_item"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["use_declaration"]
    }

    fn export_kinds(&self) -> &'static [&'static str] {
        &["function_item", "struct_item", "enum_item", "trait_item"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "for_expression",
            "while_expression",
            "loop_expression",
            "match_arm",
            "binary_expression", // for && and ||
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "for_expression",
            "while_expression",
            "loop_expression",
            "function_item",
            "impl_item",
            "trait_item",
            "mod_item",
        ]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        // Additional scope-creating nodes beyond functions and containers
        &[
            "block",
            "for_expression",
            "while_expression",
            "loop_expression",
            "closure_expression",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "for_expression",
            "while_expression",
            "loop_expression",
            "return_expression",
            "break_expression",
            "continue_expression",
        ]
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        // Get visibility modifier
        let mut vis = String::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                vis = format!("{} ", &content[child.byte_range()]);
                break;
            }
        }

        let params = node
            .child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        let return_type = node
            .child_by_field_name("return_type")
            .map(|r| format!(" -> {}", &content[r.byte_range()]))
            .unwrap_or_default();

        let signature = format!("{}fn {}{}{}", vis, name, params, return_type);

        Some(Symbol {
            name: name.to_string(),
            kind: if in_container {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            },
            signature,
            docstring: self.extract_docstring(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: self.get_visibility(node, content),
            children: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        match node.kind() {
            "impl_item" => {
                let type_node = node.child_by_field_name("type")?;
                let type_name = &content[type_node.byte_range()];

                Some(Symbol {
                    name: type_name.to_string(),
                    kind: SymbolKind::Module, // impl blocks are like modules
                    signature: format!("impl {}", type_name),
                    docstring: None,
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Public,
                    children: Vec::new(),
                })
            }
            "trait_item" => {
                let name = self.node_name(node, content)?;
                let vis = self.extract_visibility_prefix(node, content);

                Some(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Trait,
                    signature: format!("{}trait {}", vis, name),
                    docstring: self.extract_docstring(node, content),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: self.get_visibility(node, content),
                    children: Vec::new(),
                })
            }
            _ => None,
        }
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let vis = self.extract_visibility_prefix(node, content);

        let (kind, keyword) = match node.kind() {
            "struct_item" => (SymbolKind::Struct, "struct"),
            "enum_item" => (SymbolKind::Enum, "enum"),
            "type_item" => (SymbolKind::Type, "type"),
            "trait_item" => (SymbolKind::Trait, "trait"),
            _ => return None,
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{}{} {}", vis, keyword, name),
            docstring: self.extract_docstring(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: self.get_visibility(node, content),
            children: Vec::new(),
        })
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Look for doc comments before the node
        let lines: Vec<&str> = content.lines().collect();
        let start_line = node.start_position().row;

        if start_line == 0 {
            return None;
        }

        let mut doc_lines = Vec::new();
        for i in (0..start_line).rev() {
            let line = lines.get(i)?.trim();
            if line.starts_with("///") {
                let doc = line.trim_start_matches("///").trim();
                doc_lines.insert(0, doc.to_string());
            } else if line.starts_with("//!") {
                break; // Module-level doc
            } else if line.is_empty() {
                if !doc_lines.is_empty() {
                    break;
                }
            } else {
                break;
            }
        }

        if doc_lines.is_empty() {
            None
        } else {
            Some(doc_lines.join("\n"))
        }
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "use_declaration" {
            return Vec::new();
        }

        let line = node.start_position().row + 1;
        let text = &content[node.byte_range()];
        let module = text.trim_start_matches("use ").trim_end_matches(';').trim();

        // Check for braced imports: use foo::{bar, baz}
        let mut names = Vec::new();
        let is_relative = module.starts_with("crate")
            || module.starts_with("self")
            || module.starts_with("super");

        if let Some(brace_start) = module.find('{') {
            let prefix = module[..brace_start].trim_end_matches("::");
            if let Some(brace_end) = module.find('}') {
                let items = &module[brace_start + 1..brace_end];
                for item in items.split(',') {
                    let trimmed = item.trim();
                    if !trimmed.is_empty() {
                        names.push(trimmed.to_string());
                    }
                }
            }
            vec![Import {
                module: prefix.to_string(),
                names,
                alias: None,
                is_wildcard: false,
                is_relative,
                line,
            }]
        } else {
            // Simple import: use foo::bar or use foo::bar as baz
            let (module_part, alias) = if let Some(as_pos) = module.find(" as ") {
                (&module[..as_pos], Some(module[as_pos + 4..].to_string()))
            } else {
                (module, None)
            };

            vec![Import {
                module: module_part.to_string(),
                names: Vec::new(),
                alias,
                is_wildcard: module_part.ends_with("::*"),
                is_relative,
                line,
            }]
        }
    }

    fn extract_exports(&self, node: &Node, content: &str) -> Vec<Export> {
        let line = node.start_position().row + 1;

        // Only export pub items
        if !self.is_public(node, content) {
            return Vec::new();
        }

        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "function_item" => SymbolKind::Function,
            "struct_item" => SymbolKind::Struct,
            "enum_item" => SymbolKind::Enum,
            "trait_item" => SymbolKind::Trait,
            _ => return Vec::new(),
        };

        vec![Export { name, kind, line }]
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                let vis = &content[child.byte_range()];
                return vis.starts_with("pub");
            }
        }
        false
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                let vis = &content[child.byte_range()];
                if vis == "pub" {
                    return Visibility::Public;
                } else if vis.starts_with("pub(crate)") {
                    return Visibility::Internal;
                } else if vis.starts_with("pub(super)") || vis.starts_with("pub(in") {
                    return Visibility::Protected;
                }
            }
        }
        Visibility::Private
    }
}

impl RustSupport {
    fn extract_visibility_prefix(&self, node: &Node, content: &str) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                return format!("{} ", &content[child.byte_range()]);
            }
        }
        String::new()
    }
}
