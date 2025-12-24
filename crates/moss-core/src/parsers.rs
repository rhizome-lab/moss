//! Tree-sitter parser initialization and management.

use crate::Language;
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

    /// Get the arborium language name for our Language enum.
    fn arborium_name(lang: Language) -> &'static str {
        match lang {
            Language::Python => "python",
            Language::Rust => "rust",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
            Language::Markdown => "markdown",
            Language::Json => "json",
            Language::Yaml => "yaml",
            Language::Html => "html",
            Language::Css => "css",
            Language::Go => "go",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Java => "java",
            Language::Ruby => "ruby",
            Language::Bash => "bash",
            Language::Toml => "toml",
            Language::Scala => "scala",
            Language::Vue => "vue",
        }
    }

    /// Create a parser for a specific language.
    pub fn parser_for(&self, lang: Language) -> Option<Parser> {
        let name = Self::arborium_name(lang);
        let grammar = self.store.get(name)?;
        let mut parser = Parser::new();
        parser.set_language(grammar.language()).ok()?;
        Some(parser)
    }

    /// Parse source code for a specific language.
    pub fn parse_lang(
        &self,
        lang: Language,
        source: &str,
    ) -> Option<arborium::tree_sitter::Tree> {
        let mut parser = self.parser_for(lang)?;
        parser.parse(source, None)
    }

    /// Parse source code, auto-detecting language from path.
    pub fn parse(
        &self,
        path: &std::path::Path,
        source: &str,
    ) -> Option<(Language, arborium::tree_sitter::Tree)> {
        let lang = Language::from_path(path)?;
        let mut parser = self.parser_for(lang)?;
        let tree = parser.parse(source, None)?;
        Some((lang, tree))
    }
}

impl Default for Parsers {
    fn default() -> Self {
        Self::new()
    }
}
