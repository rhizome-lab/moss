//! Index management commands.

use crate::index;
use crate::paths::get_moss_dir;
use crate::skeleton;
use clap::Subcommand;
use moss_languages::external_packages;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum IndexAction {
    /// Rebuild the file index
    Rebuild {
        /// Also rebuild the call graph (slower, parses all files)
        #[arg(short, long = "call-graph")]
        call_graph: bool,
    },

    /// Show index statistics (DB size vs codebase size)
    Stats,

    /// List indexed files (with optional prefix filter)
    Files {
        /// Filter files by prefix
        prefix: Option<String>,

        /// Maximum number of files to show
        #[arg(short, long, default_value = "100")]
        limit: usize,
    },

    /// Index external packages (stdlib, site-packages) into global cache
    Packages {
        /// Ecosystems to index (python, go, js, deno, java, cpp, rust). Defaults to all available.
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,

        /// Clear existing index before re-indexing
        #[arg(long)]
        clear: bool,
    },
}

/// Run an index management action
pub fn cmd_index(action: IndexAction, root: Option<&Path>, json: bool) -> i32 {
    match action {
        IndexAction::Rebuild { call_graph } => cmd_rebuild(root, call_graph),
        IndexAction::Stats => cmd_stats(root, json),
        IndexAction::Files { prefix, limit } => {
            cmd_list_files(prefix.as_deref(), root, limit, json)
        }
        IndexAction::Packages { only, clear } => cmd_packages(&only, clear, root, json),
    }
}

// =============================================================================
// Rebuild
// =============================================================================

fn cmd_rebuild(root: Option<&Path>, call_graph: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    match index::FileIndex::open(&root) {
        Ok(mut idx) => match idx.refresh() {
            Ok(count) => {
                println!("Indexed {} files", count);

                if call_graph {
                    match idx.refresh_call_graph() {
                        Ok(stats) => {
                            println!(
                                "Indexed {} symbols, {} calls, {} imports",
                                stats.symbols, stats.calls, stats.imports
                            );
                        }
                        Err(e) => {
                            eprintln!("Error indexing call graph: {}", e);
                            return 1;
                        }
                    }
                }
                0
            }
            Err(e) => {
                eprintln!("Error refreshing index: {}", e);
                1
            }
        },
        Err(e) => {
            eprintln!("Error opening index: {}", e);
            1
        }
    }
}

// =============================================================================
// Stats
// =============================================================================

/// Check if a file is binary by looking for null bytes
fn is_binary_file(path: &Path) -> bool {
    use std::io::Read;

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };

    let mut buffer = [0u8; 8192];
    let Ok(bytes_read) = file.read(&mut buffer) else {
        return false;
    };

    buffer[..bytes_read].contains(&0)
}

fn cmd_stats(root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let moss_dir = get_moss_dir(&root);
    let db_path = moss_dir.join("index.sqlite");

    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    let idx = match index::FileIndex::open(&root) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to open index: {}", e);
            return 1;
        }
    };

    let files = match idx.all_files() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to read files: {}", e);
            return 1;
        }
    };

    let file_count = files.iter().filter(|f| !f.is_dir).count();
    let dir_count = files.iter().filter(|f| f.is_dir).count();

    // Count by extension (detect binary files)
    let mut ext_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in &files {
        if f.is_dir {
            continue;
        }
        let path = std::path::Path::new(&f.path);
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_string(),
            None => {
                let full_path = root.join(&f.path);
                if is_binary_file(&full_path) {
                    "(binary)".to_string()
                } else {
                    "(no ext)".to_string()
                }
            }
        };
        *ext_counts.entry(ext).or_insert(0) += 1;
    }

    let mut ext_list: Vec<_> = ext_counts.into_iter().collect();
    ext_list.sort_by(|a, b| b.1.cmp(&a.1));

    let stats = idx.call_graph_stats().unwrap_or_default();

    // Calculate codebase size
    let mut codebase_size: u64 = 0;
    for f in &files {
        if !f.is_dir {
            let full_path = root.join(&f.path);
            if let Ok(meta) = std::fs::metadata(&full_path) {
                codebase_size += meta.len();
            }
        }
    }

    if json {
        let output = serde_json::json!({
            "db_size_bytes": db_size,
            "codebase_size_bytes": codebase_size,
            "ratio": if codebase_size > 0 { db_size as f64 / codebase_size as f64 } else { 0.0 },
            "file_count": file_count,
            "dir_count": dir_count,
            "symbol_count": stats.symbols,
            "call_count": stats.calls,
            "import_count": stats.imports,
            "extensions": ext_list.iter().take(20).map(|(e, c)| serde_json::json!({"ext": e, "count": c})).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Index Statistics");
        println!("================");
        println!();
        println!(
            "Database:     {} ({:.1} KB)",
            db_path.display(),
            db_size as f64 / 1024.0
        );
        println!(
            "Codebase:     {:.1} MB",
            codebase_size as f64 / 1024.0 / 1024.0
        );
        println!(
            "Ratio:        {:.2}%",
            if codebase_size > 0 {
                db_size as f64 / codebase_size as f64 * 100.0
            } else {
                0.0
            }
        );
        println!();
        println!("Files:        {} ({} dirs)", file_count, dir_count);
        println!("Symbols:      {}", stats.symbols);
        println!("Calls:        {}", stats.calls);
        println!("Imports:      {}", stats.imports);
        println!();
        println!("Top extensions:");
        for (ext, count) in ext_list.iter().take(15) {
            println!("  {:12} {:>6}", ext, count);
        }
    }

    0
}

// =============================================================================
// List Files
// =============================================================================

fn cmd_list_files(prefix: Option<&str>, root: Option<&Path>, limit: usize, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let idx = match index::FileIndex::open(&root) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to open index: {}", e);
            return 1;
        }
    };

    let files = match idx.all_files() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to read files: {}", e);
            return 1;
        }
    };

    let prefix_str = prefix.unwrap_or("");
    let filtered: Vec<&str> = files
        .iter()
        .filter(|f| !f.is_dir && f.path.starts_with(prefix_str))
        .take(limit)
        .map(|f| f.path.as_str())
        .collect();

    if json {
        println!("{}", serde_json::to_string(&filtered).unwrap());
    } else {
        for path in &filtered {
            println!("{}", path);
        }
    }

    0
}

