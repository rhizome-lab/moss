//! External package resolution - shared types only.
//!
//! Language-specific resolution has been moved to individual language modules:
//! - Python: python.rs
//! - Go: go.rs
//! - Rust: rust.rs
//! - JavaScript/TypeScript/Deno: ecmascript.rs
//! - Java: java.rs
//! - C/C++: c_cpp.rs
//!
//! This module contains:
//! - ResolvedPackage: Common result type for package resolution
//! - Global cache: ~/.cache/moss/ for indexed packages
//! - PackageIndex: SQLite-backed package/symbol index

use std::path::PathBuf;

// =============================================================================
// Shared Types
// =============================================================================

/// Result of resolving an external package
#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    /// Path to the package source
    pub path: PathBuf,
    /// Package name as imported
    pub name: String,
    /// Whether this is a namespace package (no __init__.py)
    pub is_namespace: bool,
}

// =============================================================================
// Global Cache
// =============================================================================

/// Get the global moss cache directory (~/.cache/moss/).
pub fn get_global_cache_dir() -> Option<PathBuf> {
    let cache_base = if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache")
    } else if let Ok(home) = std::env::var("USERPROFILE") {
        PathBuf::from(home).join(".cache")
    } else {
        return None;
    };

    let moss_cache = cache_base.join("moss");
    if !moss_cache.exists() {
        std::fs::create_dir_all(&moss_cache).ok()?;
    }

    Some(moss_cache)
}

/// Get the path to the unified global package index database.
pub fn get_global_packages_db() -> Option<PathBuf> {
    let cache = get_global_cache_dir()?;
    Some(cache.join("packages.db"))
}

/// Compare version strings semver-style.
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<u32> = a.split('.').filter_map(|p| p.parse().ok()).collect();
    let b_parts: Vec<u32> = b.split('.').filter_map(|p| p.parse().ok()).collect();

    for (ap, bp) in a_parts.iter().zip(b_parts.iter()) {
        match ap.cmp(bp) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    a_parts.len().cmp(&b_parts.len())
}

// =============================================================================
// Global Package Index Database
// =============================================================================

use libsql::{Connection, Database, params};

/// Parsed version as (major, minor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
}

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() >= 2 {
            Some(Version {
                major: parts[0].parse().ok()?,
                minor: parts[1].parse().ok()?,
            })
        } else {
            None
        }
    }

    pub fn in_range(&self, min: Version, max: Option<Version>) -> bool {
        if *self < min {
            return false;
        }
        if let Some(max) = max {
            if *self > max {
                return false;
            }
        }
        true
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.major.cmp(&other.major) {
            std::cmp::Ordering::Equal => self.minor.cmp(&other.minor),
            ord => ord,
        }
    }
}

/// A package record in the index.
#[derive(Debug, Clone)]
pub struct PackageRecord {
    pub id: i64,
    pub language: String,
    pub name: String,
    pub path: String,
    pub min_major: u32,
    pub min_minor: u32,
    pub max_major: Option<u32>,
    pub max_minor: Option<u32>,
}

impl PackageRecord {
    pub fn min_version(&self) -> Version {
        Version {
            major: self.min_major,
            minor: self.min_minor,
        }
    }

    pub fn max_version(&self) -> Option<Version> {
        match (self.max_major, self.max_minor) {
            (Some(major), Some(minor)) => Some(Version { major, minor }),
            _ => None,
        }
    }
}

/// A symbol record in the index.
#[derive(Debug, Clone)]
pub struct SymbolRecord {
    pub id: i64,
    pub package_id: i64,
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub line: u32,
}

/// Global package index backed by libSQL.
pub struct PackageIndex {
    conn: Connection,
    #[allow(dead_code)]
    db: Database,
}

impl PackageIndex {
    pub async fn open() -> Result<Self, libsql::Error> {
        let db_path = get_global_packages_db().ok_or_else(|| {
            libsql::Error::SqliteFailure(1, "Cannot determine cache directory".into())
        })?;

        let db = libsql::Builder::new_local(db_path).build().await?;
        let conn = db.connect()?;
        let index = PackageIndex { conn, db };
        index.init_schema().await?;
        Ok(index)
    }

