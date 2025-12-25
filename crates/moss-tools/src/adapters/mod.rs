//! Tool adapters.
//!
//! Each adapter wraps an external tool and provides:
//! - Availability detection
//! - Project relevance detection
//! - Output parsing to diagnostics

mod biome;
mod clippy;
mod gofmt;
mod oxfmt;
mod oxlint;
mod prettier;
mod ruff;
mod rustfmt;
mod tsc;

pub use biome::{BiomeFormat, BiomeLint};
pub use clippy::Clippy;
pub use gofmt::{Gofmt, Govet};
pub use oxfmt::Oxfmt;
pub use oxlint::Oxlint;
pub use prettier::Prettier;
pub use ruff::Ruff;
pub use rustfmt::Rustfmt;
pub use tsc::Tsc;

use crate::Tool;

/// Create a registry with all built-in adapters.
pub fn all_adapters() -> Vec<Box<dyn Tool>> {
    vec![
        // Python
        Box::new(Ruff::new()),
        // JavaScript/TypeScript (oxc toolchain preferred over eslint/prettier)
        Box::new(Oxlint::new()),
        Box::new(Oxfmt::new()),
        Box::new(BiomeLint::new()),
        Box::new(BiomeFormat::new()),
        Box::new(Prettier::new()),
        Box::new(Tsc::new()),
        // Rust
        Box::new(Clippy::new()),
        Box::new(Rustfmt::new()),
        // Go
        Box::new(Gofmt::new()),
        Box::new(Govet::new()),
    ]
}
