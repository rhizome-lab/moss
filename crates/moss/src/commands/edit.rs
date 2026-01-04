//! Edit command for moss CLI.

use crate::commands::filter::detect_project_languages;
use crate::config::MossConfig;
use crate::filter::Filter;
use crate::shadow::{EditInfo, Shadow};
use crate::{daemon, edit, path_resolve};
use std::path::Path;

/// Position for insert/move/copy operations
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Position {
    /// Before the destination (sibling)
    Before,
    /// After the destination (sibling)
    After,
    /// At start of container
    Prepend,
    /// At end of container
    Append,
}

/// Internal representation of operations (for output)
#[derive(Clone, Copy)]
pub enum Operation {
    Delete,
    Replace,
    Swap,
    Insert(Position),
    Move(Position),
    Copy(Position),
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operation::Delete => write!(f, "delete"),
            Operation::Replace => write!(f, "replace"),
            Operation::Swap => write!(f, "swap"),
            Operation::Insert(Position::Before) => write!(f, "insert_before"),
            Operation::Insert(Position::After) => write!(f, "insert_after"),
            Operation::Insert(Position::Prepend) => write!(f, "prepend"),
            Operation::Insert(Position::Append) => write!(f, "append"),
            Operation::Move(Position::Before) => write!(f, "move_before"),
            Operation::Move(Position::After) => write!(f, "move_after"),
            Operation::Move(Position::Prepend) => write!(f, "move_prepend"),
            Operation::Move(Position::Append) => write!(f, "move_append"),
            Operation::Copy(Position::Before) => write!(f, "copy_before"),
            Operation::Copy(Position::After) => write!(f, "copy_after"),
            Operation::Copy(Position::Prepend) => write!(f, "copy_prepend"),
            Operation::Copy(Position::Append) => write!(f, "copy_append"),
        }
    }
}

/// Edit action to perform (CLI)
#[derive(clap::Subcommand)]
pub enum EditAction {
    /// Delete the target symbol
    Delete,

    /// Replace target with new content
    Replace {
        /// New content to replace with
        content: String,
    },

    /// Swap target with another symbol
    Swap {
        /// Symbol to swap with
        other: String,
    },

    /// Insert content relative to target
    Insert {
        /// Content to insert
        content: String,
        /// Where to insert: before, after, prepend, append
        #[arg(long)]
        at: Position,
    },

    /// Move target to a new location
    Move {
        /// Destination symbol or container
        destination: String,
        /// Where to place: before, after, prepend, append
        #[arg(long)]
        at: Position,
    },

    /// Copy target to a new location
    Copy {
        /// Destination symbol or container
        destination: String,
        /// Where to place: before, after, prepend, append
        #[arg(long)]
        at: Position,
    },
}

