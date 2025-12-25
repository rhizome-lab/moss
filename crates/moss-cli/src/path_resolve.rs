use ignore::WalkBuilder;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use std::path::Path;

use crate::index::FileIndex;

#[derive(Debug, Clone)]
pub struct PathMatch {
    pub path: String,
    pub kind: String,
    pub score: u32,
}

/// Result of resolving a unified path like `src/main.py/Foo/bar`
#[derive(Debug, Clone)]
pub struct UnifiedPath {
    /// The file path portion (e.g., "src/main.py")
    pub file_path: String,
    /// The symbol path within the file (e.g., "Foo/bar"), empty if pointing to file itself
    pub symbol_path: Vec<String>,
    /// Whether the path resolved to a directory (no symbol path possible)
    pub is_directory: bool,
}

/// Normalize a unified path query, converting various separator styles to `/`.
/// Supports: `::` (Rust-style), `#` (URL fragment), `:` (compact)
fn normalize_separators(query: &str) -> String {
    query
        .replace("::", "/")
        .replace('#', "/")
        // Only replace single : if it looks like file:symbol (has file extension before it)
        .split(':')
        .enumerate()
        .map(|(i, part)| {
            if i == 0 {
                part.to_string()
            } else {
                format!("/{}", part)
            }
        })
        .collect::<String>()
}

/// Resolve a unified path like `src/main.py/Foo/bar` to file + symbol components.
///
/// Uses filesystem as source of truth: walks segments left-to-right, checking
/// at each step whether the path exists as file or directory. Once we hit a file,
/// remaining segments are the symbol path.
///
/// Strategy:
/// 1. Walk path segments, checking each accumulated path against filesystem
/// 2. When we hit a file, everything after is symbol path
/// 3. If exact path doesn't exist, try fuzzy matching for the file portion
pub fn resolve_unified(query: &str, root: &Path) -> Option<UnifiedPath> {
    let normalized = normalize_separators(query);

    // Handle absolute paths (start with /) - use filesystem root instead of project root
    let (segments, base_path): (Vec<&str>, std::path::PathBuf) = if normalized.starts_with('/') {
        let segs: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
        (segs, std::path::PathBuf::from("/"))
    } else {
        let segs: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
        (segs, root.to_path_buf())
    };
    let is_absolute = normalized.starts_with('/');

    if segments.is_empty() {
        return None;
    }

    // Strategy 1: Walk exact path segments
    let mut current_path = base_path.clone();
    for (idx, segment) in segments.iter().enumerate() {
        let test_path = current_path.join(segment);

        if test_path.is_file() {
            // Found a file - this is the boundary
            // For absolute paths, keep full path; for relative, strip root prefix
            let file_path = if is_absolute {
                test_path.to_string_lossy().to_string()
            } else {
                test_path
                    .strip_prefix(root)
                    .unwrap_or(&test_path)
                    .to_string_lossy()
                    .to_string()
            };
            return Some(UnifiedPath {
                file_path,
                symbol_path: segments[idx + 1..].iter().map(|s| s.to_string()).collect(),
                is_directory: false,
            });
        } else if test_path.is_dir() {
            current_path = test_path;
        } else {
            // Path doesn't exist - try fuzzy resolution (only for relative paths)
            break;
        }
    }

    // Check if we ended at a directory
    if current_path != base_path && current_path.is_dir() {
        let dir_path = if is_absolute {
            current_path.to_string_lossy().to_string()
        } else {
            current_path
                .strip_prefix(root)
                .unwrap_or(&current_path)
                .to_string_lossy()
                .to_string()
        };
        let matched_segments = dir_path.matches('/').count() + 1;
        if matched_segments >= segments.len() {
            return Some(UnifiedPath {
                file_path: dir_path,
                symbol_path: vec![],
                is_directory: true,
            });
        }
    }

    // Strategy 2: Try fuzzy matching (only for relative paths within project)
    if !is_absolute {
        for split_point in (1..=segments.len()).rev() {
            let file_query = segments[..split_point].join("/");
            let matches = resolve(&file_query, root);

            if let Some(m) = matches.first() {
                if m.kind == "file" {
                    return Some(UnifiedPath {
                        file_path: m.path.clone(),
                        symbol_path: segments[split_point..]
                            .iter()
                            .map(|s| s.to_string())
                            .collect(),
                        is_directory: false,
                    });
                } else if m.kind == "directory" && split_point == segments.len() {
                    // Only return directory if it's the full query
                    return Some(UnifiedPath {
                        file_path: m.path.clone(),
                        symbol_path: vec![],
                        is_directory: true,
                    });
                }
            }
        }
    }

    None
}

