use crate::config::MossConfig;
use crate::paths::get_moss_dir;
use ignore::WalkBuilder;
use libsql::{Connection, Database, params};
use rayon::prelude::*;
use rhizome_moss_languages::support_for_path;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::symbols::{FlatImport, FlatSymbol, SymbolParser};

/// Parsed data for a single file, ready for database insertion
struct ParsedFileData {
    file_path: String,
    /// (name, kind, start_line, end_line, parent)
    symbols: Vec<(String, String, usize, usize, Option<String>)>,
    /// (caller_symbol, callee_name, callee_qualifier, line)
    calls: Vec<(String, String, Option<String>, usize)>,
    /// imports (for Python files only)
    imports: Vec<FlatImport>,
    /// (type_name, method_name) for interface/class method signatures
    type_methods: Vec<(String, String)>,
}

// Not yet public - just delete .moss/index.sqlite on schema changes
const SCHEMA_VERSION: i64 = 1;

/// Check if a file path has a supported source extension.
fn is_source_file(path: &str) -> bool {
    rhizome_moss_languages::support_for_path(std::path::Path::new(path)).is_some()
}

/// Generate SQL WHERE clause for filtering source files.
/// Returns: "path LIKE '%.py' OR path LIKE '%.rs' OR ..."
fn source_extensions_sql_filter() -> String {
    let mut extensions: Vec<&str> = rhizome_moss_languages::supported_languages()
        .iter()
        .flat_map(|lang| lang.extensions().iter().copied())
        .collect();
    extensions.sort_unstable();
    extensions.dedup();
    extensions
        .iter()
        .map(|ext| format!("path LIKE '%.{}'", ext))
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
    #[allow(dead_code)]
    db: Database,
    root: PathBuf,
}

impl FileIndex {
    /// Open or create an index for a directory.
    /// Index is stored in .moss/index.sqlite (or MOSS_INDEX_DIR if set)
    /// On corruption, automatically deletes and recreates the index.
    pub async fn open(root: &Path) -> Result<Self, libsql::Error> {
        let moss_dir = get_moss_dir(root);
        std::fs::create_dir_all(&moss_dir).ok();

        let db_path = moss_dir.join("index.sqlite");

        // Try to open, with recovery on corruption
        match Self::try_open(&db_path, root).await {
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
                    Self::try_open(&db_path, root).await
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Open index only if indexing is enabled in config.
    /// Returns None if `[index] enabled = false`.
    pub async fn open_if_enabled(root: &Path) -> Option<Self> {
        let config = MossConfig::load(root);
        if !config.index.enabled() {
            return None;
        }
        Self::open(root).await.ok()
    }

    /// Internal: try to open database without recovery
    async fn try_open(db_path: &Path, root: &Path) -> Result<Self, libsql::Error> {
        let db = libsql::Builder::new_local(db_path).build().await?;
        let conn = db.connect()?;

        // Quick integrity check - this will catch most corruption
        // PRAGMA quick_check is faster than full integrity_check
        let mut rows = conn.query("PRAGMA quick_check(1)", ()).await?;
        let integrity: String = if let Some(row) = rows.next().await? {
            row.get(0).unwrap_or_else(|_| "error".to_string())
        } else {
            "error".to_string()
        };
        if integrity != "ok" {
            return Err(libsql::Error::SqliteFailure(
                11, // SQLITE_CORRUPT
                format!("Database integrity check failed: {}", integrity),
            ));
        }

        // Initialize schema
        conn.execute(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT
            )",
            (),
        )
        .await?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                is_dir INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                lines INTEGER NOT NULL DEFAULT 0
            )",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_name ON files(path)",
            (),
        )
        .await?;

