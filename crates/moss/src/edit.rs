use crate::parsers;
use crate::path_resolve;
use crate::skeleton::SkeletonExtractor;
use moss_languages::{Language, SymbolKind, support_for_path};
use std::path::Path;

/// Result of finding a symbol in a file
#[derive(Debug)]
#[allow(dead_code)] // Fields used by Debug trait and for edit operations
pub struct SymbolLocation {
    pub name: String,
    pub kind: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub indent: String,
}

/// Location of a container's body (for prepend/append operations)
#[derive(Debug)]
pub struct ContainerBody {
    /// Byte offset where body content starts (after opening, any docstring)
    pub content_start: usize,
    /// Byte offset where body content ends (before closing brace/dedent)
    pub content_end: usize,
    /// Indentation for items inside this container
    pub inner_indent: String,
    /// Whether the body is currently empty (or just has a docstring/pass)
    pub is_empty: bool,
}

/// Convert a 1-based line number to byte offset in content.
/// Clamps to content length for safety (last line may not have trailing newline).
fn line_to_byte(content: &str, line: usize) -> usize {
    let pos: usize = content
        .lines()
        .take(line.saturating_sub(1))
        .map(|l| l.len() + 1)
        .sum();
    pos.min(content.len())
}

/// Editor for structural code modifications
pub struct Editor {}

impl Editor {
    pub fn new() -> Self {
        Self {}
    }

    /// Find a symbol by name in a file (uses skeleton extractor)
    pub fn find_symbol(
        &self,
        path: &Path,
        content: &str,
        name: &str,
        case_insensitive: bool,
    ) -> Option<SymbolLocation> {
        let extractor = SkeletonExtractor::new();
        let result = extractor.extract(path, content);

        fn search_symbols(
            symbols: &[crate::skeleton::SkeletonSymbol],
            name: &str,
            content: &str,
            case_insensitive: bool,
        ) -> Option<SymbolLocation> {
            for sym in symbols {
                let matches = if case_insensitive {
                    sym.name.eq_ignore_ascii_case(name)
                } else {
                    sym.name == name
                };
                if matches {
                    let start_byte = line_to_byte(content, sym.start_line);
                    let end_byte = line_to_byte(content, sym.end_line + 1);

                    return Some(SymbolLocation {
                        name: sym.name.clone(),
                        kind: sym.kind.as_str().to_string(),
                        start_byte,
                        end_byte,
                        start_line: sym.start_line,
                        end_line: sym.end_line,
                        indent: String::new(),
                    });
                }
                // Search children
                if let Some(loc) = search_symbols(&sym.children, name, content, case_insensitive) {
                    return Some(loc);
                }
            }
            None
        }

        search_symbols(&result.symbols, name, content, case_insensitive)
    }

    /// Check if a pattern contains glob characters (delegates to path_resolve)
    pub fn is_glob_pattern(pattern: &str) -> bool {
        path_resolve::is_glob_pattern(pattern)
    }

