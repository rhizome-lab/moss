//! Duplicate function and type detection.

use crate::extract::Extractor;
use crate::parsers;
use moss_languages::support_for_path;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// A group of duplicate functions
#[derive(Debug)]
struct DuplicateFunctionGroup {
    hash: u64,
    locations: Vec<DuplicateFunctionLocation>,
    line_count: usize,
}

/// Location of a duplicate function instance
#[derive(Debug)]
struct DuplicateFunctionLocation {
    file: String,
    symbol: String,
    start_line: usize,
    end_line: usize,
}

/// Load allowed duplicate function locations from .moss/duplicate-functions-allow file
fn load_duplicate_functions_allowlist(root: &Path) -> HashSet<String> {
    let path = root.join(".moss/duplicate-functions-allow");
    let mut allowed = HashSet::new();
    if let Ok(content) = std::fs::read_to_string(&path) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            allowed.insert(line.to_string());
        }
    }
    allowed
}

/// Detect all duplicate function groups in the codebase (before filtering by allowlist)
fn detect_duplicate_function_groups(
    root: &Path,
    elide_identifiers: bool,
    elide_literals: bool,
    min_lines: usize,
) -> Vec<DuplicateFunctionGroup> {
    let extractor = Extractor::new();

    let mut hash_groups: HashMap<u64, Vec<DuplicateFunctionLocation>> = HashMap::new();

    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker.filter_map(|e| e.ok()).filter(|e| {
        let path = e.path();
        path.is_file() && super::is_source_file(path)
    }) {
        let path = entry.path();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let support = match support_for_path(path) {
            Some(s) => s,
            None => continue,
        };

        let tree = match parsers::parse_with_grammar(support.grammar_name(), &content) {
            Some(t) => t,
            None => continue,
        };

        let result = extractor.extract(path, &content);

        for sym in result.symbols.iter().flat_map(|s| flatten_symbols(s)) {
            let kind = sym.kind.as_str();
            if kind != "function" && kind != "method" {
                continue;
            }

            if let Some(node) = find_function_node(&tree, sym.start_line) {
                let line_count = sym.end_line.saturating_sub(sym.start_line) + 1;
                if line_count < min_lines {
                    continue;
                }

                let hash = compute_function_hash(
                    &node,
                    content.as_bytes(),
                    elide_identifiers,
                    elide_literals,
                );

                let rel_path = path
                    .strip_prefix(root)
                    .unwrap_or(path)
                    .display()
                    .to_string();

                hash_groups
                    .entry(hash)
                    .or_default()
                    .push(DuplicateFunctionLocation {
                        file: rel_path,
                        symbol: sym.name.clone(),
                        start_line: sym.start_line,
                        end_line: sym.end_line,
                    });
            }
        }
    }

    // Filter to groups with 2+ instances (actual duplicates)
    let mut groups: Vec<DuplicateFunctionGroup> = hash_groups
        .into_iter()
        .filter(|(_, locs)| locs.len() >= 2)
        .map(|(hash, locations)| {
            let line_count = locations
                .first()
                .map(|l| l.end_line - l.start_line + 1)
                .unwrap_or(0);
            DuplicateFunctionGroup {
                hash,
                locations,
                line_count,
            }
        })
        .collect();

    // Sort by line count (larger duplicates first), then by number of instances
    groups.sort_by(|a, b| {
        b.line_count
            .cmp(&a.line_count)
            .then_with(|| b.locations.len().cmp(&a.locations.len()))
    });

    groups
}