/// Perform structural edits on a file
#[allow(clippy::too_many_arguments)]
pub fn cmd_edit(
    target: &str,
    action: EditAction,
    root: Option<&Path>,
    dry_run: bool,
    yes: bool,
    json: bool,
    exclude: &[String],
    only: &[String],
    multiple: bool,
    message: Option<&str>,
    case_insensitive: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Load config for shadow git setting
    let config = MossConfig::load(&root);
    let shadow_enabled = config.shadow.enabled();

    // Check for delete confirmation if warn_on_delete is enabled
    if matches!(action, EditAction::Delete) && !yes && !dry_run {
        if config.shadow.warn_on_delete.unwrap_or(true) {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "error": "Delete requires confirmation",
                        "hint": "Use --yes or -y to confirm deletion, or set [shadow] warn_on_delete = false"
                    })
                );
            } else {
                eprintln!("Delete requires confirmation. Use --yes or -y to confirm.");
                eprintln!("To disable this warning: set [shadow] warn_on_delete = false in config");
            }
            return 1;
        }
    }

    // Ensure daemon is running if configured (will pick up edits)
    daemon::maybe_start_daemon(&root);

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

        let filter = match Filter::new(exclude, only, &config.aliases, &lang_refs) {
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
        return handle_file_level(
            &action,
            &editor,
            &content,
            &file_path,
            &unified.file_path,
            dry_run,
            json,
            &root,
            shadow_enabled,
            message,
        );
    }

    // Symbol-level operations
    let symbol_pattern = unified.symbol_path.join("/");

    // Check if this is a glob pattern (contains *, ?, or [)
    if edit::Editor::is_glob_pattern(&symbol_pattern) {
        return handle_glob_edit(
            &symbol_pattern,
            action,
            &editor,
            &content,
            &file_path,
            &unified.file_path,
            dry_run,
            json,
            multiple,
            &root,
            shadow_enabled,
            message,
            case_insensitive,
        );
    }

    // Exact symbol match
    let symbol_name = unified.symbol_path.last().unwrap();
    let loc = match editor.find_symbol(&file_path, &content, symbol_name, case_insensitive) {
        Some(l) => l,
        None => {
            eprintln!("Symbol not found: {}", symbol_name);
            return 1;
        }
    };

    let (operation, new_content) = match action {
        EditAction::Delete => (Operation::Delete, editor.delete_symbol(&content, &loc)),

        EditAction::Replace {
            content: ref new_code,
        } => (
            Operation::Replace,
            editor.replace_symbol(&content, &loc, new_code),
        ),

        EditAction::Swap { ref other } => {
            let other_loc = match editor.find_symbol(&file_path, &content, other, case_insensitive)
            {
                Some(l) => l,
                None => {
                    eprintln!("Other symbol not found: {}", other);
                    return 1;
                }
            };
            let (first_loc, second_loc) = if loc.start_byte < other_loc.start_byte {
                (&loc, &other_loc)
            } else {
                (&other_loc, &loc)
            };
            let first_content = content[first_loc.start_byte..first_loc.end_byte].to_string();
            let second_content = content[second_loc.start_byte..second_loc.end_byte].to_string();
            let mut new = content.clone();
            new.replace_range(second_loc.start_byte..second_loc.end_byte, &first_content);
            new.replace_range(first_loc.start_byte..first_loc.end_byte, &second_content);
            (Operation::Swap, new)
        }

        EditAction::Insert {
            content: ref insert_content,
            at,
        } => {
            let result = match at {
                Position::Before => editor.insert_before(&content, &loc, insert_content),
                Position::After => editor.insert_after(&content, &loc, insert_content),
                Position::Prepend | Position::Append => {
                    let body = match editor.find_container_body(&file_path, &content, symbol_name) {
                        Some(b) => b,
                        None => {
                            eprintln!("Error: '{}' is not a container", symbol_name);
                            return 1;
                        }
                    };
                    if matches!(at, Position::Prepend) {
                        editor.prepend_to_container(&content, &body, insert_content)
                    } else {
                        editor.append_to_container(&content, &body, insert_content)
                    }
                }
            };
            (Operation::Insert(at), result)
        }

        EditAction::Move {
            ref destination,
            at,
        } => {
            let source_content = content[loc.start_byte..loc.end_byte].to_string();
            let without_source = editor.delete_symbol(&content, &loc);

            let result = match at {
                Position::Before | Position::After => {
                    let dest_loc = match editor.find_symbol(
                        &file_path,
                        &without_source,
                        destination,
                        case_insensitive,
                    ) {
                        Some(l) => l,
                        None => {
                            eprintln!("Destination not found: {}", destination);
                            return 1;
                        }
                    };
                    if matches!(at, Position::Before) {
                        editor.insert_before(&without_source, &dest_loc, &source_content)
                    } else {
                        editor.insert_after(&without_source, &dest_loc, &source_content)
                    }
                }
                Position::Prepend | Position::Append => {
                    let body = match editor.find_container_body(
                        &file_path,
                        &without_source,
                        destination,
                    ) {
                        Some(b) => b,
                        None => {
                            eprintln!("Container not found: {}", destination);
                            return 1;
                        }
                    };
                    if matches!(at, Position::Prepend) {
                        editor.prepend_to_container(&without_source, &body, &source_content)
                    } else {
                        editor.append_to_container(&without_source, &body, &source_content)
                    }
                }
            };
            (Operation::Move(at), result)
        }

        EditAction::Copy {
            ref destination,
            at,
        } => {
            let source_content = &content[loc.start_byte..loc.end_byte];

            let result = match at {
                Position::Before | Position::After => {
                    let dest_loc = match editor.find_symbol(
                        &file_path,
                        &content,
                        destination,
                        case_insensitive,
                    ) {
                        Some(l) => l,
                        None => {
                            eprintln!("Destination not found: {}", destination);
                            return 1;
                        }
                    };
                    if matches!(at, Position::Before) {
                        editor.insert_before(&content, &dest_loc, source_content)
                    } else {
                        editor.insert_after(&content, &dest_loc, source_content)
                    }
                }
                Position::Prepend | Position::Append => {
                    let body = match editor.find_container_body(&file_path, &content, destination) {
                        Some(b) => b,
                        None => {
                            eprintln!("Container not found: {}", destination);
                            return 1;
                        }
                    };
                    if matches!(at, Position::Prepend) {
                        editor.prepend_to_container(&content, &body, source_content)
                    } else {
                        editor.append_to_container(&content, &body, source_content)
                    }
                }
            };
            (Operation::Copy(at), result)
        }
    };

    output_result(
        dry_run,
        json,
        &unified.file_path,
        Some(symbol_name),
        operation,
        &new_content,
        &file_path,
        &root,
        shadow_enabled,
        message,
    )
}