    /// Find all symbols matching a glob pattern in their path.
    /// Returns matches sorted by byte offset (reverse order for safe deletion).
    pub fn find_symbols_matching(
        &self,
        path: &Path,
        content: &str,
        pattern: &str,
    ) -> Vec<SymbolLocation> {
        let symbol_matches = path_resolve::resolve_symbol_glob(path, content, pattern);

        let mut locations: Vec<SymbolLocation> = symbol_matches
            .into_iter()
            .map(|m| {
                let start_byte = line_to_byte(content, m.symbol.start_line);
                let end_byte = line_to_byte(content, m.symbol.end_line + 1);
                SymbolLocation {
                    name: m.symbol.name,
                    kind: m.symbol.kind.as_str().to_string(),
                    start_byte,
                    end_byte,
                    start_line: m.symbol.start_line,
                    end_line: m.symbol.end_line,
                    indent: String::new(),
                }
            })
            .collect();

        // Sort by start position (reverse for safe deletion from end to start)
        locations.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));
        locations
    }

    /// Delete a symbol from the content
    pub fn delete_symbol(&self, content: &str, loc: &SymbolLocation) -> String {
        let mut result = String::new();

        // Find the start of the line containing the symbol
        let line_start = content[..loc.start_byte]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        // Find the end of the line containing the symbol end (include trailing newline)
        let mut end_byte = loc.end_byte;
        if end_byte < content.len() && content.as_bytes()[end_byte] == b'\n' {
            end_byte += 1;
        }

        // Smart whitespace: consume trailing blank lines to avoid double-blanks
        // But only if there's already a blank line before the symbol
        let has_blank_before =
            line_start >= 2 && &content[line_start.saturating_sub(2)..line_start] == "\n\n";

        if has_blank_before {
            // Consume trailing blank lines (up to one full blank line)
            while end_byte < content.len() && content.as_bytes()[end_byte] == b'\n' {
                end_byte += 1;
                // Only consume one blank line worth
                if end_byte < content.len() && content.as_bytes()[end_byte] != b'\n' {
                    break;
                }
            }
        }

        result.push_str(&content[..line_start]);
        result.push_str(&content[end_byte..]);

        result
    }

    /// Replace a symbol with new content
    pub fn replace_symbol(&self, content: &str, loc: &SymbolLocation, new_content: &str) -> String {
        let mut result = String::new();

        // Apply indentation to new content
        let indented = self.apply_indent(new_content, &loc.indent);

        result.push_str(&content[..loc.start_byte]);
        result.push_str(&indented);
        result.push_str(&content[loc.end_byte..]);

        result
    }

    /// Count blank lines before a position
    fn count_blank_lines_before(&self, content: &str, pos: usize) -> usize {
        let mut count = 0usize;
        let mut i = pos;
        while i > 0 {
            i -= 1;
            if content.as_bytes()[i] == b'\n' {
                count += 1;
            } else if !content.as_bytes()[i].is_ascii_whitespace() {
                break;
            }
        }
        count.saturating_sub(1) // Don't count the newline ending the previous line
    }

    /// Count blank lines after a position (after any trailing newline)
    fn count_blank_lines_after(&self, content: &str, pos: usize) -> usize {
        let mut count = 0;
        let mut i = pos;
        // Skip past the first newline (end of current symbol)
        if i < content.len() && content.as_bytes()[i] == b'\n' {
            i += 1;
        }
        while i < content.len() {
            if content.as_bytes()[i] == b'\n' {
                count += 1;
                i += 1;
            } else if content.as_bytes()[i].is_ascii_whitespace() {
                i += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Insert content before a symbol
    pub fn insert_before(&self, content: &str, loc: &SymbolLocation, new_content: &str) -> String {
        let mut result = String::new();

        // Find the start of the line containing the symbol
        let line_start = content[..loc.start_byte]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        // Detect spacing convention: how many blank lines before this symbol?
        let blank_lines = self.count_blank_lines_before(content, line_start);
        // +1 for the newline ending the content, +N for N blank lines
        let spacing = "\n".repeat(blank_lines.max(1) + 1);

        // Apply indentation to new content
        let indented = self.apply_indent(new_content, &loc.indent);

        result.push_str(&content[..line_start]);
        result.push_str(&indented);
        result.push_str(&spacing);
        result.push_str(&content[line_start..]);

        result
    }

    /// Insert content after a symbol
    pub fn insert_after(&self, content: &str, loc: &SymbolLocation, new_content: &str) -> String {
        let mut result = String::new();

        // Apply indentation to new content
        let indented = self.apply_indent(new_content, &loc.indent);

        // Find the end of the symbol (include trailing newline)
        let end_pos = if loc.end_byte < content.len() && content.as_bytes()[loc.end_byte] == b'\n' {
            loc.end_byte + 1
        } else {
            loc.end_byte
        };

        // Detect spacing convention: how many blank lines after this symbol?
        let blank_lines = self.count_blank_lines_after(content, loc.end_byte);
        // end_pos already includes trailing newline, so just add N newlines for N blank lines
        let spacing = "\n".repeat(blank_lines.max(1));

        // Find where the next non-blank content starts
        let mut next_content_pos = end_pos;
        while next_content_pos < content.len() && content.as_bytes()[next_content_pos] == b'\n' {
            next_content_pos += 1;
        }

        result.push_str(&content[..end_pos]);
        result.push_str(&spacing);
        result.push_str(&indented);

        if next_content_pos < content.len() {
            // +1 for the newline ending the inserted content
            result.push_str(&"\n".repeat(blank_lines.max(1) + 1));
            result.push_str(&content[next_content_pos..]);
        } else {
            result.push('\n');
        }

        result
    }

    /// Insert content at the beginning of a file
    pub fn prepend_to_file(&self, content: &str, new_content: &str) -> String {
        let mut result = String::new();
        result.push_str(new_content);
        if !new_content.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(content);
        result
    }

    /// Insert content at the end of a file
    pub fn append_to_file(&self, content: &str, new_content: &str) -> String {
        let mut result = String::new();
        result.push_str(content);
        if !content.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(new_content);
        if !new_content.ends_with('\n') {
            result.push('\n');
        }
        result
    }

    /// Find the body of a container symbol (class, impl block, markdown section) for prepend/append
    pub fn find_container_body(
        &self,
        path: &Path,
        content: &str,
        name: &str,
    ) -> Option<ContainerBody> {
        // Try skeleton-based lookup first (handles markdown sections)
        if let Some(body) = self.find_container_body_via_skeleton(path, content, name) {
            return Some(body);
        }

        // Fall back to AST-based lookup for languages with explicit body nodes
        let support = support_for_path(path)?;
        let grammar = support.grammar_name();
        let tree = parsers::parse_with_grammar(grammar, content)?;
        let root = tree.root_node();
        self.find_container_body_with_trait(root, content, name, grammar, support)
    }

    /// Find container body using skeleton (for markdown sections)
    fn find_container_body_via_skeleton(
        &self,
        path: &Path,
        content: &str,
        name: &str,
    ) -> Option<ContainerBody> {
        let extractor = SkeletonExtractor::new();
        let result = extractor.extract(path, content);

        fn search_symbols(
            symbols: &[crate::skeleton::SkeletonSymbol],
            name: &str,
            content: &str,
        ) -> Option<ContainerBody> {
            for sym in symbols {
                if sym.name == name && sym.kind == SymbolKind::Heading {
                    // For markdown: body starts after heading line, ends at section end
                    let content_start = line_to_byte(content, sym.start_line + 1);
                    let content_end = line_to_byte(content, sym.end_line + 1);

                    return Some(ContainerBody {
                        content_start,
                        content_end,
                        inner_indent: String::new(),
                        is_empty: content_start >= content_end,
                    });
                }
                // Search children
                if let Some(body) = search_symbols(&sym.children, name, content) {
                    return Some(body);
                }
            }
            None
        }

        search_symbols(&result.symbols, name, content)
    }

    fn find_container_body_with_trait(
        &self,
        node: tree_sitter::Node,
        content: &str,
        name: &str,
        grammar: &str,
        support: &dyn Language,
    ) -> Option<ContainerBody> {
        let kind = node.kind();

        // Only check container types
        if support.container_kinds().contains(&kind) {
            // Get the name of this container
            let name_node = node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("type"))?; // impl blocks use "type"
            let container_name = &content[name_node.byte_range()];

            if container_name == name {
                // Get the body node using trait method
                let body_node = support.container_body(&node)?;

                // Calculate inner indentation
                let start_byte = node.start_byte();
                let line_start = content[..start_byte]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let container_indent: String = content[line_start..start_byte]
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect();
                let inner_indent = format!("{}    ", container_indent);

                // Use language-specific body analysis
                return match grammar {
                    "python" => self.analyze_python_class_body(&body_node, content, &inner_indent),
                    "rust" => self.analyze_rust_impl_body(&body_node, content, &inner_indent),
                    _ => None,
                };
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(body) =
                self.find_container_body_with_trait(child, content, name, grammar, support)
            {
                return Some(body);
            }
        }

        None
    }

    fn analyze_python_class_body(
        &self,
        body_node: &tree_sitter::Node,
        content: &str,
        inner_indent: &str,
    ) -> Option<ContainerBody> {
        // Python class body is a "block" node
        // Children can include: docstring (expression_statement with string), pass, methods, etc.
        let mut cursor = body_node.walk();
        let children: Vec<_> = body_node.children(&mut cursor).collect();

        if children.is_empty() {
            // Empty body - insert at start
            return Some(ContainerBody {
                content_start: body_node.start_byte(),
                content_end: body_node.end_byte(),
                inner_indent: inner_indent.to_string(),
                is_empty: true,
            });
        }

        // Find first "real" content (skip docstrings)
        // Handle both grammar versions:
        // - Old: expression_statement > string
        // - New (arborium): string directly
        let mut first_real_idx = 0;
        for (i, child) in children.iter().enumerate() {
            let is_docstring = if child.kind() == "expression_statement" {
                // Could be a docstring - check if it's a string
                let mut child_cursor = child.walk();
                let first_child = child.children(&mut child_cursor).next();
                first_child.map(|fc| fc.kind() == "string").unwrap_or(false)
            } else if child.kind() == "string" {
                // Arborium-style: direct string node
                true
            } else {
                false
            };

            if is_docstring && i == 0 {
                first_real_idx = i + 1;
                continue;
            }
            break;
        }

        // Check if body is effectively empty (just docstring and/or pass)
        let is_empty = children.iter().skip(first_real_idx).all(|c| {
            if c.kind() == "pass_statement" {
                return true;
            }
            // Handle both grammar versions for docstrings
            if c.kind() == "string" {
                return true;
            }
            if c.kind() == "expression_statement" {
                // Check if it's a string (docstring)
                if let Some(first_child) = c.child(0) {
                    return first_child.kind() == "string";
                }
            }
            false
        });

        // For prepend: insert after docstring (if any), at start of first_real_idx position
        // For append: insert at end of body
        let content_start = if first_real_idx < children.len() {
            // Find the line start of the first real child
            let child_start = children[first_real_idx].start_byte();
            content[..child_start]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(child_start)
        } else if !children.is_empty() {
            // Only docstring exists - insert after it
            let last = children.last().unwrap();
            // Find end of last child's line
            let last_end = last.end_byte();
            if last_end < content.len() && content.as_bytes()[last_end] == b'\n' {
                last_end + 1
            } else {
                last_end
            }
        } else {
            body_node.start_byte()
        };

        let content_end = body_node.end_byte();

        Some(ContainerBody {
            content_start,
            content_end,
            inner_indent: inner_indent.to_string(),
            is_empty,
        })
    }

    fn analyze_rust_impl_body(
        &self,
        body_node: &tree_sitter::Node,
        content: &str,
        inner_indent: &str,
    ) -> Option<ContainerBody> {
        // Rust impl body is a "declaration_list" node: { ... }
        // We need to insert after the opening { and before the closing }
        let body_start = body_node.start_byte();
        let body_end = body_node.end_byte();

        // Find the opening brace
        let mut content_start = body_start;
        for (i, byte) in content[body_start..body_end].bytes().enumerate() {
            if byte == b'{' {
                content_start = body_start + i + 1;
                // Skip whitespace/newline after brace
                while content_start < body_end {
                    let b = content.as_bytes()[content_start];
                    if b == b'\n' {
                        content_start += 1;
                        break;
                    } else if b.is_ascii_whitespace() {
                        content_start += 1;
                    } else {
                        break;
                    }
                }
                break;
            }
        }

        // Find the closing brace
        let mut content_end = body_end;
        for (i, byte) in content[body_start..body_end].bytes().rev().enumerate() {
            if byte == b'}' {
                content_end = body_end - i - 1;
                // Go back to include the newline before the brace
                while content_end > content_start && content.as_bytes()[content_end - 1] == b' ' {
                    content_end -= 1;
                }
                break;
            }
        }

        // Check if body is empty
        let body_content = content[content_start..content_end].trim();
        let is_empty = body_content.is_empty();

        Some(ContainerBody {
            content_start,
            content_end,
            inner_indent: inner_indent.to_string(),
            is_empty,
        })
    }

    /// Prepend content inside a container (class/impl body)
    pub fn prepend_to_container(
        &self,
        content: &str,
        body: &ContainerBody,
        new_content: &str,
    ) -> String {
        let mut result = String::new();

        // Apply indentation to new content
        let indented = self.apply_indent(new_content, &body.inner_indent);

        result.push_str(&content[..body.content_start]);

        // Add the new content
        result.push_str(&indented);
        result.push('\n');

        // Add spacing if there's existing content
        if !body.is_empty {
            result.push('\n');
        }

        result.push_str(&content[body.content_start..]);

        result
    }

    /// Append content inside a container (class/impl body)
    pub fn append_to_container(
        &self,
        content: &str,
        body: &ContainerBody,
        new_content: &str,
    ) -> String {
        let mut result = String::new();

        // Apply indentation to new content
        let indented = self.apply_indent(new_content, &body.inner_indent);

        // Trim trailing whitespace/newlines from existing content
        let mut end_pos = body.content_end;
        while end_pos > 0
            && content
                .as_bytes()
                .get(end_pos - 1)
                .map(|&b| b == b'\n' || b == b' ')
                == Some(true)
        {
            end_pos -= 1;
        }

        result.push_str(&content[..end_pos]);

        // Add blank line before new content (Python/Rust convention for methods)
        if !body.is_empty {
            result.push_str("\n\n");
        } else {
            result.push('\n');
        }

        // Add the new content
        result.push_str(&indented);
        result.push('\n');

        result.push_str(&content[body.content_end..]);

        result
    }

    /// Apply indentation to content
    fn apply_indent(&self, content: &str, indent: &str) -> String {
        content
            .lines()
            .enumerate()
            .map(|(i, line)| {
                if i == 0 {
                    format!("{}{}", indent, line)
                } else if line.is_empty() {
                    line.to_string()
                } else {
                    format!("{}{}", indent, line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ============================================================================
// Batch Edit Support
// ============================================================================

use std::collections::HashMap;
use std::path::PathBuf;

/// Action to perform in a batch edit
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BatchAction {
    /// Delete a symbol
    Delete,
    /// Replace a symbol with new content
    Replace { content: String },
    /// Insert content relative to a symbol
    Insert {
        content: String,
        #[serde(default = "default_position")]
        position: String, // "before", "after", "prepend", "append"
    },
}

fn default_position() -> String {
    "after".to_string()
}

/// A single edit operation in a batch
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BatchEditOp {
    /// Target path (e.g., "src/main.py/foo" or "src/main.py:42")
    pub target: String,
    /// Action to perform
    #[serde(flatten)]
    pub action: BatchAction,
}

/// Result of applying a batch edit
#[derive(Debug)]
pub struct BatchEditResult {
    /// Files that were modified
    pub files_modified: Vec<PathBuf>,
    /// Number of edits applied
    pub edits_applied: usize,
    /// Errors encountered (target -> error message)
    pub errors: Vec<(String, String)>,
}

/// Preview of a file's changes before applying
#[derive(Debug)]
pub struct FilePreview {
    /// Path to the file
    pub path: PathBuf,
    /// Original content
    pub original: String,
    /// Modified content
    pub modified: String,
    /// Number of edits in this file
    pub edit_count: usize,
}

/// Result of previewing a batch edit
#[derive(Debug)]
pub struct BatchPreviewResult {
    /// Previews for each file that would be modified
    pub files: Vec<FilePreview>,
    /// Total number of edits
    pub total_edits: usize,
}

/// Batch editor for atomic multi-file edits
pub struct BatchEdit {
    edits: Vec<BatchEditOp>,
    message: Option<String>,
}

impl BatchEdit {
    /// Create a new batch edit
    pub fn new() -> Self {
        Self {
            edits: Vec::new(),
            message: None,
        }
    }

    /// Create batch edit from JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        let edits: Vec<BatchEditOp> =
            serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {}", e))?;
        Ok(Self {
            edits,
            message: None,
        })
    }

    /// Set the commit message for shadow git
    pub fn with_message(mut self, message: &str) -> Self {
        self.message = Some(message.to_string());
        self
    }

    /// Add an edit operation
    pub fn add(&mut self, op: BatchEditOp) {
        self.edits.push(op);
    }

    /// Apply all edits atomically
    ///
    /// Returns error if any edit fails validation. Edits are applied bottom-up
    /// within each file to preserve line numbers.
    pub fn apply(&self, root: &Path) -> Result<BatchEditResult, String> {
        if self.edits.is_empty() {
            return Ok(BatchEditResult {
                files_modified: Vec::new(),
                edits_applied: 0,
                errors: Vec::new(),
            });
        }

        // Phase 1: Resolve all targets and group by file
        let mut by_file: HashMap<PathBuf, Vec<(usize, &BatchEditOp, SymbolLocation)>> =
            HashMap::new();
        let mut errors = Vec::new();
        let editor = Editor::new();

        for (idx, op) in self.edits.iter().enumerate() {
            match self.resolve_target(root, &op.target, &editor) {
                Ok((file_path, location)) => {
                    by_file
                        .entry(file_path)
                        .or_default()
                        .push((idx, op, location));
                }
                Err(e) => {
                    errors.push((op.target.clone(), e));
                }
            }
        }

        // If any target failed to resolve, abort
        if !errors.is_empty() {
            return Err(format!(
                "Failed to resolve {} target(s): {}",
                errors.len(),
                errors
                    .iter()
                    .map(|(t, e)| format!("{}: {}", t, e))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }

        // Phase 2: Check for overlapping edits within files
        for (path, file_edits) in &by_file {
            self.check_overlaps(path, file_edits)?;
        }

        // Phase 3: Apply edits in memory (bottom-up to preserve line numbers)
        // Collect all modified contents before writing anything - true atomicity
        let mut modified_contents: Vec<(PathBuf, String)> = Vec::new();
        let mut edits_applied = 0;

        for (path, mut file_edits) in by_file {
            // Sort by start line descending (apply bottom-up)
            file_edits.sort_by(|a, b| b.2.start_line.cmp(&a.2.start_line));

            let mut content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

            for (_idx, op, loc) in &file_edits {
                content = self.apply_single_edit(&editor, &content, loc, &op.action)?;
                edits_applied += 1;
            }

            modified_contents.push((path, content));
        }

        // Phase 4: Write all files atomically (only if all edits succeeded)
        let mut files_modified = Vec::new();
        for (path, content) in modified_contents {
            std::fs::write(&path, &content)
                .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
            files_modified.push(path);
        }

        Ok(BatchEditResult {
            files_modified,
            edits_applied,
            errors,
        })
    }

    /// Preview all edits without applying them
    ///
    /// Returns the original and modified content for each file so callers can
    /// display a diff. Does not modify any files.
    pub fn preview(&self, root: &Path) -> Result<BatchPreviewResult, String> {
        if self.edits.is_empty() {
            return Ok(BatchPreviewResult {
                files: Vec::new(),
                total_edits: 0,
            });
        }

        // Phase 1: Resolve all targets and group by file
        let mut by_file: HashMap<PathBuf, Vec<(usize, &BatchEditOp, SymbolLocation)>> =
            HashMap::new();
        let mut errors = Vec::new();
        let editor = Editor::new();

        for (idx, op) in self.edits.iter().enumerate() {
            match self.resolve_target(root, &op.target, &editor) {
                Ok((file_path, location)) => {
                    by_file
                        .entry(file_path)
                        .or_default()
                        .push((idx, op, location));
                }
                Err(e) => {
                    errors.push((op.target.clone(), e));
                }
            }
        }

        if !errors.is_empty() {
            return Err(format!(
                "Failed to resolve {} target(s): {}",
                errors.len(),
                errors
                    .iter()
                    .map(|(t, e)| format!("{}: {}", t, e))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }

        // Phase 2: Check for overlapping edits
        for (path, file_edits) in &by_file {
            self.check_overlaps(path, file_edits)?;
        }

        // Phase 3: Compute modified content for each file
        let mut file_previews = Vec::new();
        let mut total_edits = 0;

        for (path, mut file_edits) in by_file {
            file_edits.sort_by(|a, b| b.2.start_line.cmp(&a.2.start_line));

            let original = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

            let mut modified = original.clone();
            let edit_count = file_edits.len();

            for (_idx, op, loc) in &file_edits {
                modified = self.apply_single_edit(&editor, &modified, loc, &op.action)?;
            }

            total_edits += edit_count;
            file_previews.push(FilePreview {
                path,
                original,
                modified,
                edit_count,
            });
        }

        Ok(BatchPreviewResult {
            files: file_previews,
            total_edits,
        })
    }

    /// Resolve a target string to file path and symbol location
    fn resolve_target(
        &self,
        root: &Path,
        target: &str,
        editor: &Editor,
    ) -> Result<(PathBuf, SymbolLocation), String> {
        // Use unified path resolution
        let unified = path_resolve::resolve_unified(target, root)
            .ok_or_else(|| format!("Could not resolve path: {}", target))?;

        let file_path = root.join(&unified.file_path);
        if !file_path.exists() {
            return Err(format!("File not found: {}", file_path.display()));
        }

        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        // Get symbol name from path
        let symbol_name = unified
            .symbol_path
            .last()
            .ok_or_else(|| format!("No symbol specified in target: {}", target))?;

        let location = editor
            .find_symbol(&file_path, &content, symbol_name, false)
            .ok_or_else(|| format!("Symbol not found: {}", symbol_name))?;

        Ok((file_path, location))
    }

    /// Check for overlapping edits in a file
    fn check_overlaps(
        &self,
        path: &Path,
        edits: &[(usize, &BatchEditOp, SymbolLocation)],
    ) -> Result<(), String> {
        for i in 0..edits.len() {
            for j in (i + 1)..edits.len() {
                let (_, op_a, loc_a) = &edits[i];
                let (_, op_b, loc_b) = &edits[j];

                // Check if ranges overlap
                let overlaps =
                    loc_a.start_line <= loc_b.end_line && loc_b.start_line <= loc_a.end_line;

                if overlaps {
                    return Err(format!(
                        "Overlapping edits in {}: {} (L{}-{}) and {} (L{}-{})",
                        path.display(),
                        op_a.target,
                        loc_a.start_line,
                        loc_a.end_line,
                        op_b.target,
                        loc_b.start_line,
                        loc_b.end_line
                    ));
                }
            }
        }
        Ok(())
    }

    /// Apply a single edit operation
    fn apply_single_edit(
        &self,
        editor: &Editor,
        content: &str,
        loc: &SymbolLocation,
        action: &BatchAction,
    ) -> Result<String, String> {
        match action {
            BatchAction::Delete => Ok(editor.delete_symbol(content, loc)),
            BatchAction::Replace { content: new } => Ok(editor.replace_symbol(content, loc, new)),
            BatchAction::Insert {
                content: new,
                position,
            } => match position.as_str() {
                "before" => Ok(editor.insert_before(content, loc, new)),
                "after" => Ok(editor.insert_after(content, loc, new)),
                _ => Err(format!("Invalid position: {} (use before/after)", position)),
            },
        }
    }
}

impl Default for BatchEdit {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_find_python_function() {
        let editor = Editor::new();
        let content = r#"
def foo():
    pass

def bar():
    return 42
"#;
        let loc = editor.find_symbol(&PathBuf::from("test.py"), content, "bar", false);
        assert!(loc.is_some());
        let loc = loc.unwrap();
        assert_eq!(loc.name, "bar");
        assert_eq!(loc.kind, "function");
    }

    #[test]
    fn test_delete_symbol() {
        let editor = Editor::new();
        let content = "def foo():\n    pass\n\ndef bar():\n    return 42\n";
        let loc = editor
            .find_symbol(&PathBuf::from("test.py"), content, "bar", false)
            .unwrap();
        let result = editor.delete_symbol(content, &loc);
        assert!(!result.contains("bar"));
        assert!(result.contains("foo"));
    }

    #[test]
    fn test_insert_before() {
        let editor = Editor::new();
        let content = "def foo():\n    pass\n\ndef bar():\n    return 42\n";
        let loc = editor
            .find_symbol(&PathBuf::from("test.py"), content, "bar", false)
            .unwrap();
        let result = editor.insert_before(content, &loc, "def baz():\n    pass");
        assert!(result.contains("baz"));
        assert!(result.find("baz").unwrap() < result.find("bar").unwrap());
    }

    #[test]
    fn test_prepend_to_python_class() {
        let editor = Editor::new();
        let content = r#"class Foo:
    """Docstring."""

    def first(self):
        pass
"#;
        let body = editor
            .find_container_body(&PathBuf::from("test.py"), content, "Foo")
            .unwrap();
        let result =
            editor.prepend_to_container(content, &body, "def new_method(self):\n    return 1");
        // New method should appear after docstring but before first
        assert!(result.contains("new_method"));
        let docstring_pos = result.find("Docstring").unwrap();
        let new_method_pos = result.find("new_method").unwrap();
        let first_pos = result.find("first").unwrap();
        assert!(docstring_pos < new_method_pos);
        assert!(new_method_pos < first_pos);
    }

    #[test]
    fn test_append_to_python_class() {
        let editor = Editor::new();
        let content = r#"class Foo:
    def first(self):
        pass

    def second(self):
        return 42
"#;
        let body = editor
            .find_container_body(&PathBuf::from("test.py"), content, "Foo")
            .unwrap();
        let result = editor.append_to_container(content, &body, "def last(self):\n    return 99");
        // New method should appear after second
        assert!(result.contains("last"));
        let second_pos = result.find("second").unwrap();
        let last_pos = result.find("last").unwrap();
        assert!(second_pos < last_pos);
    }

    #[test]
    fn test_prepend_to_rust_impl() {
        let editor = Editor::new();
        let content = r#"impl Foo {
    fn first(&self) -> i32 {
        1
    }
}
"#;
        let body = editor
            .find_container_body(&PathBuf::from("test.rs"), content, "Foo")
            .unwrap();
        let result =
            editor.prepend_to_container(content, &body, "fn new() -> Self {\n    Self {}\n}");
        assert!(result.contains("new"));
        let new_pos = result.find("new").unwrap();
        let first_pos = result.find("first").unwrap();
        assert!(new_pos < first_pos);
    }

    #[test]
    fn test_append_to_rust_impl() {
        let editor = Editor::new();
        let content = r#"impl Foo {
    fn first(&self) -> i32 {
        1
    }
}
"#;
        let body = editor
            .find_container_body(&PathBuf::from("test.rs"), content, "Foo")
            .unwrap();
        let result =
            editor.append_to_container(content, &body, "fn last(&self) -> i32 {\n    99\n}");
        assert!(result.contains("last"));
        let first_pos = result.find("first").unwrap();
        let last_pos = result.find("last").unwrap();
        assert!(first_pos < last_pos);
        // Should still have closing brace
        assert!(result.contains("}"));
    }
}
