//! Cross-language FFI binding detection.
//!
//! This module provides trait-based FFI detection for identifying
//! cross-language bindings like PyO3, wasm-bindgen, napi-rs, ctypes, etc.

use std::path::Path;

/// A detected FFI module/crate.
#[derive(Debug, Clone)]
pub struct FfiModule {
    /// Name of the module/crate (e.g., "my-pyo3-lib")
    pub name: String,
    /// Path to the main source file (e.g., "src/lib.rs")
    pub lib_path: String,
    /// The binding type that detected this module
    pub binding_type: &'static str,
    /// Source language
    pub source_lang: &'static str,
    /// Target language
    pub target_lang: &'static str,
}

/// A cross-language reference found in source code.
#[derive(Debug, Clone)]
pub struct CrossRef {
    /// File containing the reference
    pub source_file: String,
    /// Language of the source file
    pub source_lang: &'static str,
    /// Target module/crate being referenced
    pub target_module: String,
    /// Language of the target
    pub target_lang: &'static str,
    /// Type of reference (e.g., "pyo3_import", "ctypes_usage")
    pub ref_type: &'static str,
    /// Line number of the reference
    pub line: usize,
}

/// Trait for FFI binding detection.
///
/// Each binding type (PyO3, wasm-bindgen, etc.) implements this trait
/// to describe how to detect it in a project.
pub trait FfiBinding: Send + Sync {
    /// Unique identifier for this binding type (e.g., "pyo3", "wasm-bindgen")
    fn name(&self) -> &'static str;

    /// Source language for this binding (e.g., "rust")
    fn source_lang(&self) -> &'static str;

    /// Target language for this binding (e.g., "python")
    fn target_lang(&self) -> &'static str;

    /// Check if a build file (e.g., Cargo.toml) indicates this binding is used.
    /// Returns the module name if detected.
    fn detect_in_build_file(&self, path: &Path, content: &str) -> Option<String>;

    /// File extensions that may contain imports of this binding's modules.
    fn consumer_extensions(&self) -> &[&'static str];

    /// Check if an import line references a module from this binding.
    /// `module_name` is the crate/package name (with underscores for Rust).
    fn matches_import(&self, import_module: &str, import_name: &str, known_module: &str) -> bool;
}

// ============================================================================
// Built-in FFI Binding Implementations
// ============================================================================

/// PyO3 - Rust to Python bindings
pub struct PyO3Binding;

impl FfiBinding for PyO3Binding {
    fn name(&self) -> &'static str {
        "pyo3"
    }

    fn source_lang(&self) -> &'static str {
        "rust"
    }

    fn target_lang(&self) -> &'static str {
        "python"
    }

    fn detect_in_build_file(&self, path: &Path, content: &str) -> Option<String> {
        if path.file_name()? != "Cargo.toml" {
            return None;
        }
        if !content.contains("pyo3") && !content.contains("PyO3") {
            return None;
        }
        extract_cargo_crate_name(content)
    }

    fn consumer_extensions(&self) -> &[&'static str] {
        &["py"]
    }

    fn matches_import(&self, import_module: &str, import_name: &str, known_module: &str) -> bool {
        // PyO3 modules use underscores instead of hyphens
        let module_name = known_module.replace('-', "_");
        import_module == module_name
            || import_module.starts_with(&format!("{}.", module_name))
            || import_name == module_name
    }
}

/// wasm-bindgen - Rust to WebAssembly/JavaScript
pub struct WasmBindgenBinding;

impl FfiBinding for WasmBindgenBinding {
    fn name(&self) -> &'static str {
        "wasm-bindgen"
    }

    fn source_lang(&self) -> &'static str {
        "rust"
    }

    fn target_lang(&self) -> &'static str {
        "javascript"
    }

    fn detect_in_build_file(&self, path: &Path, content: &str) -> Option<String> {
        if path.file_name()? != "Cargo.toml" {
            return None;
        }
        if !content.contains("wasm-bindgen") {
            return None;
        }
        extract_cargo_crate_name(content)
    }

    fn consumer_extensions(&self) -> &[&'static str] {
        &["js", "ts", "tsx", "mjs"]
    }

    fn matches_import(&self, import_module: &str, import_name: &str, known_module: &str) -> bool {
        let module_name = known_module.replace('-', "_");
        import_module == module_name
            || import_module.starts_with(&format!("{}.", module_name))
            || import_name == module_name
            || import_module.contains(&format!("/{}", module_name))
    }
}

