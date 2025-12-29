//! Analysis passes for code quality metrics.

pub mod complexity;
pub mod function_length;

/// Generic report for file-level analysis (shared by complexity and length).
#[derive(Debug)]
pub struct FileReport<T> {
    pub functions: Vec<T>,
    pub file_path: String,
}