    pub async fn open_in_memory() -> Result<Self, libsql::Error> {
        let db = libsql::Builder::new_local(":memory:").build().await?;
        let conn = db.connect()?;
        let index = PackageIndex { conn, db };
        index.init_schema().await?;
        Ok(index)
    }

    async fn init_schema(&self) -> Result<(), libsql::Error> {
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS packages (
                id INTEGER PRIMARY KEY,
                language TEXT NOT NULL,
                name TEXT NOT NULL,
                path TEXT NOT NULL,
                min_major INTEGER NOT NULL,
                min_minor INTEGER NOT NULL,
                max_major INTEGER,
                max_minor INTEGER,
                indexed_at INTEGER NOT NULL
            )",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_packages_lang_name ON packages(language, name)",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS symbols (
                id INTEGER PRIMARY KEY,
                package_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                signature TEXT NOT NULL,
                line INTEGER NOT NULL,
                FOREIGN KEY (package_id) REFERENCES packages(id) ON DELETE CASCADE
            )",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_symbols_package ON symbols(package_id)",
                (),
            )
            .await?;
        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name)",
                (),
            )
            .await?;

        Ok(())
    }

    pub async fn insert_package(
        &self,
        language: &str,
        name: &str,
        path: &str,
        min_version: Version,
        max_version: Option<Version>,
    ) -> Result<i64, libsql::Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT INTO packages (language, name, path, min_major, min_minor, max_major, max_minor, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                language,
                name,
                path,
                min_version.major,
                min_version.minor,
                max_version.map(|v| v.major),
                max_version.map(|v| v.minor),
                now,
            ],
        ).await?;
        Ok(self.conn.last_insert_rowid())
    }

    pub async fn insert_symbol(
        &self,
        package_id: i64,
        name: &str,
        kind: &str,
        signature: &str,
        line: u32,
    ) -> Result<i64, libsql::Error> {
        self.conn
            .execute(
                "INSERT INTO symbols (package_id, name, kind, signature, line)
             VALUES (?1, ?2, ?3, ?4, ?5)",
                params![package_id, name, kind, signature, line],
            )
            .await?;
        Ok(self.conn.last_insert_rowid())
    }

    pub async fn find_package(
        &self,
        language: &str,
        name: &str,
        version: Option<Version>,
    ) -> Result<Option<PackageRecord>, libsql::Error> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, language, name, path, min_major, min_minor, max_major, max_minor
             FROM packages WHERE language = ?1 AND name = ?2",
                params![language, name],
            )
            .await?;

        let mut packages = Vec::new();
        while let Some(row) = rows.next().await? {
            packages.push(PackageRecord {
                id: row.get(0)?,
                language: row.get(1)?,
                name: row.get(2)?,
                path: row.get(3)?,
                min_major: row.get(4)?,
                min_minor: row.get(5)?,
                max_major: row.get(6)?,
                max_minor: row.get(7)?,
            });
        }

        if let Some(ver) = version {
            for pkg in packages {
                if ver.in_range(pkg.min_version(), pkg.max_version()) {
                    return Ok(Some(pkg));
                }
            }
            Ok(None)
        } else {
            Ok(packages.into_iter().next())
        }
    }

    pub async fn get_symbols(&self, package_id: i64) -> Result<Vec<SymbolRecord>, libsql::Error> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, package_id, name, kind, signature, line
             FROM symbols WHERE package_id = ?1 ORDER BY line",
                params![package_id],
            )
            .await?;

        let mut symbols = Vec::new();
        while let Some(row) = rows.next().await? {
            symbols.push(SymbolRecord {
                id: row.get(0)?,
                package_id: row.get(1)?,
                name: row.get(2)?,
                kind: row.get(3)?,
                signature: row.get(4)?,
                line: row.get(5)?,
            });
        }

        Ok(symbols)
    }

    pub async fn find_symbol(
        &self,
        language: &str,
        symbol_name: &str,
        version: Option<Version>,
    ) -> Result<Vec<(PackageRecord, SymbolRecord)>, libsql::Error> {
        let mut rows = self.conn.query(
            "SELECT p.id, p.language, p.name, p.path, p.min_major, p.min_minor, p.max_major, p.max_minor,
                    s.id, s.package_id, s.name, s.kind, s.signature, s.line
             FROM symbols s
             JOIN packages p ON s.package_id = p.id
             WHERE p.language = ?1 AND s.name = ?2",
            params![language, symbol_name],
        ).await?;

        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            results.push((
                PackageRecord {
                    id: row.get(0)?,
                    language: row.get(1)?,
                    name: row.get(2)?,
                    path: row.get(3)?,
                    min_major: row.get(4)?,
                    min_minor: row.get(5)?,
                    max_major: row.get(6)?,
                    max_minor: row.get(7)?,
                },
                SymbolRecord {
                    id: row.get(8)?,
                    package_id: row.get(9)?,
                    name: row.get(10)?,
                    kind: row.get(11)?,
                    signature: row.get(12)?,
                    line: row.get(13)?,
                },
            ));
        }

        if let Some(ver) = version {
            Ok(results
                .into_iter()
                .filter(|(pkg, _)| ver.in_range(pkg.min_version(), pkg.max_version()))
                .collect())
        } else {
            Ok(results)
        }
    }

    pub async fn is_indexed(&self, language: &str, name: &str) -> Result<bool, libsql::Error> {
        let mut rows = self
            .conn
            .query(
                "SELECT COUNT(*) FROM packages WHERE language = ?1 AND name = ?2",
                params![language, name],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let count: i64 = row.get(0)?;
            Ok(count > 0)
        } else {
            Ok(false)
        }
    }

    pub async fn delete_package(&self, package_id: i64) -> Result<(), libsql::Error> {
        self.conn
            .execute(
                "DELETE FROM symbols WHERE package_id = ?1",
                params![package_id],
            )
            .await?;
        self.conn
            .execute("DELETE FROM packages WHERE id = ?1", params![package_id])
            .await?;
        Ok(())
    }

    pub async fn clear(&self) -> Result<(), libsql::Error> {
        self.conn.execute("DELETE FROM symbols", ()).await?;
        self.conn.execute("DELETE FROM packages", ()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        assert_eq!(
            Version::parse("3.11"),
            Some(Version {
                major: 3,
                minor: 11
            })
        );
        assert_eq!(
            Version::parse("1.21"),
            Some(Version {
                major: 1,
                minor: 21
            })
        );
        assert_eq!(Version::parse("invalid"), None);
    }

    #[test]
    fn test_version_comparison() {
        let v1 = Version {
            major: 3,
            minor: 10,
        };
        let v2 = Version {
            major: 3,
            minor: 11,
        };
        let v3 = Version { major: 4, minor: 0 };

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v1.in_range(v1, Some(v2)));
        assert!(!v3.in_range(v1, Some(v2)));
    }

    #[tokio::test]
    async fn test_package_index() {
        let index = PackageIndex::open_in_memory().await.unwrap();

        // Insert a package
        let pkg_id = index
            .insert_package(
                "python",
                "requests",
                "/path/to/requests",
                Version { major: 3, minor: 8 },
                None,
            )
            .await
            .unwrap();

        // Insert a symbol
        index
            .insert_symbol(pkg_id, "get", "function", "def get(url)", 10)
            .await
            .unwrap();

        // Find the package
        let found = index
            .find_package("python", "requests", None)
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "requests");

        // Find the symbol
        let symbols = index.get_symbols(pkg_id).await.unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "get");

        // Check indexed
        assert!(index.is_indexed("python", "requests").await.unwrap());
        assert!(!index.is_indexed("python", "nonexistent").await.unwrap());
    }
}
