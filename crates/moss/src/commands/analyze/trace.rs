//! Value provenance tracing for symbols.

use crate::index;
use crate::parsers;
use crate::path_resolve::resolve_unified;
use std::collections::HashMap;
use std::path::Path;

/// Trace value provenance for a symbol.
#[allow(clippy::too_many_arguments)]
pub fn cmd_trace(
    symbol: &str,
    target: Option<&str>,
    root: &Path,
    max_depth: usize,
    recursive: bool,
    _case_insensitive: bool, // Index find_symbols uses LOWER() by default
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
    let lang = match rhizome_moss_languages::support_for_path(&full_path) {
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

    // Build signature map for same-file function lookups
    let extractor = crate::extract::Extractor::new();
    let extract_result = extractor.extract(&full_path, &content);
    let mut signature_map: HashMap<String, FunctionInfo> = HashMap::new();
    fn collect_signatures(sym: &rhizome_moss_languages::Symbol, map: &mut HashMap<String, FunctionInfo>) {
        if !sym.signature.is_empty() {
            map.insert(
                sym.name.clone(),
                FunctionInfo {
                    signature: sym.signature.clone(),
                    start_line: sym.start_line,
                    end_line: sym.end_line,
                },
            );
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

            // Recursive tracing: show what called functions return
            if recursive {
                let mut seen_funcs: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                for call in &t.calls {
                    // Skip if we've already shown returns for this function
                    if seen_funcs.contains(&call.name) {
                        continue;
                    }

                    // Try same-file first, then cross-file via index
                    let (returns, ext_file): (Vec<ReturnTrace>, Option<String>) =
                        if let (Some(start), Some(end)) = (call.defined_at, call.defined_end) {
                            // Same-file: use current tree
                            let returns =
                                trace_returns(&tree.root_node(), source_bytes, start, end);
                            (returns, None)
                        } else if let Some(cross) = trace_cross_file_returns(&call.name, root) {
                            // Cross-file: look up in index
                            (cross.returns, Some(cross.file))
                        } else {
                            continue;
                        };

                    if returns.is_empty() {
                        continue;
                    }

                    seen_funcs.insert(call.name.clone());
                    let file_suffix = ext_file
                        .as_ref()
                        .map(|f| format!(" ({})", f))
                        .unwrap_or_default();
                    if pretty {
                        println!(
                            "    {} returns:{}",
                            nu_ansi_term::Color::Magenta.paint(&call.name),
                            nu_ansi_term::Color::DarkGray.paint(&file_suffix)
                        );
                    } else {
                        println!("    {} returns:{}", call.name, file_suffix);
                    }
                    for ret in &returns {
                        let branch_info = ret
                            .branch_context
                            .as_ref()
                            .map(|b| format!(" ({})", b))
                            .unwrap_or_default();
                        if pretty {
                            let branch_colored =
                                nu_ansi_term::Color::Blue.paint(&branch_info).to_string();
                            println!(
                                "      L{}: {}{}",
                                nu_ansi_term::Color::Yellow.paint(ret.line.to_string()),
                                ret.value,
                                branch_colored
                            );
                        } else {
                            println!("      L{}: {}{}", ret.line, ret.value, branch_info);
                        }
                    }
                }
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

/// Function signature info for cross-function tracing.
#[derive(Debug, Clone)]
struct FunctionInfo {
    signature: String,
    start_line: usize,
    end_line: usize,
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

#[derive(Debug, Clone)]
struct CallInfo {
    name: String,
    signature: Option<String>,
    /// Line where the called function is defined (for cross-function tracing)
    defined_at: Option<usize>,
    /// End line of the called function (for recursive tracing)
    defined_end: Option<usize>,
}

/// A return statement found during recursive tracing.
#[derive(Debug)]
struct ReturnTrace {
    line: usize,
    value: String,
    branch_context: Option<String>,
}

/// Result of cross-file return tracing.
struct CrossFileReturns {
    returns: Vec<ReturnTrace>,
    file: String,
}

/// Trace assignments within a function.
fn trace_assignments(
    root: &tree_sitter::Node,
    source: &[u8],
    start_line: usize,
    end_line: usize,
    max_depth: usize,
    signature_map: &HashMap<String, FunctionInfo>,
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
    signature_map: &HashMap<String, FunctionInfo>,
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
    signature_map: &HashMap<String, FunctionInfo>,
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
    signature_map: &HashMap<String, FunctionInfo>,
) -> Vec<CallInfo> {
    let mut calls = Vec::new();
    let mut cursor = node.walk();

    fn collect(
        cursor: &mut tree_sitter::TreeCursor,
        source: &[u8],
        calls: &mut Vec<CallInfo>,
        signature_map: &HashMap<String, FunctionInfo>,
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
                    let info = signature_map.get(simple_name);
                    calls.push(CallInfo {
                        name,
                        signature: info.map(|i| i.signature.clone()),
                        defined_at: info.map(|i| i.start_line),
                        defined_end: info.map(|i| i.end_line),
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

/// Trace return statements within a function (for recursive tracing).
fn trace_returns(
    root: &tree_sitter::Node,
    source: &[u8],
    start_line: usize,
    end_line: usize,
) -> Vec<ReturnTrace> {
    let mut returns = Vec::new();
    let mut cursor = root.walk();

    fn collect_returns(
        cursor: &mut tree_sitter::TreeCursor,
        source: &[u8],
        start_line: usize,
        end_line: usize,
        returns: &mut Vec<ReturnTrace>,
        branch_context: Option<&str>,
        skip_children: bool,
    ) {
        loop {
            let node = cursor.node();
            let line = node.start_position().row + 1;
            let kind = node.kind();

            // Check for branch context
            let child_context =
                detect_branch_context(&node, source).or(branch_context.map(|s| s.to_string()));
            let child_context_ref = child_context.as_deref();

            let mut found_return = false;

            // Only look at nodes within our range
            if line >= start_line && line <= end_line && !skip_children {
                // Look for return statements (prefer return_expression for Rust)
                if kind == "return_expression" || kind == "return_statement" {
                    // Get the return value - skip the "return" keyword (child 0)
                    let value = node
                        .child(1)
                        .or_else(|| node.child_by_field_name("value"))
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|| "(void)".to_string());

                    returns.push(ReturnTrace {
                        line,
                        value,
                        branch_context: branch_context.map(|s| s.to_string()),
                    });
                    found_return = true;
                }
            }

            // Don't descend into children of return expressions (avoid duplicates)
            if cursor.goto_first_child() && !found_return {
                collect_returns(
                    cursor,
                    source,
                    start_line,
                    end_line,
                    returns,
                    child_context_ref,
                    false,
                );
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    collect_returns(
        &mut cursor,
        source,
        start_line,
        end_line,
        &mut returns,
        None,
        false,
    );
    returns
}

/// Look up a function in the index and trace its returns (cross-file).
fn trace_cross_file_returns(call_name: &str, root: &std::path::Path) -> Option<CrossFileReturns> {
    // Extract simple function name (last segment of method chain)
    let simple_name = call_name.split(&['.', ':'][..]).last().unwrap_or(call_name);

    // Look up in index
    let mut idx = index::FileIndex::open_if_enabled(root)?;
    let _ = idx.incremental_refresh();
    let matches = idx
        .find_symbols(simple_name, Some("function"), false, 5)
        .ok()?;

    // Find exact match
    let sym = matches.iter().find(|m| m.name == simple_name)?;

    // Read and parse the file
    let full_path = root.join(&sym.file);
    let content = std::fs::read_to_string(&full_path).ok()?;
    let lang = rhizome_moss_languages::support_for_path(&full_path)?;
    let tree = parsers::parse_with_grammar(lang.grammar_name(), &content)?;

    // Trace returns
    let returns = trace_returns(
        &tree.root_node(),
        content.as_bytes(),
        sym.start_line,
        sym.end_line,
    );

    if returns.is_empty() {
        None
    } else {
        Some(CrossFileReturns {
            returns,
            file: sym.file.clone(),
        })
    }
}
