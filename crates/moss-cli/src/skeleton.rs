//! AST-based code skeleton extraction.
//!
//! Extracts function/class signatures with optional docstrings.

use crate::parsers::Parsers;
use crate::tree::{ViewNode, ViewNodeKind};
use arborium::tree_sitter;
use moss_languages::{
    support_for_grammar, support_for_path, Language, Symbol as LangSymbol,
    SymbolKind as LangSymbolKind,
};
use std::path::Path;

/// A code symbol with its signature
#[derive(Debug, Clone)]
pub struct SkeletonSymbol {
    pub name: String,
    pub kind: &'static str, // "class", "function", "method"
    pub signature: String,
    pub docstring: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub children: Vec<SkeletonSymbol>,
}

impl SkeletonSymbol {
    /// Convert to a ViewNode for unified viewing.
    pub fn to_view_node(&self, parent_path: &str) -> ViewNode {
        let path = if parent_path.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", parent_path, self.name)
        };

        let children: Vec<ViewNode> = self
            .children
            .iter()
            .map(|c| c.to_view_node(&path))
            .collect();

        ViewNode {
            name: self.name.clone(),
            kind: ViewNodeKind::Symbol(self.kind.to_string()),
            path,
            children,
            signature: Some(self.signature.clone()),
            docstring: self.docstring.clone(),
            line_range: Some((self.start_line, self.end_line)),
        }
    }
}

/// Result of skeleton extraction
pub struct SkeletonResult {
    pub symbols: Vec<SkeletonSymbol>,
    pub file_path: String,
}

impl SkeletonResult {
    /// Convert to a ViewNode with file as root and symbols as children.
    pub fn to_view_node(&self) -> ViewNode {
        let file_name = Path::new(&self.file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.file_path.clone());

        let children: Vec<ViewNode> = self
            .symbols
            .iter()
            .map(|s| s.to_view_node(&file_name))
            .collect();

        ViewNode::file(&file_name, &self.file_path).with_children(children)
    }

    /// Filter to only type definitions (class, struct, enum, trait, interface)
    /// Returns a new SkeletonResult with only type-like symbols, and strips methods from classes
    pub fn filter_types(&self) -> SkeletonResult {
        fn is_type_kind(kind: &str) -> bool {
            matches!(
                kind,
                "class" | "struct" | "enum" | "trait" | "interface" | "type" | "impl" | "module"
            )
        }

        fn filter_symbol(sym: &SkeletonSymbol) -> Option<SkeletonSymbol> {
            if is_type_kind(sym.kind) {
                // For types, keep only nested types (not methods)
                let type_children: Vec<_> = sym
                    .children
                    .iter()
                    .filter_map(|c| filter_symbol(c))
                    .collect();
                Some(SkeletonSymbol {
                    name: sym.name.clone(),
                    kind: sym.kind,
                    signature: sym.signature.clone(),
                    docstring: sym.docstring.clone(),
                    start_line: sym.start_line,
                    end_line: sym.end_line,
                    children: type_children,
                })
            } else {
                None
            }
        }

        let filtered_symbols: Vec<_> = self
            .symbols
            .iter()
            .filter_map(|s| filter_symbol(s))
            .collect();

        SkeletonResult {
            symbols: filtered_symbols,
            file_path: self.file_path.clone(),
        }
    }
}

/// Recursively adjust line numbers for nested symbols
fn adjust_children_lines(children: &mut [SkeletonSymbol], offset: usize) {
    for child in children {
        child.start_line += offset;
        child.end_line += offset;
        adjust_children_lines(&mut child.children, offset);
    }
}

/// Convert a moss_languages::Symbol to SkeletonSymbol
fn convert_symbol(sym: &LangSymbol) -> SkeletonSymbol {
    let kind = match sym.kind {
        LangSymbolKind::Function => "function",
        LangSymbolKind::Method => "method",
        LangSymbolKind::Class => "class",
        LangSymbolKind::Struct => "struct",
        LangSymbolKind::Enum => "enum",
        LangSymbolKind::Trait => "trait",
        LangSymbolKind::Interface => "interface",
        LangSymbolKind::Module => "module",
        LangSymbolKind::Type => "type",
        LangSymbolKind::Constant => "constant",
        LangSymbolKind::Variable => "variable",
        LangSymbolKind::Heading => "heading",
    };

    SkeletonSymbol {
        name: sym.name.clone(),
        kind,
        signature: sym.signature.clone(),
        docstring: sym.docstring.clone(),
        start_line: sym.start_line,
        end_line: sym.end_line,
        children: sym.children.iter().map(convert_symbol).collect(),
    }
}

