//! Index packages command - index external packages into the global cache.

use crate::skeleton;
use moss_languages::external_packages;
use std::path::{Path, PathBuf};

/// Index external packages into the global cache.
pub fn cmd_index_packages(only: &[String], clear: bool, root: Option<&Path>, json: bool) -> i32 {
    let root = root.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });

    // Open or create the index
    let index = match external_packages::PackageIndex::open() {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to open package index: {}", e);
            return 1;
        }
    };

    if clear {
        if let Err(e) = index.clear() {
            eprintln!("Failed to clear index: {}", e);
            return 1;
        }
        if !json {
            println!("Cleared existing index");
        }
    }

    // Collect results per language
    let mut results: std::collections::HashMap<&str, (usize, usize)> = std::collections::HashMap::new();

    // Get all available lang_keys from registered languages
    let available: Vec<&str> = moss_languages::supported_languages()
        .iter()
        .map(|l| l.lang_key())
        .filter(|k| !k.is_empty())
        .collect();

    // Filter to requested ecosystems
    let ecosystems: Vec<&str> = if only.is_empty() {
        available.clone()
    } else {
        only.iter()
            .map(|s| s.as_str())
            .filter(|s| available.contains(s))
            .collect()
    };

    // Log error for unknown ecosystems
    for eco in only {
        if !available.contains(&eco.as_str()) {
            eprintln!("Error: unknown ecosystem '{}', valid options: {}", eco, available.join(", "));
        }
    }

    // Index each language using the generic indexer
    for lang in moss_languages::supported_languages() {
        let lang_key = lang.lang_key();
        if lang_key.is_empty() || !ecosystems.contains(&lang_key) {
            continue;
        }
        // Skip if we already indexed this lang_key (e.g., TypeScript shares "js" with JavaScript)
        if results.contains_key(lang_key) {
            continue;
        }
        let (pkgs, syms) = index_language_packages(lang, &index, &root, json);
        results.insert(lang_key, (pkgs, syms));
    }

    // Output results
    if json {
        let mut json_obj = serde_json::Map::new();
        for (key, (pkgs, syms)) in &results {
            json_obj.insert(format!("{}_packages", key), serde_json::json!(pkgs));
            json_obj.insert(format!("{}_symbols", key), serde_json::json!(syms));
        }
        println!("{}", serde_json::Value::Object(json_obj));
    } else {
        println!("\nIndexing complete:");
        for (key, (pkgs, syms)) in &results {
            println!("  {}: {} packages, {} symbols", key, pkgs, syms);
        }
    }

    0
}

/// Count symbols and insert them into the index.
fn count_and_insert_symbols(
    index: &external_packages::PackageIndex,
    pkg_id: i64,
    symbols: &[skeleton::SkeletonSymbol],
) -> usize {
    let mut count = 0;
    for sym in symbols {
        let _ = index.insert_symbol(
            pkg_id,
            &sym.name,
            sym.kind,
            &sym.signature,
            sym.start_line as u32,
        );
        count += 1;
        count += count_and_insert_symbols(index, pkg_id, &sym.children);
    }
    count
}

/// Index packages for a language using its package_sources().
fn index_language_packages(
    lang: &dyn moss_languages::Language,
    index: &external_packages::PackageIndex,
    project_root: &Path,
    json: bool,
) -> (usize, usize) {
    let version = lang.get_version(project_root)
        .and_then(|v| external_packages::Version::parse(&v));

    let lang_key = lang.lang_key();
    if lang_key.is_empty() {
        return (0, 0);
    }

    if !json {
        println!("Indexing {} packages (version {:?})...", lang.name(), version);
    }

    let sources = lang.package_sources(project_root);
    if sources.is_empty() {
        if !json {
            println!("  No package sources found");
        }
        return (0, 0);
    }

    let min_version = version.unwrap_or(external_packages::Version { major: 0, minor: 0 });
    let mut extractor = skeleton::SkeletonExtractor::new();
    let mut total_packages = 0;
    let mut total_symbols = 0;

    for source in sources {
        if !json {
            println!("  {}: {}", source.name, source.path.display());
        }

        let max_version = if source.version_specific { version } else { None };

        // Use the trait's discover_packages method - no kind-specific dispatch here
        let discovered = lang.discover_packages(&source);

        for (pkg_name, pkg_path) in discovered {
            if let Ok(true) = index.is_indexed(lang_key, &pkg_name) {
                continue;
            }

            let pkg_id = match index.insert_package(
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
            total_symbols += index_package_symbols(lang, index, &mut extractor, pkg_id, &pkg_path);
        }
    }

    (total_packages, total_symbols)
}

/// Index symbols from a package path (file or directory).
fn index_package_symbols(
    lang: &dyn moss_languages::Language,
    index: &external_packages::PackageIndex,
    extractor: &mut skeleton::SkeletonExtractor,
    pkg_id: i64,
    path: &Path,
) -> usize {
    // Use trait method to find entry point
    let entry = match lang.find_package_entry(path) {
        Some(e) => e,
        None => return 0,
    };

    if let Ok(content) = std::fs::read_to_string(&entry) {
        let result = extractor.extract(&entry, &content);
        return count_and_insert_symbols(index, pkg_id, &result.symbols);
    }

    0
}
