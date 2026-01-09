//! Directory tree visualization.
//!
//! Git-aware tree display using the `ignore` crate for gitignore support.

use crate::parsers::grammar_loader;
use crate::skeleton::{SkeletonExtractor, SkeletonSymbol};
use ignore::WalkBuilder;
use rhizome_moss_languages::{GrammarLoader, support_for_grammar, support_for_path};
use nu_ansi_term::Color::{LightCyan, LightGreen, LightMagenta, Red, White as LightGray, Yellow};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor};

/// Cached compiled queries for highlighting - Query::new is expensive (~100ms).
static HIGHLIGHT_QUERY_CACHE: OnceLock<RwLock<HashMap<String, Arc<Query>>>> = OnceLock::new();
static INJECTION_QUERY_CACHE: OnceLock<RwLock<HashMap<String, Arc<Query>>>> = OnceLock::new();

fn get_highlight_query(grammar: &str, language: &tree_sitter::Language) -> Option<Arc<Query>> {
    let cache = HIGHLIGHT_QUERY_CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    // Check cache first
    if let Ok(read_guard) = cache.read()
        && let Some(query) = read_guard.get(grammar)
    {
        return Some(Arc::clone(query));
    }

    // Not cached - compile and store
    let loader = grammar_loader();
    let query_str = loader.get_highlights(grammar)?;
    let query = Arc::new(Query::new(language, &query_str).ok()?);

    if let Ok(mut write_guard) = cache.write() {
        write_guard.insert(grammar.to_string(), Arc::clone(&query));
    }

    Some(query)
}

fn get_injection_query(grammar: &str, language: &tree_sitter::Language) -> Option<Arc<Query>> {
    let cache = INJECTION_QUERY_CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    // Check cache first
    if let Ok(read_guard) = cache.read()
        && let Some(query) = read_guard.get(grammar)
    {
        return Some(Arc::clone(query));
    }

    // Not cached - compile and store
    let loader = grammar_loader();
    let query_str = loader.get_injections(grammar)?;
    let query = Arc::new(Query::new(language, &query_str).ok()?);

    if let Ok(mut write_guard) = cache.write() {
        write_guard.insert(grammar.to_string(), Arc::clone(&query));
    }

    Some(query)
}

/// Unified node for viewing directories, files, and symbols.
///
/// This is the common abstraction for `moss view` - directories contain files,
/// files contain symbols, symbols can contain nested symbols.
#[derive(Debug, Clone, Serialize)]
pub struct ViewNode {
    /// Display name (filename, symbol name)
    pub name: String,
    /// Node type
    pub kind: ViewNodeKind,
    /// Full path from root (e.g., "src/main.rs" or "src/main.rs/Foo/bar")
    pub path: String,
    /// Child nodes
    pub children: Vec<ViewNode>,

    /// Signature (for symbols: "def foo(x: int) -> str")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Docstring (for symbols)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    /// Line range in file (start, end)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_range: Option<(usize, usize)>,
    /// Grammar name for syntax highlighting (e.g., "rust", "python")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grammar: Option<String>,
}

/// Type of node in the view tree.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ViewNodeKind {
    Directory,
    File,
    /// Symbol with its kind (class, function, method, etc.)
    #[serde(rename = "symbol")]
    Symbol(String),
}

impl ViewNode {
    /// Create a file node.
    pub fn file(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: ViewNodeKind::File,
            path: path.into(),
            children: Vec::new(),
            signature: None,
            docstring: None,
            line_range: None,
            grammar: None,
        }
    }

    /// Add multiple children.
    pub fn with_children(mut self, children: Vec<ViewNode>) -> Self {
        self.children = children;
        self
    }

    /// Set grammar for syntax highlighting.
    pub fn with_grammar(mut self, grammar: impl Into<String>) -> Self {
        self.grammar = Some(grammar.into());
        self
    }
}

/// How to display docstrings in formatted output.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DocstringDisplay {
    /// No docstrings (skeleton mode).
    None,
    /// Show summary only - first paragraph, up to double blank line.
    #[default]
    Summary,
    /// Show full docstrings (--docs mode).
    Full,
}

/// Options for formatting ViewNodes.
#[derive(Clone, Default)]
pub struct FormatOptions {
    /// How to display docstrings.
    pub docstrings: DocstringDisplay,
    /// Maximum depth to display (None = unlimited).
    pub max_depth: Option<usize>,
    /// Show line numbers for symbols.
    pub line_numbers: bool,
    /// Skip the root node and only show children (useful for file views).
    pub skip_root: bool,
    /// Minimal mode: plain indentation, elide keywords (LLM-optimized).
    pub minimal: bool,
    /// Use ANSI colors in output (respects NO_COLOR and config).
    pub use_colors: bool,
}

/// Extract docstring summary (everything up to double blank line).
///
/// The double blank line convention: `\n\n\n` separates summary from extended docs.
/// Single blank lines (`\n\n`) are normal paragraph breaks within the summary.
/// If no double blank, the entire docstring is considered the summary.
pub fn docstring_summary(doc: &str) -> &str {
    if let Some(pos) = doc.find("\n\n\n") {
        doc[..pos].trim()
    } else {
        doc.trim()
    }
}