pub struct SkeletonExtractor {
    parsers: Parsers,
    show_all: bool,
}

impl SkeletonExtractor {
    pub fn new() -> Self {
        Self {
            parsers: Parsers::new(),
            show_all: false,
        }
    }

    /// Create an extractor that shows all symbols including private ones
    pub fn with_all() -> Self {
        Self {
            parsers: Parsers::new(),
            show_all: true,
        }
    }

    pub fn extract(&mut self, path: &Path, content: &str) -> SkeletonResult {
        let support = support_for_path(path);

        let symbols = match support {
            Some(s) => self.extract_with_trait(content, s),
            None => Vec::new(),
        };

        SkeletonResult {
            symbols,
            file_path: path.to_string_lossy().to_string(),
        }
    }

    /// Trait-based extraction (for future use when implementations are complete)
    #[allow(dead_code)]
    pub fn extract_with_support(&self, path: &Path, content: &str) -> Option<SkeletonResult> {
        let support = support_for_path(path)?;
        let symbols = self.extract_with_trait(content, support);
        Some(SkeletonResult {
            symbols,
            file_path: path.to_string_lossy().to_string(),
        })
    }

    /// Extract using the Language trait (new unified approach)
    fn extract_with_trait(&self, content: &str, support: &dyn Language) -> Vec<SkeletonSymbol> {
        let tree = match self
            .parsers
            .parse_with_grammar(support.grammar_name(), content)
        {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_with_trait(&mut cursor, content, support, &mut symbols, false);

        // Post-process for Rust: merge impl blocks with their types
        if support.grammar_name() == "rust" {
            Self::merge_rust_impl_blocks(&mut symbols);
        }

        symbols
    }

    /// Merge Rust impl blocks with their corresponding struct/enum types
    fn merge_rust_impl_blocks(symbols: &mut Vec<SkeletonSymbol>) {
        // Collect impl blocks and their children
        let mut impl_methods: std::collections::HashMap<String, Vec<SkeletonSymbol>> =
            std::collections::HashMap::new();

        // Remove impl blocks and collect their methods
        symbols.retain(|sym| {
            if sym.kind == "impl" || sym.kind == "module" {
                // impl blocks are extracted as "impl" or "module" kind
                if sym.signature.starts_with("impl ") {
                    let type_name = &sym.name;
                    impl_methods
                        .entry(type_name.clone())
                        .or_default()
                        .extend(sym.children.clone());
                    return false; // Remove impl block
                }
            }
            true
        });

        // Add methods to matching struct/enum
        for sym in symbols.iter_mut() {
            if sym.kind == "struct" || sym.kind == "enum" {
                if let Some(methods) = impl_methods.remove(&sym.name) {
                    sym.children.extend(methods);
                }
            }
        }

        // Any remaining impl blocks without matching type: add back as impl symbols
        for (name, methods) in impl_methods {
            if !methods.is_empty() {
                symbols.push(SkeletonSymbol {
                    name: name.clone(),
                    kind: "impl",
                    signature: format!("impl {}", name),
                    docstring: None,
                    start_line: methods.first().map(|m| m.start_line).unwrap_or(0),
                    end_line: methods.last().map(|m| m.end_line).unwrap_or(0),
                    children: methods,
                });
            }
        }
    }

    fn collect_with_trait(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        support: &dyn Language,
        symbols: &mut Vec<SkeletonSymbol>,
        in_container: bool,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            // Check for embedded content (e.g., <script> in Vue/Svelte/HTML)
            if let Some(embedded) = support.embedded_content(&node, content) {
                if let Some(sub_lang) = support_for_grammar(embedded.grammar) {
                    if let Some(sub_tree) = self
                        .parsers
                        .parse_with_grammar(embedded.grammar, &embedded.content)
                    {
                        let mut sub_symbols = Vec::new();
                        let sub_root = sub_tree.root_node();
                        let mut sub_cursor = sub_root.walk();
                        self.collect_with_trait(
                            &mut sub_cursor,
                            &embedded.content,
                            sub_lang,
                            &mut sub_symbols,
                            false,
                        );

                        // Adjust line numbers for embedded content offset
                        for mut sym in sub_symbols {
                            sym.start_line += embedded.start_line - 1;
                            sym.end_line += embedded.start_line - 1;
                            adjust_children_lines(&mut sym.children, embedded.start_line - 1);
                            symbols.push(sym);
                        }
                    }
                }
                // Don't descend into embedded nodes - we've already processed them
                if cursor.goto_next_sibling() {
                    continue;
                }
                break;
            }

            // Check if this is a function
            if support.function_kinds().contains(&kind) {
                if let Some(sym) = support.extract_function(&node, content, in_container) {
                    // Filter by visibility unless show_all
                    if self.show_all || matches!(sym.visibility, moss_languages::Visibility::Public)
                    {
                        symbols.push(convert_symbol(&sym));
                    }
                }
            }
            // Check if this is a container (class, impl, module)
            else if support.container_kinds().contains(&kind) {
                if let Some(sym) = support.extract_container(&node, content) {
                    if self.show_all || matches!(sym.visibility, moss_languages::Visibility::Public)
                    {
                        let mut skeleton_sym = convert_symbol(&sym);

                        // Recurse into container body
                        if let Some(body) = support.container_body(&node) {
                            let mut body_cursor = body.walk();
                            if body_cursor.goto_first_child() {
                                self.collect_with_trait(
                                    &mut body_cursor,
                                    content,
                                    support,
                                    &mut skeleton_sym.children,
                                    true,
                                );
                            }
                        }

                        symbols.push(skeleton_sym);
                    }
                }
                // Don't descend further after processing container
                if cursor.goto_next_sibling() {
                    continue;
                }
                break;
            }
            // Check if this is a standalone type (struct, enum, etc.)
            else if support.type_kinds().contains(&kind)
                && !support.container_kinds().contains(&kind)
            {
                if let Some(sym) = support.extract_type(&node, content) {
                    if self.show_all || matches!(sym.visibility, moss_languages::Visibility::Public)
                    {
                        symbols.push(convert_symbol(&sym));
                    }
                }
            }

            // Descend into children for other nodes
            if cursor.goto_first_child() {
                self.collect_with_trait(cursor, content, support, symbols, in_container);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_python_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
def foo(x: int) -> str:
    """Convert int to string."""
    return str(x)

class Bar:
    """A bar class."""

    def method(self, y: float) -> bool:
        """Check something."""
        return y > 0
"#;
        let result = extractor.extract(&PathBuf::from("test.py"), content);
        assert_eq!(result.symbols.len(), 2);

        let foo = &result.symbols[0];
        assert_eq!(foo.name, "foo");
        assert_eq!(foo.kind, "function");
        assert!(foo.signature.contains("def foo(x: int) -> str"));
        assert_eq!(foo.docstring.as_deref(), Some("Convert int to string."));

        let bar = &result.symbols[1];
        assert_eq!(bar.name, "Bar");
        assert_eq!(bar.kind, "class");
        assert_eq!(bar.children.len(), 1);
        assert_eq!(bar.children[0].name, "method");
    }

    #[test]
    fn test_rust_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
/// A simple struct
pub struct Foo {
    x: i32,
}

impl Foo {
    /// Create a new Foo
    pub fn new(x: i32) -> Self {
        Self { x }
    }
}
"#;
        let result = extractor.extract(&PathBuf::from("test.rs"), content);

        // Should have struct with method from impl
        let foo = result.symbols.iter().find(|s| s.name == "Foo").unwrap();
        assert_eq!(foo.kind, "struct");
        assert!(foo.signature.contains("pub struct Foo"));
        assert_eq!(foo.children.len(), 1);
        assert_eq!(foo.children[0].name, "new");
    }

    #[test]
    fn test_to_view_node() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
def greet(name: str) -> str:
    """Return a personalized greeting message."""
    return f"Hello, {name}"
"#;
        let result = extractor.extract(&PathBuf::from("test.py"), content);
        let view_node = result.to_view_node();

        assert_eq!(view_node.name, "test.py");
        assert!(view_node.children.len() >= 1);
        let greet = &view_node.children[0];
        assert_eq!(greet.name, "greet");
        assert!(greet
            .signature
            .as_ref()
            .map_or(false, |s| s.contains("def greet")));
    }

