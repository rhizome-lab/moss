//! Rust language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use std::process::Command;
use tree_sitter::Node;

// ============================================================================
// Rust external package resolution
// ============================================================================

/// Get Rust version.
pub fn get_rust_version() -> Option<String> {
    let output = Command::new("rustc").args(["--version"]).output().ok()?;

    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        // "rustc 1.75.0 (82e1608df 2023-12-21)" -> "1.75"
        for part in version_str.split_whitespace() {
            if part.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                let parts: Vec<&str> = part.split('.').collect();
                if parts.len() >= 2 {
                    return Some(format!("{}.{}", parts[0], parts[1]));
                }
            }
        }
    }

    None
}

/// Find cargo registry source directory.
/// Structure: ~/.cargo/registry/src/
pub fn find_cargo_registry() -> Option<PathBuf> {
    // Check CARGO_HOME env var
    if let Ok(cargo_home) = std::env::var("CARGO_HOME") {
        let registry = PathBuf::from(cargo_home).join("registry").join("src");
        if registry.is_dir() {
            return Some(registry);
        }
    }

    // Fall back to ~/.cargo/registry/src
    if let Ok(home) = std::env::var("HOME") {
        let registry = PathBuf::from(home)
            .join(".cargo")
            .join("registry")
            .join("src");
        if registry.is_dir() {
            return Some(registry);
        }
    }

    // Windows fallback
    if let Ok(home) = std::env::var("USERPROFILE") {
        let registry = PathBuf::from(home)
            .join(".cargo")
            .join("registry")
            .join("src");
        if registry.is_dir() {
            return Some(registry);
        }
    }

    None
}

/// Resolve a Rust crate import to its source location.
fn resolve_rust_crate(crate_name: &str, registry: &Path) -> Option<ResolvedPackage> {
    // Registry structure: registry/src/index.crates.io-*/crate-version/
    if let Ok(indices) = std::fs::read_dir(registry) {
        for index_entry in indices.flatten() {
            let index_path = index_entry.path();
            if !index_path.is_dir() {
                continue;
            }

            if let Ok(crates) = std::fs::read_dir(&index_path) {
                for crate_entry in crates.flatten() {
                    let crate_dir = crate_entry.path();
                    let dir_name = crate_entry.file_name().to_string_lossy().to_string();

                    // Check if this is our crate (name-version pattern)
                    if dir_name.starts_with(&format!("{}-", crate_name)) {
                        let lib_rs = crate_dir.join("src").join("lib.rs");
                        if lib_rs.is_file() {
                            return Some(ResolvedPackage {
                                path: lib_rs,
                                name: crate_name.to_string(),
                                is_namespace: false,
                            });
                        }
                    }
                }
            }
        }
    }

    None
}

// ============================================================================
// Rust language support
// ============================================================================

/// Rust language support.
pub struct Rust;