/// Format a ViewNode as text output.
///
/// Handles all node types (directory, file, symbol) with consistent tree-style formatting.
pub fn format_view_node(node: &ViewNode, options: &FormatOptions) -> Vec<String> {
    let mut lines = Vec::new();

    if !options.skip_root {
        // Root line: name with optional signature and line numbers
        let root_line = format_node_line(node, options);
        lines.push(root_line);

        // Add docstring based on display mode
        if let Some(doc) = &node.docstring {
            format_docstring(
                doc,
                &node.name,
                "    ",
                options.docstrings,
                node.grammar.as_deref(),
                &mut lines,
            );
        }
    }

    // Render children
    let prefix = if options.skip_root { "" } else { "" };
    format_children(
        &node.children,
        prefix,
        &mut lines,
        options,
        0,
        node.grammar.as_deref(),
    );

    lines
}

/// Docstring display style based on language.
enum DocstringStyle {
    /// Line-prefix style: each line prefixed (e.g., `/// ` for Rust)
    LinePrefix(&'static str),
    /// Block style: open delimiter, close delimiter (e.g., `"""` for Python)
    Block(&'static str, &'static str),
}

/// Get appropriate docstring style for a grammar.
fn docstring_style_for_grammar(grammar: Option<&str>) -> DocstringStyle {
    match grammar {
        // Line-prefix languages
        Some("rust") => DocstringStyle::LinePrefix("/// "),
        Some("go") => DocstringStyle::LinePrefix("// "),
        Some("c" | "cpp") => DocstringStyle::LinePrefix("// "),
        Some("ruby") => DocstringStyle::LinePrefix("# "),
        Some("bash" | "fish" | "zsh") => DocstringStyle::LinePrefix("# "),
        Some("perl") => DocstringStyle::LinePrefix("# "),
        Some("r") => DocstringStyle::LinePrefix("# "),
        Some("nim") => DocstringStyle::LinePrefix("## "),
        Some("haskell") => DocstringStyle::LinePrefix("-- "),
        Some("lua") => DocstringStyle::LinePrefix("--- "),
        Some("sql") => DocstringStyle::LinePrefix("-- "),
        Some("elisp" | "commonlisp" | "scheme" | "clojure") => DocstringStyle::LinePrefix("; "),

        // Block-style languages
        Some("python") => DocstringStyle::Block("\"\"\"", "\"\"\""),
        Some("javascript" | "typescript" | "tsx" | "jsx") => DocstringStyle::Block("/** ", " */"),
        Some("java" | "kotlin" | "scala") => DocstringStyle::Block("/** ", " */"),
        Some("swift") => DocstringStyle::Block("/** ", " */"),
        Some("php") => DocstringStyle::Block("/** ", " */"),
        Some("css") => DocstringStyle::Block("/* ", " */"),

        // Default to Python-style for unknown
        _ => DocstringStyle::Block("\"\"\"", "\"\"\""),
    }
}

/// Format a docstring according to the display mode and language.
fn format_docstring(
    doc: &str,
    name: &str,
    prefix: &str,
    mode: DocstringDisplay,
    grammar: Option<&str>,
    lines: &mut Vec<String>,
) {
    let style = docstring_style_for_grammar(grammar);

    match mode {
        DocstringDisplay::None => {}
        DocstringDisplay::Summary => {
            let summary = docstring_summary(doc);
            if summary.is_empty()
                || is_useless_docstring(name, summary.lines().next().unwrap_or(""))
            {
                return;
            }
            format_docstring_lines(&summary.lines().collect::<Vec<_>>(), prefix, &style, lines);
        }
        DocstringDisplay::Full => {
            let trimmed = doc.trim();
            if trimmed.is_empty()
                || is_useless_docstring(name, trimmed.lines().next().unwrap_or(""))
            {
                return;
            }
            format_docstring_lines(&trimmed.lines().collect::<Vec<_>>(), prefix, &style, lines);
        }
    }
}

/// Format docstring lines with the appropriate style.
fn format_docstring_lines(
    doc_lines: &[&str],
    prefix: &str,
    style: &DocstringStyle,
    lines: &mut Vec<String>,
) {
    match style {
        DocstringStyle::LinePrefix(line_prefix) => {
            for line in doc_lines {
                lines.push(format!("{}{}{}", prefix, line_prefix, line));
            }
        }
        DocstringStyle::Block(open, close) => {
            if doc_lines.len() == 1 {
                lines.push(format!("{}{}{}{}", prefix, open, doc_lines[0], close));
            } else {
                lines.push(format!("{}{}", prefix, open));
                for line in doc_lines {
                    lines.push(format!("{}{}", prefix, line));
                }
                lines.push(format!("{}{}", prefix, close));
            }
        }
    }
}

/// Format a single node line with optional line numbers.
fn format_node_line(node: &ViewNode, options: &FormatOptions) -> String {
    let base = match &node.kind {
        ViewNodeKind::Symbol(_) => {
            if let Some(sig) = &node.signature {
                // Minimal: elide keywords; Pretty: highlight with AST
                let sig_display = if options.minimal {
                    elide_keywords(sig)
                } else if let Some(grammar) = &node.grammar {
                    // Get language-specific suffix for parsing incomplete signatures
                    let suffix = support_for_grammar(grammar)
                        .map(|lang| lang.signature_suffix())
                        .unwrap_or("");

                    // Append suffix so tree-sitter can parse the fragment correctly
                    let parseable = if suffix.is_empty() {
                        sig.clone()
                    } else {
                        format!("{}{}", sig, suffix)
                    };

                    let highlighted = highlight_source(&parseable, grammar, options.use_colors);

                    // Strip the appended suffix from output (handle ANSI codes)
                    if !suffix.is_empty() {
                        strip_suffix_with_ansi(&highlighted, suffix)
                    } else {
                        highlighted
                    }
                } else {
                    sig.clone()
                };
                format!("{}:", sig_display)
            } else {
                format!("{}:", node.name)
            }
        }
        _ => node.name.clone(),
    };

    // Add line info for symbols if requested
    if options.line_numbers {
        if let Some((start, end)) = node.line_range {
            return format!("{} L{}-{}", base, start, end);
        }
    }

    base
}

/// Elide visibility and declaration keywords for minimal output.
fn elide_keywords(sig: &str) -> String {
    let mut s = sig.to_string();
    // Visibility
    for kw in [
        "pub ",
        "pub(crate) ",
        "pub(super) ",
        "pub(self) ",
        "private ",
    ] {
        s = s.replacen(kw, "", 1);
    }
    // Declaration keywords (keep the name/signature, remove the keyword)
    for kw in [
        "fn ",
        "async fn ",
        "const fn ",
        "unsafe fn ",
        "struct ",
        "enum ",
        "trait ",
        "impl ",
        "type ",
        "const ",
        "static ",
        "mod ",
        "class ",
        "def ",
        "async def ",
    ] {
        s = s.replacen(kw, "", 1);
    }
    s
}

/// Strip a suffix from highlighted text, handling ANSI escape codes.
/// The suffix may be highlighted with its own colors, so we need to find
/// and remove both the text and any surrounding ANSI codes.
fn strip_suffix_with_ansi(highlighted: &str, suffix: &str) -> String {
    // Find the first character of the suffix to locate where it starts
    let suffix_first_char = match suffix.trim_start().chars().next() {
        Some(c) => c,
        None => return highlighted.to_string(),
    };

    // Find the last occurrence of this character (could be highlighted differently)
    if let Some(pos) = highlighted.rfind(suffix_first_char) {
        let mut result = highlighted[..pos].to_string();

        // Strip trailing ANSI escape sequences and whitespace
        loop {
            let trimmed = result.trim_end();
            if trimmed.len() < result.len() {
                result = trimmed.to_string();
                continue;
            }
            // Check for trailing ANSI escape: must end with 'm' preceded by digits/semicolons after ESC[
            if result.ends_with('m') {
                if let Some(esc_pos) = result.rfind("\x1b[") {
                    // Verify everything between ESC[ and m is digits/semicolons
                    let params = &result[esc_pos + 2..result.len() - 1];
                    if params.chars().all(|c| c.is_ascii_digit() || c == ';') {
                        result.truncate(esc_pos);
                        continue;
                    }
                }
            }
            break;
        }
        result
    } else {
        highlighted.to_string()
    }
}

/// A span of text with highlight information.
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub kind: HighlightKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HighlightKind {
    Keyword,
    Type,
    Comment,
    String,
    Number,
    Constant, // true, false, nil, null, undefined, NaN, Infinity
    Attribute,
    FunctionName,
    Default,
}

/// Highlight source code using AST-based syntax highlighting.
///
/// Uses tree-sitter Query API with .scm highlight files when available,
/// falling back to manual node classification otherwise.
/// Also processes injections for embedded languages (e.g., code blocks in markdown).
pub fn highlight_source(source: &str, grammar: &str, use_colors: bool) -> String {
    // If colors disabled, just return the source as-is
    if !use_colors {
        return source.to_string();
    }

    let loader = grammar_loader();
    let language = match loader.get(grammar) {
        Some(lang) => lang,
        None => return source.to_string(),
    };

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return source.to_string();
    }

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return source.to_string(),
    };

    // Collect base highlight spans (use cached queries)
    let mut spans = if let Some(query) = get_highlight_query(grammar, &language) {
        collect_query_spans(&query, tree.root_node(), source)
    } else {
        collect_manual_spans(tree.root_node())
    };

    // Process injections (embedded languages like code blocks in markdown)
    if let Some(injection_query) = get_injection_query(grammar, &language) {
        let injection_spans = collect_injection_spans(
            &injection_query,
            tree.root_node(),
            source,
            &*grammar_loader(),
        );
        // Injection spans take precedence - remove base spans that overlap
        spans = merge_injection_spans(spans, injection_spans);
    }

    render_highlighted(source, spans)
}