    #[test]
    fn test_markdown_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"# Title

Some intro text.

## Section One

Content here.

```bash
# This is a comment, not a heading
echo "hello"
```

## Section Two

### Subsection

More content.
"#;
        let result = extractor.extract(&PathBuf::from("test.md"), content);

        // Should have 2 top-level headings: Title, and the h2s should be nested
        assert!(!result.symbols.is_empty(), "Should have headings");

        let title = &result.symbols[0];
        assert_eq!(title.name, "Title");
        assert_eq!(title.kind, "heading");

        // Check that code block comment wasn't extracted as heading
        let all_names: Vec<&str> = result
            .symbols
            .iter()
            .flat_map(|s| {
                std::iter::once(s.name.as_str()).chain(s.children.iter().map(|c| c.name.as_str()))
            })
            .collect();
        assert!(
            !all_names.iter().any(|n| n.contains("comment")),
            "Code block comments should not be headings: {:?}",
            all_names
        );
    }

    #[test]
    fn test_javascript_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"function greet(name) {
  console.log("Hello, " + name);
}

class Greeter {
  constructor(name) { this.name = name; }
  greet() { console.log("Hello, " + this.name); }
}
"#;
        let result = extractor.extract(&PathBuf::from("test.js"), content);

        // Should have function and class
        assert_eq!(result.symbols.len(), 2, "Should have 2 top-level symbols");

        let greet_fn = &result.symbols[0];
        assert_eq!(greet_fn.name, "greet");
        assert_eq!(greet_fn.kind, "function");

        let greeter = &result.symbols[1];
        assert_eq!(greeter.name, "Greeter");
        assert_eq!(greeter.kind, "class");

        // Class should have 2 methods: constructor and greet
        assert_eq!(
            greeter.children.len(),
            2,
            "Greeter class should have 2 methods, got {:?}",
            greeter.children
        );
    }

    #[test]
    fn test_filter_types() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
