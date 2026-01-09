//! File skeleton viewing for view command.

use super::symbol::find_symbol_signature;
use crate::tree::{DocstringDisplay, FormatOptions};
use crate::{deps, skeleton, tree};
use rhizome_moss_languages::support_for_path;
use std::path::{Path, PathBuf};

/// Resolve an import to a local file path based on the source file's language.
fn resolve_import(module: &str, current_file: &Path, root: &Path) -> Option<PathBuf> {
    let lang = rhizome_moss_languages::support_for_path(current_file)?;

    if let Some(path) = lang.resolve_local_import(module, current_file, root) {
        return Some(path);
    }

    lang.resolve_external_import(module, root)
        .map(|pkg| pkg.path)
}

/// View a file's skeleton (symbols, imports, exports)
#[allow(clippy::too_many_arguments)]
pub fn cmd_view_file(
    file_path: &str,
    root: &Path,
    depth: i32,
    _line_numbers: bool,
    show_deps: bool,
    types_only: bool,
    show_tests: bool,
    focus: Option<&str>,
    resolve_imports: bool,
    show_docs: bool,
    context: bool,
    json: bool,
    pretty: bool,
    use_colors: bool,
) -> i32 {
    let full_path = root.join(file_path);
    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", file_path, e);
            return 1;
        }
    };

    if !(0..=2).contains(&depth) {
        // Full content view
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "type": "file",
                    "path": file_path,
                    "content": content
                })
            );
        } else {
            let grammar = support_for_path(&full_path).map(|s| s.grammar_name().to_string());
            let output = if pretty {
                if let Some(ref g) = grammar {
                    tree::highlight_source(&content, g, use_colors)
                } else {
                    content.clone()
                }
            } else {
                content.clone()
            };
            print!("{}", output);
        }
        return 0;
    }

    let grammar = support_for_path(&full_path).map(|s| s.grammar_name().to_string());

    let extractor = skeleton::SkeletonExtractor::new();
    let skeleton_result = extractor.extract(&full_path, &content);

    let skeleton_result = if types_only {
        skeleton_result.filter_types()
    } else if !show_tests {
        skeleton_result.filter_tests()
    } else {
        skeleton_result
    };

    let deps_result = if show_deps || focus.is_some() || resolve_imports || context {
        let deps_extractor = deps::DepsExtractor::new();
        Some(deps_extractor.extract(&full_path, &content))
    } else {
        None
    };

    if json {
        let view_node = skeleton_result.to_view_node(grammar.as_deref());
        println!("{}", serde_json::to_string(&view_node).unwrap());
    } else {
        println!("# {}", file_path);
        println!("Lines: {}", content.lines().count());

        if let Some(ref deps) = deps_result {
            let show = show_deps || context;
            if show && !deps.imports.is_empty() {
                println!("\n## Imports");
                for imp in &deps.imports {
                    if imp.names.is_empty() {
                        println!("  import {}", imp.module);
                    } else {
                        println!("  from {} import {}", imp.module, imp.names.join(", "));
                    }
                }
            }

            if show && !deps.exports.is_empty() {
                println!("\n## Exports");
                for exp in &deps.exports {
                    println!("  {}", exp.name);
                }
            }

            if show && !deps.reexports.is_empty() {
                println!("\n## Re-exports");
                for reexp in &deps.reexports {
                    if reexp.is_star {
                        println!("  export * from '{}'", reexp.module);
                    } else {
                        println!(
                            "  export {{ {} }} from '{}'",
                            reexp.names.join(", "),
                            reexp.module
                        );
                    }
                }
            }
        }

        if depth >= 1 && (!show_deps || context) {
            let view_node = skeleton_result.to_view_node(grammar.as_deref());
            let format_options = FormatOptions {
                docstrings: if context {
                    DocstringDisplay::None
                } else if show_docs {
                    DocstringDisplay::Full
                } else {
                    DocstringDisplay::Summary
                },
                line_numbers: true,
                skip_root: true,
                max_depth: None,
                minimal: !pretty,
                use_colors,
            };
            let lines = tree::format_view_node(&view_node, &format_options);
            if !lines.is_empty() {
                println!("\n## Symbols");
                for line in lines {
                    println!("{}", line);
                }
            }
        }

        // Fisheye mode: show skeletons of imported files
        if let Some(focus_filter) = focus {
            let deps = deps_result.as_ref().unwrap();
            let filter_all = focus_filter == "*";

            let mut resolved: Vec<(String, PathBuf, String)> = Vec::new();
            for imp in &deps.imports {
                let matches_filter =
                    filter_all || imp.module.contains(focus_filter) || imp.module == focus_filter;

                if matches_filter {
                    if let Some(resolved_path) = resolve_import(&imp.module, &full_path, root) {
                        let display = if let Ok(rel_path) = resolved_path.strip_prefix(root) {
                            rel_path.display().to_string()
                        } else {
                            format!("[{}]", imp.module)
                        };
                        resolved.push((imp.module.clone(), resolved_path, display));
                    }
                }
            }

            if !resolved.is_empty() {
                println!("\n## Imported Modules (Skeletons)");
                let deps_extractor = deps::DepsExtractor::new();

                for (module_name, resolved_path, display) in resolved {
                    if let Ok(import_content) = std::fs::read_to_string(&resolved_path) {
                        let import_extractor = skeleton::SkeletonExtractor::new();
                        let import_skeleton =
                            import_extractor.extract(&resolved_path, &import_content);
                        let import_skeleton = if types_only {
                            import_skeleton.filter_types()
                        } else {
                            import_skeleton
                        };

                        let import_grammar =
                            support_for_path(&resolved_path).map(|s| s.grammar_name().to_string());
                        let view_node = import_skeleton.to_view_node(import_grammar.as_deref());
                        let format_options = FormatOptions {
                            docstrings: DocstringDisplay::None,
                            line_numbers: true,
                            skip_root: true,
                            max_depth: None,
                            minimal: !pretty,
                            use_colors,
                        };
                        let lines = tree::format_view_node(&view_node, &format_options);
                        if !lines.is_empty() {
                            println!("\n### {} ({})", module_name, display);
                            for line in lines {
                                println!("{}", line);
                            }
                        }

                        // Check for barrel file re-exports
                        let import_deps = deps_extractor.extract(&resolved_path, &import_content);
                        for reexp in &import_deps.reexports {
                            if let Some(reexp_path) =
                                resolve_import(&reexp.module, &resolved_path, root)
                            {
                                if let Ok(reexp_content) = std::fs::read_to_string(&reexp_path) {
                                    let reexp_extractor = skeleton::SkeletonExtractor::new();
                                    let reexp_skeleton =
                                        reexp_extractor.extract(&reexp_path, &reexp_content);
                                    let reexp_skeleton = if types_only {
                                        reexp_skeleton.filter_types()
                                    } else {
                                        reexp_skeleton
                                    };

                                    let reexp_grammar = support_for_path(&reexp_path)
                                        .map(|s| s.grammar_name().to_string());
                                    let view_node =
                                        reexp_skeleton.to_view_node(reexp_grammar.as_deref());
                                    let format_options = FormatOptions {
                                        docstrings: DocstringDisplay::None,
                                        line_numbers: true,
                                        skip_root: true,
                                        max_depth: None,
                                        minimal: !pretty,
                                        use_colors,
                                    };
                                    let lines = tree::format_view_node(&view_node, &format_options);
                                    if !lines.is_empty() {
                                        let reexp_display = reexp_path
                                            .strip_prefix(root)
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(|_| format!("[{}]", reexp.module));
                                        let export_desc = if reexp.is_star {
                                            format!("export * from '{}'", reexp.module)
                                        } else {
                                            format!(
                                                "export {{ {} }} from '{}'",
                                                reexp.names.join(", "),
                                                reexp.module
                                            )
                                        };
                                        println!(
                                            "\n### {} → {} ({})",
                                            module_name, export_desc, reexp_display
                                        );
                                        for line in lines {
                                            println!("{}", line);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Resolve imports mode
        if resolve_imports {
            let deps = deps_result.as_ref().unwrap();

            let mut resolved_symbols: Vec<(String, String, String)> = Vec::new();

            for imp in &deps.imports {
                if imp.names.is_empty() {
                    continue;
                }

                if let Some(resolved_path) = resolve_import(&imp.module, &full_path, root) {
                    if let Ok(import_content) = std::fs::read_to_string(&resolved_path) {
                        let import_extractor = skeleton::SkeletonExtractor::new();
                        let import_skeleton =
                            import_extractor.extract(&resolved_path, &import_content);

                        for name in &imp.names {
                            if let Some(sig) = find_symbol_signature(&import_skeleton.symbols, name)
                            {
                                resolved_symbols.push((imp.module.clone(), name.clone(), sig));
                            }
                        }
                    }
                }
            }

            if !resolved_symbols.is_empty() {
                println!("\n## Resolved Imports");
                let mut current_module = String::new();
                for (module, _name, sig) in resolved_symbols {
                    if module != current_module {
                        println!("\n# from {}", module);
                        current_module = module;
                    }
                    println!("{}", sig);
                }
            }
        }
    }
    0
}