/// Collect highlight spans from injected languages.
fn collect_injection_spans(
    query: &Query,
    root: tree_sitter::Node,
    source: &str,
    loader: &GrammarLoader,
) -> Vec<HighlightSpan> {
    let mut cursor = QueryCursor::new();
    let mut spans = Vec::new();

    // Find capture indices for injection.language and injection.content
    let lang_idx = query
        .capture_names()
        .iter()
        .position(|n| *n == "injection.language");
    let content_idx = query
        .capture_names()
        .iter()
        .position(|n| *n == "injection.content");

    if content_idx.is_none() {
        return spans;
    }
    let content_idx = content_idx.unwrap() as u32;

    let mut matches = cursor.matches(query, root, source.as_bytes());
    while let Some(match_) = matches.next() {
        let mut lang_name: Option<String> = None;
        let mut content_node: Option<tree_sitter::Node> = None;

        for capture in match_.captures {
            if Some(capture.index as usize) == lang_idx {
                // Extract language name from the captured node
                let text = &source[capture.node.byte_range()];
                lang_name = Some(normalize_language_name(text));
            } else if capture.index == content_idx {
                content_node = Some(capture.node);
            }
        }

        // Check for #set! injection.language directive in properties
        if lang_name.is_none() {
            for prop in query.property_settings(match_.pattern_index) {
                if &*prop.key == "injection.language" {
                    if let Some(val) = &prop.value {
                        lang_name = Some(val.to_string());
                    }
                }
            }
        }

        if let (Some(lang), Some(node)) = (lang_name, content_node) {
            // Check if we can load this language
            if loader.get(&lang).is_some() {
                let content = &source[node.byte_range()];
                let offset = node.start_byte();

                // Recursively highlight the injected content
                let inner_spans = collect_inner_spans(content, &lang, offset);
                spans.extend(inner_spans);
            }
        }
    }

    spans
}

