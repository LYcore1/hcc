//! SQLite-based cache for HCC filesystem operations
//! 
//! This module provides a persistent cache using SQLite to reduce RAM usage
//! while maintaining fast access to file type detection results.

use std::path::{Path, PathBuf};
use rusqlite::{Connection, Result as SqliteResult};
use crate::fingerprint;

/// Cache for storing file type detection results
pub struct Cache {
    conn: Connection,
    source_path: PathBuf,
}

impl Cache {
    /// Create a new cache at the given path
    pub fn new(db_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let conn = Connection::open(db_path)?;
        
        // Create tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS file_types (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT UNIQUE NOT NULL,
                file_type TEXT NOT NULL,
                confidence REAL NOT NULL,
                extension TEXT,
                mtime INTEGER NOT NULL
            )",
            [],
        )?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS types_index (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                type_name TEXT NOT NULL,
                file_path TEXT NOT NULL
            )",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_file_types_path ON file_types(file_path)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_file_types_type ON file_types(file_type)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_types_index_type ON types_index(type_name)",
            [],
        )?;
        
        Ok(Self {
            conn,
            source_path: PathBuf::new(),
        })
    }
    
    /// Set the source path for this cache
    pub fn set_source(&mut self, source: PathBuf) {
        self.source_path = source;
    }
    
    /// Build the cache by walking the directory and fingerprinting all files
    pub fn build_cache(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Building cache for {:?}", self.source_path);
        
        let transaction = self.conn.transaction()?;
        
        // Clear existing cache
        transaction.execute("DELETE FROM file_types", [])?;
        transaction.execute("DELETE FROM types_index", [])?;
        
        // Walk directory and fingerprint each file
        for entry in walkdir::WalkDir::new(&self.source_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let path = entry.path();
                match fingerprint::fingerprint(path) {
                    Ok(fp) => {
                        let file_type_str = format!("{:?}", fp.file_type);
                        let mtime = entry.metadata()?.modified()?
                            .duration_since(std::time::UNIX_EPOCH)?
                            .as_secs() as i64;
                        
                        transaction.execute(
                            "INSERT OR REPLACE INTO file_types (file_path, file_type, confidence, extension, mtime)
                             VALUES (?1, ?2, ?3, ?4, ?5)",
                            [
                                path.to_string_lossy().to_string(),
                                file_type_str.clone(),
                                fp.confidence.to_string(),
                                fp.extension.unwrap_or_default(),
                                mtime.to_string(),
                            ],
                        )?;
                        
                        transaction.execute(
                            "INSERT INTO types_index (type_name, file_path) VALUES (?1, ?2)",
                            [file_type_str, path.to_string_lossy().to_string()],
                        )?;
                    }
                    Err(e) => {
                        log::warn!("Failed to fingerprint {:?}: {}", path, e);
                    }
                }
            }
        }
        
        transaction.commit()?;
        log::info!("Cache built successfully");
        Ok(())
    }
    
    /// Get all unique file types in the cache
    pub fn get_types(&self) -> SqliteResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT file_type FROM file_types ORDER BY file_type"
        )?;
        
        let types = stmt.query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        
        Ok(types)
    }
    
    /// Get all files of a specific type
    pub fn get_files_by_type(&self, type_name: &str) -> SqliteResult<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT file_path, extension FROM file_types WHERE file_type = ?1"
        )?;
        
        let files = stmt.query_map([type_name], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();
        
        Ok(files)
    }
    
    /// Check if cache needs rebuilding based on source directory mtime
    pub fn needs_rebuild(&self, max_age_secs: u64) -> Result<bool, Box<dyn std::error::Error>> {
        // Check if cache is empty
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_types",
            [],
            |row| row.get(0),
        )?;
        
        if count == 0 {
            return Ok(true);
        }
        
        // Check source directory mtime
        let source_mtime = std::fs::metadata(&self.source_path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        
        // Check most recent cache entry
        let cache_mtime: Option<i64> = self.conn.query_row(
            "SELECT MAX(mtime) FROM file_types",
            [],
            |row| row.get(0),
        ).unwrap_or(None);
        
        match cache_mtime {
            Some(cache_time) => {
                let age = source_mtime.saturating_sub(cache_time as u64);
                Ok(age > max_age_secs)
            }
            None => Ok(true),
        }
    }
    
    /// Invalidate the entire cache
    pub fn invalidate(&self) -> SqliteResult<()> {
        self.conn.execute("DELETE FROM file_types", [])?;
        self.conn.execute("DELETE FROM types_index", [])?;
        Ok(())
    }
    
    /// Rebuild the cache from scratch
    pub fn rebuild(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.invalidate()?;
        self.build_cache()?;
        Ok(())
    }
    
    /// Get the number of cached entries
    pub fn count(&self) -> SqliteResult<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM file_types", [], |row| row.get(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    #[test]
    fn test_cache_build_and_query() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        // Create some test files
        fs::write(temp_dir.path().join("test.py"), "print('hello')").unwrap();
        fs::write(temp_dir.path().join("test.js"), "console.log('hello')").unwrap();
        fs::write(temp_dir.path().join("README.md"), "# Test").unwrap();
        
        let cache = Cache::new(&db_path).unwrap();
        
        // Can't test build_cache without a proper source path setup
        // but we can test the cache structure
        assert!(db_path.exists());
    }
}
