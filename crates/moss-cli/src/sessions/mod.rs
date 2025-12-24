//! Session log analysis with plugin-based format support.
//!
//! Parses session logs from various AI coding agents:
//! - Claude Code (JSONL)
//! - Gemini CLI (JSON)
//! - Moss internal sessions (JSON)
//!
//! # Plugin Architecture
//!
//! Each log format implements the `LogFormat` trait, which provides:
//! - Format detection from file content
//! - Parsing into a common `SessionAnalysis` structure
//!
//! # Example
//!
//! ```ignore
//! use moss_cli::sessions::{analyze_session, SessionAnalysis};
//!
//! let analysis = analyze_session("~/.claude/projects/foo/session.jsonl")?;
//! println!("{}", analysis.to_markdown());
//! ```

mod analysis;
mod formats;

pub use analysis::*;
pub use formats::*;
