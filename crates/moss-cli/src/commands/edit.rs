//! Edit command for moss CLI.

use crate::commands::filter::detect_project_languages;
use crate::config::MossConfig;
use crate::filter::Filter;
use crate::{daemon, edit, path_resolve};
use std::path::Path;

/// Perform structural edits on a file
#[allow(clippy::too_many_arguments)]
pub fn cmd_edit(
    target: &str,
    root: Option<&Path>,
    delete: bool,
    replace: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
    prepend: Option<&str>,
    append: Option<&str>,
    move_before: Option<&str>,
    move_after: Option<&str>,
    copy_before: Option<&str>,
    copy_after: Option<&str>,
    move_prepend: Option<&str>,
    move_append: Option<&str>,
    copy_prepend: Option<&str>,
    copy_append: Option<&str>,
    swap: Option<&str>,
    dry_run: bool,
    json: bool,
    exclude: &[String],
    only: &[String],
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Ensure daemon is running if configured (will pick up edits)
    daemon::maybe_start_daemon(&root);

    // Count operations to ensure exactly one is specified
    let ops = [
        delete,
        replace.is_some(),
        before.is_some(),
        after.is_some(),
        prepend.is_some(),
        append.is_some(),
        move_before.is_some(),
        move_after.is_some(),
        copy_before.is_some(),
        copy_after.is_some(),
        move_prepend.is_some(),
        move_append.is_some(),
        copy_prepend.is_some(),
        copy_append.is_some(),
        swap.is_some(),
    ];
    let op_count = ops.iter().filter(|&&x| x).count();

    if op_count == 0 {
        eprintln!("Error: No operation specified. Use --delete, --replace, --before, --after, --prepend, --append, --move-*, --copy-*, or --swap");
        return 1;
    }
    if op_count > 1 {
        eprintln!("Error: Only one operation can be specified at a time");
        return 1;
    }

    // Resolve the target path
    let unified = match path_resolve::resolve_unified(target, &root) {
        Some(u) => u,
        None => {
            eprintln!("No matches for: {}", target);
            return 1;
        }
    };

    // We need a file path (cannot edit directories)
    if unified.is_directory {
        eprintln!("Cannot edit a directory: {}", target);
        return 1;
    }

    // Apply filter if specified
    if !exclude.is_empty() || !only.is_empty() {
        let config = MossConfig::load(&root);
        let languages = detect_project_languages(&root);
        let lang_refs: Vec<&str> = languages.iter().map(|s| s.as_str()).collect();

        let filter = match Filter::new(exclude, only, &config.filter, &lang_refs) {
            Ok(f) => {
                for warning in f.warnings() {
                    eprintln!("warning: {}", warning);
                }
                f
            }
            Err(e) => {
                eprintln!("error: {}", e);
                return 1;
            }
        };

        if !filter.matches(Path::new(&unified.file_path)) {
            eprintln!(
                "Target '{}' excluded by filter (resolved to {})",
                target, unified.file_path
            );
            return 1;
        }
    }

    let file_path = root.join(&unified.file_path);
    let content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return 1;
        }
    };

    let editor = edit::Editor::new();

    // Handle file-level operations (prepend/append without a symbol)
    if unified.symbol_path.is_empty() {
        // File-level operations
        let new_content = if let Some(content_to_prepend) = prepend {
            editor.prepend_to_file(&content, content_to_prepend)
        } else if let Some(content_to_append) = append {
            editor.append_to_file(&content, content_to_append)
        } else {
            eprintln!("Error: --delete, --replace, --before, --after require a symbol target");
            eprintln!("Hint: Use a path like 'src/foo.py/MyClass' to target a symbol");
            return 1;
        };

        if dry_run {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "dry_run": true,
                        "file": unified.file_path,
                        "operation": if prepend.is_some() { "prepend" } else { "append" },
                        "new_content": new_content
                    })
                );
            } else {
                println!("--- Dry run: {} ---", unified.file_path);
                println!("{}", new_content);
            }
            return 0;
        }

        if let Err(e) = std::fs::write(&file_path, &new_content) {
            eprintln!("Error writing file: {}", e);
            return 1;
        }

        if json {
            println!(
                "{}",
                serde_json::json!({
                    "success": true,
                    "file": unified.file_path,
                    "operation": if prepend.is_some() { "prepend" } else { "append" }
                })
            );
        } else {
            println!(
                "{}: {}",
                if prepend.is_some() {
                    "Prepended to"
                } else {
                    "Appended to"
                },
                unified.file_path
            );
        }
        return 0;
    }

    // Symbol-level operations
    let symbol_name = unified.symbol_path.last().unwrap();
    let loc = match editor.find_symbol(&file_path, &content, symbol_name) {
        Some(l) => l,
        None => {
            eprintln!("Symbol not found: {}", symbol_name);
            return 1;
        }
    };

    let (operation, new_content) = if delete {
        ("delete", editor.delete_symbol(&content, &loc))
    } else if let Some(new_code) = replace {
        ("replace", editor.replace_symbol(&content, &loc, new_code))
    } else if let Some(code) = before {
        ("insert_before", editor.insert_before(&content, &loc, code))
    } else if let Some(code) = after {
        ("insert_after", editor.insert_after(&content, &loc, code))
    } else if let Some(code) = prepend {
        // Prepend inside a container (class/impl)
        let body = match editor.find_container_body(&file_path, &content, symbol_name) {
            Some(b) => b,
            None => {
                eprintln!("Error: '{}' is not a container (class/impl)", symbol_name);
                eprintln!("Hint: --prepend works on classes and impl blocks");
                return 1;
            }
        };
        (
            "prepend",
            editor.prepend_to_container(&content, &body, code),
        )
    } else if let Some(code) = append {
        // Append inside a container (class/impl)
        let body = match editor.find_container_body(&file_path, &content, symbol_name) {
            Some(b) => b,
            None => {
                eprintln!("Error: '{}' is not a container (class/impl)", symbol_name);
                eprintln!("Hint: --append works on classes and impl blocks");
                return 1;
            }
        };
        ("append", editor.append_to_container(&content, &body, code))
    } else if let Some(dest) = move_before {
        // Move operation: delete from source, insert before destination
        // First verify destination exists
        let _dest_loc = match editor.find_symbol(&file_path, &content, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found: {}", dest);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        let without_source = editor.delete_symbol(&content, &loc);
        // Re-find destination after deletion (location may have shifted)
        let dest_loc_adjusted = match editor.find_symbol(&file_path, &without_source, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found after deletion: {}", dest);
                return 1;
            }
        };
        (
            "move_before",
            editor.insert_before(&without_source, &dest_loc_adjusted, source_content),
        )
    } else if let Some(dest) = move_after {
        // First verify destination exists
        let _dest_loc = match editor.find_symbol(&file_path, &content, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found: {}", dest);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        let without_source = editor.delete_symbol(&content, &loc);
        // Re-find destination after deletion (location may have shifted)
        let dest_loc_adjusted = match editor.find_symbol(&file_path, &without_source, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found after deletion: {}", dest);
                return 1;
            }
        };
        (
            "move_after",
            editor.insert_after(&without_source, &dest_loc_adjusted, source_content),
        )
    } else if let Some(dest) = copy_before {
        // Copy operation: insert copy before destination (keep original)
        let dest_loc = match editor.find_symbol(&file_path, &content, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found: {}", dest);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        (
            "copy_before",
            editor.insert_before(&content, &dest_loc, source_content),
        )
    } else if let Some(dest) = copy_after {
        // Copy operation: insert copy after destination (keep original)
        let dest_loc = match editor.find_symbol(&file_path, &content, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found: {}", dest);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        (
            "copy_after",
            editor.insert_after(&content, &dest_loc, source_content),
        )
    } else if let Some(container) = move_prepend {
        // Move to beginning of container
        // First verify container exists
        let _body = match editor.find_container_body(&file_path, &content, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found: {}", container);
                return 1;
            }
        };
        let source_content = content[loc.start_byte..loc.end_byte].to_string();
        let without_source = editor.delete_symbol(&content, &loc);
        // Re-find container body after deletion (location may have shifted)
        let body = match editor.find_container_body(&file_path, &without_source, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found after deletion: {}", container);
                return 1;
            }
        };
        (
            "move_prepend",
            editor.prepend_to_container(&without_source, &body, &source_content),
        )
    } else if let Some(container) = move_append {
        // Move to end of container
        // First verify container exists
        let _body = match editor.find_container_body(&file_path, &content, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found: {}", container);
                return 1;
            }
        };
        let source_content = content[loc.start_byte..loc.end_byte].to_string();
        let without_source = editor.delete_symbol(&content, &loc);
        // Re-find container body after deletion (location may have shifted)
        let body = match editor.find_container_body(&file_path, &without_source, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found after deletion: {}", container);
                return 1;
            }
        };
        (
            "move_append",
            editor.append_to_container(&without_source, &body, &source_content),
        )
    } else if let Some(container) = copy_prepend {
        // Copy to beginning of container
        let body = match editor.find_container_body(&file_path, &content, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found: {}", container);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        (
            "copy_prepend",
            editor.prepend_to_container(&content, &body, source_content),
        )
    } else if let Some(container) = copy_append {
        // Copy to end of container
        let body = match editor.find_container_body(&file_path, &content, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found: {}", container);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        (
            "copy_append",
            editor.append_to_container(&content, &body, source_content),
        )
    } else if let Some(other) = swap {
        let other_loc = match editor.find_symbol(&file_path, &content, other) {
            Some(l) => l,
            None => {
                eprintln!("Other symbol not found: {}", other);
                return 1;
            }
        };
        // Swap: get both contents, then replace in order (handle offsets)
        let (first_loc, second_loc) = if loc.start_byte < other_loc.start_byte {
            (&loc, &other_loc)
        } else {
            (&other_loc, &loc)
        };
        let first_content = content[first_loc.start_byte..first_loc.end_byte].to_string();
        let second_content = content[second_loc.start_byte..second_loc.end_byte].to_string();

        // Build new content by replacing second first (to preserve offsets), then first
        let mut new = content.clone();
        new.replace_range(second_loc.start_byte..second_loc.end_byte, &first_content);
        new.replace_range(first_loc.start_byte..first_loc.end_byte, &second_content);
        ("swap", new)
    } else {
        eprintln!("Error: No valid operation");
        return 1;
    };

    if dry_run {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "dry_run": true,
                    "file": unified.file_path,
                    "symbol": symbol_name,
                    "operation": operation,
                    "new_content": new_content
                })
            );
        } else {
            println!("--- Dry run: {} on {} ---", operation, symbol_name);
            println!("{}", new_content);
        }
        return 0;
    }

    if let Err(e) = std::fs::write(&file_path, &new_content) {
        eprintln!("Error writing file: {}", e);
        return 1;
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "file": unified.file_path,
                "symbol": symbol_name,
                "operation": operation
            })
        );
    } else {
        println!("{}: {} in {}", operation, symbol_name, unified.file_path);
    }

    0
}