/// napi-rs - Rust to Node.js native modules
pub struct NapiRsBinding;

impl FfiBinding for NapiRsBinding {
    fn name(&self) -> &'static str {
        "napi-rs"
    }

    fn source_lang(&self) -> &'static str {
        "rust"
    }

    fn target_lang(&self) -> &'static str {
        "javascript"
    }

    fn detect_in_build_file(&self, path: &Path, content: &str) -> Option<String> {
        if path.file_name()? != "Cargo.toml" {
            return None;
        }
        if !content.contains("napi") {
            return None;
        }
        extract_cargo_crate_name(content)
    }

    fn consumer_extensions(&self) -> &[&'static str] {
        &["js", "ts", "tsx", "mjs"]
    }

    fn matches_import(&self, import_module: &str, import_name: &str, known_module: &str) -> bool {
        let module_name = known_module.replace('-', "_");
        import_module == module_name
            || import_module.starts_with(&format!("{}.", module_name))
            || import_name == module_name
    }
}

/// Generic cdylib - Rust C ABI exports
pub struct CdylibBinding;

impl FfiBinding for CdylibBinding {
    fn name(&self) -> &'static str {
        "cdylib"
    }

    fn source_lang(&self) -> &'static str {
        "rust"
    }

    fn target_lang(&self) -> &'static str {
        "c"
    }

    fn detect_in_build_file(&self, path: &Path, content: &str) -> Option<String> {
        if path.file_name()? != "Cargo.toml" {
            return None;
        }
        // Only match cdylib that isn't already caught by pyo3/wasm-bindgen/napi
        if !content.contains("cdylib") {
            return None;
        }
        if content.contains("pyo3") || content.contains("wasm-bindgen") || content.contains("napi")
        {
            return None;
        }
        extract_cargo_crate_name(content)
    }

    fn consumer_extensions(&self) -> &[&'static str] {
        &["c", "cpp", "h", "hpp", "py"] // Could be called from many languages
    }

    fn matches_import(
        &self,
        _import_module: &str,
        _import_name: &str,
        _known_module: &str,
    ) -> bool {
        // Generic cdylib matching is harder - would need to look for dlopen/LoadLibrary calls
        false
    }
}

/// Python ctypes - Python calling C libraries
pub struct CtypesBinding;

impl FfiBinding for CtypesBinding {
    fn name(&self) -> &'static str {
        "ctypes"
    }

    fn source_lang(&self) -> &'static str {
        "python"
    }

    fn target_lang(&self) -> &'static str {
        "c"
    }

    fn detect_in_build_file(&self, _path: &Path, _content: &str) -> Option<String> {
        // ctypes doesn't have a build file indicator
        None
    }

    fn consumer_extensions(&self) -> &[&'static str] {
        &["py"]
    }

    fn matches_import(&self, import_module: &str, import_name: &str, _known_module: &str) -> bool {
        import_module == "ctypes" || import_name == "ctypes" || import_name == "CDLL"
    }
}

/// Python cffi - Python calling C libraries
pub struct CffiBinding;

impl FfiBinding for CffiBinding {
    fn name(&self) -> &'static str {
        "cffi"
    }

    fn source_lang(&self) -> &'static str {
        "python"
    }

    fn target_lang(&self) -> &'static str {
        "c"
    }

    fn detect_in_build_file(&self, _path: &Path, _content: &str) -> Option<String> {
        None
    }

    fn consumer_extensions(&self) -> &[&'static str] {
        &["py"]
    }

    fn matches_import(&self, import_module: &str, import_name: &str, _known_module: &str) -> bool {
        import_module == "cffi" || import_name == "cffi" || import_name == "FFI"
    }
}