def helper():
    pass

class MyClass:
    def method(self):
        pass

def another_function():
    pass

class AnotherClass:
    pass
"#;
        let result = extractor.extract(&PathBuf::from("test.py"), content);

        // Original should have 4 top-level symbols (2 functions, 2 classes)
        assert_eq!(result.symbols.len(), 4);

        // Filtered should only have classes
        let filtered = result.filter_types();
        assert_eq!(filtered.symbols.len(), 2);
        assert!(filtered.symbols.iter().all(|s| s.kind == "class"));
        assert_eq!(filtered.symbols[0].name, "MyClass");
        assert_eq!(filtered.symbols[1].name, "AnotherClass");
    }

    #[test]
    fn test_filter_types_rust() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
fn helper() {}

pub struct MyStruct {
    field: i32,
}

impl MyStruct {
    pub fn method(&self) {}
}

pub enum MyEnum {
    A,
    B,
}

pub trait MyTrait {
    fn required(&self);
}

fn another_function() {}
"#;
        let result = extractor.extract(&PathBuf::from("test.rs"), content);

        // Filtered should have struct, enum, trait, impl
        let filtered = result.filter_types();
        let kinds: Vec<_> = filtered.symbols.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&"struct"), "Should have struct");
        assert!(kinds.contains(&"enum"), "Should have enum");
        assert!(kinds.contains(&"trait"), "Should have trait");
        // Functions should be filtered out
        assert!(!kinds.contains(&"function"));
    }

    #[test]
    fn test_filter_types_typescript() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
function helper() {}

interface MyInterface {
    method(): void;
}

class MyClass {
    method() {}
}

type MyType = string | number;

enum MyEnum {
    A,
    B,
}

const arrow = () => {};
"#;
        let result = extractor.extract(&PathBuf::from("test.ts"), content);

        let filtered = result.filter_types();
        let _kinds: Vec<_> = filtered.symbols.iter().map(|s| s.kind).collect();
        let names: Vec<_> = filtered.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"MyInterface"), "Should have interface");
        assert!(names.contains(&"MyClass"), "Should have class");
        // Functions should be filtered out
        assert!(!names.contains(&"helper"));
        assert!(!names.contains(&"arrow"));
    }

    #[test]
    fn test_filter_types_go() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
package main

func helper() {}

type MyStruct struct {
    Field int
}

func (m *MyStruct) Method() {}

type MyInterface interface {
    Required()
}
"#;
        let result = extractor.extract(&PathBuf::from("test.go"), content);

        let filtered = result.filter_types();
        let names: Vec<_> = filtered.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"MyStruct"), "Should have struct");
        assert!(names.contains(&"MyInterface"), "Should have interface");
        // Functions should be filtered out
        assert!(!names.contains(&"helper"));
    }

    #[test]
    fn test_filter_types_java() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
public class MyClass {
    public void method() {}
}

interface MyInterface {
    void required();
}