impl Language for Rust {
    fn name(&self) -> &'static str {
        "Rust"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }
    fn grammar_name(&self) -> &'static str {
        "rust"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["impl_item", "trait_item", "mod_item"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_item"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["struct_item", "enum_item", "type_item", "trait_item"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["use_declaration"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_item", "struct_item", "enum_item", "trait_item"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AccessModifier
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "for_expression",
            "while_expression",
            "loop_expression",
            "match_arm",
            "binary_expression", // for && and ||
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "for_expression",
            "while_expression",
            "loop_expression",
            "function_item",
            "impl_item",
            "trait_item",
            "mod_item",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        " {}"
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        // Additional scope-creating nodes beyond functions and containers
        &[
            "block",
            "for_expression",
            "while_expression",
            "loop_expression",
            "closure_expression",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "for_expression",
            "while_expression",
            "loop_expression",
            "return_expression",
            "break_expression",
            "continue_expression",
        ]
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        // Get visibility modifier
        let mut vis = String::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                vis = format!("{} ", &content[child.byte_range()]);
                break;
            }
        }

        let params = node
            .child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        let return_type = node
            .child_by_field_name("return_type")
            .map(|r| format!(" -> {}", &content[r.byte_range()]))
            .unwrap_or_default();

        let signature = format!("{}fn {}{}{}", vis, name, params, return_type);

        Some(Symbol {
            name: name.to_string(),
            kind: if in_container {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            },
            signature,
            docstring: self.extract_docstring(node, content),
            attributes: self.extract_attributes(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: self.get_visibility(node, content),
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        match node.kind() {
            "impl_item" => {
                let type_node = node.child_by_field_name("type")?;
                let type_name = &content[type_node.byte_range()];

                // Check if this is a trait impl (impl Trait for Type)
                let is_trait_impl = node.child_by_field_name("trait").is_some();

                let signature = if let Some(trait_node) = node.child_by_field_name("trait") {
                    let trait_name = &content[trait_node.byte_range()];
                    format!("impl {} for {}", trait_name, type_name)
                } else {
                    format!("impl {}", type_name)
                };

                Some(Symbol {
                    name: type_name.to_string(),
                    kind: SymbolKind::Module, // impl blocks are like modules
                    signature,
                    docstring: None,
                    attributes: self.extract_attributes(node, content),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Public,
                    children: Vec::new(),
                    is_interface_impl: is_trait_impl,
                    implements: Vec::new(),
                })
            }
            "trait_item" => {
                let name = self.node_name(node, content)?;
                let vis = self.extract_visibility_prefix(node, content);

                Some(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Trait,
                    signature: format!("{}trait {}", vis, name),
                    docstring: self.extract_docstring(node, content),
                    attributes: self.extract_attributes(node, content),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: self.get_visibility(node, content),
                    children: Vec::new(),
                    is_interface_impl: false,
                    implements: Vec::new(),
                })
            }
            "mod_item" => {
                // Only extract inline mod blocks (with declaration_list), not `mod foo;` declarations
                node.child_by_field_name("body")?;
                let name = self.node_name(node, content)?;
                let vis = self.extract_visibility_prefix(node, content);

                Some(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Module,
                    signature: format!("{}mod {}", vis, name),
                    docstring: self.extract_docstring(node, content),
                    attributes: self.extract_attributes(node, content),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: self.get_visibility(node, content),
                    children: Vec::new(),
                    is_interface_impl: false,
                    implements: Vec::new(),
                })
            }
            _ => None,
        }
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let vis = self.extract_visibility_prefix(node, content);

        let (kind, keyword) = match node.kind() {
            "struct_item" => (SymbolKind::Struct, "struct"),
            "enum_item" => (SymbolKind::Enum, "enum"),
            "type_item" => (SymbolKind::Type, "type"),
            "trait_item" => (SymbolKind::Trait, "trait"),
            _ => return None,
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{}{} {}", vis, keyword, name),
            docstring: self.extract_docstring(node, content),
            attributes: self.extract_attributes(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: self.get_visibility(node, content),
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Look for doc comments in the attributes child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "attributes" {
                let mut doc_lines = Vec::new();
                let mut attr_cursor = child.walk();
                for attr_child in child.children(&mut attr_cursor) {
                    if attr_child.kind() == "line_outer_doc_comment" {
                        let text = &content[attr_child.byte_range()];
                        let doc = text.trim_start_matches("///").trim();
                        if !doc.is_empty() {
                            doc_lines.push(doc.to_string());
                        }
                    }
                }
                if !doc_lines.is_empty() {
                    return Some(doc_lines.join("\n"));
                }
            }
        }
        None
    }

    fn extract_attributes(&self, node: &Node, content: &str) -> Vec<String> {
        let mut attrs = Vec::new();

        // Check for attributes child (e.g., #[test], #[cfg(test)])
        if let Some(attr_node) = node.child_by_field_name("attributes") {
            let mut cursor = attr_node.walk();
            for child in attr_node.children(&mut cursor) {
                if child.kind() == "attribute_item" {
                    attrs.push(content[child.byte_range()].to_string());
                }
            }
        }

        // Also check preceding siblings for outer attributes
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            if sibling.kind() == "attribute_item" {
                // Insert at beginning to maintain order
                attrs.insert(0, content[sibling.byte_range()].to_string());
                prev = sibling.prev_sibling();
            } else {
                break;
            }
        }

        attrs
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "use_declaration" {
            return Vec::new();
        }

        let line = node.start_position().row + 1;
        let text = &content[node.byte_range()];
        let module = text.trim_start_matches("use ").trim_end_matches(';').trim();

        // Check for braced imports: use foo::{bar, baz}
        let mut names = Vec::new();
        let is_relative = module.starts_with("crate")
            || module.starts_with("self")
            || module.starts_with("super");

        if let Some(brace_start) = module.find('{') {
            let prefix = module[..brace_start].trim_end_matches("::");
            if let Some(brace_end) = module.find('}') {
                let items = &module[brace_start + 1..brace_end];
                for item in items.split(',') {
                    let trimmed = item.trim();
                    if !trimmed.is_empty() {
                        names.push(trimmed.to_string());
                    }
                }
            }
            vec![Import {
                module: prefix.to_string(),
                names,
                alias: None,
                is_wildcard: false,
                is_relative,
                line,
            }]
        } else {
            // Simple import: use foo::bar or use foo::bar as baz
            let (module_part, alias) = if let Some(as_pos) = module.find(" as ") {
                (&module[..as_pos], Some(module[as_pos + 4..].to_string()))
            } else {
                (module, None)
            };

            vec![Import {
                module: module_part.to_string(),
                names: Vec::new(),
                alias,
                is_wildcard: module_part.ends_with("::*"),
                is_relative,
                line,
            }]
        }
    }

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());

        if import.is_wildcard {
            // Module already contains ::* from parsing
            format!("use {};", import.module)
        } else if names_to_use.is_empty() {
            format!("use {};", import.module)
        } else if names_to_use.len() == 1 {
            format!("use {}::{};", import.module, names_to_use[0])
        } else {
            format!("use {}::{{{}}};", import.module, names_to_use.join(", "))
        }
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let line = node.start_position().row + 1;

        // Only export pub items
        if !self.is_public(node, content) {
            return Vec::new();
        }

        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "function_item" => SymbolKind::Function,
            "struct_item" => SymbolKind::Struct,
            "enum_item" => SymbolKind::Enum,
            "trait_item" => SymbolKind::Trait,
            _ => return Vec::new(),
        };

        vec![Export { name, kind, line }]
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                let vis = &content[child.byte_range()];
                return vis.starts_with("pub");
            }
        }
        false
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                let vis = &content[child.byte_range()];
                if vis == "pub" {
                    return Visibility::Public;
                } else if vis.starts_with("pub(crate)") {
                    return Visibility::Internal;
                } else if vis.starts_with("pub(super)") || vis.starts_with("pub(in") {
                    return Visibility::Protected;
                }
            }
        }
        Visibility::Private
    }

    fn is_test_symbol(&self, symbol: &crate::Symbol) -> bool {
        let in_attrs = symbol
            .attributes
            .iter()
            .any(|a| a.contains("#[test]") || a.contains("#[cfg(test)]"));
        let in_sig =
            symbol.signature.contains("#[test]") || symbol.signature.contains("#[cfg(test)]");
        if in_attrs || in_sig {
            return true;
        }
        match symbol.kind {
            crate::SymbolKind::Function | crate::SymbolKind::Method => {
                symbol.name.starts_with("test_")
            }
            crate::SymbolKind::Module => symbol.name == "tests",
            _ => false,
        }
    }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> {
        None
    }

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        // Rust doesn't have body docstrings, only outer doc comments
        false
    }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        let name_node = node.child_by_field_name("name")?;
        Some(&content[name_node.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        // Only Rust files
        if path.extension()?.to_str()? != "rs" {
            return None;
        }

        let path_str = path.to_str()?;

        // Strip src/ prefix if present
        let rel_path = path_str.strip_prefix("src/").unwrap_or(path_str);

        // Remove .rs extension
        let module_path = rel_path.strip_suffix(".rs")?;

        // Handle mod.rs and lib.rs - use parent directory as module
        let module_path = if module_path.ends_with("/mod") || module_path.ends_with("/lib") {
            module_path.rsplit_once('/')?.0
        } else {
            module_path
        };

        // Convert path separators to ::
        Some(module_path.replace('/', "::"))
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        let rel_path = module.replace("::", "/");

        vec![
            format!("src/{}.rs", rel_path),
            format!("src/{}/mod.rs", rel_path),
        ]
    }

    // === Import Resolution ===

    fn lang_key(&self) -> &'static str {
        "rust"
    }

    fn resolve_local_import(
        &self,
        module: &str,
        current_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        // Find the crate root (directory containing Cargo.toml)
        let crate_root = find_crate_root(current_file, project_root)?;

        if module.starts_with("crate::") {
            // crate::foo::bar -> src/foo/bar.rs or src/foo/bar/mod.rs
            let path_part = module.strip_prefix("crate::")?.replace("::", "/");
            let src_dir = crate_root.join("src");

            // Try foo/bar.rs
            let direct = src_dir.join(format!("{}.rs", path_part));
            if direct.exists() {
                return Some(direct);
            }

            // Try foo/bar/mod.rs
            let mod_file = src_dir.join(&path_part).join("mod.rs");
            if mod_file.exists() {
                return Some(mod_file);
            }
        } else if module.starts_with("super::") {
            // super::foo -> parent directory's foo
            let current_dir = current_file.parent()?;
            let parent_dir = current_dir.parent()?;
            let path_part = module.strip_prefix("super::")?.replace("::", "/");

            // Try parent/foo.rs
            let direct = parent_dir.join(format!("{}.rs", path_part));
            if direct.exists() {
                return Some(direct);
            }

            // Try parent/foo/mod.rs
            let mod_file = parent_dir.join(&path_part).join("mod.rs");
            if mod_file.exists() {
                return Some(mod_file);
            }
        } else if module.starts_with("self::") {
            // self::foo -> same directory's foo
            let current_dir = current_file.parent()?;
            let path_part = module.strip_prefix("self::")?.replace("::", "/");

            // Try dir/foo.rs
            let direct = current_dir.join(format!("{}.rs", path_part));
            if direct.exists() {
                return Some(direct);
            }

            // Try dir/foo/mod.rs
            let mod_file = current_dir.join(&path_part).join("mod.rs");
            if mod_file.exists() {
                return Some(mod_file);
            }
        }

        None
    }

    fn resolve_external_import(
        &self,
        crate_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        let registry = find_cargo_registry()?;
        resolve_rust_crate(crate_name, &registry)
    }

    fn get_version(&self, _project_root: &Path) -> Option<String> {
        get_rust_version()
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        find_cargo_registry()
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool {
        // Rust stdlib is part of the compiler, no separate source to index
        false
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        // Rust stdlib is part of the compiler, no separate path
        None
    }

    fn package_sources(&self, project_root: &Path) -> Vec<crate::PackageSource> {
        use crate::{PackageSource, PackageSourceKind};
        let mut sources = Vec::new();
        if let Some(cache) = self.find_package_cache(project_root) {
            sources.push(PackageSource {
                name: "cargo-registry",
                path: cache,
                kind: PackageSourceKind::Cargo,
                version_specific: false,
            });
        }
        sources
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        // Skip target, tests directories
        if is_dir
            && (name == "target" || name == "tests" || name == "benches" || name == "examples")
        {
            return true;
        }
        // Only index .rs files
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        if source.kind != crate::PackageSourceKind::Cargo {
            return Vec::new();
        }
        discover_cargo_packages(&source.path)
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        // Strip .rs extension
        entry_name
            .strip_suffix(".rs")
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        // Rust packages use src/lib.rs as entry point
        let lib_rs = path.join("src").join("lib.rs");
        if lib_rs.is_file() {
            return Some(lib_rs);
        }
        // Or mod.rs in the directory itself
        let mod_rs = path.join("mod.rs");
        if mod_rs.is_file() {
            return Some(mod_rs);
        }
        None
    }
}

