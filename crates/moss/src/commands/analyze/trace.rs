//! Value provenance tracing for symbols.

use crate::index;
use crate::parsers;
use crate::path_resolve::resolve_unified;
use std::collections::HashMap;
use std::path::Path;

/// Trace value provenance for a symbol.
pub fn cmd_trace(
    symbol: &str,
    target: Option<&str>,
    root: &Path,
    max_depth: usize,
    json: bool,
    pretty: bool,
) -> i32 {
    // Parse the symbol argument as a unified path (file/symbol)
    let (file_path, symbol_name) = if let Some(unified) = resolve_unified(symbol, root) {
        if unified.symbol_path.is_empty() {
            eprintln!("No symbol specified in path: {}", symbol);
            return 1;
        }
        (Some(unified.file_path), unified.symbol_path.join("."))
    } else if let Some(t) = target {
        // Legacy: separate --target and symbol args
        (Some(t.to_string()), symbol.to_string())
    } else {
        // Try as a global symbol name (index lookup)
        (None, symbol.to_string())
    };

    // Find the symbol - try index first, fall back to file parsing
    let symbol_matches = if let Some(mut idx) = index::FileIndex::open_if_enabled(root) {
        let _ = idx.incremental_refresh();
        match idx.find_symbols(&symbol_name, None, false, 10) {
            Ok(matches) if !matches.is_empty() => matches,
            _ => fallback_parse_symbol(&symbol_name, file_path.as_deref(), root),
        }
    } else {
        fallback_parse_symbol(&symbol_name, file_path.as_deref(), root)
    };

    if symbol_matches.is_empty() {
        eprintln!("Symbol not found: {}", symbol);
        return 1;
    }

    let sym = &symbol_matches[0];
    let full_path = root.join(&sym.file);
    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return 1;
        }
    };

    // Parse the file
    let lang = match moss_languages::support_for_path(&full_path) {
        Some(l) => l,
        None => {
            eprintln!("No language support for file");
            return 1;
        }
    };
    let tree = match parsers::parse_with_grammar(lang.grammar_name(), &content) {
        Some(t) => t,
        None => {
            eprintln!("Failed to parse file");
            return 1;
        }
    };

    let source_bytes = content.as_bytes();

    let start_line = sym.start_line;
    let end_line = sym.end_line;

    // Build signature map for same-file function lookups (name -> (signature, line))
    let extractor = crate::extract::Extractor::new();
    let extract_result = extractor.extract(&full_path, &content);
    let mut signature_map: HashMap<String, (String, usize)> = HashMap::new();
    fn collect_signatures(
        sym: &moss_languages::Symbol,
        map: &mut HashMap<String, (String, usize)>,
    ) {
        if !sym.signature.is_empty() {
            map.insert(sym.name.clone(), (sym.signature.clone(), sym.start_line));
        }
        for child in &sym.children {
            collect_signatures(child, map);
        }
    }
    for sym in &extract_result.symbols {
        collect_signatures(sym, &mut signature_map);
    }

    // Trace assignments within the function
    let trace_results = trace_assignments(
        &tree.root_node(),
        source_bytes,
        start_line,
        end_line,
        max_depth,
        &signature_map,
    );

    if json {
        let trace_json: Vec<serde_json::Value> = trace_results
            .iter()
            .map(|t| {
                let calls_json: Vec<serde_json::Value> = t
                    .calls
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "name": c.name,
                            "signature": c.signature,
                            "defined_at": c.defined_at
                        })
                    })
                    .collect();
                serde_json::json!({
                    "variable": t.variable,
                    "line": t.line,
                    "source": t.source,
                    "flows_from": t.flows_from,
                    "is_terminal": t.is_terminal,
                    "calls": calls_json,
                    "branch": t.branch_context
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "symbol": symbol,
                "file": sym.file,
                "start_line": start_line,
                "end_line": end_line,
                "trace": trace_json
            })
        );
        return 0;
    }

    // Pretty output
    println!("# Trace: {} ({}:{})", symbol, sym.file, start_line);
    println!();

    if trace_results.is_empty() {
        println!("No assignments found in this symbol.");
    } else {
        for t in &trace_results {
            let flows = if t.is_terminal {
                " (terminal)".to_string()
            } else if t.flows_from.is_empty() {
                String::new()
            } else {
                format!(" ← {}", t.flows_from.join(", "))
            };

            // Format calls info (with signatures and locations when available)
            let calls_info = if t.calls.is_empty() {
                String::new()
            } else {
                let call_strs: Vec<String> = t
                    .calls
                    .iter()
                    .map(|c| {
                        let mut s = c.name.clone();
                        if let Some(ref sig) = c.signature {
                            s = format!("{}({})", s, sig);
                        }
                        if let Some(line) = c.defined_at {
                            s = format!("{} @L{}", s, line);
                        }
                        s
                    })
                    .collect();
                format!(" [calls: {}]", call_strs.join(", "))
            };

            // Format branch context
            let branch_info = t
                .branch_context
                .as_ref()
                .map(|b| format!(" ({})", b))
                .unwrap_or_default();

            if pretty {
                let flows_colored = if t.is_terminal {
                    nu_ansi_term::Color::Green.paint(&flows).to_string()
                } else {
                    nu_ansi_term::Color::DarkGray.paint(&flows).to_string()
                };
                let calls_colored = nu_ansi_term::Color::Magenta.paint(&calls_info).to_string();
                let branch_colored = nu_ansi_term::Color::Blue.paint(&branch_info).to_string();
                println!(
                    "  L{}: {} = {}{}{}{}",
                    nu_ansi_term::Color::Yellow.paint(t.line.to_string()),
                    nu_ansi_term::Color::Cyan.paint(&t.variable),
                    t.source,
                    flows_colored,
                    calls_colored,
                    branch_colored
                );
            } else {
                println!(
                    "  L{}: {} = {}{}{}{}",
                    t.line, t.variable, t.source, flows, calls_info, branch_info
                );
            }
        }
    }

    0
}