        // Call graph for fast caller/callee lookups
        conn.execute(
            "CREATE TABLE IF NOT EXISTS calls (
                caller_file TEXT NOT NULL,
                caller_symbol TEXT NOT NULL,
                callee_name TEXT NOT NULL,
                callee_qualifier TEXT,
                line INTEGER NOT NULL
            )",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_calls_callee ON calls(callee_name)",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_calls_caller ON calls(caller_file, caller_symbol)",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_calls_qualifier ON calls(callee_qualifier)",
            (),
        )
        .await?;

        // Symbol definitions
        conn.execute(
            "CREATE TABLE IF NOT EXISTS symbols (
                file TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                parent TEXT
            )",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name)",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file)",
            (),
        )
        .await?;

        // Import tracking
        conn.execute(
            "CREATE TABLE IF NOT EXISTS imports (
                file TEXT NOT NULL,
                module TEXT,
                name TEXT NOT NULL,
                alias TEXT,
                line INTEGER NOT NULL
            )",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_imports_file ON imports(file)",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_imports_name ON imports(name)",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_imports_module ON imports(module)",
            (),
        )
        .await?;

        // Type method signatures
        conn.execute(
            "CREATE TABLE IF NOT EXISTS type_methods (
                file TEXT NOT NULL,
                type_name TEXT NOT NULL,
                method_name TEXT NOT NULL,
                PRIMARY KEY (file, type_name, method_name)
            )",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_type_methods_type ON type_methods(type_name)",
            (),
        )
        .await?;

        // Check schema version
        let mut rows = conn
            .query(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'schema_version'",
                (),
            )
            .await?;
        let version: i64 = if let Some(row) = rows.next().await? {
            row.get(0).unwrap_or(0)
        } else {
            0
        };

        if version != SCHEMA_VERSION {
            // Reset on schema change
            conn.execute("DELETE FROM files", ()).await?;
            conn.execute("DELETE FROM calls", ()).await.ok();
            conn.execute("DELETE FROM symbols", ()).await.ok();
            conn.execute("DELETE FROM imports", ()).await.ok();
            conn.execute("DELETE FROM type_methods", ()).await.ok();
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
                params![SCHEMA_VERSION.to_string()],
            )
            .await?;
        }

        Ok(Self {
            conn,
            db,
            root: root.to_path_buf(),
        })
    }

    /// Get a reference to the underlying SQLite connection for direct queries
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Get files that have changed since last index
    pub async fn get_changed_files(&self) -> Result<ChangedFiles, libsql::Error> {
        let mut result = ChangedFiles::default();

        // Get all indexed files with their mtimes
        let mut indexed: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        {
            let mut rows = self
                .conn
                .query("SELECT path, mtime FROM files WHERE is_dir = 0", ())
                .await?;
            while let Some(row) = rows.next().await? {
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
    async fn needs_refresh(&self) -> bool {
        let mut rows = match self
            .conn
            .query(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'last_indexed'",
                (),
            )
            .await
        {
            Ok(r) => r,
            Err(_) => return true,
        };
        let last_indexed: i64 = match rows.next().await {
            Ok(Some(row)) => row.get(0).unwrap_or(0),
            _ => 0,
        };

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
                if let Ok(meta) = entry.metadata()
                    && let Ok(mtime) = meta.modified()
                {
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

        // Sample some indexed files to catch modifications
        // Check ~100 files spread across the index
        if let Ok(mut rows) = self
            .conn
            .query(
                "SELECT path, mtime FROM files WHERE is_dir = 0 ORDER BY RANDOM() LIMIT 100",
                (),
            )
            .await
        {
            while let Ok(Some(row)) = rows.next().await {
                let path: String = match row.get(0) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let indexed_mtime: i64 = match row.get(1) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let full_path = self.root.join(&path);
                if let Ok(meta) = full_path.metadata()
                    && let Ok(mtime) = meta.modified()
                {
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

        false
    }

    /// Refresh only files that have changed (faster than full refresh)
    /// Returns number of files updated
    pub async fn incremental_refresh(&mut self) -> Result<usize, libsql::Error> {
        if !self.needs_refresh().await {
            return Ok(0);
        }

        let changed = self.get_changed_files().await?;
        let total_changes = changed.added.len() + changed.modified.len() + changed.deleted.len();

        if total_changes == 0 {
            return Ok(0);
        }

        // Delete removed files
        for path in &changed.deleted {
            self.conn
                .execute("DELETE FROM files WHERE path = ?1", params![path.clone()])
                .await?;
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

            self.conn.execute(
                "INSERT OR REPLACE INTO files (path, is_dir, mtime, lines) VALUES (?1, ?2, ?3, ?4)",
                params![path.clone(), is_dir as i64, mtime, lines as i64],
            ).await?;
        }

        // Update last indexed time
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.conn
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_indexed', ?1)",
                params![now.to_string()],
            )
            .await?;

        Ok(total_changes)
    }

    /// Execute a raw SQL statement (for maintenance operations).
    pub async fn execute(&self, sql: &str) -> Result<u64, libsql::Error> {
        self.conn.execute(sql, ()).await
    }

    /// Refresh the index by walking the filesystem
    pub async fn refresh(&mut self) -> Result<usize, libsql::Error> {
        let walker = WalkBuilder::new(&self.root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        // Clear existing files
        self.conn.execute("DELETE FROM files", ()).await?;

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

                self.conn
                    .execute(
                        "INSERT INTO files (path, is_dir, mtime, lines) VALUES (?1, ?2, ?3, ?4)",
                        params![rel_str, is_dir as i64, mtime, lines as i64],
                    )
                    .await?;
                count += 1;
            }
        }

        // Update last indexed time
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.conn
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_indexed', ?1)",
                params![now.to_string()],
            )
            .await?;

        Ok(count)
    }

    /// Get all files from the index
    pub async fn all_files(&self) -> Result<Vec<IndexedFile>, libsql::Error> {
        let mut rows = self
            .conn
            .query("SELECT path, is_dir, mtime, lines FROM files", ())
            .await?;
        let mut files = Vec::new();
        while let Some(row) = rows.next().await? {
            files.push(IndexedFile {
                path: row.get(0)?,
                is_dir: row.get::<i64>(1)? != 0,
                mtime: row.get(2)?,
                lines: row.get::<i64>(3)? as usize,
            });
        }
        Ok(files)
    }

    /// Search files by exact name match
    pub async fn find_by_name(&self, name: &str) -> Result<Vec<IndexedFile>, libsql::Error> {
        let pattern = format!("%/{}", name);
        let mut rows = self
            .conn
            .query(
                "SELECT path, is_dir, mtime, lines FROM files WHERE path LIKE ?1 OR path = ?2",
                params![pattern, name],
            )
            .await?;
        let mut files = Vec::new();
        while let Some(row) = rows.next().await? {
            files.push(IndexedFile {
                path: row.get(0)?,
                is_dir: row.get::<i64>(1)? != 0,
                mtime: row.get(2)?,
                lines: row.get::<i64>(3)? as usize,
            });
        }
        Ok(files)
    }

    /// Search files by stem (filename without extension)
    pub async fn find_by_stem(&self, stem: &str) -> Result<Vec<IndexedFile>, libsql::Error> {
        let pattern = format!("%/{}%", stem);
        let mut rows = self
            .conn
            .query(
                "SELECT path, is_dir, mtime, lines FROM files WHERE path LIKE ?1",
                params![pattern],
            )
            .await?;
        let mut files = Vec::new();
        while let Some(row) = rows.next().await? {
            files.push(IndexedFile {
                path: row.get(0)?,
                is_dir: row.get::<i64>(1)? != 0,
                mtime: row.get(2)?,
                lines: row.get::<i64>(3)? as usize,
            });
        }
        Ok(files)
    }

    /// Count indexed files
    pub async fn count(&self) -> Result<usize, libsql::Error> {
        let mut rows = self.conn.query("SELECT COUNT(*) FROM files", ()).await?;
        if let Some(row) = rows.next().await? {
            Ok(row.get::<i64>(0)? as usize)
        } else {
            Ok(0)
        }
    }

    /// Index symbols and call graph for a file
    #[allow(dead_code)] // FileIndex API - used by daemon
    pub async fn index_file_symbols(
        &self,
        path: &str,
        symbols: &[FlatSymbol],
        calls: &[(String, String, usize)],
    ) -> Result<(), libsql::Error> {
        // Insert symbols
        for sym in symbols {
            self.conn.execute(
                "INSERT INTO symbols (file, name, kind, start_line, end_line, parent) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![path.to_string(), sym.name.clone(), sym.kind.as_str(), sym.start_line as i64, sym.end_line as i64, sym.parent.clone()],
            ).await?;
        }

        // Insert calls (caller_symbol, callee_name, line)
        for (caller_symbol, callee_name, line) in calls {
            self.conn.execute(
                "INSERT INTO calls (caller_file, caller_symbol, callee_name, line) VALUES (?1, ?2, ?3, ?4)",
                params![path.to_string(), caller_symbol.clone(), callee_name.clone(), *line as i64],
            ).await?;
        }

        Ok(())
    }

    /// Find callers of a symbol by name (from call graph)
    /// Resolves through imports: if file A imports X as Y and calls Y(), finds that as a caller of X
    /// Also handles qualified calls: if file A does `import foo` and calls `foo.bar()`, finds caller of `bar`
    /// Also handles method calls: `self.method()` is resolved to the containing class's method
    pub async fn find_callers(
        &self,
        symbol_name: &str,
    ) -> Result<Vec<(String, String, usize)>, libsql::Error> {
        // Handle Class.method format - split and search for method within class
        let (class_filter, method_name) = if symbol_name.contains('.') {
            let parts: Vec<&str> = symbol_name.splitn(2, '.').collect();
            (Some(parts[0]), parts[1])
        } else {
            (None, symbol_name)
        };

        // If searching for Class.method, find callers that call self.method within that class
        if let Some(class_name) = class_filter {
            let mut rows = self
                .conn
                .query(
                    "SELECT c.caller_file, c.caller_symbol, c.line
                 FROM calls c
                 JOIN symbols s ON c.caller_file = s.file AND c.caller_symbol = s.name
                 WHERE c.callee_name = ?1 AND c.callee_qualifier = 'self' AND s.parent = ?2",
                    params![method_name, class_name],
                )
                .await?;
            let mut callers = Vec::new();
            while let Some(row) = rows.next().await? {
                callers.push((row.get(0)?, row.get(1)?, row.get::<i64>(2)? as usize));
            }

            if !callers.is_empty() {
                return Ok(callers);
            }
        }

        // Combined query: direct calls + calls via import aliases + qualified calls via module imports
        let mut rows = self.conn.query(
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
             WHERE c.callee_name = ?1 AND c.callee_qualifier = 'self' AND s.parent IS NOT NULL",
            params![method_name],
        ).await?;
        let mut callers = Vec::new();
        while let Some(row) = rows.next().await? {
            callers.push((row.get(0)?, row.get(1)?, row.get::<i64>(2)? as usize));
        }

        if !callers.is_empty() {
            return Ok(callers);
        }

        // Try case-insensitive match (direct only for simplicity)
        let mut rows = self.conn.query(
            "SELECT caller_file, caller_symbol, line FROM calls WHERE LOWER(callee_name) = LOWER(?1)",
            params![method_name],
        ).await?;
        let mut callers = Vec::new();
        while let Some(row) = rows.next().await? {
            callers.push((row.get(0)?, row.get(1)?, row.get::<i64>(2)? as usize));
        }

        if !callers.is_empty() {
            return Ok(callers);
        }

        // Try LIKE pattern match (contains)
        let pattern = format!("%{}%", method_name);
        let mut rows = self.conn.query(
            "SELECT caller_file, caller_symbol, line FROM calls WHERE LOWER(callee_name) LIKE LOWER(?1) LIMIT 100",
            params![pattern],
        ).await?;
        let mut callers = Vec::new();
        while let Some(row) = rows.next().await? {
            callers.push((row.get(0)?, row.get(1)?, row.get::<i64>(2)? as usize));
        }

        Ok(callers)
    }

    /// Find callees of a symbol (what it calls)
    pub async fn find_callees(
        &self,
        file: &str,
        symbol_name: &str,
    ) -> Result<Vec<(String, usize)>, libsql::Error> {
        let mut rows = self
            .conn
            .query(
                "SELECT callee_name, line FROM calls WHERE caller_file = ?1 AND caller_symbol = ?2",
                params![file, symbol_name],
            )
            .await?;
        let mut callees = Vec::new();
        while let Some(row) = rows.next().await? {
            callees.push((row.get(0)?, row.get::<i64>(1)? as usize));
        }
        Ok(callees)
    }

    /// Find callees with resolved import info (name, line, source_module)
    /// Returns: (local_name, line, Option<(source_module, original_name)>)
    pub async fn find_callees_resolved(
        &self,
        file: &str,
        symbol_name: &str,
    ) -> Result<Vec<(String, usize, Option<(String, String)>)>, libsql::Error> {
        let callees = self.find_callees(file, symbol_name).await?;
        let mut resolved = Vec::with_capacity(callees.len());

        for (callee_name, line) in callees {
            let source = self.resolve_import(file, &callee_name).await?;
            resolved.push((callee_name, line, source));
        }

        Ok(resolved)
    }

    /// Find a symbol by name
    pub async fn find_symbol(
        &self,
        name: &str,
    ) -> Result<Vec<(String, String, usize, usize)>, libsql::Error> {
        let mut rows = self
            .conn
            .query(
                "SELECT file, kind, start_line, end_line FROM symbols WHERE name = ?1",
                params![name],
            )
            .await?;
        let mut symbols = Vec::new();
        while let Some(row) = rows.next().await? {
            symbols.push((
                row.get(0)?,
                row.get(1)?,
                row.get::<i64>(2)? as usize,
                row.get::<i64>(3)? as usize,
            ));
        }
        Ok(symbols)
    }

    /// Get all distinct symbol names as a HashSet.
    pub async fn all_symbol_names(
        &self,
    ) -> Result<std::collections::HashSet<String>, libsql::Error> {
        let mut rows = self
            .conn
            .query("SELECT DISTINCT name FROM symbols", ())
            .await?;
        let mut names = std::collections::HashSet::new();
        while let Some(row) = rows.next().await? {
            names.insert(row.get(0)?);
        }
        Ok(names)
    }

    /// Find symbols by name with fuzzy matching, optional kind filter, and limit
    pub async fn find_symbols(
        &self,
        query: &str,
        kind: Option<&str>,
        fuzzy: bool,
        limit: usize,
    ) -> Result<Vec<SymbolMatch>, libsql::Error> {
        let query_lower = query.to_lowercase();
        let prefix_pattern = format!("{}%", query_lower);
        let limit_i64 = limit as i64;

        let mut symbols = Vec::new();

        if fuzzy {
            let pattern = format!("%{}%", query_lower);
            let mut rows = if let Some(k) = kind {
                self.conn
                    .query(
                        "SELECT name, kind, file, start_line, end_line, parent FROM symbols
                     WHERE LOWER(name) LIKE ?1 AND kind = ?2
                     ORDER BY
                       CASE WHEN LOWER(name) = ?3 THEN 0
                            WHEN LOWER(name) LIKE ?4 THEN 1
                            ELSE 2 END,
                       LENGTH(name), name
                     LIMIT ?5",
                        params![pattern, k, query_lower, prefix_pattern, limit_i64],
                    )
                    .await?
            } else {
                self.conn
                    .query(
                        "SELECT name, kind, file, start_line, end_line, parent FROM symbols
                     WHERE LOWER(name) LIKE ?1
                     ORDER BY
                       CASE WHEN LOWER(name) = ?2 THEN 0
                            WHEN LOWER(name) LIKE ?3 THEN 1
                            ELSE 2 END,
                       LENGTH(name), name
                     LIMIT ?4",
                        params![pattern, query_lower, prefix_pattern, limit_i64],
                    )
                    .await?
            };

            while let Some(row) = rows.next().await? {
                symbols.push(SymbolMatch {
                    name: row.get(0)?,
                    kind: row.get(1)?,
                    file: row.get(2)?,
                    start_line: row.get::<i64>(3)? as usize,
                    end_line: row.get::<i64>(4)? as usize,
                    parent: row.get(5)?,
                });
            }
        } else {
            // Exact match
            let mut rows = if let Some(k) = kind {
                self.conn
                    .query(
                        "SELECT name, kind, file, start_line, end_line, parent FROM symbols
                     WHERE LOWER(name) = LOWER(?1) AND kind = ?2
                     LIMIT ?3",
                        params![query, k, limit_i64],
                    )
                    .await?
            } else {
                self.conn
                    .query(
                        "SELECT name, kind, file, start_line, end_line, parent FROM symbols
                     WHERE LOWER(name) = LOWER(?1)
                     LIMIT ?2",
                        params![query, limit_i64],
                    )
                    .await?
            };

            while let Some(row) = rows.next().await? {
                symbols.push(SymbolMatch {
                    name: row.get(0)?,
                    kind: row.get(1)?,
                    file: row.get(2)?,
                    start_line: row.get::<i64>(3)? as usize,
                    end_line: row.get::<i64>(4)? as usize,
                    parent: row.get(5)?,
                });
            }
        }

        Ok(symbols)
    }

    /// Get call graph stats
    pub async fn call_graph_stats(&self) -> Result<CallGraphStats, libsql::Error> {
        let symbols = {
            let mut rows = self.conn.query("SELECT COUNT(*) FROM symbols", ()).await?;
            if let Some(row) = rows.next().await? {
                row.get::<i64>(0)? as usize
            } else {
                0
            }
        };
        let calls = {
            let mut rows = self.conn.query("SELECT COUNT(*) FROM calls", ()).await?;
            if let Some(row) = rows.next().await? {
                row.get::<i64>(0)? as usize
            } else {
                0
            }
        };
        let imports = {
            let mut rows = self.conn.query("SELECT COUNT(*) FROM imports", ()).await?;
            if let Some(row) = rows.next().await? {
                row.get::<i64>(0).unwrap_or(0) as usize
            } else {
                0
            }
        };
        Ok(CallGraphStats {
            symbols,
            calls,
            imports,
        })
    }

    /// Convert a module name to possible file paths using the language's trait method.
    /// Returns only paths that exist in the index.
    async fn module_to_files(&self, module: &str, source_file: &str) -> Vec<String> {
        // Get language from the source file extension
        let lang = match support_for_path(Path::new(source_file)) {
            Some(l) => l,
            None => return vec![],
        };

        // Get candidate paths from the language trait
        let candidates = lang.module_name_to_paths(module);

        // Filter to files that exist in index
        let mut result = Vec::new();
        for path in candidates {
            let mut rows = match self
                .conn
                .query("SELECT 1 FROM files WHERE path = ?1", params![path.clone()])
                .await
            {
                Ok(r) => r,
                Err(_) => continue,
            };
            if rows.next().await.ok().flatten().is_some() {
                result.push(path);
            }
        }
        result
    }

    /// Check if a file exports (defines) a given symbol
    async fn file_exports_symbol(&self, file: &str, symbol: &str) -> Result<bool, libsql::Error> {
        // Check if symbol is defined in this file (top-level only, parent IS NULL)
        let mut rows = self
            .conn
            .query(
                "SELECT COUNT(*) FROM symbols WHERE file = ?1 AND name = ?2 AND parent IS NULL",
                params![file, symbol],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            let count: i64 = row.get(0)?;
            Ok(count > 0)
        } else {
            Ok(false)
        }
    }

    /// Resolve a name in a file's context to its source module
    /// Returns: (source_module, original_name) if found
    pub async fn resolve_import(
        &self,
        file: &str,
        name: &str,
    ) -> Result<Option<(String, String)>, libsql::Error> {
        // Check for direct import or alias
        let mut rows = self
            .conn
            .query(
                "SELECT module, name FROM imports WHERE file = ?1 AND (name = ?2 OR alias = ?2)",
                params![file, name],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let module: Option<String> = row.get(0)?;
            let orig_name: String = row.get(1)?;
            if let Some(module) = module {
                return Ok(Some((module, orig_name)));
            } else {
                // Plain import (import X), module is the name
                return Ok(Some((orig_name.clone(), orig_name)));
            }
        }

        // Check for wildcard imports - name could come from any of them
        let mut rows = self
            .conn
            .query(
                "SELECT module FROM imports WHERE file = ?1 AND name = '*'",
                params![file],
            )
            .await?;
        let mut wildcards = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Ok(Some(module)) = row.get::<Option<String>>(0) {
                wildcards.push(module);
            }
        }

        // Check each wildcard source to see if it exports the symbol
        for module in &wildcards {
            let files = self.module_to_files(module, file).await;
            for module_file in files {
                if self.file_exports_symbol(&module_file, name).await? {
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
    pub async fn find_importers(
        &self,
        module: &str,
    ) -> Result<Vec<(String, String, usize)>, libsql::Error> {
        let pattern = format!("{}%", module);
        let mut rows = self
            .conn
            .query(
                "SELECT file, name, line FROM imports WHERE module = ?1 OR module LIKE ?2",
                params![module, pattern],
            )
            .await?;
        let mut importers = Vec::new();
        while let Some(row) = rows.next().await? {
            importers.push((row.get(0)?, row.get(1)?, row.get::<i64>(2)? as usize));
        }
        Ok(importers)
    }

    /// Get method names for a type (interface/class) in a specific file.
    /// Used for cross-file interface implementation detection.
    pub async fn get_type_methods(
        &self,
        file: &str,
        type_name: &str,
    ) -> Result<Vec<String>, libsql::Error> {
        let mut rows = self
            .conn
            .query(
                "SELECT method_name FROM type_methods WHERE file = ?1 AND type_name = ?2",
                params![file, type_name],
            )
            .await?;
        let mut methods = Vec::new();
        while let Some(row) = rows.next().await? {
            methods.push(row.get(0)?);
        }
        Ok(methods)
    }

    /// Find files that define a type by name.
    /// Returns all files that have a type (interface/class) with the given name.
    pub async fn find_type_definitions(
        &self,
        type_name: &str,
    ) -> Result<Vec<String>, libsql::Error> {
        let mut rows = self
            .conn
            .query(
                "SELECT DISTINCT file FROM type_methods WHERE type_name = ?1",
                params![type_name],
            )
            .await?;
        let mut files = Vec::new();
        while let Some(row) = rows.next().await? {
            files.push(row.get(0)?);
        }
        Ok(files)
    }

    /// Refresh the call graph by parsing all supported source files
    /// This is more expensive than file refresh since it parses every file
    /// Uses parallel processing for parsing, sequential insertion for SQLite
    pub async fn refresh_call_graph(&mut self) -> Result<CallGraphStats, libsql::Error> {
        // Get all indexed source files
        let files: Vec<String> = {
            let sql = format!(
                "SELECT path FROM files WHERE is_dir = 0 AND ({})",
                source_extensions_sql_filter()
            );
            let mut rows = self.conn.query(&sql, ()).await?;
            let mut files = Vec::new();
            while let Some(row) = rows.next().await? {
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

                // Extract type methods for cross-file interface resolution
                // We need to use the full symbol extraction to get hierarchy
                let extractor = crate::extract::Extractor::new();
                let extract_result = extractor.extract(&full_path, &content);
                let mut type_methods = Vec::new();
                for sym in &extract_result.symbols {
                    if matches!(
                        sym.kind,
                        rhizome_moss_languages::SymbolKind::Interface
                            | rhizome_moss_languages::SymbolKind::Class
                    ) {
                        for child in &sym.children {
                            if matches!(
                                child.kind,
                                rhizome_moss_languages::SymbolKind::Method
                                    | rhizome_moss_languages::SymbolKind::Function
                            ) {
                                type_methods.push((sym.name.clone(), child.name.clone()));
                            }
                        }
                    }
                }

                Some(ParsedFileData {
                    file_path: file_path.clone(),
                    symbols: sym_data,
                    calls: call_data,
                    imports,
                    type_methods,
                })
            })
            .collect();

        // Clear existing data
        self.conn.execute("DELETE FROM symbols", ()).await?;
        self.conn.execute("DELETE FROM calls", ()).await?;
        self.conn.execute("DELETE FROM imports", ()).await?;
        self.conn.execute("DELETE FROM type_methods", ()).await?;

        let mut symbol_count = 0;
        let mut call_count = 0;
        let mut import_count = 0;

        for data in &parsed_data {
            for (name, kind, start_line, end_line, parent) in &data.symbols {
                self.conn.execute(
                    "INSERT INTO symbols (file, name, kind, start_line, end_line, parent) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![data.file_path.clone(), name.clone(), kind.clone(), *start_line as i64, *end_line as i64, parent.clone()],
                ).await?;
                symbol_count += 1;
            }

            for (caller_symbol, callee_name, qualifier, line) in &data.calls {
                self.conn.execute(
                    "INSERT INTO calls (caller_file, caller_symbol, callee_name, callee_qualifier, line) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![data.file_path.clone(), caller_symbol.clone(), callee_name.clone(), qualifier.clone(), *line as i64],
                ).await?;
                call_count += 1;
            }

            for imp in &data.imports {
                self.conn.execute(
                    "INSERT INTO imports (file, module, name, alias, line) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![data.file_path.clone(), imp.module.clone(), imp.name.clone(), imp.alias.clone(), imp.line as i64],
                ).await?;
                import_count += 1;
            }

            for (type_name, method_name) in &data.type_methods {
                self.conn.execute(
                    "INSERT OR IGNORE INTO type_methods (file, type_name, method_name) VALUES (?1, ?2, ?3)",
                    params![data.file_path.clone(), type_name.clone(), method_name.clone()],
                ).await?;
            }
        }

        Ok(CallGraphStats {
            symbols: symbol_count,
            calls: call_count,
            imports: import_count,
        })
    }

    /// Incrementally update call graph for changed files only
    /// Much faster than full refresh when few files changed
    pub async fn incremental_call_graph_refresh(
        &mut self,
    ) -> Result<CallGraphStats, libsql::Error> {
        let changed = self.get_changed_files().await?;

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

        // Remove data for deleted/modified files
        for path in deleted_source_files.iter().chain(changed_files.iter()) {
            self.conn
                .execute("DELETE FROM symbols WHERE file = ?1", params![path.clone()])
                .await?;
            self.conn
                .execute(
                    "DELETE FROM calls WHERE caller_file = ?1",
                    params![path.clone()],
                )
                .await?;
            self.conn
                .execute("DELETE FROM imports WHERE file = ?1", params![path.clone()])
                .await?;
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
                self.conn.execute(
                    "INSERT INTO symbols (file, name, kind, start_line, end_line, parent) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![file_path.clone(), sym.name.clone(), sym.kind.as_str(), sym.start_line as i64, sym.end_line as i64, sym.parent.clone()],
                ).await?;
                symbol_count += 1;

                let kind = sym.kind.as_str();
                if kind == "function" || kind == "method" {
                    let calls = parser.find_callees_for_symbol(&full_path, &content, sym);
                    for (callee_name, line, qualifier) in calls {
                        self.conn.execute(
                            "INSERT INTO calls (caller_file, caller_symbol, callee_name, callee_qualifier, line) VALUES (?1, ?2, ?3, ?4, ?5)",
                            params![file_path.clone(), sym.name.clone(), callee_name, qualifier, line as i64],
                        ).await?;
                        call_count += 1;
                    }
                }
            }

            // Parse imports using trait-based extraction (works for all supported languages)
            let imports = parser.parse_imports(&full_path, &content);
            for imp in imports {
                self.conn.execute(
                    "INSERT INTO imports (file, module, name, alias, line) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![file_path.clone(), imp.module, imp.name, imp.alias, imp.line as i64],
                ).await?;
                import_count += 1;
            }
        }

        Ok(CallGraphStats {
            symbols: symbol_count,
            calls: call_count,
            imports: import_count,
        })
    }

    /// Check if call graph needs refresh
    #[allow(dead_code)] // FileIndex API - used by daemon
    pub async fn needs_call_graph_refresh(&self) -> bool {
        self.call_graph_stats().await.unwrap_or_default().symbols == 0
    }

    /// Find files matching a query using LIKE (fast pre-filter)
    /// Splits query by whitespace/separators and requires all parts to match
    /// Special case: queries starting with '.' are treated as extension patterns
    pub async fn find_like(&self, query: &str) -> Result<Vec<IndexedFile>, libsql::Error> {
        // Handle extension patterns (e.g., ".rs", ".py")
        if query.starts_with('.') && !query.contains('/') {
            let pattern = format!("%{}", query.to_lowercase());
            let mut rows = self.conn.query(
                "SELECT path, is_dir, mtime, lines FROM files WHERE LOWER(path) LIKE ?1 LIMIT 1000",
                params![pattern],
            ).await?;
            let mut files = Vec::new();
            while let Some(row) = rows.next().await? {
                files.push(IndexedFile {
                    path: row.get(0)?,
                    is_dir: row.get::<i64>(1)? != 0,
                    mtime: row.get(2)?,
                    lines: row.get::<i64>(3)? as usize,
                });
            }
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

        let patterns: Vec<String> = parts
            .iter()
            .map(|p| format!("%{}%", p.to_lowercase()))
            .collect();

        // For dynamic params, we need to build them differently
        // libsql doesn't support dynamic parameter slices the same way
        // Use a simpler approach for up to common cases
        let mut files = Vec::new();
        let mut rows = match patterns.len() {
            1 => self.conn.query(&sql, params![patterns[0].clone()]).await?,
            2 => {
                self.conn
                    .query(&sql, params![patterns[0].clone(), patterns[1].clone()])
                    .await?
            }
            3 => {
                self.conn
                    .query(
                        &sql,
                        params![
                            patterns[0].clone(),
                            patterns[1].clone(),
                            patterns[2].clone()
                        ],
                    )
                    .await?
            }
            4 => {
                self.conn
                    .query(
                        &sql,
                        params![
                            patterns[0].clone(),
                            patterns[1].clone(),
                            patterns[2].clone(),
                            patterns[3].clone()
                        ],
                    )
                    .await?
            }
            _ => {
                // For more than 4 parts, just use first 4
                self.conn
                    .query(
                        &sql,
                        params![
                            patterns[0].clone(),
                            patterns[1].clone(),
                            patterns[2].clone(),
                            patterns[3].clone()
                        ],
                    )
                    .await?
            }
        };

        while let Some(row) = rows.next().await? {
            files.push(IndexedFile {
                path: row.get(0)?,
                is_dir: row.get::<i64>(1)? != 0,
                mtime: row.get(2)?,
                lines: row.get::<i64>(3)? as usize,
            });
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_index_creation() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/moss")).unwrap();
        fs::write(dir.path().join("src/moss/cli.py"), "").unwrap();
        fs::write(dir.path().join("src/moss/dwim.py"), "").unwrap();

        let mut index = FileIndex::open(dir.path()).await.unwrap();
        assert!(index.needs_refresh().await);

        let count = index.refresh().await.unwrap();
        assert!(count >= 2);

        // Should find files by name
        let matches = index.find_by_name("cli.py").await.unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].path.ends_with("cli.py"));
    }

    #[tokio::test]
    async fn test_find_by_stem() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/test.py"), "").unwrap();
        fs::write(dir.path().join("src/test.rs"), "").unwrap();

        let mut index = FileIndex::open(dir.path()).await.unwrap();
        index.refresh().await.unwrap();

        let matches = index.find_by_stem("test").await.unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[tokio::test]
    async fn test_wildcard_import_resolution() {
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

        let mut index = FileIndex::open(dir.path()).await.unwrap();
        index.refresh().await.unwrap();
        index.refresh_call_graph().await.unwrap();

        // Manually add wildcard imports (refresh_call_graph parses these)
        // The parser should have picked up the wildcard imports

        // Now resolve MyClass - should find it in mylib.exports
        let result = index
            .resolve_import("src/consumer.py", "MyClass")
            .await
            .unwrap();
        assert!(result.is_some(), "Should resolve MyClass");
        let (module, name) = result.unwrap();
        assert_eq!(module, "mylib.exports");
        assert_eq!(name, "MyClass");

        // Resolve OtherThing - should find it in mylib.other
        let result = index
            .resolve_import("src/consumer.py", "OtherThing")
            .await
            .unwrap();
        assert!(result.is_some(), "Should resolve OtherThing");
        let (module, name) = result.unwrap();
        assert_eq!(module, "mylib.other");
        assert_eq!(name, "OtherThing");
    }

    #[tokio::test]
    async fn test_method_call_resolution() {
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

        let mut index = FileIndex::open(dir.path()).await.unwrap();
        index.refresh().await.unwrap();
        index.refresh_call_graph().await.unwrap();

        // Find callers of method_b - should include method_a and method_c
        let callers = index.find_callers("method_b").await.unwrap();
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
        let callers = index.find_callers("MyClass.method_b").await.unwrap();
        assert!(
            !callers.is_empty(),
            "Should find callers of MyClass.method_b"
        );
    }
}