/// Handle file-level operations (prepend/append to file without symbol target)
#[allow(clippy::too_many_arguments)]
fn handle_file_level(
    action: &EditAction,
    editor: &edit::Editor,
    content: &str,
    file_path: &Path,
    rel_path: &str,
    dry_run: bool,
    json: bool,
    root: &Path,
    shadow_enabled: bool,
    message: Option<&str>,
) -> i32 {
    let (operation, new_content) = match action {
        EditAction::Insert {
            content: insert_content,
            at: Position::Prepend,
        } => (
            Operation::Insert(Position::Prepend),
            editor.prepend_to_file(content, insert_content),
        ),
        EditAction::Insert {
            content: insert_content,
            at: Position::Append,
        } => (
            Operation::Insert(Position::Append),
            editor.append_to_file(content, insert_content),
        ),
        _ => {
            eprintln!(
                "Error: This operation requires a symbol target. Use a path like 'src/foo.py/MyClass'"
            );
            eprintln!(
                "Hint: Only 'insert --at prepend' and 'insert --at append' work on files directly"
            );
            return 1;
        }
    };

    output_result(
        dry_run,
        json,
        rel_path,
        None,
        operation,
        &new_content,
        file_path,
        root,
        shadow_enabled,
        message,
    )
}

/// Output result (dry-run or actual write)
#[allow(clippy::too_many_arguments)]
fn output_result(
    dry_run: bool,
    json: bool,
    rel_path: &str,
    symbol: Option<&str>,
    operation: Operation,
    new_content: &str,
    file_path: &Path,
    root: &Path,
    shadow_enabled: bool,
    message: Option<&str>,
) -> i32 {
    if dry_run {
        if json {
            let mut obj = serde_json::json!({
                "dry_run": true,
                "file": rel_path,
                "operation": operation.to_string(),
                "new_content": new_content
            });
            if let Some(s) = symbol {
                obj["symbol"] = serde_json::json!(s);
            }
            println!("{}", obj);
        } else {
            if let Some(s) = symbol {
                println!("--- Dry run: {} on {} ---", operation, s);
            } else {
                println!("--- Dry run: {} ---", rel_path);
            }
            println!("{}", new_content);
        }
        return 0;
    }

    // Shadow git: capture before state
    let shadow = if shadow_enabled {
        let s = Shadow::new(root);
        if let Err(e) = s.before_edit(&[file_path]) {
            eprintln!("warning: shadow git: {}", e);
        }
        Some(s)
    } else {
        None
    };

    if let Err(e) = std::fs::write(file_path, new_content) {
        eprintln!("Error writing file: {}", e);
        return 1;
    }

    // Shadow git: capture after state and commit
    if let Some(ref s) = shadow {
        let target = match symbol {
            Some(sym) => format!("{}/{}", rel_path, sym),
            None => rel_path.to_string(),
        };
        let info = EditInfo {
            operation: operation.to_string(),
            target,
            files: vec![file_path.to_path_buf()],
            message: message.map(String::from),
            workflow: None,
        };
        if let Err(e) = s.after_edit(&info) {
            eprintln!("warning: shadow git: {}", e);
        }
    }

    if json {
        let mut obj = serde_json::json!({
            "success": true,
            "file": rel_path,
            "operation": operation.to_string()
        });
        if let Some(s) = symbol {
            obj["symbol"] = serde_json::json!(s);
        }
        println!("{}", obj);
    } else if let Some(s) = symbol {
        println!("{}: {} in {}", operation, s, rel_path);
    } else {
        println!("{}: {}", operation, rel_path);
    }

    0
}

