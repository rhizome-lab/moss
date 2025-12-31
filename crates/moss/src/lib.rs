//! Moss - Fast code intelligence library and CLI.
//!
//! This crate provides code intelligence features including:
//! - File indexing with SQLite storage
//! - Symbol extraction and navigation
//! - Dependency analysis
//! - Code complexity metrics
//! - Tree-sitter parsing integration
//!
//! # Example
//!
//! ```ignore
//! use moss::index::Index;
//! use moss::parsers;
//!
//! let index = Index::open("path/to/codebase")?;
//! let tree = parsers::parse_with_grammar("rust", "fn main() {}");
//! ```

pub mod analyze;
pub mod commands;
pub mod config;
pub mod daemon;
pub mod deps;
pub mod edit;
pub mod extract;
pub mod filter;
pub mod health;
pub mod index;
pub mod merge;
pub mod output;
pub mod parsers;
pub mod path_resolve;
pub mod paths;
pub mod serve;
pub mod sessions;
pub mod skeleton;
pub mod symbols;
pub mod text_search;
pub mod tree;
pub mod workflow;

#[cfg(test)]
mod highlight_tests;