/// Resolve a query to ALL matching unified paths (for ambiguous queries).
/// Returns empty vec if no matches, single-element vec if unambiguous,
/// or multiple elements if query matches multiple files.
pub fn resolve_unified_all(query: &str, root: &Path) -> Vec<UnifiedPath> {
    let normalized = normalize_separators(query);

    // Absolute paths: single result or none
    if normalized.starts_with('/') {
        return resolve_unified(query, root).into_iter().collect();
    }

    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return vec![];
    }

    // Try exact path first
    let mut current_path = root.to_path_buf();
    for (idx, segment) in segments.iter().enumerate() {
        let test_path = current_path.join(segment);
        if test_path.is_file() {
            // Exact match - return single result
            let file_path = test_path
                .strip_prefix(root)
                .unwrap_or(&test_path)
                .to_string_lossy()
                .to_string();
            return vec![UnifiedPath {
                file_path,
                symbol_path: segments[idx + 1..].iter().map(|s| s.to_string()).collect(),
                is_directory: false,
            }];
        } else if test_path.is_dir() {
            current_path = test_path;
        } else {
            break;
        }
    }

    // Check if we ended at a directory (exact match)
    if current_path != root.to_path_buf() && current_path.is_dir() {
        let dir_path = current_path
            .strip_prefix(root)
            .unwrap_or(&current_path)
            .to_string_lossy()
            .to_string();
        return vec![UnifiedPath {
            file_path: dir_path,
            symbol_path: vec![],
            is_directory: true,
        }];
    }

    // Fuzzy matching - return ALL matches
    for split_point in (1..=segments.len()).rev() {
        let file_query = segments[..split_point].join("/");
        let matches = resolve(&file_query, root);

        if !matches.is_empty() {
            return matches
                .into_iter()
                .map(|m| UnifiedPath {
                    file_path: m.path,
                    symbol_path: segments[split_point..]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    is_directory: m.kind == "directory",
                })
                .collect();
        }
    }

    vec![]
}

/// Get all files in the repository (uses index if available)
pub fn all_files(root: &Path) -> Vec<PathMatch> {
    get_paths_for_query(root, "")
        .into_iter()
        .map(|(path, is_dir)| PathMatch {
            path,
            kind: if is_dir { "directory" } else { "file" }.to_string(),
            score: 0,
        })
        .collect()
}

/// Resolve a fuzzy query to matching paths.
///
/// Handles:
/// - Absolute paths: /tmp/foo.py (if file exists)
/// - Extension patterns: .rs, .py (returns all matching files)
/// - Exact paths: src/moss/dwim.py
/// - Partial filenames: dwim.py, dwim
/// - Directory names: moss, src
pub fn resolve(query: &str, root: &Path) -> Vec<PathMatch> {
    // Handle absolute paths first - check if file exists directly
    if query.starts_with('/') {
        let abs_path = std::path::Path::new(query);
        if abs_path.is_file() {
            return vec![PathMatch {
                path: query.to_string(),
                kind: "file".to_string(),
                score: u32::MAX,
            }];
        } else if abs_path.is_dir() {
            return vec![PathMatch {
                path: query.to_string(),
                kind: "directory".to_string(),
                score: u32::MAX,
            }];
        }
        // Absolute path doesn't exist - return empty
        return vec![];
    }

    // Handle file:symbol syntax (defer symbol resolution to Python for now)
    if query.contains(':') {
        let file_part = query.split(':').next().unwrap();
        return resolve(file_part, root);
    }

    // Handle extension patterns (e.g., ".rs", ".py") - return all matches directly
    if query.starts_with('.') && !query.contains('/') {
        if let Ok(mut index) = FileIndex::open(root) {
            let _ = index.incremental_refresh();
            if let Ok(files) = index.find_like(query) {
                return files
                    .into_iter()
                    .map(|f| PathMatch {
                        path: f.path,
                        kind: if f.is_dir { "directory" } else { "file" }.to_string(),
                        score: u32::MAX,
                    })
                    .collect();
            }
        }
    }

    // Get candidate paths (uses LIKE for fast filtering when possible)
    let all_paths = get_paths_for_query(root, query);

    resolve_from_paths(query, &all_paths)
}