/// Collect spans for injected content, adjusting offsets.
fn collect_inner_spans(content: &str, grammar: &str, offset: usize) -> Vec<HighlightSpan> {
    let loader = grammar_loader();
    let language = match loader.get(grammar) {
        Some(lang) => lang,
        None => return Vec::new(),
    };

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }

    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    // Use cached queries
    let mut spans = if let Some(query) = get_highlight_query(grammar, &language) {
        collect_query_spans(&query, tree.root_node(), content)
    } else {
        collect_manual_spans(tree.root_node())
    };

    // Adjust offsets to be relative to original source
    for span in &mut spans {
        span.start += offset;
        span.end += offset;
    }

    // Recursively process injections in the injected content
    if let Some(injection_query) = get_injection_query(grammar, &language) {
        let nested_spans =
            collect_injection_spans(&injection_query, tree.root_node(), content, &*loader);
        // Adjust nested span offsets
        let adjusted: Vec<_> = nested_spans
            .into_iter()
            .map(|mut s| {
                s.start += offset;
                s.end += offset;
                s
            })
            .collect();
        spans = merge_injection_spans(spans, adjusted);
    }

    spans
}

/// Merge injection spans with base spans.
/// Injection spans take precedence - remove overlapping base spans.
fn merge_injection_spans(
    mut base: Vec<HighlightSpan>,
    injections: Vec<HighlightSpan>,
) -> Vec<HighlightSpan> {
    if injections.is_empty() {
        return base;
    }

    // Remove base spans that overlap with injection spans
    base.retain(|b| {
        !injections
            .iter()
            .any(|i| b.start < i.end && b.end > i.start)
    });

    base.extend(injections);
    base
}

/// Normalize language name for grammar lookup.
fn normalize_language_name(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "js" | "javascript" => "javascript".to_string(),
        "ts" | "typescript" => "typescript".to_string(),
        "py" | "python" | "python3" => "python".to_string(),
        "rb" | "ruby" => "ruby".to_string(),
        "rs" | "rust" => "rust".to_string(),
        "sh" | "bash" | "shell" | "zsh" => "bash".to_string(),
        "yml" => "yaml".to_string(),
        "md" => "markdown".to_string(),
        "cpp" | "c++" => "cpp".to_string(),
        "cs" | "csharp" => "c-sharp".to_string(),
        "dockerfile" => "dockerfile".to_string(),
        _ => name.to_lowercase(),
    }
}

/// Collect highlight spans using tree-sitter Query API.
fn collect_query_spans(query: &Query, root: tree_sitter::Node, source: &str) -> Vec<HighlightSpan> {
    let mut cursor = QueryCursor::new();
    let mut spans = Vec::new();

    let mut matches = cursor.matches(query, root, source.as_bytes());
    while let Some(match_) = matches.next() {
        for capture in match_.captures {
            let capture_name = &query.capture_names()[capture.index as usize];
            if let Some(kind) = capture_name_to_highlight_kind(capture_name) {
                spans.push(HighlightSpan {
                    start: capture.node.start_byte(),
                    end: capture.node.end_byte(),
                    kind,
                });
            }
        }
    }

    spans
}

/// Map tree-sitter query capture names to HighlightKind.
///
/// Standard capture names from tree-sitter highlight queries:
/// <https://tree-sitter.github.io/tree-sitter/syntax-highlighting#theme>
fn capture_name_to_highlight_kind(name: &str) -> Option<HighlightKind> {
    // Check full name first for specific mappings (document formats, etc.)
    match name {
        // Document formats: headings, titles
        "text.title" | "markup.heading" | "title" => return Some(HighlightKind::Keyword),

        // Document formats: code blocks, literals, raw text
        "text.literal" | "markup.raw" | "markup.raw.inline" | "markup.raw.block" => {
            return Some(HighlightKind::String);
        }

        // Document formats: links, URIs
        "text.uri" | "markup.link" | "markup.link.url" | "text.reference" => {
            return Some(HighlightKind::Attribute);
        }

        // Document formats: bold, italic (treat as keywords for emphasis)
        "text.strong" | "text.emphasis" | "markup.bold" | "markup.italic" => {
            return Some(HighlightKind::Keyword);
        }

        // Explicitly skip these (no highlighting)
        "none" | "text" | "markup" => return None,

        _ => {}
    }

    // Match on base name (before any dot) for code-oriented captures
    let base = name.split('.').next().unwrap_or(name);

    match base {
        // Keywords
        "keyword" => Some(HighlightKind::Keyword),

        // Types
        "type" => Some(HighlightKind::Type),

        // Comments
        "comment" => Some(HighlightKind::Comment),

        // Strings
        "string" | "character" => Some(HighlightKind::String),

        // Numbers
        "number" | "float" => Some(HighlightKind::Number),

        // Constants (boolean, nil, etc.)
        "constant" | "boolean" => Some(HighlightKind::Constant),

        // Attributes/annotations
        "attribute" => Some(HighlightKind::Attribute),

        // Functions
        "function" | "method" => Some(HighlightKind::FunctionName),

        // Also treat constructors and some operators as keywords
        "constructor" | "operator" => Some(HighlightKind::Keyword),

        // Control flow (if/else, for/while, try/catch) - highlight as keywords
        "conditional" | "repeat" | "exception" => Some(HighlightKind::Keyword),

        // Punctuation - subtle highlighting (grey like comments)
        "punctuation" => Some(HighlightKind::Comment),

        // Tags (HTML/XML elements) - highlight as keywords
        "tag" => Some(HighlightKind::Keyword),

        // Properties/fields - show as default (not highlighted)
        "property" | "field" | "variable" | "parameter" | "label" | "namespace" | "module"
        | "include" | "define" | "preproc" | "storageclass" | "structure" | "text" | "title"
        | "uri" | "underline" | "todo" | "note" | "warning" | "danger" | "embedded" | "error"
        | "conceal" | "spell" | "diff" | "debug" | "symbol" | "identifier" | "markup" => None,

        _ => None,
    }
}

