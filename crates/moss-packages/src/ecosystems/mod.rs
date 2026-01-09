//! Ecosystem implementations.
//!
//! # Extensibility
//!
//! Users can register custom ecosystems via [`register()`]:
//!
//! ```ignore
//! use rhizome_moss_packages::{Ecosystem, LockfileManager, register_ecosystem};
//! use std::path::Path;
//!
//! struct MyEcosystem;
//!
//! impl Ecosystem for MyEcosystem {
//!     fn name(&self) -> &'static str { "my-ecosystem" }
//!     fn manifest_files(&self) -> &'static [&'static str] { &["my-manifest.json"] }
//!     fn lockfiles(&self) -> &'static [LockfileManager] { &[] }
//!     fn tools(&self) -> &'static [&'static str] { &["my-tool"] }
//!     // ... implement other methods
//! }
//!
//! // Register before first use
//! register_ecosystem(&MyEcosystem);
//! ```

mod cargo;
mod composer;
mod conan;
mod deno;
mod gem;
mod go;
mod hex;
mod maven;
mod nix;
mod npm;
mod nuget;
mod python;

use crate::Ecosystem;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

pub use cargo::Cargo;
pub use composer::Composer;
pub use conan::Conan;
pub use deno::Deno;
pub use gem::Gem;
pub use go::Go;
pub use hex::Hex;
pub use maven::Maven;
pub use nix::Nix;
pub use npm::Npm;
pub use nuget::Nuget;
pub use python::Python;

/// Global registry of ecosystem plugins.
static ECOSYSTEMS: RwLock<Vec<&'static dyn Ecosystem>> = RwLock::new(Vec::new());
static INITIALIZED: OnceLock<()> = OnceLock::new();

/// Register a custom ecosystem plugin.
///
/// Call this before any detection operations to add custom ecosystems.
/// Built-in ecosystems are registered automatically on first use.
pub fn register(ecosystem: &'static dyn Ecosystem) {
    ECOSYSTEMS.write().unwrap().push(ecosystem);
}

/// Initialize built-in ecosystems (called automatically on first use).
fn init_builtin() {
    INITIALIZED.get_or_init(|| {
        let mut ecosystems = ECOSYSTEMS.write().unwrap();
        ecosystems.push(&Cargo);
        ecosystems.push(&Npm);
        ecosystems.push(&Deno);
        ecosystems.push(&Python);
        ecosystems.push(&Go);
        ecosystems.push(&Hex);
        ecosystems.push(&Gem);
        ecosystems.push(&Composer);
        ecosystems.push(&Maven);
        ecosystems.push(&Nuget);
        ecosystems.push(&Nix);
        ecosystems.push(&Conan);
    });
}

/// Get an ecosystem by name from the global registry.
pub fn get_ecosystem(name: &str) -> Option<&'static dyn Ecosystem> {
    init_builtin();
    ECOSYSTEMS
        .read()
        .unwrap()
        .iter()
        .find(|e| e.name() == name)
        .copied()
}

/// List all available ecosystem names from the global registry.
pub fn list_ecosystems() -> Vec<&'static str> {
    init_builtin();
    ECOSYSTEMS
        .read()
        .unwrap()
        .iter()
        .map(|e| e.name())
        .collect()
}

/// Detect ecosystem from project files.
pub fn detect_ecosystem(project_root: &Path) -> Option<&'static dyn Ecosystem> {
    detect_all_ecosystems(project_root).into_iter().next()
}

/// Detect all ecosystems from project files.
pub fn detect_all_ecosystems(project_root: &Path) -> Vec<&'static dyn Ecosystem> {
    init_builtin();
    let ecosystems = ECOSYSTEMS.read().unwrap();

    let mut found = Vec::new();
    for ecosystem in ecosystems.iter() {
        for manifest in ecosystem.manifest_files() {
            let matches = if manifest.contains('*') {
                // Glob pattern - check if any matching file exists
                if let Some(pattern) = manifest.strip_prefix('*') {
                    std::fs::read_dir(project_root)
                        .ok()
                        .map(|entries| {
                            entries
                                .flatten()
                                .any(|entry| entry.file_name().to_string_lossy().ends_with(pattern))
                        })
                        .unwrap_or(false)
                } else {
                    false
                }
            } else {
                project_root.join(manifest).exists()
            };

            if matches {
                found.push(*ecosystem);
                break; // Don't add same ecosystem twice for different manifest files
            }
        }
    }
    found
}

/// Get all registered ecosystems.
pub fn all_ecosystems() -> Vec<&'static dyn Ecosystem> {
    init_builtin();
    ECOSYSTEMS.read().unwrap().clone()
}
