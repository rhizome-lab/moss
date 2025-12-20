use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use ignore::WalkBuilder;

use crate::symbols::{Symbol, SymbolParser};

// Not yet public - just delete .moss/index.sqlite on schema changes
const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone)]
pub struct IndexedFile {
    pub path: String,
    pub is_dir: bool,
    pub mtime: i64,
}

pub struct FileIndex {
    conn: Connection,
    root: PathBuf,
}

impl FileIndex {
    /// Open or create an index for a directory.
    /// Index is stored in .moss/index.sqlite
    pub fn open(root: &Path) -> rusqlite::Result<Self> {
        let moss_dir = root.join(".moss");
        std::fs::create_dir_all(&moss_dir).ok();

        let db_path = moss_dir.join("index.sqlite");
        let conn = Connection::open(&db_path)?;

        // Initialize schema
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT
            );
            CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                is_dir INTEGER NOT NULL,
                mtime INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_files_name ON files(path);

            -- Call graph for fast caller/callee lookups
            CREATE TABLE IF NOT EXISTS calls (
                caller_file TEXT NOT NULL,
                caller_symbol TEXT NOT NULL,
                callee_name TEXT NOT NULL,
                line INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_calls_callee ON calls(callee_name);
            CREATE INDEX IF NOT EXISTS idx_calls_caller ON calls(caller_file, caller_symbol);

            -- Symbol definitions for fast symbol lookups
            CREATE TABLE IF NOT EXISTS symbols (
                file TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                parent TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
            CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file);
            "
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

    /// Check if index needs refresh based on .moss directory mtime
    pub fn needs_refresh(&self) -> bool {
        // Check if index is empty
        let file_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap_or(0);
        if file_count == 0 {
            return true;
        }

        let last_indexed: i64 = self
            .conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'last_indexed'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // If never indexed, refresh
        if last_indexed == 0 {
            return true;
        }

        // Check if any common directories have changed
        // This is a heuristic - check src/, lib/, etc.
        // Note: "." changes too often, skip it
        for dir in &["src", "lib", "crates"] {
            let path = self.root.join(dir);
            if path.exists() {
                if let Ok(meta) = path.metadata() {
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

        false
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
                if rel_str.is_empty() {
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

                tx.execute(
                    "INSERT INTO files (path, is_dir, mtime) VALUES (?1, ?2, ?3)",
                    params![rel_str, is_dir as i64, mtime],
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
        let mut stmt = self.conn.prepare("SELECT path, is_dir, mtime FROM files")?;
        let files = stmt
            .query_map([], |row| {
                Ok(IndexedFile {
                    path: row.get(0)?,
                    is_dir: row.get::<_, i64>(1)? != 0,
                    mtime: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }

    /// Search files by exact name match
    pub fn find_by_name(&self, name: &str) -> rusqlite::Result<Vec<IndexedFile>> {
        let pattern = format!("%/{}", name);
        let mut stmt = self.conn.prepare(
            "SELECT path, is_dir, mtime FROM files WHERE path LIKE ?1 OR path = ?2"
        )?;
        let files = stmt
            .query_map(params![pattern, name], |row| {
                Ok(IndexedFile {
                    path: row.get(0)?,
                    is_dir: row.get::<_, i64>(1)? != 0,
                    mtime: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }

    /// Search files by stem (filename without extension)
    pub fn find_by_stem(&self, stem: &str) -> rusqlite::Result<Vec<IndexedFile>> {
        let pattern = format!("%/{}%", stem);
        let mut stmt = self.conn.prepare(
            "SELECT path, is_dir, mtime FROM files WHERE path LIKE ?1"
        )?;
        let files = stmt
            .query_map(params![pattern], |row| {
                Ok(IndexedFile {
                    path: row.get(0)?,
                    is_dir: row.get::<_, i64>(1)? != 0,
                    mtime: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }

    /// Count indexed files
    pub fn count(&self) -> rusqlite::Result<usize> {
        self.conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
    }

    /// Index symbols and call graph for a file
    pub fn index_file_symbols(&self, path: &str, symbols: &[Symbol], calls: &[(String, String, usize)]) -> rusqlite::Result<()> {
        // Insert symbols
        for sym in symbols {
            self.conn.execute(
                "INSERT INTO symbols (file, name, kind, start_line, end_line, parent) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![path, sym.name, sym.kind.as_str(), sym.start_line, sym.end_line, sym.parent],
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
    /// Uses case-insensitive matching and supports partial matches
    pub fn find_callers(&self, symbol_name: &str) -> rusqlite::Result<Vec<(String, String, usize)>> {
        // Try exact match first
        let mut stmt = self.conn.prepare(
            "SELECT caller_file, caller_symbol, line FROM calls WHERE callee_name = ?1"
        )?;
        let callers: Vec<(String, String, usize)> = stmt
            .query_map(params![symbol_name], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        if !callers.is_empty() {
            return Ok(callers);
        }

        // Try case-insensitive match
        let mut stmt = self.conn.prepare(
            "SELECT caller_file, caller_symbol, line FROM calls WHERE LOWER(callee_name) = LOWER(?1)"
        )?;
        let callers: Vec<(String, String, usize)> = stmt
            .query_map(params![symbol_name], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        if !callers.is_empty() {
            return Ok(callers);
        }

        // Try LIKE pattern match (contains)
        let pattern = format!("%{}%", symbol_name);
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
    pub fn find_callees(&self, file: &str, symbol_name: &str) -> rusqlite::Result<Vec<(String, usize)>> {
        let mut stmt = self.conn.prepare(
            "SELECT callee_name, line FROM calls WHERE caller_file = ?1 AND caller_symbol = ?2"
        )?;
        let callees = stmt
            .query_map(params![file, symbol_name], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(callees)
    }

    /// Find a symbol by name
    pub fn find_symbol(&self, name: &str) -> rusqlite::Result<Vec<(String, String, usize, usize)>> {
        let mut stmt = self.conn.prepare(
            "SELECT file, kind, start_line, end_line FROM symbols WHERE name = ?1"
        )?;
        let symbols = stmt
            .query_map(params![name], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(symbols)
    }

    /// Get call graph stats
    pub fn call_graph_stats(&self) -> rusqlite::Result<(usize, usize)> {
        let symbol_count: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols", [], |row| row.get(0)
        )?;
        let call_count: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM calls", [], |row| row.get(0)
        )?;
        Ok((symbol_count, call_count))
    }

    /// Refresh the call graph by parsing all Python/Rust files
    /// This is more expensive than file refresh since it parses every file
    pub fn refresh_call_graph(&mut self) -> rusqlite::Result<(usize, usize)> {
        // Get all indexed Python/Rust files BEFORE starting transaction
        let files: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT path FROM files WHERE is_dir = 0 AND (path LIKE '%.py' OR path LIKE '%.rs')"
            )?;
            let mut files = Vec::new();
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let path: String = row.get(0)?;
                files.push(path);
            }
            files
        };

        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM symbols", [])?;
        tx.execute("DELETE FROM calls", [])?;

        let mut parser = SymbolParser::new();
        let mut symbol_count = 0;
        let mut call_count = 0;

        for file_path in files {
            let full_path = self.root.join(&file_path);
            let content = match std::fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let symbols = parser.parse_file(&full_path, &content);

            // Insert symbols
            for sym in &symbols {
                tx.execute(
                    "INSERT INTO symbols (file, name, kind, start_line, end_line, parent) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![file_path, sym.name, sym.kind.as_str(), sym.start_line, sym.end_line, sym.parent],
                )?;
                symbol_count += 1;

                // Get calls for this symbol
                let calls = parser.find_callees_with_lines(&full_path, &content, &sym.name);
                for (callee_name, line) in calls {
                    tx.execute(
                        "INSERT INTO calls (caller_file, caller_symbol, callee_name, line) VALUES (?1, ?2, ?3, ?4)",
                        params![file_path, sym.name, callee_name, line],
                    )?;
                    call_count += 1;
                }
            }
        }

        tx.commit()?;
        Ok((symbol_count, call_count))
    }

    /// Check if call graph needs refresh
    pub fn needs_call_graph_refresh(&self) -> bool {
        let (symbols, _) = self.call_graph_stats().unwrap_or((0, 0));
        symbols == 0
    }

    /// Find files matching a query using LIKE (fast pre-filter)
    /// Splits query by whitespace/separators and requires all parts to match
    pub fn find_like(&self, query: &str) -> rusqlite::Result<Vec<IndexedFile>> {
        // Normalize query: split on whitespace and common separators
        let parts: Vec<&str> = query
            .split(|c: char| c.is_whitespace() || c == '_' || c == '-' || c == '.')
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
            "SELECT path, is_dir, mtime FROM files WHERE {} LIMIT 50",
            conditions.join(" AND ")
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let patterns: Vec<String> = parts.iter().map(|p| format!("%{}%", p.to_lowercase())).collect();

        // Bind all parameters
        let params: Vec<&dyn rusqlite::ToSql> = patterns.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let files = stmt
            .query_map(params.as_slice(), |row| {
                Ok(IndexedFile {
                    path: row.get(0)?,
                    is_dir: row.get::<_, i64>(1)? != 0,
                    mtime: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

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
}