/// Collect highlight spans using manual node classification (fallback).
fn collect_manual_spans(root: tree_sitter::Node) -> Vec<HighlightSpan> {
    let mut spans = Vec::new();
    collect_highlight_spans(root, &mut spans);
    spans
}

/// Render source with highlight spans applied.
fn render_highlighted(source: &str, mut spans: Vec<HighlightSpan>) -> String {
    // Sort spans by start position
    spans.sort_by_key(|s| s.start);

    // Remove overlapping spans - keep the first one encountered
    let mut filtered: Vec<HighlightSpan> = Vec::new();
    for span in spans {
        let overlaps = filtered
            .iter()
            .any(|existing| span.start < existing.end && span.end > existing.start);
        if !overlaps {
            filtered.push(span);
        }
    }

    // Build highlighted string
    let mut result = String::new();
    let mut pos = 0;

    for span in filtered {
        // Skip if we've passed this span already
        if span.start < pos {
            continue;
        }

        // Add unhighlighted text before this span
        if span.start > pos {
            result.push_str(&source[pos..span.start]);
        }

        // Add highlighted span (Monokai-inspired colors)
        let text = &source[span.start..span.end];
        let styled = match span.kind {
            HighlightKind::Keyword => Red.paint(text).to_string(), // Red for keywords
            HighlightKind::Type => LightCyan.paint(text).to_string(), // Light cyan for types
            HighlightKind::Comment => LightGray.paint(text).to_string(), // Grey for comments
            HighlightKind::String => LightGreen.paint(text).to_string(), // Light green for strings
            HighlightKind::Number => LightMagenta.paint(text).to_string(), // Magenta for numbers
            HighlightKind::Constant => LightMagenta.paint(text).to_string(), // Magenta for constants
            HighlightKind::Attribute => LightCyan.paint(text).to_string(),   // Cyan for attributes
            HighlightKind::FunctionName => Yellow.paint(text).to_string(),   // Yellow for functions
            HighlightKind::Default => text.to_string(),
        };
        result.push_str(&styled);
        pos = span.end;
    }

    // Add remaining text
    if pos < source.len() {
        result.push_str(&source[pos..]);
    }

    result
}