/// Allow a specific duplicate function group by adding it to .moss/duplicate-functions-allow
pub fn cmd_allow_duplicate_function(
    root: &Path,
    location: &str,
    reason: Option<&str>,
    elide_identifiers: bool,
    elide_literals: bool,
    min_lines: usize,
) -> i32 {
    // Detect all duplicate function groups
    let all_groups =
        detect_duplicate_function_groups(root, elide_identifiers, elide_literals, min_lines);

    // Find the group containing this location
    let target_group = all_groups.iter().find(|g| {
        g.locations.iter().any(|loc| {
            let entry = format!("{}:{}", loc.file, loc.symbol);
            entry == location
        })
    });

    let group = match target_group {
        Some(g) => g,
        None => {
            eprintln!("No duplicate function group found containing: {}", location);
            eprintln!("Run `moss analyze --duplicate-functions` to see available groups.");
            return 1;
        }
    };

    // Load existing allowlist to check for overlap
    let allowlist_path = root.join(".moss/duplicate-functions-allow");
    let existing_content = std::fs::read_to_string(&allowlist_path).unwrap_or_default();
    let existing_lines: Vec<&str> = existing_content.lines().collect();

    // Check if any entries from this group already exist
    let mut has_existing = false;
    let mut insert_after: Option<usize> = None;

    for loc in &group.locations {
        let entry = format!("{}:{}", loc.file, loc.symbol);
        for (i, line) in existing_lines.iter().enumerate() {
            if line.trim() == entry {
                has_existing = true;
                insert_after = Some(insert_after.map_or(i, |prev| prev.max(i)));
            }
        }
    }

    // Require reason for new groups
    if !has_existing && reason.is_none() {
        eprintln!("Reason required for new groups. Use --reason \"...\"");
        return 1;
    }

    // Build entries to add
    let mut to_add: Vec<String> = Vec::new();
    for loc in &group.locations {
        let entry = format!("{}:{}", loc.file, loc.symbol);
        if !existing_lines.iter().any(|l| l.trim() == entry) {
            to_add.push(entry);
        }
    }

    if to_add.is_empty() {
        println!("All entries from this group are already allowed.");
        return 0;
    }

    // Build new content with smart insertion
    let mut new_lines: Vec<String> = existing_lines.iter().map(|s| s.to_string()).collect();

    if let Some(idx) = insert_after {
        // Insert near existing entries from this group
        let insert_pos = idx + 1;
        for (i, entry) in to_add.iter().enumerate() {
            new_lines.insert(insert_pos + i, entry.clone());
        }
    } else {
        // Append at end with reason as comment
        if !new_lines.is_empty() && !new_lines.last().map_or(true, |l| l.is_empty()) {
            new_lines.push(String::new());
        }
        if let Some(r) = reason {
            new_lines.push(format!("# {}", r));
        }
        for entry in &to_add {
            new_lines.push(entry.clone());
        }
    }

    // Write back
    let new_content = new_lines.join("\n") + "\n";
    if let Err(e) = std::fs::write(&allowlist_path, new_content) {
        eprintln!("Failed to write .moss/duplicate-functions-allow: {}", e);
        return 1;
    }

    println!(
        "Added {} entries to .moss/duplicate-functions-allow:",
        to_add.len()
    );
    for entry in &to_add {
        println!("  {}", entry);
    }

    0
}

