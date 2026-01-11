//! Function length analysis.
//!
//! Identifies long functions that may be candidates for refactoring.
use crate::parsers;
use rhizome_moss_languages::{Language, support_for_path};
use serde::Serialize;
use std::path::Path;
/// Length classification for functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthCategory {
    /// 1-20 lines: concise
    Short,
    /// 21-50 lines: reasonable
    Medium,
    /// 51-100 lines: getting long
    Long,
    /// 100+ lines: should be split
    TooLong,
}
impl LengthCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            LengthCategory::Short => "short",
            LengthCategory::Medium => "medium",
            LengthCategory::Long => "long",
            LengthCategory::TooLong => "too-long",
        }
    }
    pub fn as_title(&self) -> &'static str {
        match self {
            LengthCategory::Short => "Short",
            LengthCategory::Medium => "Medium",
            LengthCategory::Long => "Long",
            LengthCategory::TooLong => "Too Long",
        }
    }
}
/// Function length data.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionLength {
    pub name: String,
    pub lines: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub parent: Option<String>,
    pub file_path: Option<String>,
}
impl FunctionLength {
    pub fn qualified_name(&self) -> String {
        let base = if let Some(parent) = &self.parent {
            format!("{}.{}", parent, self.name)
        } else {
            self.name.clone()
        };
        if let Some(fp) = &self.file_path {
            format!("{}:{}", fp, base)
        } else {
            base
        }
    }
    pub fn short_name(&self) -> String {
        if let Some(parent) = &self.parent {
            format!("{}.{}", parent, self.name)
        } else {
            self.name.clone()
        }
    }
    pub fn category(&self) -> LengthCategory {
        match self.lines {
            1..=20 => LengthCategory::Short,
            21..=50 => LengthCategory::Medium,
            51..=100 => LengthCategory::Long,
            _ => LengthCategory::TooLong,
        }
    }
}
/// Length report for a file.
pub type LengthReport = super::FileReport<FunctionLength>;
impl LengthReport {
    pub fn avg_length(&self) -> f64 {
        if self.functions.is_empty() {
            0.0
        } else {
            let total: usize = self.functions.iter().map(|f| f.lines).sum();
            total as f64 / self.functions.len() as f64
        }
    }
    pub fn max_length(&self) -> usize {
        self.functions.iter().map(|f| f.lines).max().unwrap_or(0)
    }
    pub fn long_count(&self) -> usize {
        self.functions
            .iter()
            .filter(|f| f.category() == LengthCategory::Long)
            .count()
    }
    pub fn too_long_count(&self) -> usize {
        self.functions
            .iter()
            .filter(|f| f.category() == LengthCategory::TooLong)
            .count()
    }
}
pub struct LengthAnalyzer {}
impl LengthAnalyzer {
    pub fn new() -> Self {
        Self {}
    }
    pub fn analyze(&self, path: &Path, content: &str) -> LengthReport {
        let functions = match support_for_path(path) {
            Some(support) => self.analyze_with_trait(content, support),
            None => Vec::new(),
        };
        LengthReport {
            functions,
            file_path: path.to_string_lossy().to_string(),
        }
    }
    fn analyze_with_trait(&self, content: &str, support: &dyn Language) -> Vec<FunctionLength> {
        let tree = match parsers::parse_with_grammar(support.grammar_name(), content) {
            Some(t) => t,
            None => return Vec::new(),
        };
        let mut functions = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        self.collect_functions(&mut cursor, content, support, &mut functions, None);
        functions
    }
    fn collect_functions(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        support: &dyn Language,
        functions: &mut Vec<FunctionLength>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();
            // Check if this is a function
            if support.function_kinds().contains(&kind) {
                if let Some(name) = support.node_name(&node, content) {
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let lines = end_line.saturating_sub(start_line) + 1;
                    functions.push(FunctionLength {
                        name: name.to_string(),
                        lines,
                        start_line,
                        end_line,
                        parent: parent.map(String::from),
                        file_path: None,
                    });
                }
            }
            // Check for container (class, impl, module) holding methods
            let new_parent = if support.container_kinds().contains(&kind) {
                support.node_name(&node, content).map(|s| s.to_string())
            } else {
                parent.map(String::from)
            };
            // Recurse into children
            if cursor.goto_first_child() {
                self.collect_functions(cursor, content, support, functions, new_parent.as_deref());
                cursor.goto_parent();
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
impl Default for LengthAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
