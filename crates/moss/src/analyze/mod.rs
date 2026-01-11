//! Analysis passes for code quality metrics.

pub mod complexity;
pub mod function_length;

use serde::Serialize;

/// Generic report for file-level analysis (shared by complexity and length).
#[derive(Debug, Serialize)]
pub struct FileReport<T: Serialize> {
    pub functions: Vec<T>,
    pub file_path: String,
}
