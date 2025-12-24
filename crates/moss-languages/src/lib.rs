//! Language support for moss.
//!
//! This crate provides the `LanguageSupport` trait and implementations for
//! various programming languages. Each language implementation is behind
//! a feature flag matching arborium's pattern.
//!
//! # Features
//!
//! - `all-languages` (default): Enable all supported languages
//! - `tier1`: Enable most common languages (Python, Rust, JS, TS, Go, Java, C++)
//! - `lang-python`, `lang-rust`, etc.: Enable individual languages
//!
//! # Example
//!
//! ```ignore
//! use moss_languages::{get_support, LanguageSupport};
//! use moss_core::Language;
//!
//! if let Some(support) = get_support(Language::Python) {
//!     println!("Python function kinds: {:?}", support.function_kinds());
//! }
//! ```

mod registry;
mod traits;

// Language implementations
#[cfg(feature = "lang-python")]
pub mod python;

#[cfg(feature = "lang-rust")]
pub mod rust;

#[cfg(feature = "lang-javascript")]
pub mod javascript;

#[cfg(feature = "lang-typescript")]
pub mod typescript;

#[cfg(feature = "lang-go")]
pub mod go;

#[cfg(feature = "lang-java")]
pub mod java;

#[cfg(feature = "lang-c")]
pub mod c;

#[cfg(feature = "lang-cpp")]
pub mod cpp;

#[cfg(feature = "lang-ruby")]
pub mod ruby;

#[cfg(feature = "lang-scala")]
pub mod scala;

#[cfg(feature = "lang-vue")]
pub mod vue;

#[cfg(feature = "lang-markdown")]
pub mod markdown;

#[cfg(feature = "lang-json")]
pub mod json;

#[cfg(feature = "lang-yaml")]
pub mod yaml;

#[cfg(feature = "lang-toml")]
pub mod toml;

#[cfg(feature = "lang-html")]
pub mod html;

#[cfg(feature = "lang-css")]
pub mod css;

#[cfg(feature = "lang-bash")]
pub mod bash;

// Re-exports
pub use registry::{get_support, is_supported, supported_languages};
pub use traits::{Export, Import, LanguageSupport, Symbol, SymbolKind, Visibility};
