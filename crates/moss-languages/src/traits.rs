//! Core trait for language support.

use moss_core::{tree_sitter::Node, Language};

/// Symbol kind classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Module,
    Type,
    Constant,
    Variable,
    Heading,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Class => "class",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Interface => "interface",
            SymbolKind::Module => "module",
            SymbolKind::Type => "type",
            SymbolKind::Constant => "constant",
            SymbolKind::Variable => "variable",
            SymbolKind::Heading => "heading",
        }
    }
}

/// Symbol visibility
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Public,
    Private,
    Protected,
    Internal,
}

/// A code symbol extracted from source
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,
    pub docstring: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub visibility: Visibility,
    pub children: Vec<Symbol>,
}

/// An import statement
#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub names: Vec<String>,
    pub alias: Option<String>,
    pub is_wildcard: bool,
    pub is_relative: bool,
    pub line: usize,
}

/// An export declaration
#[derive(Debug, Clone)]
pub struct Export {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
}

/// Unified language support trait.
///
/// Each language implements this trait to provide:
/// - Node kind classification
/// - Symbol extraction (functions, classes, types)
/// - Import/export parsing
/// - Complexity analysis nodes
/// - Visibility detection
/// - Edit support (container bodies, docstrings)
pub trait LanguageSupport: Send + Sync {
    /// Which Language enum variant this implements
    fn language(&self) -> Language;

    /// Grammar name for arborium (e.g., "python", "rust")
    fn grammar_name(&self) -> &'static str;

    // === Node Classification ===

    /// Container nodes that can hold methods (class, impl, module)
    fn container_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    /// Function/method definition nodes
    fn function_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    /// Type definition nodes (struct, enum, interface, type alias)
    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    /// Import statement nodes
    fn import_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    /// Export statement nodes
    fn export_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    // === Symbol Extraction ===

    /// Extract symbol from a function/method node
    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol>;

    /// Extract symbol from a container node (class, impl, module)
    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol>;

    /// Extract symbol from a type definition node
    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        // Default: types are often containers too
        self.extract_container(node, content)
    }

    /// Extract docstring/doc comment for a node
    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        let _ = (node, content);
        None
    }

    // === Import/Export ===

    /// Extract imports from an import node (may return multiple)
    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        let _ = (node, content);
        Vec::new()
    }

    /// Extract exports from an export/definition node (may return multiple)
    fn extract_exports(&self, node: &Node, content: &str) -> Vec<Export> {
        let _ = (node, content);
        Vec::new()
    }

    // === Scope Analysis ===

    /// Nodes that create new variable scopes (for scope analysis)
    /// Includes: loops, blocks, comprehensions, lambdas, with statements
    /// Note: Functions and containers (from function_kinds/container_kinds) also create scopes
    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    // === Complexity ===

    /// Nodes that increase cyclomatic complexity
    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[]
    }

    /// Nodes that indicate nesting depth
    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[]
    }

    // === Visibility ===

    /// Check if a node is public/exported
    fn is_public(&self, node: &Node, content: &str) -> bool {
        let _ = (node, content);
        true
    }

    /// Get visibility of a node
    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        if self.is_public(node, content) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }

    // === Edit Support ===

    /// Find the body node of a container (for prepend/append)
    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }

    /// Detect if first child of body is a docstring
    fn body_has_docstring(&self, body: &Node, content: &str) -> bool {
        let _ = (body, content);
        false
    }

    // === Helpers ===

    /// Get the name of a node (typically via "name" field)
    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        let name_node = node.child_by_field_name("name")?;
        Some(&content[name_node.byte_range()])
    }
}