// ============================================================================
// FFI Detector Registry
// ============================================================================

/// Registry of all FFI binding detectors.
pub struct FfiDetector {
    bindings: Vec<Box<dyn FfiBinding>>,
}

impl Default for FfiDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl FfiDetector {
    /// Create a new detector with all built-in bindings.
    pub fn new() -> Self {
        Self {
            bindings: vec![
                Box::new(PyO3Binding),
                Box::new(WasmBindgenBinding),
                Box::new(NapiRsBinding),
                Box::new(CdylibBinding),
                Box::new(CtypesBinding),
                Box::new(CffiBinding),
            ],
        }
    }

    /// Add a custom binding detector.
    pub fn add_binding(&mut self, binding: Box<dyn FfiBinding>) {
        self.bindings.push(binding);
    }

    /// Get all registered bindings.
    pub fn bindings(&self) -> &[Box<dyn FfiBinding>] {
        &self.bindings
    }

    /// Detect FFI modules from a build file.
    pub fn detect_modules(&self, path: &Path, content: &str) -> Vec<FfiModule> {
        let mut modules = Vec::new();
        let parent = path.parent().unwrap_or(Path::new(""));
        let lib_path = parent.join("src").join("lib.rs");

        for binding in &self.bindings {
            if let Some(name) = binding.detect_in_build_file(path, content) {
                modules.push(FfiModule {
                    name,
                    lib_path: lib_path.to_string_lossy().to_string(),
                    binding_type: binding.name(),
                    source_lang: binding.source_lang(),
                    target_lang: binding.target_lang(),
                });
            }
        }

        modules
    }

    /// Check if an import matches any known FFI module.
    pub fn match_import<'a>(
        &self,
        import_module: &str,
        import_name: &str,
        known_modules: &'a [FfiModule],
    ) -> Option<(&'a FfiModule, &'static str)> {
        for module in known_modules {
            for binding in &self.bindings {
                if binding.name() == module.binding_type
                    && binding.matches_import(import_module, import_name, &module.name)
                {
                    return Some((module, binding.name()));
                }
            }
        }
        None
    }

    /// Check if a file extension can consume FFI modules.
    pub fn is_consumer_extension(&self, ext: &str) -> bool {
        for binding in &self.bindings {
            if binding.consumer_extensions().contains(&ext) {
                return true;
            }
        }
        false
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Extract crate name from Cargo.toml content.
fn extract_cargo_crate_name(content: &str) -> Option<String> {
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_package = false;
            continue;
        }
        if in_package && trimmed.starts_with("name") {
            if let Some(eq_pos) = trimmed.find('=') {
                let value = trimmed[eq_pos + 1..].trim();
                let value = value.trim_matches('"').trim_matches('\'');
                return Some(value.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pyo3_detection() {
        let binding = PyO3Binding;
        let content = r#"
[package]
name = "my-lib"
version = "0.1.0"

[dependencies]
pyo3 = "0.20"
"#;
        let result = binding.detect_in_build_file(Path::new("Cargo.toml"), content);
        assert_eq!(result, Some("my-lib".to_string()));
    }

    #[test]
    fn test_pyo3_import_matching() {
        let binding = PyO3Binding;
        assert!(binding.matches_import("my_lib", "", "my-lib"));
        assert!(binding.matches_import("my_lib.submodule", "", "my-lib"));
        assert!(!binding.matches_import("other_lib", "", "my-lib"));
    }

    #[test]
    fn test_detector_registry() {
        let detector = FfiDetector::new();
        assert!(detector.bindings().len() >= 6);

        let content = r#"
[package]
name = "wasm-app"

[dependencies]
wasm-bindgen = "0.2"
"#;
        let modules = detector.detect_modules(Path::new("Cargo.toml"), content);
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "wasm-app");
        assert_eq!(modules[0].binding_type, "wasm-bindgen");
    }
}
