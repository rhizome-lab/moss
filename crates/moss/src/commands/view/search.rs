//! Symbol search functionality for view command.

use crate::{index, skeleton};
use rhizome_moss_languages::support_for_path;
use std::path::Path;

/// Check if a file has language support (symbols can be extracted)
pub fn has_language_support(path: &str) -> bool {
    support_for_path(Path::new(path))
        .map(|lang| lang.has_symbols())
        .unwrap_or(false)
}

/// Search for symbols in the index by name.
/// Supports qualified names like "ClassName/method" or "file.rs/ClassName/method"
pub fn search_symbols(query: &str, root: &Path) -> Vec<index::SymbolMatch> {
    let (file_hint, parent_hint, symbol_name) = parse_symbol_query(query);

    // Try index first - if enabled, use it (or build it if empty)
    if let Some(mut idx) = index::FileIndex::open_if_enabled(root) {
        let stats = idx.call_graph_stats().unwrap_or_default();
        if stats.symbols == 0 {
            eprintln!("Building symbol index...");
            if let Err(e) = idx.refresh_call_graph() {
                eprintln!("Warning: failed to build index: {}", e);
                return search_symbols_unindexed(query, root);
            }
        }
        if let Ok(mut symbols) = idx.find_symbols(&symbol_name, None, true, 50) {
            // Filter by parent hint if provided
            if let Some(ref parent) = parent_hint {
                let parent_lower = parent.to_lowercase();
                symbols.retain(|s| {
                    s.parent
                        .as_ref()
                        .map(|p| p.to_lowercase().contains(&parent_lower))
                        .unwrap_or(false)
                });
            }
            // Filter by file hint if provided
            if let Some(ref file) = file_hint {
                let file_lower = file.to_lowercase();
                symbols.retain(|s| s.file.to_lowercase().contains(&file_lower));
            }
            if !symbols.is_empty() {
                symbols.truncate(10);
                return symbols;
            }
        }
    }

    // Fallback: walk filesystem and parse files (only if index disabled)
    search_symbols_unindexed(query, root)
}

/// Parse a symbol query like "Tsx/format_import" or "typescript.rs/Tsx/format_import"
/// Returns (file_hint, parent_hint, symbol_name)
fn parse_symbol_query(query: &str) -> (Option<String>, Option<String>, String) {
    let parts: Vec<&str> = query.split('/').collect();
    match parts.len() {
        1 => (None, None, parts[0].to_string()),
        2 => {
            // Could be "Parent/method" or "file.rs/symbol"
            if parts[0].contains('.') && !parts[0].starts_with('.') {
                (Some(parts[0].to_string()), None, parts[1].to_string())
            } else {
                (None, Some(parts[0].to_string()), parts[1].to_string())
            }
        }
        _ => {
            let symbol = parts.last().unwrap().to_string();
            let parent = parts.get(parts.len() - 2).map(|s| s.to_string());
            if parts.len() > 2 {
                let file = parts[..parts.len() - 2].join("/");
                (Some(file), parent, symbol)
            } else {
                (None, parent, symbol)
            }
        }
    }
}

/// Search for symbols by walking filesystem and parsing files
fn search_symbols_unindexed(query: &str, root: &Path) -> Vec<index::SymbolMatch> {
    use ignore::WalkBuilder;
    use nucleo_matcher::{Config, Matcher};

    let query_lower = query.to_lowercase();
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut matches = Vec::new();

    let walker = WalkBuilder::new(root).hidden(true).git_ignore(true).build();
    let extractor = skeleton::SkeletonExtractor::new();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(lang) = support_for_path(path) else {
            continue;
        };
        if !lang.has_symbols() {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };

        let result = extractor.extract(path, &content);
        let rel_path = path.strip_prefix(root).unwrap_or(path);

        collect_matching_symbols(
            &result.symbols,
            &query_lower,
            &mut matcher,
            &rel_path.to_string_lossy(),
            None,
            &mut matches,
        );

        if matches.len() >= 20 {
            break;
        }
    }

    matches.sort_by(|a, b| b.1.cmp(&a.1));
    matches.into_iter().take(10).map(|(m, _)| m).collect()
}

fn collect_matching_symbols(
    symbols: &[skeleton::SkeletonSymbol],
    query: &str,
    matcher: &mut nucleo_matcher::Matcher,
    file: &str,
    parent: Option<&str>,
    matches: &mut Vec<(index::SymbolMatch, u32)>,
) {
    use nucleo_matcher::Utf32Str;
    use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};

    let pattern = Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );

    for sym in symbols {
        let name_lower = sym.name.to_lowercase();
        let mut buf = Vec::new();
        let haystack = Utf32Str::new(&name_lower, &mut buf);

        if let Some(score) = pattern.score(haystack, matcher) {
            matches.push((
                index::SymbolMatch {
                    name: sym.name.clone(),
                    kind: sym.kind.as_str().to_string(),
                    file: file.to_string(),
                    start_line: sym.start_line,
                    end_line: sym.end_line,
                    parent: parent.map(|s| s.to_string()),
                },
                score,
            ));
        }

        collect_matching_symbols(
            &sym.children,
            query,
            matcher,
            file,
            Some(&sym.name),
            matches,
        );
    }
}
