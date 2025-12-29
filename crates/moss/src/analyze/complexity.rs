//! Cyclomatic complexity analysis.
//!
//! Calculates McCabe cyclomatic complexity for functions.
//! Complexity = number of decision points + 1

use crate::parsers::Parsers;
use moss_languages::{support_for_path, Language};
use std::path::Path;
use tree_sitter;

/// Risk classification based on McCabe cyclomatic complexity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// 1-5: Simple, easy to test
    Low,
    /// 6-10: Manageable, may need review
    Moderate,
    /// 11-20: Complex, harder to test and maintain
    High,
    /// 21+: Should be refactored, often untestable
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Moderate => "moderate",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
        }
    }

    pub fn as_title(&self) -> &'static str {
        match self {
            RiskLevel::Low => "Low",
            RiskLevel::Moderate => "Moderate",
            RiskLevel::High => "High",
            RiskLevel::Critical => "Critical",
        }
    }
}

/// Complexity data for a function
#[derive(Debug, Clone)]
pub struct FunctionComplexity {
    pub name: String,
    pub complexity: usize,
    pub start_line: usize,
    #[allow(dead_code)] // Part of public API, may be used by consumers
    pub end_line: usize,
    pub parent: Option<String>,    // class/struct name for methods
    pub file_path: Option<String>, // file path for codebase-wide reports
}

impl FunctionComplexity {
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

    /// Line count of the function
    pub fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line) + 1
    }

    /// Risk classification based on McCabe cyclomatic complexity thresholds.
    ///
    /// Industry-standard ranges (similar to SonarQube, Code Climate):
    /// - 1-5: Low risk - simple, easy to test
    /// - 6-10: Moderate risk - still manageable, may need review
    /// - 11-20: High risk - complex, harder to test and maintain
    /// - 21+: Very high risk - should be refactored, often untestable
    ///
    /// McCabe's original paper (1976) suggested 10 as the upper limit.
    pub fn risk_level(&self) -> RiskLevel {
        match self.complexity {
            1..=5 => RiskLevel::Low,
            6..=10 => RiskLevel::Moderate,
            11..=20 => RiskLevel::High,
            _ => RiskLevel::Critical,
        }
    }
}

/// Complexity report for a file
pub type ComplexityReport = super::FileReport<FunctionComplexity>;

impl ComplexityReport {
    pub fn avg_complexity(&self) -> f64 {
        if self.functions.is_empty() {
            0.0
        } else {
            let total: usize = self.functions.iter().map(|f| f.complexity).sum();
            total as f64 / self.functions.len() as f64
        }
    }

    pub fn max_complexity(&self) -> usize {
        self.functions
            .iter()
            .map(|f| f.complexity)
            .max()
            .unwrap_or(0)
    }

    /// Count of high risk functions (complexity 11-20)
    pub fn high_risk_count(&self) -> usize {
        self.functions
            .iter()
            .filter(|f| f.risk_level() == RiskLevel::High)
            .count()
    }

    /// Count of critical risk functions (complexity 21+)
    pub fn critical_risk_count(&self) -> usize {
        self.functions
            .iter()
            .filter(|f| f.risk_level() == RiskLevel::Critical)
            .count()
    }
}

pub struct ComplexityAnalyzer {
    parsers: Parsers,
}

impl ComplexityAnalyzer {
    pub fn new() -> Self {
        Self {
            parsers: Parsers::new(),
        }
    }

    pub fn analyze(&self, path: &Path, content: &str) -> ComplexityReport {
        let functions = match support_for_path(path) {
            Some(support) => self.analyze_with_trait(content, support),
            None => Vec::new(),
        };

        ComplexityReport {
            functions,
            file_path: path.to_string_lossy().to_string(),
        }
    }