/// Handle glob pattern edits (multi-symbol operations)
#[allow(clippy::too_many_arguments)]
fn handle_glob_edit(
    pattern: &str,
    action: EditAction,
    editor: &edit::Editor,
    content: &str,
    file_path: &std::path::PathBuf,
    rel_path: &str,
    dry_run: bool,
    json: bool,
    multiple: bool,
    root: &Path,
    shadow_enabled: bool,
    message: Option<&str>,
    case_insensitive: bool,
) -> i32 {
    let matches = editor.find_symbols_matching(file_path, content, pattern);

    if matches.is_empty() {
        eprintln!("No symbols match pattern: {}", pattern);
        return 1;
    }

    let count = matches.len();

    // Require --multiple flag when matching more than one symbol
    if count > 1 && !multiple {
        eprintln!(
            "Error: Pattern '{}' matches {} symbols. Use --multiple to confirm.",
            pattern, count
        );
        for m in &matches {
            eprintln!("  - {} ({})", m.name, m.kind);
        }
        return 1;
    }
    let names: Vec<&str> = matches.iter().map(|m| m.name.as_str()).collect();

    // Matches are sorted in reverse order (highest byte offset first)
    // This ensures we can apply changes from end to start without offset shifts
    let (operation, new_content) = match action {
        EditAction::Delete => {
            let mut result = content.to_string();
            for loc in &matches {
                result = editor.delete_symbol(&result, loc);
            }
            ("delete", result)
        }

        EditAction::Replace {
            content: ref new_code,
        } => {
            let mut result = content.to_string();
            for loc in &matches {
                result = editor.replace_symbol(&result, loc, new_code);
            }
            ("replace", result)
        }

        EditAction::Insert {
            content: ref insert_content,
            at,
        } => {
            let mut result = content.to_string();
            for loc in &matches {
                result = match at {
                    Position::Before => editor.insert_before(&result, loc, insert_content),
                    Position::After => editor.insert_after(&result, loc, insert_content),
                    Position::Prepend | Position::Append => {
                        // For prepend/append, each match must be a container
                        match editor.find_container_body(file_path, &result, &loc.name) {
                            Some(body) => {
                                if matches!(at, Position::Prepend) {
                                    editor.prepend_to_container(&result, &body, insert_content)
                                } else {
                                    editor.append_to_container(&result, &body, insert_content)
                                }
                            }
                            None => {
                                eprintln!("Error: '{}' is not a container", loc.name);
                                return 1;
                            }
                        }
                    }
                };
            }
            (
                match at {
                    Position::Before => "insert_before",
                    Position::After => "insert_after",
                    Position::Prepend => "prepend",
                    Position::Append => "append",
                },
                result,
            )
        }

        EditAction::Move {
            ref destination,
            at,
        } => {
            // Delete all sources first (matches in reverse byte order for safe deletion)
            let mut result = content.to_string();
            for loc in &matches {
                result = editor.delete_symbol(&result, loc);
            }

            // Insert at destination, order depends on position type:
            // - append: original order [first..last] → iterate reversed matches
            // - others: reverse order [last..first] → iterate matches as-is
            let iter: Box<dyn Iterator<Item = _>> = if matches!(at, Position::Append) {
                Box::new(matches.iter().rev())
            } else {
                Box::new(matches.iter())
            };

            for loc in iter {
                let source_content = &content[loc.start_byte..loc.end_byte];
                result = match at {
                    Position::Before | Position::After => {
                        let dest_loc = match editor.find_symbol(
                            file_path,
                            &result,
                            destination,
                            case_insensitive,
                        ) {
                            Some(l) => l,
                            None => {
                                eprintln!("Destination not found: {}", destination);
                                return 1;
                            }
                        };
                        if matches!(at, Position::Before) {
                            editor.insert_before(&result, &dest_loc, source_content)
                        } else {
                            editor.insert_after(&result, &dest_loc, source_content)
                        }
                    }
                    Position::Prepend | Position::Append => {
                        let body = match editor.find_container_body(file_path, &result, destination)
                        {
                            Some(b) => b,
                            None => {
                                eprintln!("Container not found: {}", destination);
                                return 1;
                            }
                        };
                        if matches!(at, Position::Prepend) {
                            editor.prepend_to_container(&result, &body, source_content)
                        } else {
                            editor.append_to_container(&result, &body, source_content)
                        }
                    }
                };
            }
            (
                match at {
                    Position::Before => "move_before",
                    Position::After => "move_after",
                    Position::Prepend => "move_prepend",
                    Position::Append => "move_append",
                },
                result,
            )
        }

        EditAction::Copy {
            ref destination,
            at,
        } => {
            let mut result = content.to_string();
            // Insert at destination, order depends on position type:
            // - append: original order [first..last] → iterate reversed matches
            // - others: reverse order [last..first] → iterate matches as-is
            let iter: Box<dyn Iterator<Item = _>> = if matches!(at, Position::Append) {
                Box::new(matches.iter().rev())
            } else {
                Box::new(matches.iter())
            };

            for loc in iter {
                let source_content = &content[loc.start_byte..loc.end_byte];
                result = match at {
                    Position::Before | Position::After => {
                        let dest_loc = match editor.find_symbol(
                            file_path,
                            &result,
                            destination,
                            case_insensitive,
                        ) {
                            Some(l) => l,
                            None => {
                                eprintln!("Destination not found: {}", destination);
                                return 1;
                            }
                        };
                        if matches!(at, Position::Before) {
                            editor.insert_before(&result, &dest_loc, source_content)
                        } else {
                            editor.insert_after(&result, &dest_loc, source_content)
                        }
                    }
                    Position::Prepend | Position::Append => {
                        let body = match editor.find_container_body(file_path, &result, destination)
                        {
                            Some(b) => b,
                            None => {
                                eprintln!("Container not found: {}", destination);
                                return 1;
                            }
                        };
                        if matches!(at, Position::Prepend) {
                            editor.prepend_to_container(&result, &body, source_content)
                        } else {
                            editor.append_to_container(&result, &body, source_content)
                        }
                    }
                };
            }
            (
                match at {
                    Position::Before => "copy_before",
                    Position::After => "copy_after",
                    Position::Prepend => "copy_prepend",
                    Position::Append => "copy_append",
                },
                result,
            )
        }

        EditAction::Swap { .. } => {
            eprintln!("Error: 'swap' is not supported with glob patterns (ambiguous pairing)");
            eprintln!("Matched {} symbols: {}", count, names.join(", "));
            return 1;
        }
    };

    if dry_run {
        if json {
            let obj = serde_json::json!({
                "dry_run": true,
                "file": rel_path,
                "operation": operation,
                "pattern": pattern,
                "matched_count": count,
                "matched_symbols": names,
                "new_content": new_content
            });
            println!("{}", obj);
        } else {
            println!(
                "--- Dry run: {} {} symbols matching '{}' ---",
                operation, count, pattern
            );
            for m in &matches {
                println!("  - {} ({})", m.name, m.kind);
            }
            println!("{}", new_content);
        }
        return 0;
    }

    // Shadow git: capture before state
    let shadow = if shadow_enabled {
        let s = Shadow::new(root);
        if let Err(e) = s.before_edit(&[file_path.as_path()]) {
            eprintln!("warning: shadow git: {}", e);
        }
        Some(s)
    } else {
        None
    };

    if let Err(e) = std::fs::write(file_path, &new_content) {
        eprintln!("Error writing file: {}", e);
        return 1;
    }

    // Shadow git: capture after state and commit
    if let Some(ref s) = shadow {
        let target = format!("{}/{}", rel_path, pattern);
        let info = EditInfo {
            operation: operation.to_string(),
            target,
            files: vec![file_path.clone()],
            message: message.map(String::from),
            workflow: None,
        };
        if let Err(e) = s.after_edit(&info) {
            eprintln!("warning: shadow git: {}", e);
        }
    }

    if json {
        let obj = serde_json::json!({
            "success": true,
            "file": rel_path,
            "operation": operation,
            "pattern": pattern,
            "count": count,
            "symbols": names
        });
        println!("{}", obj);
    } else {
        println!(
            "{} {} symbols matching '{}':",
            if operation == "delete" {
                "Deleted"
            } else {
                "Replaced"
            },
            count,
            pattern
        );
        for m in &matches {
            println!("  - {} ({})", m.name, m.kind);
        }
    }
    0
}