/// Collect highlight spans from AST nodes.
pub fn collect_highlight_spans(node: tree_sitter::Node, spans: &mut Vec<HighlightSpan>) {
    let kind = node.kind();
    let highlight = classify_node_kind(kind);

    // Comments, strings, attributes, numbers: highlight entire node (don't recurse into children)
    // Numbers are included because CSS integer_value/float_value have child nodes (unit)
    if matches!(
        highlight,
        HighlightKind::Comment
            | HighlightKind::String
            | HighlightKind::Attribute
            | HighlightKind::Number
    ) {
        spans.push(HighlightSpan {
            start: node.start_byte(),
            end: node.end_byte(),
            kind: highlight,
        });
        return; // Don't recurse - these are single units
    }

    // Only highlight leaf nodes (no children) to avoid duplication
    // This means keywords and simple type identifiers get highlighted,
    // but complex types like `Vec<String>` highlight `Vec` and `String` separately
    if highlight != HighlightKind::Default && node.child_count() == 0 {
        spans.push(HighlightSpan {
            start: node.start_byte(),
            end: node.end_byte(),
            kind: highlight,
        });
    }

    // Check for function/method names and calls
    if node.child_count() == 0 {
        if let Some(parent) = node.parent() {
            let parent_kind = parent.kind();

            // Function/method definitions
            if kind == "identifier"
                && matches!(
                    parent_kind,
                    "function_item"
                        | "function_signature_item"
                        | "function_definition"
                        | "method_definition"
                        | "function_declaration"
                )
            {
                spans.push(HighlightSpan {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    kind: HighlightKind::FunctionName,
                });
            }

            // Function/macro calls: foo() - Rust: call_expression/macro_invocation, JS: call_expression, Python: call, Lua: function_call
            if kind == "identifier"
                && matches!(
                    parent_kind,
                    "call_expression" | "call" | "macro_invocation" | "function_call"
                )
            {
                spans.push(HighlightSpan {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    kind: HighlightKind::FunctionName,
                });
            }

            // Scoped function calls: serde_json::to_value()
            // identifier → scoped_identifier → call_expression
            // Only highlight the last identifier (the function name, not module path)
            if kind == "identifier" && parent_kind == "scoped_identifier" {
                if let Some(grandparent) = parent.parent() {
                    if matches!(grandparent.kind(), "call_expression" | "call") {
                        // Check this is the last identifier (no :: after it)
                        let is_last = node.next_sibling().map_or(true, |s| s.kind() != "::");
                        if is_last {
                            spans.push(HighlightSpan {
                                start: node.start_byte(),
                                end: node.end_byte(),
                                kind: HighlightKind::FunctionName,
                            });
                        }
                    }
                }
            }

            // Method calls: bar.baz()
            // - Rust: field_identifier → field_expression → call_expression
            // - JS/TS: property_identifier → member_expression → call_expression
            // - Python: identifier → attribute → call
            // - Lua: identifier → dot_index_expression/method_index_expression → function_call
            //   For Lua, only the second identifier (after . or :) is the method name
            let is_method_id = matches!(kind, "field_identifier" | "property_identifier")
                || (kind == "identifier" && parent_kind == "attribute");
            let is_lua_method = kind == "identifier"
                && matches!(
                    parent_kind,
                    "dot_index_expression" | "method_index_expression"
                )
                && node
                    .prev_sibling()
                    .map_or(false, |s| matches!(s.kind(), "." | ":"));
            let is_method_parent = matches!(
                parent_kind,
                "field_expression"
                    | "member_expression"
                    | "attribute"
                    | "dot_index_expression"
                    | "method_index_expression"
            );
            if (is_method_id || is_lua_method) && is_method_parent {
                if let Some(grandparent) = parent.parent() {
                    if matches!(
                        grandparent.kind(),
                        "call_expression" | "call" | "function_call"
                    ) {
                        spans.push(HighlightSpan {
                            start: node.start_byte(),
                            end: node.end_byte(),
                            kind: HighlightKind::FunctionName,
                        });
                    }
                }
            }

            // Inside macro token_tree: heuristic for function/method calls
            // Pattern: identifier followed by token_tree starting with (
            // Or: identifier preceded by . (method call)
            if kind == "identifier" && parent_kind == "token_tree" {
                if let Some(next) = node.next_sibling() {
                    // Check if next sibling is () or token_tree starting with (
                    let next_kind = next.kind();
                    if next_kind == "token_tree" || next_kind == "(" {
                        spans.push(HighlightSpan {
                            start: node.start_byte(),
                            end: node.end_byte(),
                            kind: HighlightKind::FunctionName,
                        });
                    }
                }
            }

            // XML: Anonymous Name nodes in tag/attribute context
            // STag > Name (tag name), ETag > Name (closing tag), Attribute > Name (attr name)
            if kind == "Name" && matches!(parent_kind, "STag" | "ETag" | "Attribute") {
                spans.push(HighlightSpan {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    kind: HighlightKind::Keyword,
                });
            }
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_highlight_spans(child, spans);
    }
}

/// Classify a node kind into a highlight category.
fn classify_node_kind(kind: &str) -> HighlightKind {
    match kind {
        // Comments (including doc comments)
        "comment"
        | "line_comment"
        | "block_comment"
        | "doc_comment"
        | "line_outer_doc_comment"
        | "line_inner_doc_comment"
        | "block_outer_doc_comment"
        | "block_inner_doc_comment" => HighlightKind::Comment,

        // Attributes / proc macros (#[derive(...)], @decorator, etc.)
        "attribute_item" | "inner_attribute_item" | "decorator" => HighlightKind::Attribute,

        // Strings (including template/interpolated strings)
        "string_literal"
        | "raw_string_literal"
        | "string"
        | "string_content"
        | "string_fragment"
        | "interpreted_string_literal"
        | "char_literal"
        | "template_string"
        | "template_literal"
        // YAML strings
        | "string_scalar"
        | "double_quote_scalar"
        | "single_quote_scalar"
        // HTML/CSS strings
        | "quoted_attribute_value"
        | "string_value"
        // XML attribute values
        | "AttValue" => HighlightKind::String,

        // Numbers
        "number"
        | "integer"
        | "float"
        | "integer_literal"
        | "float_literal"
        | "int_literal"
        | "imaginary_literal"
        | "rune_literal"
        // C/C++ numbers
        | "number_literal"
        // YAML numbers
        | "integer_scalar"
        | "float_scalar"
        // CSS numbers
        | "integer_value"
        | "float_value" => HighlightKind::Number,

        // Constants (booleans, nil/null, special values)
        "true"
        | "false"
        | "boolean_literal"
        | "nil"
        | "null"
        | "none"
        | "undefined"
        // Java null
        | "null_literal"
        // TOML booleans
        | "boolean"
        // YAML booleans
        | "boolean_scalar" => HighlightKind::Constant,

        // Types (check first - more specific)
        "type_identifier"
        | "primitive_type"
        | "generic_type"
        | "scoped_type_identifier"
        | "builtin_type" => HighlightKind::Type,

        // Keywords (cross-language)
        "fn" | "function" | "def" | "async" | "await" | "pub" | "struct" | "enum" | "trait"
        | "impl" | "type" | "const" | "static" | "let" | "mut" | "ref" | "class" | "interface"
        | "extends" | "implements" | "import" | "from" | "export" | "return" | "if" | "else"
        | "for" | "while" | "loop" | "match" | "where" | "self" | "Self" | "super" | "crate"
        | "mod" | "use" | "as" | "in" | "unsafe" | "extern" | "dyn"
        // Lua keywords
        | "local" | "end" | "then" | "do" | "elseif" | "repeat" | "until" | "and" | "or"
        | "not" | "break" | "goto"
        // Python keywords
        | "elif" | "except" | "finally" | "try" | "with" | "yield" | "lambda" | "pass"
        | "raise" | "assert" | "global" | "nonlocal" | "del" | "is"
        // JS/TS keywords
        | "var" | "new" | "this" | "throw" | "catch" | "switch" | "case" | "default"
        | "continue" | "debugger" | "delete" | "instanceof" | "typeof" | "void"
        // Go keywords
        | "package" | "func" | "defer" | "go" | "chan" | "select" | "fallthrough"
        | "range" | "map"
        // Bash keywords (then/elif already covered above)
        | "fi" | "esac" | "done"
        // CSS/HTML/SCSS
        | "property_name" | "tag_name" | "attribute_name" | "variable" => HighlightKind::Keyword,

        _ => HighlightKind::Default,
    }
}

/// Check if a docstring is useless (just repeats the name).
fn is_useless_docstring(name: &str, doc_line: &str) -> bool {
    let doc_lower = doc_line.to_lowercase();
    let name_lower = name.to_lowercase();

    // Docstring is just the name, optionally with "function" or "class" etc.
    if doc_lower == name_lower
        || doc_lower == format!("{} function", name_lower)
        || doc_lower == format!("{} method", name_lower)
        || doc_lower == format!("{} class", name_lower)
        || doc_lower == format!("the {} function", name_lower)
        || doc_lower == format!("the {} method", name_lower)
        || doc_lower == format!("a {} function", name_lower)
    {
        return true;
    }

    // Very short and starts with the name
    doc_lower.len() < name_lower.len() + 10 && doc_lower.starts_with(&name_lower)
}

fn format_children(
    children: &[ViewNode],
    prefix: &str,
    lines: &mut Vec<String>,
    options: &FormatOptions,
    depth: usize,
    grammar: Option<&str>,
) {
    // Check depth limit
    if let Some(max) = options.max_depth {
        if depth >= max {
            return;
        }
    }

    let count = children.len();
    for (i, child) in children.iter().enumerate() {
        let _is_last = i == count - 1;

        // Always use plain indentation (no box-drawing chars)
        let child_prefix = format!("{}  ", prefix);

        // Format child line using shared formatter
        let child_line = format_node_line(child, options);
        lines.push(format!("{}{}", prefix, child_line));

        // Use child's grammar if available, otherwise inherit from parent
        let child_grammar = child.grammar.as_deref().or(grammar);

        // Add docstring based on display mode
        if let Some(doc) = &child.docstring {
            let doc_prefix = format!("{}    ", child_prefix);
            format_docstring(
                doc,
                &child.name,
                &doc_prefix,
                options.docstrings,
                child_grammar,
                lines,
            );
        }

        // Recurse into children
        if !child.children.is_empty() {
            format_children(
                &child.children,
                &child_prefix,
                lines,
                options,
                depth + 1,
                child_grammar,
            );
        }
    }
}

/// Default boilerplate directories that don't count against depth limit.
/// These are common structural directories that add noise without information.
pub const DEFAULT_BOILERPLATE_DIRS: &[&str] =
    &["src", "lib", "pkg", "packages", "crates", "internal", "cmd"];

/// Options for tree generation
#[derive(Clone)]
pub struct TreeOptions {
    /// Maximum depth to traverse (None = unlimited)
    pub max_depth: Option<usize>,
    /// Collapse single-child directory chains (src/foo/bar/ → one line)
    pub collapse_single: bool,
    /// Directories that don't count against depth limit (smart depth)
    pub boilerplate_dirs: HashSet<String>,
    /// Include symbols inside files (requires depth > 1)
    pub include_symbols: bool,
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self {
            max_depth: None,
            collapse_single: true,
            boilerplate_dirs: DEFAULT_BOILERPLATE_DIRS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            include_symbols: false,
        }
    }
}

