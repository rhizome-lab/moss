//! Tree-sitter parser initialization and management.

use arborium::tree_sitter::Parser;
use arborium::GrammarStore;
use std::sync::Arc;

/// Collection of tree-sitter parsers using arborium's grammar store.
pub struct Parsers {
    store: Arc<GrammarStore>,
}

impl Parsers {
    /// Create new parser collection with arborium's grammar store.
    pub fn new() -> Self {
        Self {
            store: Arc::new(GrammarStore::new()),
        }
    }

    /// Create a parser for a specific grammar.
    ///
    /// The grammar name should match arborium's grammar names (e.g., "python", "rust", "typescript").
    pub fn parser_for(&self, grammar: &str) -> Option<Parser> {
        let grammar = self.store.get(grammar)?;
        let mut parser = Parser::new();
        parser.set_language(grammar.language()).ok()?;
        Some(parser)
    }

    /// Parse source code with a specific grammar.
    ///
    /// The grammar name should match arborium's grammar names (e.g., "python", "rust", "typescript").
    pub fn parse_with_grammar(
        &self,
        grammar: &str,
        source: &str,
    ) -> Option<arborium::tree_sitter::Tree> {
        let mut parser = self.parser_for(grammar)?;
        parser.parse(source, None)
    }
}

impl Default for Parsers {
    fn default() -> Self {
        Self::new()
    }
}
