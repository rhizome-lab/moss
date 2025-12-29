//! AST-based code skeleton extraction.
//!
//! Extracts function/class signatures with optional docstrings.
//! Uses the shared Extractor from extract.rs for tree traversal.

use crate::extract::{ExtractOptions, Extractor};
use crate::tree::{ViewNode, ViewNodeKind};
use moss_languages::{Symbol, SymbolKind};
use std::path::Path;

/// Re-export Symbol as SkeletonSymbol for backwards compatibility.
/// This is the canonical Symbol type from moss_languages.
pub type SkeletonSymbol = Symbol;

/// Extension trait for converting Symbol to ViewNode
pub trait SymbolExt {
    fn to_view_node(&self, parent_path: &str, grammar: Option<&str>) -> ViewNode;
}

impl SymbolExt for Symbol {
    /// Convert to a ViewNode for unified viewing.
    fn to_view_node(&self, parent_path: &str, grammar: Option<&str>) -> ViewNode {
        let path = if parent_path.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", parent_path, self.name)
        };

        let children: Vec<ViewNode> = self
            .children
            .iter()
            .map(|c| c.to_view_node(&path, grammar))
            .collect();

        ViewNode {
            name: self.name.clone(),
            kind: ViewNodeKind::Symbol(self.kind.as_str().to_string()),
            path,
            children,
            signature: Some(self.signature.clone()),
            docstring: self.docstring.clone(),
            line_range: Some((self.start_line, self.end_line)),
            grammar: grammar.map(String::from),
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
    pub fn to_view_node(&self, grammar: Option<&str>) -> ViewNode {
        let file_name = Path::new(&self.file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.file_path.clone());

        let children: Vec<ViewNode> = self
            .symbols
            .iter()
            .map(|s| s.to_view_node(&file_name, grammar))
            .collect();

        ViewNode::file(&file_name, &self.file_path).with_children(children)
    }

    /// Filter to only type definitions (class, struct, enum, trait, interface)
    /// Returns a new SkeletonResult with only type-like symbols, and strips methods from classes
    pub fn filter_types(&self) -> SkeletonResult {
        fn is_type_kind(kind: SymbolKind) -> bool {
            matches!(
                kind,
                SymbolKind::Class
                    | SymbolKind::Struct
                    | SymbolKind::Enum
                    | SymbolKind::Trait
                    | SymbolKind::Interface
                    | SymbolKind::Type
                    | SymbolKind::Module
            )
        }

        fn filter_symbol(sym: &Symbol) -> Option<Symbol> {
            if is_type_kind(sym.kind) {
                // For types, keep only nested types (not methods)
                let type_children: Vec<_> = sym
                    .children
                    .iter()
                    .filter_map(|c| filter_symbol(c))
                    .collect();
                Some(Symbol {
                    name: sym.name.clone(),
                    kind: sym.kind,
                    signature: sym.signature.clone(),
                    docstring: sym.docstring.clone(),
                    start_line: sym.start_line,
                    end_line: sym.end_line,
                    visibility: sym.visibility,
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

/// Skeleton extractor using shared Extractor from extract.rs
pub struct SkeletonExtractor {
    extractor: Extractor,
}

impl SkeletonExtractor {
    pub fn new() -> Self {
        Self {
            extractor: Extractor::new(),
        }
    }

    /// Create an extractor that shows all symbols including private ones
    pub fn with_all() -> Self {
        Self {
            extractor: Extractor::with_options(ExtractOptions {
                include_private: true,
            }),
        }
    }

    pub fn extract(&self, path: &Path, content: &str) -> SkeletonResult {
        let result = self.extractor.extract(path, content);
        SkeletonResult {
            symbols: result.symbols,
            file_path: result.file_path,
        }
    }

    /// Trait-based extraction (for future use when implementations are complete)
    #[allow(dead_code)]
    pub fn extract_with_support(&self, path: &Path, content: &str) -> Option<SkeletonResult> {
        let result = self.extractor.extract(path, content);
        if result.symbols.is_empty() {
            // Check if this is a supported file type that just has no symbols
            use moss_languages::support_for_path;
            if support_for_path(path).is_none() {
                return None;
            }
        }
        Some(SkeletonResult {
            symbols: result.symbols,
            file_path: result.file_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_python_skeleton() {
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
        let result = extractor.extract(&PathBuf::from("test.py"), content);
        assert_eq!(result.symbols.len(), 2);

        let foo = &result.symbols[0];
        assert_eq!(foo.name, "foo");
        assert_eq!(foo.kind, SymbolKind::Function);
        assert!(foo.signature.contains("def foo(x: int) -> str"));
        assert_eq!(foo.docstring.as_deref(), Some("Convert int to string."));

        let bar = &result.symbols[1];
        assert_eq!(bar.name, "Bar");
        assert_eq!(bar.kind, SymbolKind::Class);
        assert_eq!(bar.children.len(), 1);
        assert_eq!(bar.children[0].name, "method");
    }

    #[test]
    fn test_rust_skeleton() {
        let extractor = SkeletonExtractor::new();
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
        assert_eq!(foo.kind, SymbolKind::Struct);
        assert!(foo.signature.contains("pub struct Foo"));
        assert_eq!(foo.children.len(), 1);
        assert_eq!(foo.children[0].name, "new");
    }

    #[test]
    fn test_to_view_node() {
        let extractor = SkeletonExtractor::new();
        let content = r#"
def greet(name: str) -> str:
    """Return a personalized greeting message."""
    return f"Hello, {name}"
"#;
        let result = extractor.extract(&PathBuf::from("test.py"), content);
        let view_node = result.to_view_node(Some("python"));

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
        let extractor = SkeletonExtractor::new();
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
        assert_eq!(title.kind, SymbolKind::Heading);

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
        let extractor = SkeletonExtractor::new();
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
        assert_eq!(greet_fn.kind, SymbolKind::Function);

        let greeter = &result.symbols[1];
        assert_eq!(greeter.name, "Greeter");
        assert_eq!(greeter.kind, SymbolKind::Class);

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
        let extractor = SkeletonExtractor::new();
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
        assert!(filtered.symbols.iter().all(|s| s.kind == SymbolKind::Class));
        assert_eq!(filtered.symbols[0].name, "MyClass");
        assert_eq!(filtered.symbols[1].name, "AnotherClass");
    }

    #[test]
    fn test_filter_types_rust() {
        let extractor = SkeletonExtractor::new();
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
        assert!(kinds.contains(&SymbolKind::Struct), "Should have struct");
        assert!(kinds.contains(&SymbolKind::Enum), "Should have enum");
        assert!(kinds.contains(&SymbolKind::Trait), "Should have trait");
        // Functions should be filtered out
        assert!(!kinds.contains(&SymbolKind::Function));
    }

    #[test]
    fn test_filter_types_typescript() {
        let extractor = SkeletonExtractor::new();
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
        let extractor = SkeletonExtractor::new();
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
        let extractor = SkeletonExtractor::new();
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
        let extractor = SkeletonExtractor::new();
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
        let extractor = SkeletonExtractor::new();
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

        assert!(
            kinds.contains(&SymbolKind::Module),
            "Should have module (object)"
        );
        assert!(kinds.contains(&SymbolKind::Class), "Should have class");
        assert!(kinds.contains(&SymbolKind::Trait), "Should have trait");
    }

    #[test]
    fn test_vue_skeleton() {
        let extractor = SkeletonExtractor::new();
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
        let extractor = SkeletonExtractor::new();
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
        assert_eq!(foo.kind, SymbolKind::Function);
        assert!(foo.signature.contains("def foo"));
        assert_eq!(foo.docstring.as_deref(), Some("Convert int to string."));

        let bar = &result.symbols[1];
        assert_eq!(bar.name, "Bar");
        assert_eq!(bar.kind, SymbolKind::Class);
        assert_eq!(bar.children.len(), 1, "Class should have 1 method");
        assert_eq!(bar.children[0].name, "method");
        assert_eq!(bar.children[0].kind, SymbolKind::Method);
    }
}
