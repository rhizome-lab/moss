//! Variable scope analysis.
//!
//! Tracks variable definitions and their scopes in source files.
//! Supports finding where a variable is defined, what's in scope at a position,
//! and detecting variable shadowing.

use moss_core::{tree_sitter::Node, Language, Parsers};
use moss_languages::get_support;
use std::path::Path;

/// A scope in the code
#[derive(Debug, Clone)]
pub struct Scope {
    pub kind: ScopeKind,
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub bindings: Vec<Binding>,
    pub children: Vec<Scope>,
}

/// Type of scope
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Some variants reserved for future language support
pub enum ScopeKind {
    Module,
    Function,
    Class,
    Method,
    Lambda,
    Comprehension,
    Loop,
    With,
    Try,
    Block, // Generic block scope (Rust)
    Impl,
}

impl ScopeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScopeKind::Module => "module",
            ScopeKind::Function => "function",
            ScopeKind::Class => "class",
            ScopeKind::Method => "method",
            ScopeKind::Lambda => "lambda",
            ScopeKind::Comprehension => "comprehension",
            ScopeKind::Loop => "loop",
            ScopeKind::With => "with",
            ScopeKind::Try => "try",
            ScopeKind::Block => "block",
            ScopeKind::Impl => "impl",
        }
    }
}

/// A variable binding (definition)
#[derive(Debug, Clone)]
pub struct Binding {
    pub name: String,
    pub kind: BindingKind,
    pub line: usize,
    pub column: usize,
    /// Inferred type (e.g., from constructor call or annotation)
    pub inferred_type: Option<String>,
}

/// Type of binding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Variable,
    Parameter,
    Function,
    Class,
    Import,
    ForLoop,
    WithItem,
    ExceptHandler,
}

impl BindingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            BindingKind::Variable => "variable",
            BindingKind::Parameter => "parameter",
            BindingKind::Function => "function",
            BindingKind::Class => "class",
            BindingKind::Import => "import",
            BindingKind::ForLoop => "for",
            BindingKind::WithItem => "with",
            BindingKind::ExceptHandler => "except",
        }
    }
}

/// Result of scope analysis
pub struct ScopeResult {
    pub root: Scope,
    pub file_path: String,
}

impl ScopeResult {
    /// Find all bindings visible at a given line
    pub fn bindings_at_line(&self, line: usize) -> Vec<&Binding> {
        let mut result = Vec::new();
        self.collect_bindings_at(&self.root, line, &mut result);
        result
    }

    fn collect_bindings_at<'a>(
        &'a self,
        scope: &'a Scope,
        line: usize,
        result: &mut Vec<&'a Binding>,
    ) {
        // Check if line is within this scope
        if line < scope.start_line || line > scope.end_line {
            return;
        }

        // Add bindings from this scope that are defined before the line
        for binding in &scope.bindings {
            if binding.line <= line {
                result.push(binding);
            }
        }

        // Recurse into child scopes
        for child in &scope.children {
            self.collect_bindings_at(child, line, result);
        }
    }

    /// Find where a name is defined at a given line
    pub fn find_definition(&self, name: &str, line: usize) -> Option<&Binding> {
        let bindings = self.bindings_at_line(line);
        // Return the most recent binding (last one shadowing previous)
        bindings
            .into_iter()
            .filter(|b| b.name == name)
            .last()
    }

    /// Format the scope tree for display
    pub fn format(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("# Scopes in {}", self.file_path));
        lines.push(String::new());
        self.format_scope(&self.root, 0, &mut lines);
        lines.join("\n")
    }

    fn format_scope(&self, scope: &Scope, indent: usize, lines: &mut Vec<String>) {
        let prefix = "  ".repeat(indent);
        let name = scope.name.as_deref().unwrap_or("<anonymous>");
        lines.push(format!(
            "{}{} {} (lines {}-{})",
            prefix,
            scope.kind.as_str(),
            name,
            scope.start_line,
            scope.end_line
        ));

        if !scope.bindings.is_empty() {
            for binding in &scope.bindings {
                let type_suffix = binding
                    .inferred_type
                    .as_ref()
                    .map(|t| format!(": {}", t))
                    .unwrap_or_default();
                lines.push(format!(
                    "{}  {} {}{} (line {})",
                    prefix,
                    binding.kind.as_str(),
                    binding.name,
                    type_suffix,
                    binding.line
                ));
            }
        }

        for child in &scope.children {
            self.format_scope(child, indent + 1, lines);
        }
    }
}

