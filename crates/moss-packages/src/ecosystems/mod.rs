//! Ecosystem implementations.

mod cargo;
mod conan;
mod composer;
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

pub use cargo::Cargo;
pub use conan::Conan;
pub use composer::Composer;
pub use gem::Gem;
pub use go::Go;
pub use hex::Hex;
pub use maven::Maven;
pub use nix::Nix;
pub use npm::Npm;
pub use nuget::Nuget;
pub use python::Python;

/// All registered ecosystems.
static ECOSYSTEMS: &[&dyn Ecosystem] = &[
    &Cargo, &Npm, &Python, &Go, &Hex, &Gem, &Composer, &Maven, &Nuget, &Nix, &Conan,
];

/// Detect ecosystem from project files.
pub fn detect(project_root: &Path) -> Option<&'static dyn Ecosystem> {
    detect_all(project_root).into_iter().next()
}

/// Detect all ecosystems from project files.
pub fn detect_all(project_root: &Path) -> Vec<&'static dyn Ecosystem> {
    let mut found = Vec::new();
    for ecosystem in ECOSYSTEMS {
        for manifest in ecosystem.manifest_files() {
            let matches = if manifest.contains('*') {
                // Glob pattern - check if any matching file exists
                if let Some(pattern) = manifest.strip_prefix('*') {
                    std::fs::read_dir(project_root)
                        .ok()
                        .map(|entries| {
                            entries.flatten().any(|entry| {
                                entry.file_name().to_string_lossy().ends_with(pattern)
                            })
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
pub fn all() -> &'static [&'static dyn Ecosystem] {
    ECOSYSTEMS
}