    /// Analyze using the Language trait
    fn analyze_with_trait(&self, content: &str, support: &dyn Language) -> Vec<FunctionComplexity> {
        let tree = match self
            .parsers
            .parse_with_grammar(support.grammar_name(), content)
        {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut functions = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_functions_with_trait(&mut cursor, content, support, &mut functions, None);
        functions
    }

    fn collect_functions_with_trait(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        support: &dyn Language,
        functions: &mut Vec<FunctionComplexity>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            // Check if this is a function
            if support.function_kinds().contains(&kind) {
                if let Some(name) = support.node_name(&node, content) {
                    let mut complexity = 1; // Base complexity
                    self.count_complexity_with_trait(&node, support, &mut complexity);

                    functions.push(FunctionComplexity {
                        name: name.to_string(),
                        complexity,
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        parent: parent.map(String::from),
                        file_path: None,
                    });
                }
            }
            // Check if this is a container (class, impl, module)
            else if support.container_kinds().contains(&kind) {
                if let Some(name) = support.node_name(&node, content) {
                    // Recurse into container with the container name as parent
                    if cursor.goto_first_child() {
                        self.collect_functions_with_trait(
                            cursor,
                            content,
                            support,
                            functions,
                            Some(name),
                        );
                        cursor.goto_parent();
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
            }

            // Recurse into other nodes
            if !support.container_kinds().contains(&kind) && cursor.goto_first_child() {
                self.collect_functions_with_trait(cursor, content, support, functions, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn count_complexity_with_trait(
        &self,
        node: &tree_sitter::Node,
        support: &dyn Language,
        complexity: &mut usize,
    ) {
        let complexity_nodes = support.complexity_nodes();
        let mut cursor = node.walk();

        if !cursor.goto_first_child() {
            return;
        }

        loop {
            let current = cursor.node();
            let kind = current.kind();

            // Count if this node type contributes to complexity
            if complexity_nodes.contains(&kind) {
                *complexity += 1;
            }

            // Depth-first traversal
            if cursor.goto_first_child() {
                continue;
            }

            if cursor.goto_next_sibling() {
                continue;
            }

            loop {
                if !cursor.goto_parent() {
                    return;
                }
                if cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_python_complexity() {
        let analyzer = ComplexityAnalyzer::new();
        let content = r#"
def simple():
    return 1

def with_if(x):
    if x > 0:
        return x
    else:
        return -x

def with_loop(items):
    total = 0
    for item in items:
        if item > 0:
            total += item
    return total
"#;
        let report = analyzer.analyze(&PathBuf::from("test.py"), content);

        let simple = report
            .functions
            .iter()
            .find(|f| f.name == "simple")
            .unwrap();
        assert_eq!(simple.complexity, 1);

        let with_if = report
            .functions
            .iter()
            .find(|f| f.name == "with_if")
            .unwrap();
        assert_eq!(with_if.complexity, 2); // 1 base + 1 if

        let with_loop = report
            .functions
            .iter()
            .find(|f| f.name == "with_loop")
            .unwrap();
        assert_eq!(with_loop.complexity, 3); // 1 base + 1 for + 1 if
    }

    #[test]
    fn test_rust_complexity() {
        let analyzer = ComplexityAnalyzer::new();
        let content = r#"
fn simple() -> i32 {
    1
}

fn with_if(x: i32) -> i32 {
    if x > 0 {
        x
    } else {
        -x
    }
}

fn with_match(x: Option<i32>) -> i32 {
    match x {
        Some(v) => v,
        None => 0,
    }
}
"#;
        let report = analyzer.analyze(&PathBuf::from("test.rs"), content);

        let simple = report
            .functions
            .iter()
            .find(|f| f.name == "simple")
            .unwrap();
        assert_eq!(simple.complexity, 1);

        let with_if = report
            .functions
            .iter()
            .find(|f| f.name == "with_if")
            .unwrap();
        assert!(
            with_if.complexity >= 2,
            "with_if should have complexity >= 2, got {}",
            with_if.complexity
        );

        let with_match = report
            .functions
            .iter()
            .find(|f| f.name == "with_match")
            .unwrap();
        assert!(
            with_match.complexity >= 1,
            "with_match should have complexity >= 1, got {}",
            with_match.complexity
        );
    }
}
