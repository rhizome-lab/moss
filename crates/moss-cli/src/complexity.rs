//! Cyclomatic complexity analysis.
//!
//! Calculates McCabe cyclomatic complexity for functions.
//! Complexity = number of decision points + 1

use moss_core::{tree_sitter, Language, Parsers};
use std::path::Path;

/// Complexity data for a function
#[derive(Debug, Clone)]
pub struct FunctionComplexity {
    pub name: String,
    pub complexity: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub parent: Option<String>, // class/struct name for methods
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

    /// Risk classification based on McCabe cyclomatic complexity thresholds.
    ///
    /// Industry-standard ranges (similar to SonarQube, Code Climate):
    /// - 1-5: Low risk - simple, easy to test
    /// - 6-10: Moderate risk - still manageable, may need review
    /// - 11-20: High risk - complex, harder to test and maintain
    /// - 21+: Very high risk - should be refactored, often untestable
    ///
    /// McCabe's original paper (1976) suggested 10 as the upper limit.
    pub fn risk_level(&self) -> &'static str {
        match self.complexity {
            1..=5 => "low",
            6..=10 => "moderate",
            11..=20 => "high",
            _ => "very-high",
        }
    }
}

/// Complexity report for a file
#[derive(Debug)]
pub struct ComplexityReport {
    pub functions: Vec<FunctionComplexity>,
    pub file_path: String,
}

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

    pub fn high_risk_count(&self) -> usize {
        self.functions.iter().filter(|f| f.complexity > 10).count()
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
        let lang = Language::from_path(path);
        let functions = match lang {
            Some(Language::Python) => self.analyze_python(content),
            Some(Language::Rust) => self.analyze_rust(content),
            _ => Vec::new(),
        };

        ComplexityReport {
            functions,
            file_path: path.to_string_lossy().to_string(),
        }
    }

    fn analyze_python(&self, content: &str) -> Vec<FunctionComplexity> {
        let tree = match self.parsers.parse_lang(Language::Python, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut functions = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_python_functions(&mut cursor, content, &mut functions, None);
        functions
    }

    fn collect_python_functions(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        functions: &mut Vec<FunctionComplexity>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_definition" | "async_function_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();

                        // Calculate complexity
                        let mut complexity = 1; // Base complexity
                        self.count_python_complexity(&node, &mut complexity);

                        functions.push(FunctionComplexity {
                            name,
                            complexity,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                            file_path: None,
                        });
                    }
                }
                "class_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let class_name = content[name_node.byte_range()].to_string();

                        // Recurse into class body
                        if cursor.goto_first_child() {
                            self.collect_python_functions(
                                cursor,
                                content,
                                functions,
                                Some(&class_name),
                            );
                            cursor.goto_parent();
                        }
                        if cursor.goto_next_sibling() {
                            continue;
                        }
                        break;
                    }
                }
                _ => {}
            }

            // Recurse
            if kind != "class_definition" && cursor.goto_first_child() {
                self.collect_python_functions(cursor, content, functions, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn count_python_complexity(&self, node: &tree_sitter::Node, complexity: &mut usize) {
        // Traverse all descendants
        let mut cursor = node.walk();

        // Start at first child if any
        if !cursor.goto_first_child() {
            return;
        }

        loop {
            let current = cursor.node();
            let kind = current.kind();

            // Count decision points
            match kind {
                "if_statement" | "elif_clause" => *complexity += 1,
                "for_statement" | "while_statement" => *complexity += 1,
                "except_clause" => *complexity += 1,
                "with_statement" => *complexity += 1,
                "assert_statement" => *complexity += 1,
                "conditional_expression" => *complexity += 1, // ternary
                "boolean_operator" => *complexity += 1,       // and/or
                "if_clause" => *complexity += 1,              // comprehension conditions
                _ => {}
            }

            // Depth-first: try to go to first child
            if cursor.goto_first_child() {
                continue;
            }

            // No children, try next sibling
            if cursor.goto_next_sibling() {
                continue;
            }

            // Go back up until we find an unvisited sibling
            loop {
                if !cursor.goto_parent() {
                    return; // Done
                }
                if cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    fn analyze_rust(&self, content: &str) -> Vec<FunctionComplexity> {
        let tree = match self.parsers.parse_lang(Language::Rust, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut functions = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_rust_functions(&mut cursor, content, &mut functions, None);
        functions
    }

    fn collect_rust_functions(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        functions: &mut Vec<FunctionComplexity>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_item" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();

                        // Calculate complexity
                        let mut complexity = 1;
                        self.count_rust_complexity(&node, &mut complexity);

                        functions.push(FunctionComplexity {
                            name,
                            complexity,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                            file_path: None,
                        });
                    }
                }
                "impl_item" => {
                    // Get the type being implemented
                    if let Some(type_node) = node.child_by_field_name("type") {
                        let impl_name = content[type_node.byte_range()].to_string();

                        // Recurse into impl body
                        if let Some(body) = node.child_by_field_name("body") {
                            let mut body_cursor = body.walk();
                            if body_cursor.goto_first_child() {
                                self.collect_rust_functions(
                                    &mut body_cursor,
                                    content,
                                    functions,
                                    Some(&impl_name),
                                );
                            }
                        }
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                _ => {}
            }

            // Recurse
            if kind != "impl_item" && cursor.goto_first_child() {
                self.collect_rust_functions(cursor, content, functions, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn count_rust_complexity(&self, node: &tree_sitter::Node, complexity: &mut usize) {
        // Traverse all descendants
        let mut cursor = node.walk();

        // Start at first child if any
        if !cursor.goto_first_child() {
            return;
        }

        loop {
            let current = cursor.node();
            let kind = current.kind();

            // Count decision points
            match kind {
                "if_expression" => *complexity += 1,
                "for_expression" | "while_expression" | "loop_expression" => *complexity += 1,
                "match_expression" => {
                    // Count match arms (minus 1 for base case)
                    let mut arm_count = 0;
                    for i in 0..current.child_count() {
                        if let Some(child) = current.child(i) {
                            if child.kind() == "match_arm" {
                                arm_count += 1;
                            }
                        }
                    }
                    if arm_count > 1 {
                        *complexity += arm_count - 1;
                    }
                }
                "binary_expression" => {
                    // Check for && or ||
                    for i in 0..current.child_count() {
                        if let Some(child) = current.child(i) {
                            if child.kind() == "&&" || child.kind() == "||" {
                                *complexity += 1;
                            }
                        }
                    }
                }
                "try_expression" => *complexity += 1, // ? operator (Result/Option handling)
                _ => {}
            }

            // Depth-first: try to go to first child
            if cursor.goto_first_child() {
                continue;
            }

            // No children, try next sibling
            if cursor.goto_next_sibling() {
                continue;
            }

            // Go back up until we find an unvisited sibling
            loop {
                if !cursor.goto_parent() {
                    return; // Done
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
        let mut analyzer = ComplexityAnalyzer::new();
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
        let mut analyzer = ComplexityAnalyzer::new();
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