/// Discover packages in Cargo registry structure.
/// Structure: ~/.cargo/registry/src/index.crates.io-*/crate-version/
fn discover_cargo_packages(registry: &Path) -> Vec<(String, PathBuf)> {
    let mut packages = Vec::new();

    // Registry structure: registry/src/index.crates.io-*/crate-version/
    let indices = match std::fs::read_dir(registry) {
        Ok(e) => e,
        Err(_) => return packages,
    };

    for index_entry in indices.flatten() {
        let index_path = index_entry.path();
        if !index_path.is_dir() {
            continue;
        }

        let crates = match std::fs::read_dir(&index_path) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for crate_entry in crates.flatten() {
            let crate_path = crate_entry.path();
            let crate_name = crate_entry.file_name().to_string_lossy().to_string();

            if !crate_path.is_dir() {
                continue;
            }

            // Extract crate name (remove version suffix: "foo-1.2.3" -> "foo")
            let name = crate_name
                .rsplit_once('-')
                .map(|(n, _)| n)
                .unwrap_or(&crate_name);

            // Find src/lib.rs
            let lib_rs = crate_path.join("src").join("lib.rs");
            if lib_rs.is_file() {
                packages.push((name.to_string(), lib_rs));
            }
        }
    }

    packages
}

/// Find the crate root (directory containing Cargo.toml).
fn find_crate_root(start: &Path, root: &Path) -> Option<PathBuf> {
    let mut current = start.parent()?;
    while current.starts_with(root) {
        if current.join("Cargo.toml").exists() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
    None
}

impl Rust {
    fn extract_visibility_prefix(&self, node: &Node, content: &str) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                return format!("{} ", &content[child.byte_range()]);
            }
        }
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_unused_kinds_audit;

    /// Documents node kinds that exist in the Rust grammar but aren't used in trait methods.
    /// Run `cross_check_node_kinds` in registry.rs to see all potentially useful kinds.
    #[test]
    fn unused_node_kinds_audit() {
        // Categories:
        // - STRUCTURAL: Internal/wrapper nodes
        // - CLAUSE: Sub-parts of larger constructs
        // - EXPRESSION: Expressions (we track statements/definitions)
        // - TYPE: Type-related nodes
        // - MODIFIER: Visibility/async/unsafe modifiers
        // - PATTERN: Pattern matching internals
        // - MACRO: Macro-related nodes
        // - TODO: Potentially useful

        #[rustfmt::skip]
        let documented_unused: &[&str] = &[
            // STRUCTURAL
            "block_comment",           // comments
            "declaration_list",        // extern block contents
            "field_declaration",       // struct field
            "field_declaration_list",  // struct body
            "field_expression",        // foo.bar
            "field_identifier",        // field name
            "identifier",              // too common
            "lifetime",                // 'a
            "lifetime_parameter",      // <'a>
            "ordered_field_declaration_list", // tuple struct fields
            "scoped_identifier",       // path::to::thing
            "scoped_type_identifier",  // path::to::Type
            "shorthand_field_identifier", // struct init shorthand
            "type_identifier",         // type names
            "visibility_modifier",     // pub, pub(crate)

            // CLAUSE
            "else_clause",             // part of if
            "enum_variant",            // enum variant
            "enum_variant_list",       // enum body
            "match_block",             // match body
            "match_pattern",           // match arm pattern
            "trait_bounds",            // T: Foo + Bar
            "where_clause",            // where T: Foo

            // EXPRESSION
            "array_expression",        // [1, 2, 3]
            "assignment_expression",   // x = y
            "async_block",             // async { }
            "await_expression",        // foo.await
            "call_expression",         // foo()
            "generic_function",        // foo::<T>()
            "index_expression",        // arr[i]
            "parenthesized_expression",// (expr)
            "range_expression",        // 0..10
            "reference_expression",    // &x
            "struct_expression",       // Foo { x: 1 }
            "try_expression",          // foo?
            "tuple_expression",        // (a, b)
            "type_cast_expression",    // x as T
            "unary_expression",        // -x, !x
            "unit_expression",         // ()
            "yield_expression",        // yield x

            // TYPE
            "abstract_type",           // impl Trait
            "array_type",              // [T; N]
            "bounded_type",            // T: Foo
            "bracketed_type",          // <T>
            "dynamic_type",            // dyn Trait
            "function_type",           // fn(T) -> U
            "generic_type",            // Vec<T>
            "generic_type_with_turbofish", // Vec::<T>
            "higher_ranked_trait_bound", // for<'a>
            "never_type",              // !
            "pointer_type",            // *const T
            "primitive_type",          // i32, bool
            "qualified_type",          // <T as Trait>::Item
            "reference_type",          // &T
            "removed_trait_bound",     // ?Sized
            "tuple_type",              // (A, B)
            "type_arguments",          // <T, U>
            "type_binding",            // Item = T
            "type_parameter",          // T
            "type_parameters",         // <T, U>
            "unit_type",               // ()
            "unsafe_bound_type",       // unsafe trait bound

            // MODIFIER
            "block_outer_doc_comment", // //!
            "extern_modifier",         // extern "C"
            "function_modifiers",      // async, const, unsafe
            "mutable_specifier",       // mut

            // PATTERN
            "struct_pattern",          // Foo { x, y }
            "tuple_struct_pattern",    // Foo(x, y)

            // MACRO
            "fragment_specifier",      // $x:expr
            "macro_arguments_declaration", // macro args
            "macro_body_v2",           // macro body
            "macro_definition",        // macro_rules!
            "macro_definition_v2",     // macro 2.0

            // OTHER
            "block_expression_with_attribute", // #[attr] { }
            "const_block",             // const { }
            "expression_statement",    // expr;
            "expression_with_attribute", // #[attr] expr
            "extern_crate_declaration",// extern crate
            "foreign_mod_item",        // extern block item
            "function_signature_item", // fn signature in trait
            "gen_block",               // gen { }
            "let_declaration",         // let x = y
            "try_block",               // try { }
            "unsafe_block",            // unsafe { }
            "use_as_clause",           // use foo as bar
            "empty_statement",         // ;
        ];

        validate_unused_kinds_audit(&Rust, documented_unused)
            .expect("Rust unused node kinds audit failed");
    }
}