/// Parse a file to find a symbol (fallback when index unavailable/empty).
fn fallback_parse_symbol(
    symbol: &str,
    target: Option<&str>,
    root: &Path,
) -> Vec<index::SymbolMatch> {
    let Some(path) = target else {
        return Vec::new();
    };

    let full_path = root.join(path);
    if !full_path.exists() {
        return Vec::new();
    }

    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let parser = crate::symbols::SymbolParser::new();
    let symbols = parser.parse_file(&full_path, &content);
    symbols
        .into_iter()
        .filter(|s| {
            s.name == symbol
                || s.parent.as_ref().map(|p| format!("{}/{}", p, s.name))
                    == Some(symbol.to_string())
        })
        .map(|s| index::SymbolMatch {
            file: path.to_string(),
            name: s.name,
            kind: s.kind.as_str().to_string(),
            parent: s.parent,
            start_line: s.start_line,
            end_line: s.end_line,
        })
        .collect()
}

/// A traced assignment.
#[derive(Debug)]
struct TraceEntry {
    variable: String,
    line: usize,
    source: String,
    flows_from: Vec<String>,
    /// True if the value is a literal (terminal - no further tracing needed)
    is_terminal: bool,
    /// Function calls in the RHS (name -> signature if known)
    calls: Vec<CallInfo>,
    /// Conditional branch context (e.g., "if", "else", "match arm")
    branch_context: Option<String>,
}

#[derive(Debug)]
struct CallInfo {
    name: String,
    signature: Option<String>,
    /// Line where the called function is defined (for cross-function tracing)
    defined_at: Option<usize>,
}

/// Trace assignments within a function.
fn trace_assignments(
    root: &tree_sitter::Node,
    source: &[u8],
    start_line: usize,
    end_line: usize,
    max_depth: usize,
    signature_map: &HashMap<String, (String, usize)>,
) -> Vec<TraceEntry> {
    let mut entries = Vec::new();
    let mut cursor = root.walk();

    // Walk the AST looking for assignments
    trace_node(
        &mut cursor,
        source,
        start_line,
        end_line,
        &mut entries,
        signature_map,
        None, // no initial branch context
    );

    // Limit results (max_depth acts as max items until cross-function tracing is added)
    if entries.len() > max_depth {
        entries.truncate(max_depth);
    }

    entries
}

/// Detect if a node creates a branch context for its children.
/// This checks the parent chain to find if we're inside a conditional branch.
fn detect_branch_context(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let kind = node.kind();

    // If this is an else_clause, children are in "else" branch
    if kind == "else_clause" || kind == "else" {
        return Some("else".to_string());
    }

    // If this is a block, check if parent is if_expression (then branch) or else_clause (else branch)
    if kind == "block" {
        if let Some(parent) = node.parent() {
            let parent_kind = parent.kind();
            if parent_kind == "if_expression" || parent_kind == "if_statement" {
                // Check if we're the consequence (then) or part of else
                // If the block's previous sibling is "else", we're in else branch
                if let Some(prev) = node.prev_sibling() {
                    if prev.kind() == "else" {
                        return Some("else".to_string());
                    }
                }
                return Some("if".to_string());
            }
            if parent_kind == "else_clause" {
                return Some("else".to_string());
            }
        }
    }

    // Match/switch arms
    if kind == "match_arm" || kind == "case_clause" || kind == "switch_case" {
        // Try to get the pattern text
        if let Some(pattern) = node.child_by_field_name("pattern") {
            if let Ok(text) = pattern.utf8_text(source) {
                let preview: String = text.chars().take(20).collect();
                return Some(format!("match {}", preview));
            }
        }
        return Some("match arm".to_string());
    }

    None
}