/// Handle undo/redo/goto operations on shadow git history.
pub fn cmd_undo_redo(
    root: Option<&Path>,
    undo: Option<usize>,
    redo: bool,
    goto: Option<&str>,
    file_filter: Option<&str>,
    cross_checkpoint: bool,
    dry_run: bool,
    force: bool,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let shadow = Shadow::new(&root);

    if !shadow.exists() {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "error": "No shadow history exists"
                })
            );
        } else {
            eprintln!("No shadow history exists. Make an edit first with `moss edit`.");
        }
        return 1;
    }

    // Handle goto first (takes precedence)
    if let Some(ref_str) = goto {
        match shadow.goto(ref_str, dry_run, force) {
            Ok(result) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "operation": if dry_run { "goto_preview" } else { "goto" },
                            "target": ref_str,
                            "description": result.description,
                            "files": result.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                            "commit": result.undone_commit
                        })
                    );
                } else {
                    if dry_run {
                        println!("Would restore state from: {}", result.description);
                    } else {
                        println!("Restored state from: {}", result.description);
                    }
                    for file in &result.files {
                        println!("  {}", file.display());
                    }
                }
                return 0;
            }
            Err(e) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "error": e.to_string()
                        })
                    );
                } else {
                    eprintln!("{}", e);
                }
                return 1;
            }
        }
    }

    if redo {
        match shadow.redo() {
            Ok(result) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "operation": "redo",
                            "description": result.description,
                            "files": result.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                            "commit": result.undone_commit
                        })
                    );
                } else {
                    println!("Redone: {}", result.description);
                    for file in &result.files {
                        println!("  {}", file.display());
                    }
                }
                0
            }
            Err(e) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "error": e.to_string()
                        })
                    );
                } else {
                    eprintln!("{}", e);
                }
                1
            }
        }
    } else if let Some(count) = undo {
        let count = if count == 0 { 1 } else { count };
        match shadow.undo(count, file_filter, cross_checkpoint, dry_run, force) {
            Ok(results) => {
                // Collect all conflicts across results
                let all_conflicts: Vec<_> =
                    results.iter().flat_map(|r| r.conflicts.clone()).collect();

                if json {
                    let items: Vec<_> = results
                        .iter()
                        .map(|r| {
                            let mut obj = serde_json::json!({
                                "description": r.description,
                                "files": r.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                                "commit": r.undone_commit
                            });
                            if !r.conflicts.is_empty() {
                                obj["conflicts"] = serde_json::json!(r.conflicts);
                            }
                            obj
                        })
                        .collect();
                    let mut output = serde_json::json!({
                        "operation": if dry_run { "undo_preview" } else { "undo" },
                        "count": results.len(),
                        "undone": items
                    });
                    if !all_conflicts.is_empty() {
                        output["has_conflicts"] = serde_json::json!(true);
                    }
                    println!("{}", output);
                } else {
                    if dry_run {
                        println!(
                            "Would undo {} edit{}:",
                            results.len(),
                            if results.len() == 1 { "" } else { "s" }
                        );
                    } else {
                        println!(
                            "Undone {} edit{}:",
                            results.len(),
                            if results.len() == 1 { "" } else { "s" }
                        );
                    }
                    for result in &results {
                        println!("  {} ({})", result.description, result.undone_commit);
                        for file in &result.files {
                            println!("    {}", file.display());
                        }
                        if !result.conflicts.is_empty() {
                            println!("    ⚠ Conflicts (modified externally):");
                            for conflict in &result.conflicts {
                                println!("      {}", conflict);
                            }
                        }
                    }
                    if !all_conflicts.is_empty() && dry_run {
                        println!(
                            "\n⚠ Warning: {} file(s) modified externally. Use --force to override.",
                            all_conflicts.len()
                        );
                    }
                }
                0
            }
            Err(e) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "error": e.to_string()
                        })
                    );
                } else {
                    eprintln!("{}", e);
                }
                1
            }
        }
    } else {
        eprintln!("No undo or redo operation specified");
        1
    }
}