/// Get paths matching query using LIKE, fallback to all files
fn get_paths_for_query(root: &Path, query: &str) -> Vec<(String, bool)> {
    if let Ok(mut index) = FileIndex::open(root) {
        let _ = index.incremental_refresh();
        // Try LIKE first for faster queries
        if !query.is_empty() {
            if let Ok(files) = index.find_like(query) {
                if !files.is_empty() {
                    return files.into_iter().map(|f| (f.path, f.is_dir)).collect();
                }
            }
        }
        // Fall back to all files for empty query or no LIKE matches
        if let Ok(files) = index.all_files() {
            return files.into_iter().map(|f| (f.path, f.is_dir)).collect();
        }
    }
    // Fall back to filesystem walk
    let mut all_paths: Vec<(String, bool)> = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy().to_string();
            // Skip empty paths and .git directory
            if rel_str.is_empty() || rel_str == ".git" || rel_str.starts_with(".git/") {
                continue;
            }
            let is_dir = path.is_dir();
            all_paths.push((rel_str, is_dir));
        }
    }

    all_paths
}

/// Normalize a char for comparison
#[inline]
fn normalize_char(c: char) -> char {
    match c {
        '-' | '.' | '_' => ' ',
        c => c.to_ascii_lowercase(),
    }
}

/// Compare two strings with normalization (no allocation)
fn eq_normalized(a: &str, b: &str) -> bool {
    let mut a_chars = a.chars().map(normalize_char);
    let mut b_chars = b.chars().map(normalize_char);
    loop {
        match (a_chars.next(), b_chars.next()) {
            (Some(ac), Some(bc)) if ac == bc => continue,
            (None, None) => return true,
            _ => return false,
        }
    }
}

/// Normalize string for comparison (used for filename matching)
fn normalize_for_match(s: &str) -> String {
    s.chars().map(normalize_char).collect()
}