// =============================================================================
// Packages
// =============================================================================

/// Result of indexing packages for a language
struct IndexedCounts {
    packages: usize,
    symbols: usize,
}

fn cmd_packages(only: &[String], clear: bool, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let pkg_index = match external_packages::PackageIndex::open() {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to open package index: {}", e);
            return 1;
        }
    };

    if clear {
        if let Err(e) = pkg_index.clear() {
            eprintln!("Failed to clear index: {}", e);
            return 1;
        }
        if !json {
            println!("Cleared existing index");
        }
    }

    let mut results: std::collections::HashMap<&str, IndexedCounts> =
        std::collections::HashMap::new();

    let available: Vec<&str> = moss_languages::supported_languages()
        .iter()
        .map(|l| l.lang_key())
        .filter(|k| !k.is_empty())
        .collect();

    let ecosystems: Vec<&str> = if only.is_empty() {
        available.clone()
    } else {
        only.iter()
            .map(|s| s.as_str())
            .filter(|s| available.contains(s))
            .collect()
    };

    for eco in only {
        if !available.contains(&eco.as_str()) {
            eprintln!(
                "Error: unknown ecosystem '{}', valid options: {}",
                eco,
                available.join(", ")
            );
        }
    }

    for lang in moss_languages::supported_languages() {
        let lang_key = lang.lang_key();
        if lang_key.is_empty() || !ecosystems.contains(&lang_key) {
            continue;
        }
        if results.contains_key(lang_key) {
            continue;
        }
        let counts = index_language_packages(lang, &pkg_index, &root, json);
        results.insert(lang_key, counts);
    }

    if json {
        let mut json_obj = serde_json::Map::new();
        for (key, counts) in &results {
            json_obj.insert(
                format!("{}_packages", key),
                serde_json::json!(counts.packages),
            );
            json_obj.insert(
                format!("{}_symbols", key),
                serde_json::json!(counts.symbols),
            );
        }
        println!("{}", serde_json::Value::Object(json_obj));
    } else {
        println!("\nIndexing complete:");
        for (key, counts) in &results {
            println!(
                "  {}: {} packages, {} symbols",
                key, counts.packages, counts.symbols
            );
        }
    }

    0
}

fn count_and_insert_symbols(
    pkg_index: &external_packages::PackageIndex,
    pkg_id: i64,
    symbols: &[skeleton::SkeletonSymbol],
) -> usize {
    let mut count = 0;
    for sym in symbols {
        let _ = pkg_index.insert_symbol(
            pkg_id,
            &sym.name,
            sym.kind,
            &sym.signature,
            sym.start_line as u32,
        );
        count += 1;
        count += count_and_insert_symbols(pkg_index, pkg_id, &sym.children);
    }
    count
}

fn index_language_packages(
    lang: &dyn moss_languages::Language,
    pkg_index: &external_packages::PackageIndex,
    project_root: &Path,
    json: bool,
) -> IndexedCounts {
    let version = lang
        .get_version(project_root)
        .and_then(|v| external_packages::Version::parse(&v));

    let lang_key = lang.lang_key();
    if lang_key.is_empty() {
        return IndexedCounts {
            packages: 0,
            symbols: 0,
        };
    }

    if !json {
        println!(
            "Indexing {} packages (version {:?})...",
            lang.name(),
            version
        );
    }

    let sources = lang.package_sources(project_root);
    if sources.is_empty() {
        if !json {
            println!("  No package sources found");
        }
        return IndexedCounts {
            packages: 0,
            symbols: 0,
        };
    }

    let min_version = version.unwrap_or(external_packages::Version { major: 0, minor: 0 });
    let mut extractor = skeleton::SkeletonExtractor::new();
    let mut total_packages = 0;
    let mut total_symbols = 0;

    for source in sources {
        if !json {
            println!("  {}: {}", source.name, source.path.display());
        }

        let max_version = if source.version_specific {
            version
        } else {
            None
        };
        let discovered = lang.discover_packages(&source);

        for (pkg_name, pkg_path) in discovered {
            if let Ok(true) = pkg_index.is_indexed(lang_key, &pkg_name) {
                continue;
            }

            let pkg_id = match pkg_index.insert_package(
                lang_key,
                &pkg_name,
                &pkg_path.to_string_lossy(),
                min_version,
                max_version,
            ) {
                Ok(id) => id,
                Err(_) => continue,
            };

            total_packages += 1;
            total_symbols +=
                index_package_symbols(lang, pkg_index, &mut extractor, pkg_id, &pkg_path);
        }
    }

    IndexedCounts {
        packages: total_packages,
        symbols: total_symbols,
    }
}

fn index_package_symbols(
    lang: &dyn moss_languages::Language,
    pkg_index: &external_packages::PackageIndex,
    extractor: &mut skeleton::SkeletonExtractor,
    pkg_id: i64,
    path: &Path,
) -> usize {
    let entry = match lang.find_package_entry(path) {
        Some(e) => e,
        None => return 0,
    };

    if let Ok(content) = std::fs::read_to_string(&entry) {
        let result = extractor.extract(&entry, &content);
        return count_and_insert_symbols(pkg_index, pkg_id, &result.symbols);
    }

    0
}