/// Detect duplicate functions - returns (exit_code, group_count)
pub fn cmd_duplicate_functions_with_count(
    root: &Path,
    elide_identifiers: bool,
    elide_literals: bool,
    show_source: bool,
    min_lines: usize,
    json: bool,
) -> (i32, usize) {
    let extractor = Extractor::new();

    let allowlist = load_duplicate_functions_allowlist(root);

    // Collect function hashes: hash -> [(file, symbol, start, end)]
    let mut hash_groups: HashMap<u64, Vec<DuplicateFunctionLocation>> = HashMap::new();
    let mut files_scanned = 0;
    let mut functions_hashed = 0;

    // Walk source files, respecting .gitignore
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker.filter_map(|e| e.ok()).filter(|e| {
        let path = e.path();
        path.is_file() && super::is_source_file(path)
    }) {
        let path = entry.path();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let support = match support_for_path(path) {
            Some(s) => s,
            None => continue,
        };

        // Parse the file
        let tree = match parsers::parse_with_grammar(support.grammar_name(), &content) {
            Some(t) => t,
            None => continue,
        };

        files_scanned += 1;

        // Extract symbols to find functions/methods
        let result = extractor.extract(path, &content);

        // Find and hash each function/method
        for sym in result.symbols.iter().flat_map(|s| flatten_symbols(s)) {
            let kind = sym.kind.as_str();
            if kind != "function" && kind != "method" {
                continue;
            }

            // Find the function node
            if let Some(node) = find_function_node(&tree, sym.start_line) {
                let line_count = sym.end_line.saturating_sub(sym.start_line) + 1;
                if line_count < min_lines {
                    continue;
                }

                let hash = compute_function_hash(
                    &node,
                    content.as_bytes(),
                    elide_identifiers,
                    elide_literals,
                );
                functions_hashed += 1;

                let rel_path = path
                    .strip_prefix(root)
                    .unwrap_or(path)
                    .display()
                    .to_string();

                hash_groups
                    .entry(hash)
                    .or_default()
                    .push(DuplicateFunctionLocation {
                        file: rel_path,
                        symbol: sym.name.clone(),
                        start_line: sym.start_line,
                        end_line: sym.end_line,
                    });
            }
        }
    }

    // Filter to groups with 2+ instances (actual duplicates)
    // Skip groups where ALL locations are in the allowlist
    let mut groups: Vec<DuplicateFunctionGroup> = hash_groups
        .into_iter()
        .filter(|(_, locs)| locs.len() >= 2)
        .filter(|(_, locs)| {
            // Keep if any location is NOT allowed
            locs.iter()
                .any(|loc| !allowlist.contains(&format!("{}:{}", loc.file, loc.symbol)))
        })
        .map(|(hash, locations)| {
            let line_count = locations
                .first()
                .map(|l| l.end_line - l.start_line + 1)
                .unwrap_or(0);
            DuplicateFunctionGroup {
                hash,
                locations,
                line_count,
            }
        })
        .collect();

    // Sort by line count (larger duplicates first), then by number of instances
    groups.sort_by(|a, b| {
        b.line_count
            .cmp(&a.line_count)
            .then_with(|| b.locations.len().cmp(&a.locations.len()))
    });

    let total_duplicates: usize = groups.iter().map(|g| g.locations.len()).sum();
    let duplicated_lines: usize = groups
        .iter()
        .map(|g| g.line_count * g.locations.len())
        .sum();

    if json {
        let output = serde_json::json!({
            "files_scanned": files_scanned,
            "functions_hashed": functions_hashed,
            "duplicate_groups": groups.len(),
            "total_duplicates": total_duplicates,
            "duplicated_lines": duplicated_lines,
            "elide_identifiers": elide_identifiers,
            "elide_literals": elide_literals,
            "groups": groups.iter().map(|g| {
                serde_json::json!({
                    "hash": format!("{:016x}", g.hash),
                    "line_count": g.line_count,
                    "instances": g.locations.len(),
                    "locations": g.locations.iter().map(|l| {
                        serde_json::json!({
                            "file": l.file,
                            "symbol": l.symbol,
                            "start_line": l.start_line,
                            "end_line": l.end_line,
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Duplicate Function Detection");
        println!();
        println!("Files scanned: {}", files_scanned);
        println!("Functions hashed: {}", functions_hashed);
        println!("Duplicate groups: {}", groups.len());
        println!("Total duplicates: {}", total_duplicates);
        println!("Duplicated lines: ~{}", duplicated_lines);
        println!();

        if groups.is_empty() {
            println!("No duplicate functions detected.");
        } else {
            println!("Duplicate Groups (sorted by size):");
            println!();

            for (i, group) in groups.iter().take(20).enumerate() {
                println!(
                    "{}. {} lines, {} instances:",
                    i + 1,
                    group.line_count,
                    group.locations.len()
                );

                for loc in &group.locations {
                    println!(
                        "   {}:{}-{} ({})",
                        loc.file, loc.start_line, loc.end_line, loc.symbol
                    );

                    // Show source code if requested
                    if show_source {
                        let file_path = root.join(&loc.file);
                        if let Ok(content) = std::fs::read_to_string(&file_path) {
                            let lines: Vec<&str> = content.lines().collect();
                            let start = loc.start_line.saturating_sub(1);
                            let end = loc.end_line.min(lines.len());
                            for (j, line) in lines[start..end].iter().enumerate() {
                                println!("        {:4} │ {}", start + j + 1, line);
                            }
                            println!();
                        }
                    }
                }
                println!();
            }

            if groups.len() > 20 {
                println!("... and {} more groups", groups.len() - 20);
            }
        }
    }

    let count = groups.len();
    let exit_code = if count == 0 { 0 } else { 1 };
    (exit_code, count)
}

/// Detect duplicate type definitions (structs with similar fields)
pub fn cmd_duplicate_types(
    root: &Path,
    config_root: &Path,
    min_overlap_percent: usize,
    json: bool,
) -> i32 {
    use regex::Regex;

    let extractor = Extractor::new();

    // Load allowlist
    let allowlist_path = config_root.join(".moss/duplicate-types-allow");
    let allowed_pairs: HashSet<(String, String)> = std::fs::read_to_string(&allowlist_path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .filter_map(|l| {
            let parts: Vec<&str> = l.trim().split_whitespace().collect();
            if parts.len() == 2 {
                // Store in sorted order for consistent matching
                let (a, b) = if parts[0] < parts[1] {
                    (parts[0].to_string(), parts[1].to_string())
                } else {
                    (parts[1].to_string(), parts[0].to_string())
                };
                Some((a, b))
            } else {
                None
            }
        })
        .collect();

    // Type location info
    #[derive(Debug, Clone)]
    struct TypeInfo {
        file: String,
        name: String,
        start_line: usize,
        fields: Vec<String>,
    }

    // Collect types with their fields
    let mut types: Vec<TypeInfo> = Vec::new();
    let mut files_scanned = 0;

    // Regex to extract field names from struct definitions
    // Matches patterns like: field_name: Type or pub field_name: Type
    let field_re = Regex::new(r"(?m)^\s*(?:pub\s+)?(\w+)\s*:\s*\S").unwrap();

    // Collect files to scan - either a single file or walk a directory
    let files: Vec<PathBuf> = if root.is_file() {
        vec![root.to_path_buf()]
    } else {
        ignore::WalkBuilder::new(root)
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.is_file() && super::is_source_file(path)
            })
            .map(|e| e.path().to_path_buf())
            .collect()
    };

    for path in &files {
        let path = path.as_path();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        files_scanned += 1;

        // Extract symbols
        let result = extractor.extract(path, &content);
        let lines: Vec<&str> = content.lines().collect();

        // Find type symbols (struct, class, interface, etc.)
        for sym in result.symbols.iter().flat_map(|s| flatten_symbols(s)) {
            let kind = sym.kind.as_str();
            if !matches!(kind, "struct" | "class" | "interface" | "type") {
                continue;
            }

            // Extract field names from source
            let start = sym.start_line.saturating_sub(1);
            let end = sym.end_line.min(lines.len());
            let source: String = lines[start..end].join("\n");

            let fields: Vec<String> = field_re
                .captures_iter(&source)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .collect();

            // Skip types with too few fields
            if fields.len() < 2 {
                continue;
            }

            let rel_path = if root.is_file() {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string())
            } else {
                path.strip_prefix(root)
                    .unwrap_or(path)
                    .display()
                    .to_string()
            };

            types.push(TypeInfo {
                file: rel_path,
                name: sym.name.clone(),
                start_line: sym.start_line,
                fields,
            });
        }
    }

    // Find duplicate pairs based on field overlap
    #[derive(Debug)]
    struct DuplicatePair {
        type1: TypeInfo,
        type2: TypeInfo,
        overlap_percent: usize,
        common_fields: Vec<String>,
    }

    let mut duplicates: Vec<DuplicatePair> = Vec::new();

    for i in 0..types.len() {
        for j in (i + 1)..types.len() {
            let t1 = &types[i];
            let t2 = &types[j];

            // Skip if same name (intentional reimplementation)
            if t1.name == t2.name {
                continue;
            }

            // Skip if pair is in allowlist
            let pair_key = if t1.name < t2.name {
                (t1.name.clone(), t2.name.clone())
            } else {
                (t2.name.clone(), t1.name.clone())
            };
            if allowed_pairs.contains(&pair_key) {
                continue;
            }

            // Calculate field overlap
            let set1: HashSet<_> = t1.fields.iter().collect();
            let set2: HashSet<_> = t2.fields.iter().collect();

            let common: Vec<String> = set1.intersection(&set2).map(|s| (*s).clone()).collect();

            let min_size = t1.fields.len().min(t2.fields.len());
            let overlap_percent = if min_size > 0 {
                (common.len() * 100) / min_size
            } else {
                0
            };

            if overlap_percent >= min_overlap_percent {
                duplicates.push(DuplicatePair {
                    type1: t1.clone(),
                    type2: t2.clone(),
                    overlap_percent,
                    common_fields: common,
                });
            }
        }
    }

    // Sort by overlap percentage (highest first)
    duplicates.sort_by(|a, b| b.overlap_percent.cmp(&a.overlap_percent));

    // Output results
    if json {
        let output = serde_json::json!({
            "files_scanned": files_scanned,
            "types_analyzed": types.len(),
            "duplicate_pairs": duplicates.len(),
            "min_overlap_percent": min_overlap_percent,
            "duplicates": duplicates.iter().map(|d| {
                serde_json::json!({
                    "overlap_percent": d.overlap_percent,
                    "common_fields": d.common_fields,
                    "type1": {
                        "file": d.type1.file,
                        "name": d.type1.name,
                        "line": d.type1.start_line,
                        "fields": d.type1.fields,
                    },
                    "type2": {
                        "file": d.type2.file,
                        "name": d.type2.name,
                        "line": d.type2.start_line,
                        "fields": d.type2.fields,
                    },
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Duplicate Type Detection");
        println!();
        println!("Files scanned: {}", files_scanned);
        println!("Types analyzed: {}", types.len());
        println!("Duplicate pairs: {}", duplicates.len());
        println!("Min overlap: {}%", min_overlap_percent);
        println!();

        if duplicates.is_empty() {
            println!("No duplicate types detected.");
        } else {
            println!("Potential Duplicates (sorted by overlap):");
            println!();

            for (i, dup) in duplicates.iter().take(20).enumerate() {
                println!(
                    "{}. {}% overlap ({} common fields):",
                    i + 1,
                    dup.overlap_percent,
                    dup.common_fields.len()
                );
                println!(
                    "   {} ({}:{}) - {} fields",
                    dup.type1.name,
                    dup.type1.file,
                    dup.type1.start_line,
                    dup.type1.fields.len()
                );
                println!(
                    "   {} ({}:{}) - {} fields",
                    dup.type2.name,
                    dup.type2.file,
                    dup.type2.start_line,
                    dup.type2.fields.len()
                );
                println!("   Common: {}", dup.common_fields.join(", "));
                println!();
            }

            if duplicates.len() > 20 {
                println!("... and {} more pairs", duplicates.len() - 20);
            }
        }
    }

    if duplicates.is_empty() { 0 } else { 1 }
}

/// Allow a duplicate type pair by adding to .moss/duplicate-types-allow
pub fn cmd_allow_duplicate_type(
    root: &Path,
    type1: &str,
    type2: &str,
    reason: Option<&str>,
) -> i32 {
    // Normalize to sorted order
    let (type1, type2) = if type1 < type2 {
        (type1, type2)
    } else {
        (type2, type1)
    };
    let entry = format!("{} {}", type1, type2);

    // Load existing allowlist
    let allowlist_path = root.join(".moss/duplicate-types-allow");
    let existing_content = std::fs::read_to_string(&allowlist_path).unwrap_or_default();
    let existing_lines: Vec<&str> = existing_content.lines().collect();

    // Check if already exists
    for line in &existing_lines {
        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.len() == 2 {
            let (a, b) = if parts[0] < parts[1] {
                (parts[0], parts[1])
            } else {
                (parts[1], parts[0])
            };
            if a == type1 && b == type2 {
                println!("Pair already allowed: {}", entry);
                return 0;
            }
        }
    }

    // Require reason for new entries
    if reason.is_none() {
        eprintln!("Reason required for new type pairs. Use --reason \"...\"");
        return 1;
    }

    // Build new content
    let mut new_lines: Vec<String> = existing_lines.iter().map(|s| s.to_string()).collect();
    if !new_lines.is_empty() && !new_lines.last().map_or(true, |l| l.is_empty()) {
        new_lines.push(String::new());
    }
    if let Some(r) = reason {
        new_lines.push(format!("# {}", r));
    }
    new_lines.push(entry.clone());

    // Ensure .moss directory exists
    let moss_dir = root.join(".moss");
    if !moss_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&moss_dir) {
            eprintln!("Failed to create .moss directory: {}", e);
            return 1;
        }
    }

    // Write back
    let new_content = new_lines.join("\n") + "\n";
    if let Err(e) = std::fs::write(&allowlist_path, new_content) {
        eprintln!("Failed to write .moss/duplicate-types-allow: {}", e);
        return 1;
    }

    println!("Added to .moss/duplicate-types-allow: {}", entry);
    0
}

/// Flatten nested symbols into a flat list
fn flatten_symbols(sym: &moss_languages::Symbol) -> Vec<&moss_languages::Symbol> {
    let mut result = vec![sym];
    for child in &sym.children {
        result.extend(flatten_symbols(child));
    }
    result
}

/// Find a function node at a given line
fn find_function_node(
    tree: &tree_sitter::Tree,
    target_line: usize,
) -> Option<tree_sitter::Node<'_>> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    find_node_at_line_recursive(&mut cursor, target_line)
}

fn find_node_at_line_recursive<'a>(
    cursor: &mut tree_sitter::TreeCursor<'a>,
    target_line: usize,
) -> Option<tree_sitter::Node<'a>> {
    loop {
        let node = cursor.node();
        let start = node.start_position().row + 1;

        if start == target_line {
            let kind = node.kind();
            if kind.contains("function")
                || kind.contains("method")
                || kind == "function_definition"
                || kind == "method_definition"
                || kind == "function_item"
                || kind == "function_declaration"
                || kind == "arrow_function"
                || kind == "generator_function"
            {
                return Some(node);
            }
        }

        if cursor.goto_first_child() {
            if let Some(found) = find_node_at_line_recursive(cursor, target_line) {
                return Some(found);
            }
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
    None
}

/// Compute a normalized AST hash for duplicate function detection.
fn compute_function_hash(
    node: &tree_sitter::Node,
    content: &[u8],
    elide_identifiers: bool,
    elide_literals: bool,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    hash_node_recursive(
        node,
        content,
        &mut hasher,
        elide_identifiers,
        elide_literals,
    );
    hasher.finish()
}

/// Recursively hash a node and its children.
fn hash_node_recursive(
    node: &tree_sitter::Node,
    content: &[u8],
    hasher: &mut impl Hasher,
    elide_identifiers: bool,
    elide_literals: bool,
) {
    let kind = node.kind();

    // Hash the node kind (structure)
    kind.hash(hasher);

    // For leaf nodes, decide whether to hash content
    if node.child_count() == 0 {
        let should_hash = if is_identifier_kind(kind) {
            !elide_identifiers
        } else if is_literal_kind(kind) {
            !elide_literals
        } else {
            // Operators, keywords - their kind is sufficient
            false
        };

        if should_hash {
            let text = &content[node.start_byte()..node.end_byte()];
            text.hash(hasher);
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        hash_node_recursive(&child, content, hasher, elide_identifiers, elide_literals);
    }
}

/// Check if a node kind represents an identifier.
fn is_identifier_kind(kind: &str) -> bool {
    kind == "identifier"
        || kind == "field_identifier"
        || kind == "type_identifier"
        || kind == "property_identifier"
        || kind.ends_with("_identifier")
}

/// Check if a node kind represents a literal value.
fn is_literal_kind(kind: &str) -> bool {
    kind.contains("string")
        || kind.contains("integer")
        || kind.contains("float")
        || kind.contains("number")
        || kind.contains("boolean")
        || kind == "true"
        || kind == "false"
        || kind == "nil"
        || kind == "null"
        || kind == "none"
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_duplicate_functions_allowlist_empty() {
        let tmp = tempdir().unwrap();
        let allowlist = load_duplicate_functions_allowlist(tmp.path());
        assert!(allowlist.is_empty());
    }
}
