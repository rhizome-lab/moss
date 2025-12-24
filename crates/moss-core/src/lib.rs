//! Shared types and utilities for moss crates.

mod language;
mod parsers;
mod paths;

pub use language::Language;
pub use parsers::Parsers;
pub use paths::get_moss_dir;

// Re-export arborium and its tree-sitter for use in other modules
pub use arborium;
pub use arborium::tree_sitter;

/// Symbol kind in source code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
    Method,
    Variable,
    Import,
    Struct,
    Enum,
    Trait,
    Interface,
    Constant,
    Module,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Class => "class",
            SymbolKind::Method => "method",
            SymbolKind::Variable => "variable",
            SymbolKind::Import => "import",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Interface => "interface",
            SymbolKind::Constant => "constant",
            SymbolKind::Module => "module",
        }
    }
}
