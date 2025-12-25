//! Tool adapters.
//!
//! Each adapter wraps an external tool and provides:
//! - Availability detection
//! - Project relevance detection
//! - Output parsing to diagnostics

mod ruff;

pub use ruff::Ruff;

use crate::Tool;

/// Create a registry with all built-in adapters.
pub fn all_adapters() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(Ruff::new()),
        // Add more adapters here as they're implemented:
        // Box::new(Oxlint::new()),
        // Box::new(Biome::new()),
        // Box::new(Prettier::new()),
        // Box::new(Tsc::new()),
        // Box::new(Mypy::new()),
        // Box::new(Clippy::new()),
        // etc.
    ]
}