pub struct ScopeAnalyzer {
    parsers: Parsers,
}

impl ScopeAnalyzer {
    pub fn new() -> Self {
        Self {
            parsers: Parsers::new(),
        }
    }

    pub fn analyze(&self, path: &Path, content: &str) -> ScopeResult {
        let lang = Language::from_path(path);
        let root = match lang {
            Some(Language::Python) => self.analyze_python(content),
            Some(Language::Rust) => self.analyze_rust(content),
            _ => Scope {
                kind: ScopeKind::Module,
                name: None,
                start_line: 1,
                end_line: content.lines().count(),
                bindings: Vec::new(),
                children: Vec::new(),
            },
        };

        ScopeResult {
            root,
            file_path: path.to_string_lossy().to_string(),
        }
    }

    /// Check if a node kind creates a new scope using trait-based detection
    #[allow(dead_code)]
    fn is_scope_creating(&self, lang: Language, kind: &str) -> bool {
        if let Some(support) = get_support(lang) {
            // Check trait-defined scope-creating kinds
            if support.scope_creating_kinds().contains(&kind) {
                return true;
            }
            // Functions and containers also create scopes
            if support.function_kinds().contains(&kind) {
                return true;
            }
            if support.container_kinds().contains(&kind) {
                return true;
            }
        }
        false
    }

    fn analyze_python(&self, content: &str) -> Scope {
        let tree = match self.parsers.parse_lang(Language::Python, content) {
            Some(t) => t,
            None => {
                return Scope {
                    kind: ScopeKind::Module,
                    name: None,
                    start_line: 1,
                    end_line: content.lines().count(),
                    bindings: Vec::new(),
                    children: Vec::new(),
                }
            }
        };

        let root = tree.root_node();
        let source = content.as_bytes();

        self.build_python_scope(root, source, ScopeKind::Module, None)
    }

    fn build_python_scope(
        &self,
        node: Node,
        source: &[u8],
        kind: ScopeKind,
        name: Option<String>,
    ) -> Scope {
        let mut bindings = Vec::new();
        let mut children = Vec::new();

        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                // Function definitions create new scopes
                "function_definition" => {
                    let func_name = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string());

                    // Add function name as binding in current scope
                    if let Some(ref name) = func_name {
                        bindings.push(Binding {
                            name: name.clone(),
                            kind: BindingKind::Function,
                            line: child.start_position().row + 1,
                            column: child.start_position().column,
                            inferred_type: None,
                        });
                    }

                    // Create child scope for function body
                    let scope_kind = if kind == ScopeKind::Class {
                        ScopeKind::Method
                    } else {
                        ScopeKind::Function
                    };
                    let mut func_scope = self.build_python_scope(child, source, scope_kind, func_name);

                    // Extract parameters
                    if let Some(params) = child.child_by_field_name("parameters") {
                        self.extract_python_params(params, source, &mut func_scope.bindings);
                    }

                    children.push(func_scope);
                }

                // Class definitions create new scopes
                "class_definition" => {
                    let class_name = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string());

                    // Add class name as binding in current scope
                    if let Some(ref name) = class_name {
                        bindings.push(Binding {
                            name: name.clone(),
                            kind: BindingKind::Class,
                            line: child.start_position().row + 1,
                            column: child.start_position().column,
                            inferred_type: None,
                        });
                    }