/// A node in the internal file tree (used during construction).
#[derive(Default)]
struct InternalTreeNode {
    children: BTreeMap<String, InternalTreeNode>,
    is_dir: bool,
}

impl InternalTreeNode {
    fn add_path(
        &mut self,
        parts: &[&str],
        is_dir: bool,
        max_depth: Option<usize>,
        boilerplate_dirs: &HashSet<String>,
        effective_depth: usize,
    ) {
        if parts.is_empty() {
            return;
        }

        let name = parts[0];

        // Check if we've exceeded max depth (using effective depth that excludes boilerplate)
        // Boilerplate dirs themselves are always shown, but they don't increase depth
        if let Some(max) = max_depth {
            let is_boilerplate = boilerplate_dirs.contains(name);
            // Block if we're at max depth, unless this is a boilerplate dir (which gets a pass)
            if effective_depth >= max && !is_boilerplate {
                return;
            }
        }

        let child = self.children.entry(name.to_string()).or_default();

        if parts.len() == 1 {
            child.is_dir = is_dir;
        } else {
            child.is_dir = true; // intermediate nodes are directories
            // Boilerplate dirs don't count against depth
            let next_depth = if boilerplate_dirs.contains(name) {
                effective_depth
            } else {
                effective_depth + 1
            };
            child.add_path(&parts[1..], is_dir, max_depth, boilerplate_dirs, next_depth);
        }
    }
}

/// Generate a ViewNode tree for a directory.
///
/// Returns a unified ViewNode that can be formatted consistently with file and symbol views.
pub fn generate_view_tree(root: &Path, options: &TreeOptions) -> ViewNode {
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    // Don't use WalkBuilder's max_depth - we handle it with smart depth (boilerplate awareness)
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    let mut tree = InternalTreeNode::default();
    tree.is_dir = true;

    for entry in walker.flatten() {
        let path = entry.path();
        if path == root {
            continue;
        }

        if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy();
            if rel_str.is_empty() {
                continue;
            }

            let is_dir = path.is_dir();
            let parts: Vec<&str> = rel_str.split('/').filter(|s| !s.is_empty()).collect();
            if !parts.is_empty() {
                tree.add_path(
                    &parts,
                    is_dir,
                    options.max_depth,
                    &options.boilerplate_dirs,
                    0,
                );
            }
        }
    }

    // Convert internal tree to ViewNode
    tree_node_to_view_node(&root_name, "", &tree, options, root)
}

