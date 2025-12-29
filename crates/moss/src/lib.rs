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
//! use moss::parsers::Parsers;
//!
//! let index = Index::open("path/to/codebase")?;
//! let parsers = Parsers::new();
//! ```

pub mod analysis_report;
pub mod analyze;
pub mod commands;
pub mod config;
pub mod daemon;
pub mod deps;
pub mod edit;
pub mod extract;
pub mod filter;
pub mod grep;
pub mod health;
pub mod index;
pub mod merge;
pub mod output;
pub mod overview;
pub mod parsers;
pub mod path_resolve;
pub mod paths;
pub mod serve;
pub mod sessions;
pub mod skeleton;
pub mod symbols;
pub mod tree;
pub mod workflow;

#[cfg(test)]
mod highlight_tests;