/// Recursively trace assignments in a node.
fn trace_node(
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    start_line: usize,
    end_line: usize,
    entries: &mut Vec<TraceEntry>,
    signature_map: &HashMap<String, (String, usize)>,
    branch_context: Option<&str>,
) {
    loop {
        let node = cursor.node();
        let line = node.start_position().row + 1;
        let kind = node.kind();

        // Determine branch context for children
        let child_context =
            detect_branch_context(&node, source).or(branch_context.map(|s| s.to_string()));
        let child_context_ref = child_context.as_deref();

        // Only process nodes within our range
        if line >= start_line && line <= end_line {
            // Look for assignment-like nodes
            if kind == "assignment_expression"
                || kind == "assignment"
                || kind == "let_declaration"
                || kind == "variable_declarator"
                || kind == "short_var_declaration"
            {
                if let Some(mut entry) = extract_assignment(&node, source, line, signature_map) {
                    entry.branch_context = branch_context.map(|s| s.to_string());
                    entries.push(entry);
                }
            }
        }

        if cursor.goto_first_child() {
            trace_node(
                cursor,
                source,
                start_line,
                end_line,
                entries,
                signature_map,
                child_context_ref,
            );
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Extract assignment information from a node.
fn extract_assignment(
    node: &tree_sitter::Node,
    source: &[u8],
    line: usize,
    signature_map: &HashMap<String, (String, usize)>,
) -> Option<TraceEntry> {
    // Try to find left and right sides
    let lhs = node
        .child_by_field_name("left")
        .or_else(|| node.child_by_field_name("name"))
        .or_else(|| node.child_by_field_name("pattern"));
    let rhs = node
        .child_by_field_name("right")
        .or_else(|| node.child_by_field_name("value"))
        .or_else(|| node.child_by_field_name("init"));

    let variable = lhs
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string())?;
    let source_text = rhs
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "?".to_string());

    // Check if RHS is a literal (terminal value)
    let is_terminal = rhs.map(|n| is_literal_node(&n)).unwrap_or(false);

    // Extract function calls from RHS
    let calls = if let Some(rhs_node) = rhs {
        extract_calls_from_node(&rhs_node, source, signature_map)
    } else {
        Vec::new()
    };

    // Extract identifiers from RHS that the value flows from
    let flows_from = if is_terminal {
        Vec::new() // Literals don't flow from anything
    } else if let Some(rhs_node) = rhs {
        extract_identifiers_from_node(&rhs_node, source)
    } else {
        Vec::new()
    };

    Some(TraceEntry {
        variable,
        line,
        source: source_text,
        flows_from,
        is_terminal,
        calls,
        branch_context: None, // set by trace_node
    })
}

/// Check if a node represents a literal value.
fn is_literal_node(node: &tree_sitter::Node) -> bool {
    let kind = node.kind();
    // Common literal node kinds across languages
    kind.contains("literal")           // Rust: integer_literal, string_literal, etc.
        || kind == "integer"           // Python
        || kind == "float"             // Python
        || kind == "string"            // Python, JS
        || kind == "number"            // JavaScript
        || kind == "true"              // Python, JS
        || kind == "false"             // Python, JS
        || kind == "null"              // JavaScript
        || kind == "nil"               // Lua, Ruby
        || kind == "none" // Python
}

/// Extract function calls from a node.
fn extract_calls_from_node(
    node: &tree_sitter::Node,
    source: &[u8],
    signature_map: &HashMap<String, (String, usize)>,
) -> Vec<CallInfo> {
    let mut calls = Vec::new();
    let mut cursor = node.walk();

    fn collect(
        cursor: &mut tree_sitter::TreeCursor,
        source: &[u8],
        calls: &mut Vec<CallInfo>,
        signature_map: &HashMap<String, (String, usize)>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            // Detect call expressions across languages
            if kind == "call_expression"      // Rust, JS, Go
                || kind == "call"             // Python, Lua
                || kind == "method_call"      // Java
                || kind == "invocation_expression"
            // C#
            {
                // Get the function name (usually first child or 'function' field)
                let func_name = node
                    .child_by_field_name("function")
                    .or_else(|| node.child(0))
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.to_string());

                if let Some(name) = func_name {
                    // Try to look up signature and location
                    let simple_name = name.split(&['.', ':'][..]).last().unwrap_or(&name);
                    let (signature, defined_at) = signature_map
                        .get(simple_name)
                        .map(|(sig, line)| (Some(sig.clone()), Some(*line)))
                        .unwrap_or((None, None));
                    calls.push(CallInfo {
                        name,
                        signature,
                        defined_at,
                    });
                }
            }

            if cursor.goto_first_child() {
                collect(cursor, source, calls, signature_map);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    collect(&mut cursor, source, &mut calls, signature_map);
    calls
}

/// Extract identifier names from a node.
fn extract_identifiers_from_node(node: &tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut identifiers = Vec::new();
    let mut cursor = node.walk();

    fn collect(cursor: &mut tree_sitter::TreeCursor, source: &[u8], ids: &mut Vec<String>) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            if kind == "identifier" || kind == "field_identifier" || kind.ends_with("_identifier") {
                if let Ok(text) = node.utf8_text(source) {
                    // Skip keywords and common non-identifier patterns
                    if ![
                        "let", "mut", "const", "var", "true", "false", "nil", "null", "self",
                        "this",
                    ]
                    .contains(&text)
                    {
                        ids.push(text.to_string());
                    }
                }
            }

            if cursor.goto_first_child() {
                collect(cursor, source, ids);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    collect(&mut cursor, source, &mut identifiers);

    // Deduplicate
    identifiers.sort();
    identifiers.dedup();
    identifiers
}
