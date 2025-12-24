//! Python language support.

use crate::{Export, Import, LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct PythonSupport;

impl LanguageSupport for PythonSupport {
    fn language(&self) -> Language {
        Language::Python
    }

    fn grammar_name(&self) -> &'static str {
        "python"
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_definition"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "async_function_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["class_definition"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_statement", "import_from_statement"]
    }

    fn export_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "async_function_definition", "class_definition"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "try_statement",
            "except_clause",
            "with_statement",
            "match_statement",
            "case_clause",
            "and",
            "or",
            "conditional_expression",
            "list_comprehension",
            "dictionary_comprehension",
            "set_comprehension",
            "generator_expression",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "try_statement",
            "with_statement",
            "match_statement",
            "function_definition",
            "async_function_definition",
            "class_definition",
        ]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        // Additional scope-creating nodes beyond functions and containers
        &[
            "for_statement",
            "with_statement",
            "list_comprehension",
            "set_comprehension",
            "dictionary_comprehension",
            "generator_expression",
            "lambda",
        ]
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        // Skip private methods unless they're dunder methods
        // (visibility filtering can be done by caller)

        let is_async = node.kind() == "async_function_definition";
        let prefix = if is_async { "async def" } else { "def" };

        let params = node
            .child_by_field_name("parameters")
            .map(|p| &content[p.byte_range()])
            .unwrap_or("()");

        let return_type = node
            .child_by_field_name("return_type")
            .map(|r| format!(" -> {}", &content[r.byte_range()]))
            .unwrap_or_default();

        let signature = format!("{} {}{}{}", prefix, name, params, return_type);
        let visibility = self.get_visibility(node, content);

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
            visibility,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        let bases = node
            .child_by_field_name("superclasses")
            .map(|b| &content[b.byte_range()])
            .unwrap_or("");

        let signature = if bases.is_empty() {
            format!("class {}", name)
        } else {
            format!("class {}{}", name, bases)
        };

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Class,
            signature,
            docstring: self.extract_docstring(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: self.get_visibility(node, content),
            children: Vec::new(), // Caller fills this in
        })
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        let body = node.child_by_field_name("body")?;
        let first = body.child(0)?;

        // Handle both grammar versions:
        // - Old: expression_statement > string
        // - New (arborium): string directly, with string_content child
        let string_node = match first.kind() {
            "string" => Some(first),
            "expression_statement" => first.child(0).filter(|n| n.kind() == "string"),
            _ => None,
        }?;

        // Try string_content child (arborium style)
        let mut cursor = string_node.walk();
        for child in string_node.children(&mut cursor) {
            if child.kind() == "string_content" {
                let doc = content[child.byte_range()].trim();
                if !doc.is_empty() {
                    return Some(doc.to_string());
                }
            }
        }

        // Fallback: extract from full string text (old style)
        let text = &content[string_node.byte_range()];
        let doc = text
            .trim_start_matches("\"\"\"")
            .trim_start_matches("'''")
            .trim_start_matches('"')
            .trim_start_matches('\'')
            .trim_end_matches("\"\"\"")
            .trim_end_matches("'''")
            .trim_end_matches('"')
            .trim_end_matches('\'')
            .trim();

        if !doc.is_empty() {
            Some(doc.to_string())
        } else {
            None
        }
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        let line = node.start_position().row + 1;

        match node.kind() {
            "import_statement" => {
                // import foo, import foo as bar
                let mut imports = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "dotted_name" {
                        let module = content[child.byte_range()].to_string();
                        imports.push(Import {
                            module,
                            names: Vec::new(),
                            alias: None,
                            is_wildcard: false,
                            is_relative: false,
                            line,
                        });
                    } else if child.kind() == "aliased_import" {
                        if let Some(name) = child.child_by_field_name("name") {
                            let module = content[name.byte_range()].to_string();
                            let alias = child
                                .child_by_field_name("alias")
                                .map(|a| content[a.byte_range()].to_string());
                            imports.push(Import {
                                module,
                                names: Vec::new(),
                                alias,
                                is_wildcard: false,
                                is_relative: false,
                                line,
                            });
                        }
                    }
                }
                imports
            }
            "import_from_statement" => {
                // from foo import bar, baz
                let module = node
                    .child_by_field_name("module_name")
                    .map(|m| content[m.byte_range()].to_string())
                    .unwrap_or_default();

                // Check for relative import (from . or from .. or from .foo)
                let text = &content[node.byte_range()];
                let is_relative = text.starts_with("from .");

                let mut names = Vec::new();
                let mut is_wildcard = false;
                let module_end = node
                    .child_by_field_name("module_name")
                    .map(|m| m.end_byte())
                    .unwrap_or(0);

                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "dotted_name" | "identifier" => {
                            // Skip the module name itself
                            if child.start_byte() > module_end {
                                names.push(content[child.byte_range()].to_string());
                            }
                        }
                        "aliased_import" => {
                            if let Some(name) = child.child_by_field_name("name") {
                                names.push(content[name.byte_range()].to_string());
                            }
                        }
                        "wildcard_import" => {
                            is_wildcard = true;
                        }
                        _ => {}
                    }
                }

                vec![Import {
                    module,
                    names,
                    alias: None,
                    is_wildcard,
                    is_relative,
                    line,
                }]
            }
            _ => Vec::new(),
        }
    }

    fn extract_exports(&self, node: &Node, content: &str) -> Vec<Export> {
        let line = node.start_position().row + 1;

        match node.kind() {
            "function_definition" | "async_function_definition" => {
                if let Some(name) = self.node_name(node, content) {
                    if !name.starts_with('_') {
                        return vec![Export {
                            name: name.to_string(),
                            kind: SymbolKind::Function,
                            line,
                        }];
                    }
                }
                Vec::new()
            }
            "class_definition" => {
                if let Some(name) = self.node_name(node, content) {
                    if !name.starts_with('_') {
                        return vec![Export {
                            name: name.to_string(),
                            kind: SymbolKind::Class,
                            line,
                        }];
                    }
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        if let Some(name) = self.node_name(node, content) {
            // Public if doesn't start with _ or is dunder method
            !name.starts_with('_') || name.starts_with("__")
        } else {
            true
        }
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        if let Some(name) = self.node_name(node, content) {
            if name.starts_with("__") && name.ends_with("__") {
                Visibility::Public // dunder methods
            } else if name.starts_with("__") {
                Visibility::Private // name mangled
            } else if name.starts_with('_') {
                Visibility::Protected // convention private
            } else {
                Visibility::Public
            }
        } else {
            Visibility::Public
        }
    }

    fn body_has_docstring(&self, body: &Node, content: &str) -> bool {
        let _ = content;
        body.child(0)
            .map(|c| {
                c.kind() == "string"
                    || (c.kind() == "expression_statement"
                        && c.child(0).map(|n| n.kind() == "string").unwrap_or(false))
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moss_core::Parsers;

    #[test]
    fn test_python_function_kinds() {
        let support = PythonSupport;
        assert!(support.function_kinds().contains(&"function_definition"));
        assert!(support.function_kinds().contains(&"async_function_definition"));
    }

    #[test]
    fn test_python_extract_function() {
        let support = PythonSupport;
        let parsers = Parsers::new();
        let content = r#"def foo(x: int) -> str:
    """Convert to string."""
    return str(x)
"#;
        let tree = parsers.parse_lang(Language::Python, content).unwrap();
        let root = tree.root_node();

        // Find function node
        let mut cursor = root.walk();
        let func = root.children(&mut cursor)
            .find(|n| n.kind() == "function_definition")
            .unwrap();

        let sym = support.extract_function(&func, content, false).unwrap();
        assert_eq!(sym.name, "foo");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert!(sym.signature.contains("def foo(x: int) -> str"));
        assert_eq!(sym.docstring, Some("Convert to string.".to_string()));
    }

    #[test]
    fn test_python_extract_class() {
        let support = PythonSupport;
        let parsers = Parsers::new();
        let content = r#"class Foo(Bar):
    """A foo class."""
    pass
"#;
        let tree = parsers.parse_lang(Language::Python, content).unwrap();
        let root = tree.root_node();

        let mut cursor = root.walk();
        let class = root.children(&mut cursor)
            .find(|n| n.kind() == "class_definition")
            .unwrap();

        let sym = support.extract_container(&class, content).unwrap();
        assert_eq!(sym.name, "Foo");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert!(sym.signature.contains("class Foo(Bar)"));
        assert_eq!(sym.docstring, Some("A foo class.".to_string()));
    }

    #[test]
    fn test_python_visibility() {
        let support = PythonSupport;
        let parsers = Parsers::new();
        let content = r#"def public(): pass
def _protected(): pass
def __private(): pass
def __dunder__(): pass
"#;
        let tree = parsers.parse_lang(Language::Python, content).unwrap();
        let root = tree.root_node();

        let mut cursor = root.walk();
        let funcs: Vec<_> = root.children(&mut cursor)
            .filter(|n| n.kind() == "function_definition")
            .collect();

        assert_eq!(support.get_visibility(&funcs[0], content), Visibility::Public);
        assert_eq!(support.get_visibility(&funcs[1], content), Visibility::Protected);
        assert_eq!(support.get_visibility(&funcs[2], content), Visibility::Private);
        assert_eq!(support.get_visibility(&funcs[3], content), Visibility::Public); // dunder
    }
}
