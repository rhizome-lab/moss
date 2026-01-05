//! HCL (HashiCorp Configuration Language) support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// HCL language support (Terraform, Packer, etc.).
pub struct Hcl;

impl Language for Hcl {
    fn name(&self) -> &'static str {
        "HCL"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["tf", "tfvars", "hcl"]
    }
    fn grammar_name(&self) -> &'static str {
        "hcl"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["block"] // resource, data, module, variable, output, locals, provider
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &[]
    }
    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["block"] // module blocks are imports
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["block"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NotApplicable
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "block" {
            return Vec::new();
        }

        let (block_type, name) = match self.extract_block_info(node, content) {
            Some(info) => info,
            None => return Vec::new(),
        };

        let kind = match block_type.as_str() {
            "resource" | "data" => SymbolKind::Struct,
            "variable" | "output" | "locals" => SymbolKind::Variable,
            "module" => SymbolKind::Module,
            "provider" => SymbolKind::Class,
            _ => SymbolKind::Variable,
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["block", "object"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["conditional", "for_expr"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["conditional", "for_expr"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["block", "object"]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(
        &self,
        _node: &Node,
        _content: &str,
        _in_container: bool,
    ) -> Option<Symbol> {
        None
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "block" {
            return None;
        }

        let (block_type, name) = self.extract_block_info(node, content)?;

        let kind = match block_type.as_str() {
            "resource" | "data" => SymbolKind::Struct,
            "module" => SymbolKind::Module,
            "provider" => SymbolKind::Class,
            _ => SymbolKind::Variable,
        };

        Some(Symbol {
            name: name.clone(),
            kind,
            signature: format!("{} \"{}\"", block_type, name),
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // HCL uses # or // for comments
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" {
                let line = text.trim_start_matches('#').trim_start_matches("//").trim();
                doc_lines.push(line.to_string());
                prev = sibling.prev_sibling();
            } else {
                break;
            }
        }

        if doc_lines.is_empty() {
            return None;
        }

        doc_lines.reverse();
        Some(doc_lines.join(" "))
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "block" {
            return Vec::new();
        }

        let (block_type, _name) = match self.extract_block_info(node, content) {
            Some(info) => info,
            None => return Vec::new(),
        };

        if block_type != "module" {
            return Vec::new();
        }

        // Look for source attribute in the block
        let text = &content[node.byte_range()];
        for line in text.lines() {
            if line.trim().starts_with("source") {
                if let Some(start) = line.find('"') {
                    let rest = &line[start + 1..];
                    if let Some(end) = rest.find('"') {
                        let module = rest[..end].to_string();
                        return vec![Import {
                            module,
                            names: Vec::new(),
                            alias: None,
                            is_wildcard: false,
                            is_relative: !rest.starts_with("registry") && !rest.starts_with("git"),
                            line: node.start_position().row + 1,
                        }];
                    }
                }
            }
        }

        Vec::new()
    }

    fn format_import(&self, _import: &Import, _names: Option<&[&str]>) -> String {
        // HCL has no imports
        String::new()
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        true
    }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility {
        Visibility::Public
    }

    fn is_test_symbol(&self, symbol: &crate::Symbol) -> bool {
        let name = symbol.name.as_str();
        match symbol.kind {
            crate::SymbolKind::Function | crate::SymbolKind::Method => name.starts_with("test_"),
            crate::SymbolKind::Module => name == "tests" || name == "test",
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
        false
    }
    fn node_name<'a>(&self, _node: &Node, _content: &'a str) -> Option<&'a str> {
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "tf" && ext != "tfvars" && ext != "hcl" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.tf", module), format!("{}/main.tf", module)]
    }

    fn lang_key(&self) -> &'static str {
        "hcl"
    }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool {
        false
    }
    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    fn resolve_local_import(
        &self,
        import: &str,
        _current_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        if import.starts_with("./") || import.starts_with("../") {
            let full = project_root.join(import);
            if full.is_dir() {
                let main_tf = full.join("main.tf");
                if main_tf.is_file() {
                    return Some(main_tf);
                }
            }
        }
        None
    }

    fn resolve_external_import(
        &self,
        _import_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        // Terraform registry resolution would go here
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        // Check versions.tf or terraform block for version
        let versions = project_root.join("versions.tf");
        if versions.is_file() {
            if let Ok(content) = std::fs::read_to_string(&versions) {
                for line in content.lines() {
                    if line.contains("required_version") {
                        if let Some(start) = line.find('"') {
                            let rest = &line[start + 1..];
                            if let Some(end) = rest.find('"') {
                                return Some(rest[..end].to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        // Terraform modules cache
        if let Some(home) = std::env::var_os("HOME") {
            let cache = PathBuf::from(home).join(".terraform.d/plugin-cache");
            if cache.is_dir() {
                return Some(cache);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["tf"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        if is_dir && name == ".terraform" {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".tf")
            .or_else(|| entry_name.strip_suffix(".tfvars"))
            .or_else(|| entry_name.strip_suffix(".hcl"))
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        let main = path.join("main.tf");
        if main.is_file() {
            return Some(main);
        }
        None
    }
}

impl Hcl {
    fn extract_block_info(&self, node: &Node, content: &str) -> Option<(String, String)> {
        let mut cursor = node.walk();
        let mut block_type = None;
        let mut labels = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" if block_type.is_none() => {
                    block_type = Some(content[child.byte_range()].to_string());
                }
                "string_lit" => {
                    let text = content[child.byte_range()].trim_matches('"').to_string();
                    labels.push(text);
                }
                _ => {}
            }
        }

        let block_type = block_type?;
        let name = if labels.len() >= 2 {
            format!("{}.{}", labels[0], labels[1])
        } else if !labels.is_empty() {
            labels[0].clone()
        } else {
            block_type.clone()
        };

        Some((block_type, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_unused_kinds_audit;

    #[test]
    fn unused_node_kinds_audit() {
        // Run cross_check_node_kinds to populate this list
        #[rustfmt::skip]
        let documented_unused: &[&str] = &[
            "binary_operation", "body", "collection_value", "expression",
            "for_cond", "for_intro", "for_object_expr", "for_tuple_expr",
            "function_arguments", "function_call", "get_attr", "heredoc_identifier",
            "identifier", "index", "literal_value", "object_elem", "quoted_template",
            "template_else_intro", "template_for", "template_for_end", "template_for_start",
            "template_if", "template_if_end", "template_if_intro", "tuple",
            "block_end", "block_start",
        ];
        validate_unused_kinds_audit(&Hcl, documented_unused)
            .expect("HCL unused node kinds audit failed");
    }
}