                    children.push(self.build_python_scope(child, source, ScopeKind::Class, class_name));
                }

                // Assignments create bindings
                "assignment" | "augmented_assignment" => {
                    // Check RHS for type inference (constructor calls)
                    let inferred_type = child.child_by_field_name("right")
                        .and_then(|rhs| self.infer_python_type(rhs, source));

                    if let Some(left) = child.child_by_field_name("left") {
                        self.extract_python_targets_with_type(left, source, &mut bindings, BindingKind::Variable, inferred_type);
                    }
                }

                // Annotated assignments
                "annotated_assignment" => {
                    // First child is typically the target (use named_child to avoid borrow issues)
                    if let Some(target) = child.named_child(0) {
                        if target.kind() == "identifier" {
                            if let Ok(name) = target.utf8_text(source) {
                                bindings.push(Binding {
                                    name: name.to_string(),
                                    kind: BindingKind::Variable,
                                    line: target.start_position().row + 1,
                                    column: target.start_position().column,
                                    inferred_type: None,
                                });
                            }
                        }
                    }
                }

                // Import statements
                "import_statement" | "import_from_statement" => {
                    self.extract_python_imports(child, source, &mut bindings);
                }

                // For loops
                "for_statement" => {
                    if let Some(left) = child.child_by_field_name("left") {
                        self.extract_python_targets(left, source, &mut bindings, BindingKind::ForLoop);
                    }
                    // Recurse into body
                    let mut c = child.walk();
                    for grandchild in child.children(&mut c) {
                        if grandchild.kind() == "block" {
                            let loop_scope = self.build_python_scope(grandchild, source, ScopeKind::Loop, None);
                            if !loop_scope.bindings.is_empty() || !loop_scope.children.is_empty() {
                                children.push(loop_scope);
                            }
                        }
                    }
                }

                // With statements
                "with_statement" => {
                    let mut c = child.walk();
                    for grandchild in child.children(&mut c) {
                        if grandchild.kind() == "with_clause" {
                            let mut cc = grandchild.walk();
                            for item in grandchild.children(&mut cc) {
                                if item.kind() == "with_item" {
                                    // Look for "as" alias
                                    if let Some(alias) = item.child_by_field_name("alias") {
                                        self.extract_python_targets(alias, source, &mut bindings, BindingKind::WithItem);
                                    }
                                }
                            }
                        }
                    }
                }

                // Except handlers
                "except_clause" => {
                    // Look for the name after "as"
                    let mut c = child.walk();
                    for grandchild in child.children(&mut c) {
                        if grandchild.kind() == "identifier" {
                            if let Ok(name) = grandchild.utf8_text(source) {
                                bindings.push(Binding {
                                    name: name.to_string(),
                                    kind: BindingKind::ExceptHandler,
                                    line: grandchild.start_position().row + 1,
                                    column: grandchild.start_position().column,
                                    inferred_type: None,
                                });
                            }
                        }
                    }
                }

                // Comprehensions create their own scope
                "list_comprehension" | "set_comprehension" | "dictionary_comprehension" | "generator_expression" => {
                    let comp_scope = self.build_python_scope(child, source, ScopeKind::Comprehension, None);
                    if !comp_scope.bindings.is_empty() {
                        children.push(comp_scope);
                    }
                }

                // For clauses in comprehensions
                "for_in_clause" => {
                    if let Some(left) = child.child_by_field_name("left") {
                        self.extract_python_targets(left, source, &mut bindings, BindingKind::ForLoop);
                    }
                }

                // Lambda expressions
                "lambda" => {
                    let mut lambda_scope = Scope {
                        kind: ScopeKind::Lambda,
                        name: None,
                        start_line: child.start_position().row + 1,
                        end_line: child.end_position().row + 1,
                        bindings: Vec::new(),
                        children: Vec::new(),
                    };
                    if let Some(params) = child.child_by_field_name("parameters") {
                        self.extract_python_params(params, source, &mut lambda_scope.bindings);
                    }
                    if !lambda_scope.bindings.is_empty() {
                        children.push(lambda_scope);
                    }
                }

                // Other nodes: recurse
                _ => {
                    if child.child_count() > 0 {
                        let nested = self.build_python_scope(child, source, kind, None);
                        bindings.extend(nested.bindings);
                        children.extend(nested.children);
                    }
                }
            }
        }

        Scope {
            kind,
            name,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            bindings,
            children,
        }
    }

    fn extract_python_targets(&self, node: Node, source: &[u8], bindings: &mut Vec<Binding>, kind: BindingKind) {
        self.extract_python_targets_with_type(node, source, bindings, kind, None);
    }

    fn extract_python_targets_with_type(
        &self,
        node: Node,
        source: &[u8],
        bindings: &mut Vec<Binding>,
        kind: BindingKind,
        inferred_type: Option<String>,
    ) {
        match node.kind() {
            "identifier" => {
                if let Ok(name) = node.utf8_text(source) {
                    bindings.push(Binding {
                        name: name.to_string(),
                        kind,
                        line: node.start_position().row + 1,
                        column: node.start_position().column,
                        inferred_type,
                    });
                }
            }
            "tuple_pattern" | "list_pattern" | "pattern_list" | "tuple" | "list" => {
                // For tuple unpacking, we can't easily track types for each element
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.extract_python_targets_with_type(child, source, bindings, kind, None);
                }
            }
            _ => {}
        }
    }

    /// Infer the type from a Python expression
    fn infer_python_type(&self, node: Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            // Constructor call: SomeClass() or SomeClass(args)
            "call" => {
                if let Some(func) = node.child_by_field_name("function") {
                    match func.kind() {
                        "identifier" => {
                            // Simple constructor: MyClass()
                            if let Ok(name) = func.utf8_text(source) {
                                // Check if it looks like a class (starts with uppercase)
                                let name = name.to_string();
                                if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                                    return Some(name);
                                }
                            }
                        }
                        "attribute" => {
                            // Qualified constructor: module.MyClass()
                            if let Some(attr) = func.child_by_field_name("attribute") {
                                if let Ok(name) = attr.utf8_text(source) {
                                    let name = name.to_string();
                                    if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                                        // Include the full qualified name
                                        if let Ok(full) = func.utf8_text(source) {
                                            return Some(full.to_string());
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            // String literal
            "string" | "concatenated_string" => {
                return Some("str".to_string());
            }
            // Number literals
            "integer" => {
                return Some("int".to_string());
            }
            "float" => {
                return Some("float".to_string());
            }
            // List literal
            "list" => {
                return Some("list".to_string());
            }
            // Dict literal
            "dictionary" => {
                return Some("dict".to_string());
            }
            // Set literal
            "set" => {
                return Some("set".to_string());
            }
            // Tuple literal
            "tuple" => {
                return Some("tuple".to_string());
            }
            // Boolean literals
            "true" | "false" => {
                return Some("bool".to_string());
            }
            // None
            "none" => {
                return Some("None".to_string());
            }
            _ => {}
        }
        None
    }

    fn extract_python_params(&self, node: Node, source: &[u8], bindings: &mut Vec<Binding>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    if let Ok(name) = child.utf8_text(source) {
                        bindings.push(Binding {
                            name: name.to_string(),
                            kind: BindingKind::Parameter,
                            line: child.start_position().row + 1,
                            column: child.start_position().column,
                            inferred_type: None,
                        });
                    }
                }
                "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        if let Ok(name) = name_node.utf8_text(source) {
                            bindings.push(Binding {
                                name: name.to_string(),
                                kind: BindingKind::Parameter,
                                line: name_node.start_position().row + 1,
                                column: name_node.start_position().column,
                                inferred_type: None,
                            });
                        }
                    }
                }
                "list_splat_pattern" | "dictionary_splat_pattern" => {
                    let mut c = child.walk();
                    for grandchild in child.children(&mut c) {
                        if grandchild.kind() == "identifier" {
                            if let Ok(name) = grandchild.utf8_text(source) {
                                bindings.push(Binding {
                                    name: name.to_string(),
                                    kind: BindingKind::Parameter,
                                    line: grandchild.start_position().row + 1,
                                    column: grandchild.start_position().column,
                                    inferred_type: None,
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn extract_python_imports(&self, node: Node, source: &[u8], bindings: &mut Vec<Binding>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "dotted_name" => {
                    // For "import x", the first identifier is the binding
                    if let Some(first) = child.named_child(0) {
                        if first.kind() == "identifier" {
                            if let Ok(name) = first.utf8_text(source) {
                                bindings.push(Binding {
                                    name: name.to_string(),
                                    kind: BindingKind::Import,
                                    line: first.start_position().row + 1,
                                    column: first.start_position().column,
                                    inferred_type: None,
                                });
                            }
                        }
                    }
                }
                "aliased_import" => {
                    // Use alias if present, otherwise use name
                    let alias_name = child.child_by_field_name("alias")
                        .or_else(|| child.child_by_field_name("name"));
                    if let Some(name_node) = alias_name {
                        if let Ok(name) = name_node.utf8_text(source) {
                            bindings.push(Binding {
                                name: name.to_string(),
                                kind: BindingKind::Import,
                                line: name_node.start_position().row + 1,
                                column: name_node.start_position().column,
                                inferred_type: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn analyze_rust(&self, content: &str) -> Scope {
        let tree = match self.parsers.parse_lang(Language::Rust, content) {
            Some(t) => t,
            None => {
                return Scope {
                    kind: ScopeKind::Module,
                    name: None,
                    start_line: 1,
                    end_line: content.lines().count(),
                    bindings: Vec::new(),
                    children: Vec::new(),
                }
            }
        };

        let root = tree.root_node();
        let source = content.as_bytes();

        self.build_rust_scope(root, source, ScopeKind::Module, None)
    }

    fn build_rust_scope(
        &self,
        node: Node,
        source: &[u8],
        kind: ScopeKind,
        name: Option<String>,
    ) -> Scope {
        let mut bindings = Vec::new();
        let mut children = Vec::new();

        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                // Function definitions
                "function_item" => {
                    let func_name = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string());

                    if let Some(ref name) = func_name {
                        bindings.push(Binding {
                            name: name.clone(),
                            kind: BindingKind::Function,
                            line: child.start_position().row + 1,
                            column: child.start_position().column,
                            inferred_type: None,
                        });
                    }

                    let scope_kind = if kind == ScopeKind::Impl {
                        ScopeKind::Method
                    } else {
                        ScopeKind::Function
                    };
                    let mut func_scope = self.build_rust_scope(child, source, scope_kind, func_name);

                    // Extract parameters
                    if let Some(params) = child.child_by_field_name("parameters") {
                        self.extract_rust_params(params, source, &mut func_scope.bindings);
                    }

                    children.push(func_scope);
                }

                // Struct definitions
                "struct_item" => {
                    let struct_name = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string());

                    if let Some(ref name) = struct_name {
                        bindings.push(Binding {
                            name: name.clone(),
                            kind: BindingKind::Class,
                            line: child.start_position().row + 1,
                            column: child.start_position().column,
                            inferred_type: None,
                        });
                    }
                }

                // Enum definitions
                "enum_item" => {
                    let enum_name = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string());

                    if let Some(ref name) = enum_name {
                        bindings.push(Binding {
                            name: name.clone(),
                            kind: BindingKind::Class,
                            line: child.start_position().row + 1,
                            column: child.start_position().column,
                            inferred_type: None,
                        });
                    }
                }

                // Impl blocks
                "impl_item" => {
                    let impl_name = child
                        .child_by_field_name("type")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string());

                    children.push(self.build_rust_scope(child, source, ScopeKind::Impl, impl_name));
                }

                // Let bindings
                "let_declaration" => {
                    if let Some(pattern) = child.child_by_field_name("pattern") {
                        self.extract_rust_pattern(pattern, source, &mut bindings);
                    }
                }

                // For loops
                "for_expression" => {
                    if let Some(pattern) = child.child_by_field_name("pattern") {
                        self.extract_rust_pattern(pattern, source, &mut bindings);
                    }
                    // Recurse into body
                    if let Some(body) = child.child_by_field_name("body") {
                        let loop_scope = self.build_rust_scope(body, source, ScopeKind::Loop, None);
                        if !loop_scope.bindings.is_empty() || !loop_scope.children.is_empty() {
                            children.push(loop_scope);
                        }
                    }
                }

                // Block expressions (create new scope)
                "block" => {
                    let block_scope = self.build_rust_scope(child, source, ScopeKind::Block, None);
                    if !block_scope.bindings.is_empty() || !block_scope.children.is_empty() {
                        children.push(block_scope);
                    }
                }

                // Use declarations
                "use_declaration" => {
                    self.extract_rust_use(child, source, &mut bindings);
                }

                // Other nodes: recurse (but not into blocks which we handle separately)
                _ => {
                    if child.child_count() > 0 && child.kind() != "block" {
                        let nested = self.build_rust_scope(child, source, kind, None);
                        bindings.extend(nested.bindings);
                        children.extend(nested.children);
                    }
                }
            }
        }

        Scope {
            kind,
            name,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            bindings,
            children,
        }
    }

    fn extract_rust_params(&self, node: Node, source: &[u8], bindings: &mut Vec<Binding>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "parameter" {
                if let Some(pattern) = child.child_by_field_name("pattern") {
                    self.extract_rust_pattern(pattern, source, bindings);
                }
            } else if child.kind() == "self_parameter" {
                bindings.push(Binding {
                    name: "self".to_string(),
                    kind: BindingKind::Parameter,
                    line: child.start_position().row + 1,
                    column: child.start_position().column,
                    inferred_type: None,
                });
            }
        }
    }

    fn extract_rust_pattern(&self, node: Node, source: &[u8], bindings: &mut Vec<Binding>) {
        match node.kind() {
            "identifier" => {
                if let Ok(name) = node.utf8_text(source) {
                    // Skip _ patterns
                    if name != "_" {
                        bindings.push(Binding {
                            name: name.to_string(),
                            kind: BindingKind::Variable,
                            line: node.start_position().row + 1,
                            column: node.start_position().column,
                            inferred_type: None,
                        });
                    }
                }
            }
            "tuple_pattern" | "slice_pattern" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.extract_rust_pattern(child, source, bindings);
                }
            }
            "struct_pattern" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "field_pattern" {
                        // Could be `name` or `name: pattern`
                        if let Some(pattern) = child.child_by_field_name("pattern") {
                            self.extract_rust_pattern(pattern, source, bindings);
                        } else if let Some(name) = child.child_by_field_name("name") {
                            self.extract_rust_pattern(name, source, bindings);
                        }
                    }
                }
            }
            "ref_pattern" | "mut_pattern" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        self.extract_rust_pattern(child, source, bindings);
                    }
                }
            }
            _ => {}
        }
    }

    fn extract_rust_use(&self, node: Node, source: &[u8], bindings: &mut Vec<Binding>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "use_as_clause" => {
                    // use x as y - y is the binding
                    if let Some(alias) = child.child_by_field_name("alias") {
                        if let Ok(name) = alias.utf8_text(source) {
                            bindings.push(Binding {
                                name: name.to_string(),
                                kind: BindingKind::Import,
                                line: alias.start_position().row + 1,
                                column: alias.start_position().column,
                                inferred_type: None,
                            });
                        }
                    }
                }
                "use_list" => {
                    let mut c = child.walk();
                    for item in child.children(&mut c) {
                        if item.kind() == "identifier" {
                            if let Ok(name) = item.utf8_text(source) {
                                bindings.push(Binding {
                                    name: name.to_string(),
                                    kind: BindingKind::Import,
                                    line: item.start_position().row + 1,
                                    column: item.start_position().column,
                                    inferred_type: None,
                                });
                            }
                        } else if item.kind() == "use_as_clause" {
                            if let Some(alias) = item.child_by_field_name("alias") {
                                if let Ok(name) = alias.utf8_text(source) {
                                    bindings.push(Binding {
                                        name: name.to_string(),
                                        kind: BindingKind::Import,
                                        line: alias.start_position().row + 1,
                                        column: alias.start_position().column,
                                        inferred_type: None,
                                    });
                                }
                            } else {
                                // No alias, use the path's last component
                                let mut cc = item.walk();
                                if let Some(last) = item.children(&mut cc).last() {
                                    if last.kind() == "identifier" {
                                        if let Ok(name) = last.utf8_text(source) {
                                            bindings.push(Binding {
                                                name: name.to_string(),
                                                kind: BindingKind::Import,
                                                line: last.start_position().row + 1,
                                                column: last.start_position().column,
                                                inferred_type: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                "scoped_identifier" => {
                    // use foo::bar - bar is the binding
                    if let Some(name_node) = child.child_by_field_name("name") {
                        if let Ok(name) = name_node.utf8_text(source) {
                            bindings.push(Binding {
                                name: name.to_string(),
                                kind: BindingKind::Import,
                                line: name_node.start_position().row + 1,
                                column: name_node.start_position().column,
                                inferred_type: None,
                            });
                        }
                    }
                }
                "identifier" => {
                    if let Ok(name) = child.utf8_text(source) {
                        bindings.push(Binding {
                            name: name.to_string(),
                            kind: BindingKind::Import,
                            line: child.start_position().row + 1,
                            column: child.start_position().column,
                            inferred_type: None,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}
