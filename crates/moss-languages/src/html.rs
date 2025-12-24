//! HTML language support (parse only, minimal skeleton).

use crate::{LanguageSupport, Symbol};
use moss_core::{tree_sitter::Node, Language};

pub struct HtmlSupport;

impl LanguageSupport for HtmlSupport {
    fn language(&self) -> Language { Language::Html }
    fn grammar_name(&self) -> &'static str { "html" }

    fn extract_function(&self, _node: &Node, _content: &str, _in_container: bool) -> Option<Symbol> {
        None
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }
}