/// Resolve from a pre-loaded list of paths
fn resolve_from_paths(query: &str, all_paths: &[(String, bool)]) -> Vec<PathMatch> {
    let query_lower = query.to_lowercase();
    let query_normalized = normalize_for_match(query);

    // Try normalized path match (handles exact match too, no allocation)
    for (path, is_dir) in all_paths {
        if eq_normalized(path, query) {
            return vec![PathMatch {
                path: path.clone(),
                kind: if *is_dir { "directory" } else { "file" }.to_string(),
                score: u32::MAX,
            }];
        }
    }

    // Try exact filename/dirname match (case-insensitive, _ and - equivalent)
    let mut exact_matches: Vec<PathMatch> = Vec::new();
    for (path, is_dir) in all_paths {
        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let stem = Path::new(path)
            .file_stem()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let name_normalized = normalize_for_match(&name);
        let stem_normalized = normalize_for_match(&stem);

        if name == query_lower
            || stem == query_lower
            || name_normalized == query_normalized
            || stem_normalized == query_normalized
        {
            exact_matches.push(PathMatch {
                path: path.clone(),
                kind: if *is_dir { "directory" } else { "file" }.to_string(),
                score: u32::MAX - 1,
            });
        }
    }

    if !exact_matches.is_empty() {
        return exact_matches;
    }

    // Fuzzy match using nucleo
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

    let mut fuzzy_matches: Vec<PathMatch> = Vec::new();

    for (path, is_dir) in all_paths {
        let mut buf = Vec::new();
        if let Some(score) =
            pattern.score(nucleo_matcher::Utf32Str::new(path, &mut buf), &mut matcher)
        {
            fuzzy_matches.push(PathMatch {
                path: path.clone(),
                kind: if *is_dir { "directory" } else { "file" }.to_string(),
                score,
            });
        }
    }

    // Sort by score descending, take top 10
    fuzzy_matches.sort_by(|a, b| b.score.cmp(&a.score));
    fuzzy_matches.truncate(10);

    fuzzy_matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_exact_match() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/cli.py"), "").unwrap();

        let matches = resolve("src/moss/cli.py", dir.path());
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, "src/moss/cli.py");
    }

    #[test]
    fn test_filename_match() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/dwim.py"), "").unwrap();

        let matches = resolve("dwim.py", dir.path());
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, "src/moss/dwim.py");
    }

    #[test]
    fn test_stem_match() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/dwim.py"), "").unwrap();

        let matches = resolve("dwim", dir.path());
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, "src/moss/dwim.py");
    }

    #[test]
    fn test_underscore_hyphen_equivalence() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("docs")).unwrap();
        fs::write(dir.path().join("docs/prior-art.md"), "").unwrap();

        // underscore query should match hyphen filename
        let matches = resolve("prior_art", dir.path());
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, "docs/prior-art.md");

        // hyphen query should also work
        let matches = resolve("prior-art", dir.path());
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, "docs/prior-art.md");

        // full path with underscores should match hyphenated path
        let matches = resolve("docs/prior_art.md", dir.path());
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, "docs/prior-art.md");
    }

    #[test]
    fn test_unified_path_file_only() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/cli.py"), "").unwrap();

        let result = resolve_unified("src/moss/cli.py", dir.path());
        assert!(result.is_some());
        let u = result.unwrap();
        assert_eq!(u.file_path, "src/moss/cli.py");
        assert!(u.symbol_path.is_empty());
        assert!(!u.is_directory);
    }

    #[test]
    fn test_unified_path_with_symbol() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/cli.py"), "").unwrap();

        // File with symbol path
        let result = resolve_unified("src/moss/cli.py/Foo/bar", dir.path());
        assert!(result.is_some());
        let u = result.unwrap();
        assert_eq!(u.file_path, "src/moss/cli.py");
        assert_eq!(u.symbol_path, vec!["Foo", "bar"]);
        assert!(!u.is_directory);
    }

    #[test]
    fn test_unified_path_directory() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/cli.py"), "").unwrap();

        let result = resolve_unified("src/moss", dir.path());
        assert!(result.is_some());
        let u = result.unwrap();
        assert_eq!(u.file_path, "src/moss");
        assert!(u.symbol_path.is_empty());
        assert!(u.is_directory);
    }

    #[test]
    fn test_unified_path_rust_style_separator() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/cli.py"), "").unwrap();

        // Rust-style :: separator
        let result = resolve_unified("src/moss/cli.py::Foo::bar", dir.path());
        assert!(result.is_some());
        let u = result.unwrap();
        assert_eq!(u.file_path, "src/moss/cli.py");
        assert_eq!(u.symbol_path, vec!["Foo", "bar"]);
    }

    #[test]
    fn test_unified_path_hash_separator() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/cli.py"), "").unwrap();

        // URL fragment-style # separator
        let result = resolve_unified("src/moss/cli.py#Foo", dir.path());
        assert!(result.is_some());
        let u = result.unwrap();
        assert_eq!(u.file_path, "src/moss/cli.py");
        assert_eq!(u.symbol_path, vec!["Foo"]);
    }

    #[test]
    fn test_unified_path_colon_separator() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/cli.py"), "").unwrap();

        // Compact : separator
        let result = resolve_unified("src/moss/cli.py:Foo:bar", dir.path());
        assert!(result.is_some());
        let u = result.unwrap();
        assert_eq!(u.file_path, "src/moss/cli.py");
        assert_eq!(u.symbol_path, vec!["Foo", "bar"]);
    }

    #[test]
    fn test_unified_path_fuzzy_file() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/cli.py"), "").unwrap();

        // Fuzzy file match with symbol
        let result = resolve_unified("cli.py/Foo", dir.path());
        assert!(result.is_some());
        let u = result.unwrap();
        assert_eq!(u.file_path, "src/moss/cli.py");
        assert_eq!(u.symbol_path, vec!["Foo"]);
    }

    #[test]
    fn test_unified_path_absolute() {
        let dir = tempdir().unwrap();
        let abs_path = dir.path().join("test.py");
        fs::write(&abs_path, "def foo(): pass").unwrap();

        // Absolute path should resolve directly
        let abs_str = abs_path.to_string_lossy().to_string();
        let result = resolve_unified(&abs_str, Path::new("/some/other/root"));
        assert!(result.is_some());
        let u = result.unwrap();
        assert_eq!(u.file_path, abs_str);
        assert!(u.symbol_path.is_empty());
        assert!(!u.is_directory);
    }

    #[test]
    fn test_unified_path_absolute_with_symbol() {
        let dir = tempdir().unwrap();
        let abs_path = dir.path().join("test.py");
        fs::write(&abs_path, "def foo(): pass").unwrap();

        // Absolute path with symbol
        let query = format!("{}/foo", abs_path.to_string_lossy());
        let result = resolve_unified(&query, Path::new("/some/other/root"));
        assert!(result.is_some());
        let u = result.unwrap();
        assert_eq!(u.file_path, abs_path.to_string_lossy().to_string());
        assert_eq!(u.symbol_path, vec!["foo"]);
    }

    #[test]
    fn test_unified_path_unicode() {
        let dir = tempdir().unwrap();
        let unicode_dir = dir.path().join("日本語");
        fs::create_dir_all(&unicode_dir).unwrap();
        let unicode_file = unicode_dir.join("テスト.py");
        fs::write(&unicode_file, "def hello(): pass").unwrap();

        // Absolute unicode path
        let abs_str = unicode_file.to_string_lossy().to_string();
        let result = resolve_unified(&abs_str, Path::new("/some/other/root"));
        assert!(result.is_some());
        let u = result.unwrap();
        assert_eq!(u.file_path, abs_str);
        assert!(!u.is_directory);
    }
}