/// Convert internal TreeNode to ViewNode recursively.
fn tree_node_to_view_node(
    name: &str,
    parent_path: &str,
    node: &InternalTreeNode,
    options: &TreeOptions,
    fs_root: &Path,
) -> ViewNode {
    let path = if parent_path.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent_path, name)
    };

    let kind = if node.is_dir {
        ViewNodeKind::Directory
    } else {
        ViewNodeKind::File
    };

    // Collect and sort children: directories first, then alphabetically
    let mut children_vec: Vec<_> = node.children.iter().collect();
    children_vec.sort_by(|(a_name, a_node), (b_name, b_node)| {
        match (b_node.is_dir, a_node.is_dir) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a_name.to_lowercase().cmp(&b_name.to_lowercase()),
        }
    });

    // Handle single-child chain collapsing
    let (final_name, final_path, mut children) = if options.collapse_single && node.is_dir {
        let chain = collect_single_chain_internal(node, name);
        let collapsed_path = if parent_path.is_empty() {
            chain.path.clone()
        } else {
            format!("{}/{}", parent_path, chain.path)
        };

        let child_nodes: Vec<ViewNode> = chain
            .end_node
            .children
            .iter()
            .map(|(child_name, child_node)| {
                tree_node_to_view_node(child_name, &collapsed_path, child_node, options, fs_root)
            })
            .collect();

        (chain.path, collapsed_path, child_nodes)
    } else {
        let child_nodes: Vec<ViewNode> = children_vec
            .into_iter()
            .map(|(child_name, child_node)| {
                tree_node_to_view_node(child_name, &path, child_node, options, fs_root)
            })
            .collect();

        (name.to_string(), path, child_nodes)
    };

    // For files, extract symbols if include_symbols is enabled
    if !node.is_dir && options.include_symbols {
        // Get the actual file path on disk
        let file_path = fs_root.parent().unwrap_or(fs_root).join(&final_path);
        if let Some(symbol_children) = extract_file_symbols(&file_path, &final_path) {
            children = symbol_children;
        }
    }

    ViewNode {
        name: final_name,
        kind,
        path: final_path,
        children,
        signature: None,
        docstring: None,
        line_range: None,
        grammar: None,
    }
}

/// Extract symbols from a file and convert to ViewNodes.
fn extract_file_symbols(file_path: &Path, view_path: &str) -> Option<Vec<ViewNode>> {
    // Check if file has language support
    let support = support_for_path(file_path)?;
    let grammar = support.grammar_name().to_string();

    // Read file content
    let content = std::fs::read_to_string(file_path).ok()?;

    // Extract skeleton
    let extractor = SkeletonExtractor::new();
    let result = extractor.extract(file_path, &content);

    if result.symbols.is_empty() {
        return None;
    }

    // Convert to ViewNodes
    let children: Vec<ViewNode> = result
        .symbols
        .iter()
        .map(|sym| skeleton_to_view_node(sym, view_path, &grammar))
        .collect();

    Some(children)
}

/// Convert a SkeletonSymbol to a ViewNode.
fn skeleton_to_view_node(sym: &SkeletonSymbol, parent_path: &str, grammar: &str) -> ViewNode {
    let path = format!("{}/{}", parent_path, sym.name);

    let children: Vec<ViewNode> = sym
        .children
        .iter()
        .map(|child| skeleton_to_view_node(child, &path, grammar))
        .collect();

    ViewNode {
        name: sym.name.clone(),
        kind: ViewNodeKind::Symbol(sym.kind.as_str().to_string()),
        path,
        children,
        signature: Some(sym.signature.clone()),
        docstring: sym.docstring.clone(),
        line_range: Some((sym.start_line, sym.end_line)),
        grammar: Some(grammar.to_string()),
    }
}

/// Collect a chain of single-child directories (for ViewNode conversion).
fn collect_single_chain_internal<'a>(
    node: &'a InternalTreeNode,
    name: &str,
) -> CollapsedChain<'a, InternalTreeNode> {
    let mut current = node;
    let mut path = name.to_string();

    loop {
        if current.children.len() != 1 {
            break;
        }
        let (child_name, child_node) = current.children.iter().next().unwrap();
        if !child_node.is_dir {
            break;
        }
        path.push('/');
        path.push_str(child_name);
        current = child_node;
    }

    CollapsedChain {
        path,
        end_node: current,
    }
}

/// Result of collapsing a chain of single-child directories.
struct CollapsedChain<'a, T> {
    path: String,
    end_node: &'a T,
}

// Highlighting tests: see `highlight_tests.rs`

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_view_tree() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/foo")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "").unwrap();
        fs::write(dir.path().join("src/foo/bar.rs"), "").unwrap();
        fs::write(dir.path().join("README.md"), "").unwrap();

        let result = generate_view_tree(dir.path(), &TreeOptions::default());

        // Should have directory node with children
        assert_eq!(result.kind, ViewNodeKind::Directory);
        assert!(!result.children.is_empty());
    }

    #[test]
    fn test_view_tree_max_depth() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("alpha/beta/gamma")).unwrap();
        fs::write(dir.path().join("alpha/beta/gamma/file.txt"), "").unwrap();

        let result = generate_view_tree(
            dir.path(),
            &TreeOptions {
                max_depth: Some(2),
                collapse_single: false,
                boilerplate_dirs: HashSet::new(),
                include_symbols: false,
            },
        );

        // Should return a ViewNode structure
        assert_eq!(result.kind, ViewNodeKind::Directory);
    }
}