/// Apply batch edits from a JSON file
pub fn cmd_batch_edit(
    batch_file: &str,
    root: Option<&Path>,
    dry_run: bool,
    message: Option<&str>,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Read JSON from file or stdin
    let json_content = if batch_file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_err() {
            eprintln!("Failed to read from stdin");
            return 1;
        }
        buf
    } else {
        match std::fs::read_to_string(batch_file) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Failed to read {}: {}", batch_file, e);
                return 1;
            }
        }
    };

    // Parse batch edits
    let batch = match edit::BatchEdit::from_json(&json_content) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to parse batch edits: {}", e);
            return 1;
        }
    };

    let batch = if let Some(msg) = message {
        batch.with_message(msg)
    } else {
        batch
    };

    if dry_run {
        // For dry run, just validate and show what would happen
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "dry_run": true,
                    "message": "Batch edit validation passed"
                })
            );
        } else {
            println!("Dry run: batch edit would be applied");
        }
        return 0;
    }

    // Apply the batch
    match batch.apply(&root) {
        Ok(result) => {
            // Create shadow snapshot for batch edit
            let config = MossConfig::load(&root);
            if config.shadow.enabled() {
                let shadow = Shadow::new(&root);
                if shadow.exists() {
                    // Convert PathBufs to Path refs for before_edit
                    let file_refs: Vec<&Path> =
                        result.files_modified.iter().map(|p| p.as_path()).collect();
                    let _ = shadow.before_edit(&file_refs);

                    let edit_info = EditInfo {
                        operation: "batch".to_string(),
                        target: format!("{} files", result.files_modified.len()),
                        files: result.files_modified.clone(),
                        message: message.map(|s| s.to_string()),
                        workflow: None,
                    };
                    let _ = shadow.after_edit(&edit_info);
                }
            }

            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "success": true,
                        "files_modified": result.files_modified.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
                        "edits_applied": result.edits_applied
                    })
                );
            } else {
                println!(
                    "Applied {} edit(s) to {} file(s)",
                    result.edits_applied,
                    result.files_modified.len()
                );
            }
            0
        }
        Err(e) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "error": e
                    })
                );
            } else {
                eprintln!("Batch edit failed: {}", e);
            }
            1
        }
    }
}
