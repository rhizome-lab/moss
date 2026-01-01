//! Check documentation references for broken links

use crate::index;
use std::path::Path;

/// A broken reference found in documentation
#[derive(Debug)]
struct BrokenRef {
    file: String,
    line: usize,
    reference: String,
    context: String,
}

/// Check documentation references for broken links
pub fn cmd_check_refs(root: &Path, json: bool) -> i32 {
    use regex::Regex;

    // Open index to get known symbols
    let idx = match index::FileIndex::open_if_enabled(root) {
        Some(i) => i,
        None => {
            eprintln!("Indexing disabled or failed. Run: moss index rebuild --call-graph");
            return 1;
        }
    };

    // Get all symbol names from index
    let all_symbols = idx.all_symbol_names().unwrap_or_default();

    if all_symbols.is_empty() {
        eprintln!("No symbols indexed. Run: moss index rebuild --call-graph");
        return 1;
    }

    // Find markdown files
    let md_files: Vec<_> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("md")
                && !e
                    .path()
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    if md_files.is_empty() {
        if json {
            println!(
                "{{\"broken_refs\": [], \"files_checked\": 0, \"symbols_indexed\": {}}}",
                all_symbols.len()
            );
        } else {
            println!("No markdown files found to check.");
        }
        return 0;
    }

    // Regex for code references: `identifier` or `Module::method` or `Module.method`
    let code_ref_re =
        Regex::new(r"`([A-Z][a-zA-Z0-9_]*(?:[:\.][a-zA-Z_][a-zA-Z0-9_]*)*)`").unwrap();

    let mut broken_refs: Vec<BrokenRef> = Vec::new();

    for md_file in &md_files {
        let content = match std::fs::read_to_string(md_file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel_path = md_file
            .strip_prefix(root)
            .unwrap_or(md_file)
            .display()
            .to_string();

        for (line_num, line) in content.lines().enumerate() {
            for cap in code_ref_re.captures_iter(line) {
                let reference = &cap[1];

                // Extract symbol name (last part after :: or .)
                let symbol_name = reference
                    .rsplit(|c| c == ':' || c == '.')
                    .next()
                    .unwrap_or(reference);

                // Skip common non-symbol patterns
                if is_common_non_symbol(symbol_name) {
                    continue;
                }

                // Check if symbol exists
                if !all_symbols.contains(symbol_name) {
                    // Also check the full reference
                    let full_name = reference.replace("::", ".").replace(".", "::");
                    if !all_symbols.contains(&full_name) && !all_symbols.contains(reference) {
                        broken_refs.push(BrokenRef {
                            file: rel_path.clone(),
                            line: line_num + 1,
                            reference: reference.to_string(),
                            context: line.trim().to_string(),
                        });
                    }
                }
            }
        }
    }

    if json {
        let output = serde_json::json!({
            "broken_refs": broken_refs.iter().map(|r| {
                serde_json::json!({
                    "file": r.file,
                    "line": r.line,
                    "reference": r.reference,
                    "context": r.context,
                })
            }).collect::<Vec<_>>(),
            "files_checked": md_files.len(),
            "symbols_indexed": all_symbols.len(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Documentation Reference Check");
        println!();
        println!("Files checked: {}", md_files.len());
        println!("Symbols indexed: {}", all_symbols.len());
        println!();

        if broken_refs.is_empty() {
            println!("No broken references found.");
        } else {
            println!("Broken references ({}):", broken_refs.len());
            println!();
            for r in &broken_refs {
                println!("  {}:{}: `{}`", r.file, r.line, r.reference);
                if r.context.len() <= 80 {
                    println!("    {}", r.context);
                }
            }
        }
    }

    if broken_refs.is_empty() { 0 } else { 1 }
}

/// Check if a string is a common non-symbol pattern (command, path, etc.)
fn is_common_non_symbol(s: &str) -> bool {
    // Skip common patterns that aren't symbols
    matches!(
        s,
        "TODO"
            | "FIXME"
            | "NOTE"
            | "HACK"
            | "XXX"
            | "BUG"
            | "OK"
            | "Err"
            | "Ok"
            | "None"
            | "Some"
            | "True"
            | "False"
            | "String"
            | "Vec"
            | "Option"
            | "Result"
            | "Box"
            | "Arc"
            | "Rc"
            | "HashMap"
            | "HashSet"
            | "BTreeMap"
            | "BTreeSet"
            | "PathBuf"
            | "Path"
            | "File"
            | "Read"
            | "Write"
            | "Debug"
            | "Clone"
            | "Copy"
            | "Default"
            | "Send"
            | "Sync"
            | "Serialize"
            | "Deserialize"
    ) || s.len() < 2
        || s.chars().all(|c| c.is_uppercase() || c == '_') // ALL_CAPS constants
}
