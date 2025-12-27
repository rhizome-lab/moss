use crate::config::MossConfig;
use crate::paths::get_moss_dir;
use ignore::WalkBuilder;
use moss_languages::support_for_path;
use rayon::prelude::*;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::symbols::{Import, Symbol, SymbolParser};

/// Parsed data for a single file, ready for database insertion
struct ParsedFileData {
    file_path: String,
    /// (name, kind, start_line, end_line, parent, complexity)
    symbols: Vec<(String, String, usize, usize, Option<String>, Option<usize>)>,
    /// (caller_symbol, callee_name, callee_qualifier, line)
    calls: Vec<(String, String, Option<String>, usize)>,
    /// imports (for Python files only)
    imports: Vec<Import>,
}

// Not yet public - just delete .moss/index.sqlite on schema changes
const SCHEMA_VERSION: i64 = 5;

/// Supported source file extensions for call graph indexing
const SOURCE_EXTENSIONS: &[&str] = &[
    ".py", ".rs", ".java", ".ts", ".tsx", ".js", ".mjs", ".cjs", ".go", ".json", ".yaml", ".yml",
    ".toml",
];

/// Check if a file path has a supported source extension
fn is_source_file(path: &str) -> bool {
    SOURCE_EXTENSIONS.iter().any(|ext| path.ends_with(ext))
}