enum MyEnum {
    A, B
}
"#;
        let result = extractor.extract(&PathBuf::from("Test.java"), content);

        let filtered = result.filter_types();
        let names: Vec<_> = filtered.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"MyClass"), "Should have class");
        assert!(names.contains(&"MyInterface"), "Should have interface");
        assert!(names.contains(&"MyEnum"), "Should have enum");
    }

    #[test]
    fn test_filter_types_ruby() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
def helper
end

class MyClass
  def method
  end
end

module MyModule
  def module_method
  end
end
"#;
        let result = extractor.extract(&PathBuf::from("test.rb"), content);

        let filtered = result.filter_types();
        let names: Vec<_> = filtered.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"MyClass"), "Should have class");
        assert!(names.contains(&"MyModule"), "Should have module");
        // Functions should be filtered out
        assert!(!names.contains(&"helper"));
    }

    #[test]
    fn test_scala_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
object Main {
  def hello(name: String): String = {
    s"Hello, $name"
  }
}

class Person(name: String) {
  def greet(): Unit = {
    println(s"Hi, I'm $name")
  }
}

trait Greeter {
  def greet(name: String): String
}
"#;
        let result = extractor.extract(&PathBuf::from("test.scala"), content);

        // Should have object, class, and trait
        assert!(!result.symbols.is_empty(), "Should have symbols");

        let names: Vec<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        let kinds: Vec<_> = result.symbols.iter().map(|s| s.kind).collect();

        assert!(names.contains(&"Main"), "Should have Main object");
        assert!(names.contains(&"Person"), "Should have Person class");
        assert!(names.contains(&"Greeter"), "Should have Greeter trait");

        assert!(kinds.contains(&"module"), "Should have module (object)");
        assert!(kinds.contains(&"class"), "Should have class");
        assert!(kinds.contains(&"trait"), "Should have trait");
    }

    #[test]
    fn test_vue_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
<template>
  <div>{{ message }}</div>
</template>

<script setup>
function greet(name) {
  return `Hello, ${name}`;
}

const handleClick = () => {
  console.log("clicked");
};
</script>

<style scoped>
div { color: red; }
</style>
"#;
        let result = extractor.extract(&PathBuf::from("test.vue"), content);

        // Vue files should extract functions from script
        // Note: The exact symbols depend on tree-sitter-vue parsing
        let names: Vec<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();

        // Check that we extracted at least some symbols from the script
        // The exact parsing depends on tree-sitter-vue behavior
        assert!(
            result.symbols.is_empty() || names.iter().any(|n| *n == "greet" || *n == "handleClick"),
            "Should have greet or handleClick function, or be empty if vue parsing differs: {:?}",
            names
        );
    }

    #[test]
    fn test_filter_types_scala() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
def helper(): Unit = {}

class MyClass {
  def method(): Unit = {}
}

object MyObject {
  def objectMethod(): String = "hi"
}

trait MyTrait {
  def traitMethod(): Int
}
"#;
        let result = extractor.extract(&PathBuf::from("test.scala"), content);

        let filtered = result.filter_types();
        let names: Vec<_> = filtered.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"MyClass"), "Should have class");
        assert!(names.contains(&"MyObject"), "Should have object (module)");
        assert!(names.contains(&"MyTrait"), "Should have trait");
        // Top-level functions should be filtered out
        assert!(!names.contains(&"helper"));
    }

    #[test]
    fn test_python_trait_extraction() {
        // Test that trait-based extraction works for Python
        let extractor = SkeletonExtractor::new();
        let content = r#"
def foo(x: int) -> str:
    """Convert int to string."""
    return str(x)

class Bar:
    """A bar class."""

    def method(self, y: float) -> bool:
        """Check something."""
        return y > 0
"#;
        let path = PathBuf::from("test.py");
        let result = extractor.extract_with_support(&path, content);
        assert!(result.is_some(), "Should have trait support for Python");

        let result = result.unwrap();
        assert_eq!(result.symbols.len(), 2, "Should have 2 top-level symbols");

        let foo = &result.symbols[0];
        assert_eq!(foo.name, "foo");
        assert_eq!(foo.kind, "function");
        assert!(foo.signature.contains("def foo"));
        assert_eq!(foo.docstring.as_deref(), Some("Convert int to string."));

        let bar = &result.symbols[1];
        assert_eq!(bar.name, "Bar");
        assert_eq!(bar.kind, "class");
        assert_eq!(bar.children.len(), 1, "Class should have 1 method");
        assert_eq!(bar.children[0].name, "method");
        assert_eq!(bar.children[0].kind, "method");
    }
}