/// Generate SQL WHERE clause for filtering source files
/// Returns: "path LIKE '%.py' OR path LIKE '%.rs' OR ..."
fn source_extensions_sql_filter() -> String {
    SOURCE_EXTENSIONS
        .iter()
        .map(|ext| format!("path LIKE '%{}'", ext))
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexedFile {
    pub path: String,
    pub is_dir: bool,
    pub mtime: i64,
    pub lines: usize,
}

/// Result from symbol search
#[derive(Debug, Clone, serde::Serialize)]
pub struct SymbolMatch {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub parent: Option<String>,
}

/// Files that changed since last index
#[derive(Debug, Default)]
pub struct ChangedFiles {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

/// Call graph statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct CallGraphStats {
    pub symbols: usize,
    pub calls: usize,
    pub imports: usize,
}

pub struct FileIndex {
    conn: Connection,
    root: PathBuf,
}

impl FileIndex {
    /// Open or create an index for a directory.
    /// Index is stored in .moss/index.sqlite (or MOSS_INDEX_DIR if set)
    /// On corruption, automatically deletes and recreates the index.
    pub fn open(root: &Path) -> rusqlite::Result<Self> {
        let moss_dir = get_moss_dir(root);
        std::fs::create_dir_all(&moss_dir).ok();

        let db_path = moss_dir.join("index.sqlite");

        // Try to open, with recovery on corruption
        match Self::try_open(&db_path, root) {
            Ok(idx) => Ok(idx),
            Err(e) => {
                // Check for corruption-like errors
                let err_str = e.to_string().to_lowercase();
                let is_corruption = err_str.contains("corrupt")
                    || err_str.contains("malformed")
                    || err_str.contains("disk i/o error")
                    || err_str.contains("not a database")
                    || err_str.contains("database disk image")
                    || err_str.contains("integrity check failed");

                if is_corruption {
                    eprintln!("Index corrupted, rebuilding: {}", e);
                    // Delete corrupted database and retry
                    let _ = std::fs::remove_file(&db_path);
                    // Also remove journal/wal files if they exist
                    let _ = std::fs::remove_file(db_path.with_extension("sqlite-journal"));
                    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
                    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
                    Self::try_open(&db_path, root)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Open index only if indexing is enabled in config.
    /// Returns None if `[index] enabled = false`.
    pub fn open_if_enabled(root: &Path) -> Option<Self> {
        let config = MossConfig::load(root);
        if !config.index.enabled() {
            return None;
        }
        Self::open(root).ok()
    }

    /// Internal: try to open database without recovery
    fn try_open(db_path: &Path, root: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(&db_path)?;

        // Quick integrity check - this will catch most corruption
        // PRAGMA quick_check is faster than full integrity_check
        let integrity: String = conn
            .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
            .unwrap_or_else(|_| "error".to_string());
        if integrity != "ok" {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(11), // SQLITE_CORRUPT
                Some(format!("Database integrity check failed: {}", integrity)),
            ));
        }

        // Initialize schema
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT
            );
            CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                is_dir INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                lines INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_files_name ON files(path);

            -- Call graph for fast caller/callee lookups
            -- callee_qualifier: for foo.bar(), this is 'foo'; for bar(), this is NULL
            CREATE TABLE IF NOT EXISTS calls (
                caller_file TEXT NOT NULL,
                caller_symbol TEXT NOT NULL,
                callee_name TEXT NOT NULL,
                callee_qualifier TEXT,
                line INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_calls_callee ON calls(callee_name);
            CREATE INDEX IF NOT EXISTS idx_calls_caller ON calls(caller_file, caller_symbol);
            CREATE INDEX IF NOT EXISTS idx_calls_qualifier ON calls(callee_qualifier);

            -- Symbol definitions for fast symbol lookups
            CREATE TABLE IF NOT EXISTS symbols (
                file TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                parent TEXT,
                complexity INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
            CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file);

            -- Import tracking for cross-file resolution
            -- module = source module (e.g. 'pathlib', 'moss.gen.serialize')
            -- name = imported name (e.g. 'Path', 'emit_tool_definition', or '*' for wildcard)
            -- alias = local name if aliased (e.g. 'emit' for 'as emit')
            CREATE TABLE IF NOT EXISTS imports (
                file TEXT NOT NULL,
                module TEXT,
                name TEXT NOT NULL,
                alias TEXT,
                line INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_imports_file ON imports(file);
            CREATE INDEX IF NOT EXISTS idx_imports_name ON imports(name);
            CREATE INDEX IF NOT EXISTS idx_imports_module ON imports(module);

            -- Cross-language references (e.g., Python importing Rust PyO3 modules)
            -- source_file: file containing the import/call
            -- source_lang: language of source file (python, rust, etc.)
            -- target_crate: target crate/module name
            -- target_lang: language of target (rust, python, etc.)
            -- ref_type: pyo3_import, cffi, ctypes, etc.
            CREATE TABLE IF NOT EXISTS cross_refs (
                source_file TEXT NOT NULL,
                source_lang TEXT NOT NULL,
                target_crate TEXT NOT NULL,
                target_lang TEXT NOT NULL,
                ref_type TEXT NOT NULL,
                line INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cross_refs_source ON cross_refs(source_file);
            CREATE INDEX IF NOT EXISTS idx_cross_refs_target ON cross_refs(target_crate);
            ",
        )?;

        // Check schema version
        let version: i64 = conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if version != SCHEMA_VERSION {
            // Reset on schema change
            conn.execute("DELETE FROM files", [])?;
            conn.execute("DELETE FROM calls", []).ok();
            conn.execute("DELETE FROM symbols", []).ok();
            conn.execute("DELETE FROM imports", []).ok();
            conn.execute("DELETE FROM cross_refs", []).ok();
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
                params![SCHEMA_VERSION.to_string()],
            )?;
        }

        Ok(Self {
            conn,
            root: root.to_path_buf(),
        })
    }

    /// Get a reference to the underlying SQLite connection for direct queries
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Get files that have changed since last index
    pub fn get_changed_files(&self) -> rusqlite::Result<ChangedFiles> {
        let mut result = ChangedFiles::default();

        // Get all indexed files with their mtimes
        let mut indexed: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT path, mtime FROM files WHERE is_dir = 0")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let path: String = row.get(0)?;
                let mtime: i64 = row.get(1)?;
                indexed.insert(path, mtime);
            }
        }

        // Walk current filesystem
        let walker = WalkBuilder::new(&self.root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        let mut seen = std::collections::HashSet::new();
        for entry in walker.flatten() {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            if let Ok(rel) = path.strip_prefix(&self.root) {
                let rel_str = rel.to_string_lossy().to_string();
                // Skip internal directories
                if rel_str.is_empty() || rel_str == ".git" || rel_str.starts_with(".git/") {
                    continue;
                }
                seen.insert(rel_str.clone());

                let current_mtime = path
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                if let Some(&indexed_mtime) = indexed.get(&rel_str) {
                    if current_mtime > indexed_mtime {
                        result.modified.push(rel_str);
                    }
                } else {
                    result.added.push(rel_str);
                }
            }
        }

        // Find deleted files
        for path in indexed.keys() {
            if !seen.contains(path) {
                result.deleted.push(path.clone());
            }
        }

        Ok(result)
    }

    /// Check if refresh is needed using fast heuristics.
    /// Returns true if changes are likely.
    fn needs_refresh(&self) -> bool {
        let last_indexed: i64 = self
            .conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'last_indexed'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Never indexed
        if last_indexed == 0 {
            return true;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Allow 60s staleness - don't check on every call
        if now - last_indexed < 60 {
            return false;
        }

        // Check mtimes of top-level entries (catches new/deleted files)
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        let mtime_secs = mtime
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        if mtime_secs > last_indexed {
                            return true;
                        }
                    }
                }
            }
        }

        // Sample some indexed files to catch modifications
        // Check ~100 files spread across the index
        if let Ok(mut stmt) = self
            .conn
            .prepare("SELECT path, mtime FROM files WHERE is_dir = 0 ORDER BY RANDOM() LIMIT 100")
        {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            }) {
                for row in rows.flatten() {
                    let (path, indexed_mtime) = row;
                    let full_path = self.root.join(&path);
                    if let Ok(meta) = full_path.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            let current_mtime = mtime
                                .duration_since(UNIX_EPOCH)
                                .map(|d| d.as_secs() as i64)
                                .unwrap_or(0);
                            if current_mtime > indexed_mtime {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Refresh only files that have changed (faster than full refresh)
    /// Returns number of files updated
    pub fn incremental_refresh(&mut self) -> rusqlite::Result<usize> {
        if !self.needs_refresh() {
            return Ok(0);
        }

        let changed = self.get_changed_files()?;
        let total_changes = changed.added.len() + changed.modified.len() + changed.deleted.len();

        if total_changes == 0 {
            return Ok(0);
        }

        let tx = self.conn.transaction()?;

        // Delete removed files
        for path in &changed.deleted {
            tx.execute("DELETE FROM files WHERE path = ?1", params![path])?;
        }

        // Update/insert changed files
        for path in changed.added.iter().chain(changed.modified.iter()) {
            let full_path = self.root.join(path);
            let is_dir = full_path.is_dir();
            let mtime = full_path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            // Count lines for text files under 1MB (skip binary/large files)
            let lines = if is_dir {
                0
            } else {
                full_path
                    .metadata()
                    .ok()
                    .filter(|m| m.len() < 1_000_000)
                    .and_then(|_| std::fs::read_to_string(&full_path).ok())
                    .map(|s| s.lines().count())
                    .unwrap_or(0)
            };

            tx.execute(
                "INSERT OR REPLACE INTO files (path, is_dir, mtime, lines) VALUES (?1, ?2, ?3, ?4)",
                params![path, is_dir as i64, mtime, lines as i64],
            )?;
        }

        // Update last indexed time
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_indexed', ?1)",
            params![now.to_string()],
        )?;

        tx.commit()?;
        Ok(total_changes)
    }

    /// Refresh the index by walking the filesystem
    pub fn refresh(&mut self) -> rusqlite::Result<usize> {
        let walker = WalkBuilder::new(&self.root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        // Start transaction for batch insert
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM files", [])?;

        let mut count = 0;
        for entry in walker.flatten() {
            let path = entry.path();
            if let Ok(rel) = path.strip_prefix(&self.root) {
                let rel_str = rel.to_string_lossy().to_string();
                // Skip internal directories
                if rel_str.is_empty() || rel_str == ".git" || rel_str.starts_with(".git/") {
                    continue;
                }

                let is_dir = path.is_dir();
                let mtime = path
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                // Count lines for text files under 1MB (skip binary/large files)
                let lines = if is_dir {
                    0
                } else {
                    path.metadata()
                        .ok()
                        .filter(|m| m.len() < 1_000_000)
                        .and_then(|_| std::fs::read_to_string(path).ok())
                        .map(|s| s.lines().count())
                        .unwrap_or(0)
                };

                tx.execute(
                    "INSERT INTO files (path, is_dir, mtime, lines) VALUES (?1, ?2, ?3, ?4)",
                    params![rel_str, is_dir as i64, mtime, lines as i64],
                )?;
                count += 1;
            }
        }

        // Update last indexed time
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_indexed', ?1)",
            params![now.to_string()],
        )?;

        tx.commit()?;
        Ok(count)
    }

    /// Get all files from the index
    pub fn all_files(&self) -> rusqlite::Result<Vec<IndexedFile>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, is_dir, mtime, lines FROM files")?;
        let files = stmt
            .query_map([], |row| {
                Ok(IndexedFile {
                    path: row.get(0)?,
                    is_dir: row.get::<_, i64>(1)? != 0,
                    mtime: row.get(2)?,
                    lines: row.get::<_, i64>(3)? as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }

    /// Search files by exact name match
    pub fn find_by_name(&self, name: &str) -> rusqlite::Result<Vec<IndexedFile>> {
        let pattern = format!("%/{}", name);
        let mut stmt = self.conn.prepare(
            "SELECT path, is_dir, mtime, lines FROM files WHERE path LIKE ?1 OR path = ?2",
        )?;
        let files = stmt
            .query_map(params![pattern, name], |row| {
                Ok(IndexedFile {
                    path: row.get(0)?,
                    is_dir: row.get::<_, i64>(1)? != 0,
                    mtime: row.get(2)?,
                    lines: row.get::<_, i64>(3)? as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }

    /// Search files by stem (filename without extension)
    pub fn find_by_stem(&self, stem: &str) -> rusqlite::Result<Vec<IndexedFile>> {
        let pattern = format!("%/{}%", stem);
        let mut stmt = self
            .conn
            .prepare("SELECT path, is_dir, mtime, lines FROM files WHERE path LIKE ?1")?;
        let files = stmt
            .query_map(params![pattern], |row| {
                Ok(IndexedFile {
                    path: row.get(0)?,
                    is_dir: row.get::<_, i64>(1)? != 0,
                    mtime: row.get(2)?,
                    lines: row.get::<_, i64>(3)? as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }

    /// Count indexed files
    pub fn count(&self) -> rusqlite::Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
    }

    /// Index symbols and call graph for a file
    #[allow(dead_code)] // FileIndex API - used by daemon
    pub fn index_file_symbols(
        &self,
        path: &str,
        symbols: &[Symbol],
        calls: &[(String, String, usize)],
    ) -> rusqlite::Result<()> {
        // Insert symbols
        for sym in symbols {
            self.conn.execute(
                "INSERT INTO symbols (file, name, kind, start_line, end_line, parent, complexity) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![path, sym.name, sym.kind.as_str(), sym.start_line, sym.end_line, sym.parent, sym.complexity],
            )?;
        }

        // Insert calls (caller_symbol, callee_name, line)
        for (caller_symbol, callee_name, line) in calls {
            self.conn.execute(
                "INSERT INTO calls (caller_file, caller_symbol, callee_name, line) VALUES (?1, ?2, ?3, ?4)",
                params![path, caller_symbol, callee_name, line],
            )?;
        }

        Ok(())
    }

    /// Find callers of a symbol by name (from call graph)
    /// Resolves through imports: if file A imports X as Y and calls Y(), finds that as a caller of X
    /// Also handles qualified calls: if file A does `import foo` and calls `foo.bar()`, finds caller of `bar`
    /// Also handles method calls: `self.method()` is resolved to the containing class's method
    pub fn find_callers(
        &self,
        symbol_name: &str,
    ) -> rusqlite::Result<Vec<(String, String, usize)>> {
        // Handle Class.method format - split and search for method within class
        let (class_filter, method_name) = if symbol_name.contains('.') {
            let parts: Vec<&str> = symbol_name.splitn(2, '.').collect();
            (Some(parts[0]), parts[1])
        } else {
            (None, symbol_name)
        };

        // If searching for Class.method, find callers that call self.method within that class
        if let Some(class_name) = class_filter {
            let mut stmt = self.conn.prepare(
                "SELECT c.caller_file, c.caller_symbol, c.line
                 FROM calls c
                 JOIN symbols s ON c.caller_file = s.file AND c.caller_symbol = s.name
                 WHERE c.callee_name = ?1 AND c.callee_qualifier = 'self' AND s.parent = ?2",
            )?;
            let callers: Vec<(String, String, usize)> = stmt
                .query_map(params![method_name, class_name], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .filter_map(|r| r.ok())
                .collect();

            if !callers.is_empty() {
                return Ok(callers);
            }
        }

        // Combined query: direct calls + calls via import aliases + qualified calls via module imports
        // 1. Direct calls where callee_name matches
        // 2. Calls where the callee_name matches an import alias/name that refers to our symbol
        // 3. Qualified calls (foo.bar()) where foo is an imported module containing bar
        // 4. Method calls via self (self.method()) - caller's parent class has this method
        let mut stmt = self.conn.prepare(
            "SELECT caller_file, caller_symbol, line FROM calls WHERE callee_name = ?1
             UNION
             SELECT c.caller_file, c.caller_symbol, c.line
             FROM calls c
             JOIN imports i ON c.caller_file = i.file AND c.callee_name = COALESCE(i.alias, i.name)
             WHERE i.name = ?1
             UNION
             SELECT c.caller_file, c.caller_symbol, c.line
             FROM calls c
             JOIN imports i ON c.caller_file = i.file AND c.callee_qualifier = COALESCE(i.alias, i.name)
             WHERE c.callee_name = ?1 AND i.module IS NULL
             UNION
             SELECT c.caller_file, c.caller_symbol, c.line
             FROM calls c
             JOIN symbols s ON c.caller_file = s.file AND c.caller_symbol = s.name
             WHERE c.callee_name = ?1 AND c.callee_qualifier = 'self' AND s.parent IS NOT NULL"
        )?;
        let callers: Vec<(String, String, usize)> = stmt
            .query_map(params![method_name], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        if !callers.is_empty() {
            return Ok(callers);
        }

        // Try case-insensitive match (direct only for simplicity)
        let mut stmt = self.conn.prepare(
            "SELECT caller_file, caller_symbol, line FROM calls WHERE LOWER(callee_name) = LOWER(?1)"
        )?;
        let callers: Vec<(String, String, usize)> = stmt
            .query_map(params![method_name], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        if !callers.is_empty() {
            return Ok(callers);
        }

        // Try LIKE pattern match (contains)
        let pattern = format!("%{}%", method_name);
        let mut stmt = self.conn.prepare(
            "SELECT caller_file, caller_symbol, line FROM calls WHERE LOWER(callee_name) LIKE LOWER(?1) LIMIT 100"
        )?;
        let callers = stmt
            .query_map(params![pattern], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(callers)
    }

    /// Find callees of a symbol (what it calls)
    pub fn find_callees(
        &self,
        file: &str,
        symbol_name: &str,
    ) -> rusqlite::Result<Vec<(String, usize)>> {
        let mut stmt = self.conn.prepare(
            "SELECT callee_name, line FROM calls WHERE caller_file = ?1 AND caller_symbol = ?2",
        )?;
        let callees = stmt
            .query_map(params![file, symbol_name], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(callees)
    }

    /// Find callees with resolved import info (name, line, source_module)
    /// Returns: (local_name, line, Option<(source_module, original_name)>)
    pub fn find_callees_resolved(
        &self,
        file: &str,
        symbol_name: &str,
    ) -> rusqlite::Result<Vec<(String, usize, Option<(String, String)>)>> {
        let callees = self.find_callees(file, symbol_name)?;
        let mut resolved = Vec::with_capacity(callees.len());

        for (callee_name, line) in callees {
            let source = self.resolve_import(file, &callee_name)?;
            resolved.push((callee_name, line, source));
        }

        Ok(resolved)
    }

    /// Find a symbol by name
    pub fn find_symbol(&self, name: &str) -> rusqlite::Result<Vec<(String, String, usize, usize)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT file, kind, start_line, end_line FROM symbols WHERE name = ?1")?;
        let symbols = stmt
            .query_map(params![name], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(symbols)
    }

    /// Get all distinct symbol names as a HashSet.
    pub fn all_symbol_names(&self) -> rusqlite::Result<std::collections::HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT name FROM symbols")?;
        let names = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<std::collections::HashSet<_>, _>>()?;
        Ok(names)
    }

    /// Get complexity stats for a file (avg, max)
    pub fn file_complexity(&self, file: &str) -> rusqlite::Result<(f64, usize)> {
        let mut stmt = self.conn.prepare(
            "SELECT AVG(complexity), MAX(complexity) FROM symbols WHERE file = ?1 AND complexity > 0",
        )?;
        let result = stmt.query_row(params![file], |row| {
            let avg: Option<f64> = row.get(0)?;
            let max: Option<usize> = row.get(1)?;
            Ok((avg.unwrap_or(0.0), max.unwrap_or(0)))
        })?;
        Ok(result)
    }

    /// Find symbols by name with fuzzy matching, optional kind filter, and limit
    pub fn find_symbols(
        &self,
        query: &str,
        kind: Option<&str>,
        fuzzy: bool,
        limit: usize,
    ) -> rusqlite::Result<Vec<SymbolMatch>> {
        let query_lower = query.to_lowercase();

        // Build SQL based on fuzzy/exact mode
        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::ToSql>>) = if fuzzy {
            let pattern = format!("%{}%", query_lower);
            let sql = if kind.is_some() {
                "SELECT name, kind, file, start_line, end_line, parent FROM symbols
                 WHERE LOWER(name) LIKE ?1 AND kind = ?2
                 ORDER BY
                   CASE WHEN LOWER(name) = ?3 THEN 0
                        WHEN LOWER(name) LIKE ?4 THEN 1
                        ELSE 2 END,
                   LENGTH(name), name
                 LIMIT ?5"
                    .to_string()
            } else {
                "SELECT name, kind, file, start_line, end_line, parent FROM symbols
                 WHERE LOWER(name) LIKE ?1
                 ORDER BY
                   CASE WHEN LOWER(name) = ?2 THEN 0
                        WHEN LOWER(name) LIKE ?3 THEN 1
                        ELSE 2 END,
                   LENGTH(name), name
                 LIMIT ?4"
                    .to_string()
            };

            if let Some(k) = kind {
                let prefix_pattern = format!("{}%", query_lower);
                (
                    sql,
                    vec![
                        Box::new(pattern) as Box<dyn rusqlite::ToSql>,
                        Box::new(k.to_string()),
                        Box::new(query_lower.clone()),
                        Box::new(prefix_pattern),
                        Box::new(limit as i64),
                    ],
                )
            } else {
                let prefix_pattern = format!("{}%", query_lower);
                (
                    sql,
                    vec![
                        Box::new(pattern) as Box<dyn rusqlite::ToSql>,
                        Box::new(query_lower.clone()),
                        Box::new(prefix_pattern),
                        Box::new(limit as i64),
                    ],
                )
            }
        } else {
            // Exact match
            let sql = if kind.is_some() {
                "SELECT name, kind, file, start_line, end_line, parent FROM symbols
                 WHERE LOWER(name) = LOWER(?1) AND kind = ?2
                 LIMIT ?3"
                    .to_string()
            } else {
                "SELECT name, kind, file, start_line, end_line, parent FROM symbols
                 WHERE LOWER(name) = LOWER(?1)
                 LIMIT ?2"
                    .to_string()
            };

            if let Some(k) = kind {
                (
                    sql,
                    vec![
                        Box::new(query.to_string()) as Box<dyn rusqlite::ToSql>,
                        Box::new(k.to_string()),
                        Box::new(limit as i64),
                    ],
                )
            } else {
                (
                    sql,
                    vec![
                        Box::new(query.to_string()) as Box<dyn rusqlite::ToSql>,
                        Box::new(limit as i64),
                    ],
                )
            }
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let symbols = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(SymbolMatch {
                    name: row.get(0)?,
                    kind: row.get(1)?,
                    file: row.get(2)?,
                    start_line: row.get(3)?,
                    end_line: row.get(4)?,
                    parent: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(symbols)
    }

    /// Get call graph stats
    pub fn call_graph_stats(&self) -> rusqlite::Result<CallGraphStats> {
        Ok(CallGraphStats {
            symbols: self
                .conn
                .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?,
            calls: self
                .conn
                .query_row("SELECT COUNT(*) FROM calls", [], |row| row.get(0))?,
            imports: self
                .conn
                .query_row("SELECT COUNT(*) FROM imports", [], |row| row.get(0))
                .unwrap_or(0),
        })
    }

    /// Convert a module name to possible file paths using the language's trait method.
    /// Returns only paths that exist in the index.
    fn module_to_files(&self, module: &str, source_file: &str) -> Vec<String> {
        // Get language from the source file extension
        let lang = match support_for_path(Path::new(source_file)) {
            Some(l) => l,
            None => return vec![],
        };

        // Get candidate paths from the language trait
        let candidates = lang.module_name_to_paths(module);

        // Filter to files that exist in index
        candidates
            .into_iter()
            .filter(|path| {
                self.conn
                    .query_row("SELECT 1 FROM files WHERE path = ?1", params![path], |_| {
                        Ok(())
                    })
                    .is_ok()
            })
            .collect()
    }

    /// Check if a file exports (defines) a given symbol
    fn file_exports_symbol(&self, file: &str, symbol: &str) -> rusqlite::Result<bool> {
        // Check if symbol is defined in this file (top-level only, parent IS NULL)
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols WHERE file = ?1 AND name = ?2 AND parent IS NULL",
            params![file, symbol],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Resolve a name in a file's context to its source module
    /// Returns: (source_module, original_name) if found
    pub fn resolve_import(
        &self,
        file: &str,
        name: &str,
    ) -> rusqlite::Result<Option<(String, String)>> {
        // Check for direct import or alias
        let mut stmt = self.conn.prepare(
            "SELECT module, name FROM imports WHERE file = ?1 AND (name = ?2 OR alias = ?2)",
        )?;
        let result: Option<(Option<String>, String)> = stmt
            .query_row(params![file, name], |row| Ok((row.get(0)?, row.get(1)?)))
            .ok();

        if let Some((module, orig_name)) = result {
            if let Some(module) = module {
                return Ok(Some((module, orig_name)));
            } else {
                // Plain import (import X), module is the name
                return Ok(Some((orig_name.clone(), orig_name)));
            }
        }

        // Check for wildcard imports - name could come from any of them
        let mut stmt = self
            .conn
            .prepare("SELECT module FROM imports WHERE file = ?1 AND name = '*'")?;
        let wildcards: Vec<String> = stmt
            .query_map(params![file], |row| row.get(0))?
            .filter_map(|r: Result<Option<String>, _>| r.ok().flatten())
            .collect();

        // Check each wildcard source to see if it exports the symbol
        for module in &wildcards {
            let files = self.module_to_files(module, file);
            for module_file in files {
                if self.file_exports_symbol(&module_file, name)? {
                    return Ok(Some((module.clone(), name.to_string())));
                }
            }
        }

        // Fallback: if we have wildcards but couldn't verify, return first as possibility
        // This handles external modules (stdlib, third-party) we can't resolve
        if !wildcards.is_empty() {
            return Ok(Some((wildcards[0].clone(), name.to_string())));
        }

        Ok(None)
    }

    /// Find which files import a given module
    pub fn find_importers(&self, module: &str) -> rusqlite::Result<Vec<(String, String, usize)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT file, name, line FROM imports WHERE module = ?1 OR module LIKE ?2")?;
        let pattern = format!("{}%", module);
        let importers = stmt
            .query_map(params![module, pattern], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(importers)
    }

    /// Refresh the call graph by parsing all supported source files
    /// This is more expensive than file refresh since it parses every file
    /// Uses parallel processing for parsing, sequential insertion for SQLite
    pub fn refresh_call_graph(&mut self) -> rusqlite::Result<CallGraphStats> {
        // Get all indexed source files BEFORE starting transaction
        let files: Vec<String> = {
            let sql = format!(
                "SELECT path FROM files WHERE is_dir = 0 AND ({})",
                source_extensions_sql_filter()
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let mut files = Vec::new();
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let path: String = row.get(0)?;
                files.push(path);
            }
            files
        };

        // Parse all files in parallel
        // Each thread gets its own SymbolParser (tree-sitter parsers have mutable state)
        let root = self.root.clone();
        let parsed_data: Vec<ParsedFileData> = files
            .par_iter()
            .filter_map(|file_path| {
                let full_path = root.join(file_path);
                let content = std::fs::read_to_string(&full_path).ok()?;

                // Each thread creates its own parser
                let mut parser = SymbolParser::new();
                let symbols = parser.parse_file(&full_path, &content);

                let mut sym_data = Vec::with_capacity(symbols.len());
                let mut call_data = Vec::new();

                for sym in &symbols {
                    sym_data.push((
                        sym.name.clone(),
                        sym.kind.as_str().to_string(),
                        sym.start_line,
                        sym.end_line,
                        sym.parent.clone(),
                        sym.complexity,
                    ));

                    // Only index calls for functions/methods
                    let kind = sym.kind.as_str();
                    if kind == "function" || kind == "method" {
                        let calls = parser.find_callees_for_symbol(&full_path, &content, sym);
                        for (callee_name, line, qualifier) in calls {
                            call_data.push((sym.name.clone(), callee_name, qualifier, line));
                        }
                    }
                }

                // Parse imports using trait-based extraction (works for all supported languages)
                let imports = parser.parse_imports(&full_path, &content);

                Some(ParsedFileData {
                    file_path: file_path.clone(),
                    symbols: sym_data,
                    calls: call_data,
                    imports,
                })
            })
            .collect();

        // Insert all data in a single transaction with prepared statements
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM symbols", [])?;
        tx.execute("DELETE FROM calls", [])?;
        tx.execute("DELETE FROM imports", [])?;

        let mut symbol_count = 0;
        let mut call_count = 0;
        let mut import_count = 0;

        // Pre-compile statements for batch insertion (much faster than tx.execute per row)
        {
            let mut sym_stmt = tx.prepare_cached(
                "INSERT INTO symbols (file, name, kind, start_line, end_line, parent, complexity) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
            )?;
            let mut call_stmt = tx.prepare_cached(
                "INSERT INTO calls (caller_file, caller_symbol, callee_name, callee_qualifier, line) VALUES (?1, ?2, ?3, ?4, ?5)"
            )?;
            let mut import_stmt = tx.prepare_cached(
                "INSERT INTO imports (file, module, name, alias, line) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;

            for data in &parsed_data {
                for (name, kind, start_line, end_line, parent, complexity) in &data.symbols {
                    sym_stmt.execute(params![
                        data.file_path,
                        name,
                        kind,
                        start_line,
                        end_line,
                        parent,
                        complexity
                    ])?;
                    symbol_count += 1;
                }

                for (caller_symbol, callee_name, qualifier, line) in &data.calls {
                    call_stmt.execute(params![
                        data.file_path,
                        caller_symbol,
                        callee_name,
                        qualifier,
                        line
                    ])?;
                    call_count += 1;
                }

                for imp in &data.imports {
                    import_stmt.execute(params![
                        data.file_path,
                        imp.module,
                        imp.name,
                        imp.alias,
                        imp.line
                    ])?;
                    import_count += 1;
                }
            }
        }

        tx.commit()?;
        Ok(CallGraphStats {
            symbols: symbol_count,
            calls: call_count,
            imports: import_count,
        })
    }

    /// Incrementally update call graph for changed files only
    /// Much faster than full refresh when few files changed
    pub fn incremental_call_graph_refresh(&mut self) -> rusqlite::Result<CallGraphStats> {
        let changed = self.get_changed_files()?;

        // Only process supported source and data files
        let changed_files: Vec<String> = changed
            .added
            .into_iter()
            .chain(changed.modified.into_iter())
            .filter(|f| is_source_file(f))
            .collect();

        let deleted_source_files: Vec<String> = changed
            .deleted
            .into_iter()
            .filter(|f| is_source_file(f))
            .collect();

        if changed_files.is_empty() && deleted_source_files.is_empty() {
            return Ok(CallGraphStats::default());
        }

        let tx = self.conn.transaction()?;

        // Remove data for deleted/modified files
        for path in deleted_source_files.iter().chain(changed_files.iter()) {
            tx.execute("DELETE FROM symbols WHERE file = ?1", params![path])?;
            tx.execute("DELETE FROM calls WHERE caller_file = ?1", params![path])?;
            tx.execute("DELETE FROM imports WHERE file = ?1", params![path])?;
        }

        let mut parser = SymbolParser::new();
        let mut symbol_count = 0;
        let mut call_count = 0;
        let mut import_count = 0;

        // Parse changed files
        for file_path in &changed_files {
            let full_path = self.root.join(file_path);
            let content = match std::fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let symbols = parser.parse_file(&full_path, &content);

            for sym in &symbols {
                tx.execute(
                    "INSERT INTO symbols (file, name, kind, start_line, end_line, parent, complexity) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![file_path, sym.name, sym.kind.as_str(), sym.start_line, sym.end_line, sym.parent, sym.complexity],
                )?;
                symbol_count += 1;

                let kind = sym.kind.as_str();
                if kind == "function" || kind == "method" {
                    let calls = parser.find_callees_for_symbol(&full_path, &content, sym);
                    for (callee_name, line, qualifier) in calls {
                        tx.execute(
                            "INSERT INTO calls (caller_file, caller_symbol, callee_name, callee_qualifier, line) VALUES (?1, ?2, ?3, ?4, ?5)",
                            params![file_path, sym.name, callee_name, qualifier, line],
                        )?;
                        call_count += 1;
                    }
                }
            }

            // Parse imports using trait-based extraction (works for all supported languages)
            let imports = parser.parse_imports(&full_path, &content);
            for imp in imports {
                tx.execute(
                    "INSERT INTO imports (file, module, name, alias, line) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![file_path, imp.module, imp.name, imp.alias, imp.line],
                )?;
                import_count += 1;
            }
        }

        tx.commit()?;
        Ok(CallGraphStats {
            symbols: symbol_count,
            calls: call_count,
            imports: import_count,
        })
    }

    /// Check if call graph needs refresh
    #[allow(dead_code)] // FileIndex API - used by daemon
    pub fn needs_call_graph_refresh(&self) -> bool {
        self.call_graph_stats().unwrap_or_default().symbols == 0
    }

    /// Find files matching a query using LIKE (fast pre-filter)
    /// Splits query by whitespace/separators and requires all parts to match
    /// Special case: queries starting with '.' are treated as extension patterns
    pub fn find_like(&self, query: &str) -> rusqlite::Result<Vec<IndexedFile>> {
        // Handle extension patterns (e.g., ".rs", ".py")
        if query.starts_with('.') && !query.contains('/') {
            let sql =
                "SELECT path, is_dir, mtime, lines FROM files WHERE LOWER(path) LIKE ?1 LIMIT 1000";
            let pattern = format!("%{}", query.to_lowercase());
            let mut stmt = self.conn.prepare(sql)?;
            let files = stmt
                .query_map([pattern], |row| {
                    Ok(IndexedFile {
                        path: row.get(0)?,
                        is_dir: row.get::<_, i64>(1)? != 0,
                        mtime: row.get(2)?,
                        lines: row.get::<_, i64>(3)? as usize,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            return Ok(files);
        }

        // Normalize query: split on whitespace and common separators (but not '.')
        let parts: Vec<&str> = query
            .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
            .filter(|s| !s.is_empty())
            .collect();

        if parts.is_empty() {
            return Ok(Vec::new());
        }

        // Build WHERE clause: LOWER(path) LIKE '%part1%' AND LOWER(path) LIKE '%part2%' ...
        let conditions: Vec<String> = (0..parts.len())
            .map(|i| format!("LOWER(path) LIKE ?{}", i + 1))
            .collect();
        let sql = format!(
            "SELECT path, is_dir, mtime, lines FROM files WHERE {} LIMIT 50",
            conditions.join(" AND ")
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let patterns: Vec<String> = parts
            .iter()
            .map(|p| format!("%{}%", p.to_lowercase()))
            .collect();

        // Bind all parameters
        let params: Vec<&dyn rusqlite::ToSql> =
            patterns.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let files = stmt
            .query_map(params.as_slice(), |row| {
                Ok(IndexedFile {
                    path: row.get(0)?,
                    is_dir: row.get::<_, i64>(1)? != 0,
                    mtime: row.get(2)?,
                    lines: row.get::<_, i64>(3)? as usize,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(files)
    }

    // =========================================================================
    // Cross-language reference tracking
    // =========================================================================

    /// Refresh cross-language references by detecting FFI patterns.
    /// Uses trait-based FfiDetector from moss-languages for extensibility.
    pub fn refresh_cross_refs(&mut self) -> rusqlite::Result<usize> {
        use moss_languages::ffi::{FfiDetector, FfiModule as LangFfiModule};

        let detector = FfiDetector::new();

        // Collect all cross-refs before starting transaction
        let mut cross_refs: Vec<CrossRefData> = Vec::new();

        // 1. Find build files and detect FFI modules
        let build_files: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT path FROM files WHERE path LIKE '%Cargo.toml' OR path LIKE '%pyproject.toml'")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };

        let mut ffi_modules: Vec<LangFfiModule> = Vec::new();
        for build_path in build_files {
            let full_path = self.root.join(&build_path);
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                ffi_modules.extend(detector.detect_modules(&full_path, &content));
            }
        }

        // 2. Match imports to FFI modules
        for module in &ffi_modules {
            let module_name = module.name.replace('-', "_");
            let ref_type = format!("{}_import", module.binding_type);

            // Find imports matching this module
            let imports: Vec<(String, Option<String>, String, usize)> = {
                let mut stmt = self.conn.prepare(
                    "SELECT file, module, name, line FROM imports WHERE module = ?1 OR module LIKE ?2 OR name = ?1",
                )?;
                let pattern = format!("{}%", module_name);
                let rows = stmt.query_map(params![module_name, pattern], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?;
                rows.filter_map(|r| r.ok()).collect()
            };

            for (file, import_module, import_name, line) in imports {
                // Check file extension matches binding's consumer extensions
                let ext = file.rsplit('.').next().unwrap_or("");
                if !detector.is_consumer_extension(ext) {
                    continue;
                }

                // Determine source language from extension
                let source_lang = match ext {
                    "py" => "python",
                    "js" | "mjs" | "ts" | "tsx" => "javascript",
                    _ => continue,
                };

                // Verify import matches using binding's logic
                let import_mod = import_module.as_deref().unwrap_or("");
                if detector
                    .match_import(import_mod, &import_name, &ffi_modules)
                    .is_some()
                {
                    cross_refs.push(CrossRefData {
                        source_file: file,
                        source_lang,
                        target_module: module.name.clone(),
                        target_lang: module.target_lang,
                        ref_type: ref_type.clone(),
                        line,
                    });
                }
            }
        }

        // 3. Detect standalone FFI usage (ctypes/cffi without matching module)
        let ffi_imports = self.detect_standalone_ffi_imports(&detector)?;
        cross_refs.extend(ffi_imports);

        // Insert all cross-refs in a transaction
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM cross_refs", [])?;

        for cr in &cross_refs {
            tx.execute(
                "INSERT INTO cross_refs (source_file, source_lang, target_crate, target_lang, ref_type, line)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![cr.source_file, cr.source_lang, cr.target_module, cr.target_lang, cr.ref_type, cr.line],
            )?;
        }

        tx.commit()?;
        Ok(cross_refs.len())
    }

    /// Detect standalone FFI imports (ctypes/cffi) that don't match a known module.
    fn detect_standalone_ffi_imports(
        &self,
        detector: &moss_languages::ffi::FfiDetector,
    ) -> rusqlite::Result<Vec<CrossRefData>> {
        let mut results = Vec::new();

        // Get all imports once
        let imports: Vec<(String, Option<String>, String, usize)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT file, module, name, line FROM imports")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?;
            rows.filter_map(|r| r.ok()).collect()
        };

        // Check each binding for standalone detection
        for binding in detector.bindings() {
            // Only ctypes/cffi have standalone detection (no build file)
            if binding.name() != "ctypes" && binding.name() != "cffi" {
                continue;
            }

            for (file, import_module, import_name, line) in &imports {
                let ext = file.rsplit('.').next().unwrap_or("");
                if !binding.consumer_extensions().contains(&ext) {
                    continue;
                }

                let import_mod = import_module.as_deref().unwrap_or("");
                if binding.matches_import(import_mod, import_name, "") {
                    results.push(CrossRefData {
                        source_file: file.clone(),
                        source_lang: binding.source_lang(),
                        target_module: "native_lib".to_string(),
                        target_lang: binding.target_lang(),
                        ref_type: format!("{}_usage", binding.name()),
                        line: *line,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Find cross-language references from a source file.
    pub fn find_cross_refs(&self, file: &str) -> rusqlite::Result<Vec<CrossRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_file, source_lang, target_crate, target_lang, ref_type, line
             FROM cross_refs WHERE source_file = ?1",
        )?;
        let refs = stmt
            .query_map(params![file], |row| {
                Ok(CrossRef {
                    source_file: row.get(0)?,
                    source_lang: row.get(1)?,
                    target_crate: row.get(2)?,
                    target_lang: row.get(3)?,
                    ref_type: row.get(4)?,
                    line: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(refs)
    }

    /// Find files that reference a given crate/module across languages.
    pub fn find_cross_ref_sources(&self, target: &str) -> rusqlite::Result<Vec<CrossRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_file, source_lang, target_crate, target_lang, ref_type, line
             FROM cross_refs WHERE target_crate = ?1",
        )?;
        let refs = stmt
            .query_map(params![target], |row| {
                Ok(CrossRef {
                    source_file: row.get(0)?,
                    source_lang: row.get(1)?,
                    target_crate: row.get(2)?,
                    target_lang: row.get(3)?,
                    ref_type: row.get(4)?,
                    line: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(refs)
    }

    /// Get all cross-language references in the index.
    pub fn all_cross_refs(&self) -> rusqlite::Result<Vec<CrossRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_file, source_lang, target_crate, target_lang, ref_type, line
             FROM cross_refs ORDER BY source_file, line",
        )?;
        let refs = stmt
            .query_map([], |row| {
                Ok(CrossRef {
                    source_file: row.get(0)?,
                    source_lang: row.get(1)?,
                    target_crate: row.get(2)?,
                    target_lang: row.get(3)?,
                    ref_type: row.get(4)?,
                    line: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(refs)
    }
}

/// Internal cross-ref data for collection before insertion.
struct CrossRefData {
    source_file: String,
    source_lang: &'static str,
    target_module: String,
    target_lang: &'static str,
    ref_type: String,
    line: usize,
}

/// Cross-language reference record.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CrossRef {
    pub source_file: String,
    pub source_lang: String,
    pub target_crate: String,
    pub target_lang: String,
    pub ref_type: String,
    pub line: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_index_creation() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/cli.py"), "").unwrap();
        fs::write(dir.path().join("src/moss/dwim.py"), "").unwrap();

        let mut index = FileIndex::open(dir.path()).unwrap();
        assert!(index.needs_refresh());

        let count = index.refresh().unwrap();
        assert!(count >= 2);

        // Should find files by name
        let matches = index.find_by_name("cli.py").unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].path.ends_with("cli.py"));
    }

    #[test]
    fn test_find_by_stem() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/test.py"), "").unwrap();
        fs::write(dir.path().join("src/test.rs"), "").unwrap();

        let mut index = FileIndex::open(dir.path()).unwrap();
        index.refresh().unwrap();

        let matches = index.find_by_stem("test").unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_wildcard_import_resolution() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/mylib")).unwrap();
        // Module that exports MyClass
        fs::write(
            dir.path().join("src/mylib/exports.py"),
            "class MyClass: pass",
        )
        .unwrap();
        // Module that exports OtherThing
        fs::write(
            dir.path().join("src/mylib/other.py"),
            "def OtherThing(): pass",
        )
        .unwrap();
        // Consumer with wildcard imports
        fs::write(
            dir.path().join("src/consumer.py"),
            "from mylib.exports import *\nfrom mylib.other import *\nMyClass()",
        )
        .unwrap();

        let mut index = FileIndex::open(dir.path()).unwrap();
        index.refresh().unwrap();
        index.refresh_call_graph().unwrap();

        // Manually add wildcard imports (refresh_call_graph parses these)
        // The parser should have picked up the wildcard imports

        // Now resolve MyClass - should find it in mylib.exports
        let result = index.resolve_import("src/consumer.py", "MyClass").unwrap();
        assert!(result.is_some(), "Should resolve MyClass");
        let (module, name) = result.unwrap();
        assert_eq!(module, "mylib.exports");
        assert_eq!(name, "MyClass");

        // Resolve OtherThing - should find it in mylib.other
        let result = index
            .resolve_import("src/consumer.py", "OtherThing")
            .unwrap();
        assert!(result.is_some(), "Should resolve OtherThing");
        let (module, name) = result.unwrap();
        assert_eq!(module, "mylib.other");
        assert_eq!(name, "OtherThing");
    }

    #[test]
    fn test_method_call_resolution() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        // A class with methods that call each other
        let class_code = r#"
class MyClass:
    def method_a(self):
        self.method_b()

    def method_b(self):
        pass

    def method_c(self):
        self.method_b()
"#;
        fs::write(dir.path().join("src/myclass.py"), class_code).unwrap();

        let mut index = FileIndex::open(dir.path()).unwrap();
        index.refresh().unwrap();
        index.refresh_call_graph().unwrap();

        // Find callers of method_b - should include method_a and method_c
        let callers = index.find_callers("method_b").unwrap();
        assert!(!callers.is_empty(), "Should find callers of method_b");

        let caller_names: Vec<&str> = callers.iter().map(|(_, name, _)| name.as_str()).collect();
        assert!(
            caller_names.contains(&"method_a"),
            "method_a should call method_b"
        );
        assert!(
            caller_names.contains(&"method_c"),
            "method_c should call method_b"
        );

        // Find callers of MyClass.method_b - more specific
        let callers = index.find_callers("MyClass.method_b").unwrap();
        assert!(
            !callers.is_empty(),
            "Should find callers of MyClass.method_b"
        );
    }
}
