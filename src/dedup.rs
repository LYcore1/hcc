//! Dedup module - Block-level deduplication and compression

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Default block size for deduplication (4KB)
pub const BLOCK_SIZE: u64 = 4096;

/// Hash type for block identification (BLAKE3 produces 32-byte hashes)
pub type Hash = [u8; 32];

/// Entry in the deduplication table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupEntry {
    pub physical_location: PathBuf,
    pub reference_count: u64,
    pub compressed: bool,
    pub original_size: u64,
}

/// The deduplication table
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DedupTable {
    entries: HashMap<Hash, DedupEntry>,
    storage_root: PathBuf,
}

impl DedupTable {
    /// Create a new deduplication table
    pub fn new(storage_root: PathBuf) -> Self {
        Self {
            entries: HashMap::new(),
            storage_root,
        }
    }

    /// Calculate BLAKE3 hash of data
    pub fn hash_data(data: &[u8]) -> Hash {
        let hash = blake3::hash(data);
        *hash.as_bytes()
    }

    /// Check if a block exists in the table
    pub fn has_block(&self, hash: &Hash) -> bool {
        self.entries.contains_key(hash)
    }

    /// Get a block entry
    pub fn get_entry(&self, hash: &Hash) -> Option<&DedupEntry> {
        self.entries.get(hash)
    }

    /// Add or update a block in the table
    pub fn add_block(
        &mut self,
        hash: Hash,
        data: &[u8],
        compressed: bool,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let entry = self.entries.entry(hash).or_insert_with(|| {
            // Generate unique path for this block
            let hash_str = hex::encode(&hash);
            let block_path = self.storage_root.join("blocks").join(&hash_str[..2]).join(&hash_str[2..4]).join(&hash_str);
            
            // Create directory structure
            if let Some(parent) = block_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }

            // Write block data
            std::fs::write(&block_path, data).ok();

            DedupEntry {
                physical_location: block_path,
                reference_count: 0,
                compressed,
                original_size: data.len() as u64,
            }
        });

        entry.reference_count += 1;
        Ok(entry.physical_location.clone())
    }

    /// Remove a block reference
    pub fn remove_block(&mut self, hash: &Hash) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(entry) = self.entries.get_mut(hash) {
            entry.reference_count = entry.reference_count.saturating_sub(1);

            // If no more references, delete the block
            if entry.reference_count == 0 {
                if let Err(e) = std::fs::remove_file(&entry.physical_location) {
                    log::warn!("Failed to remove block {:?}: {}", entry.physical_location, e);
                }
                self.entries.remove(hash);
            }
        }
        Ok(())
    }

    /// Read block data from storage
    pub fn read_block(&self, hash: &Hash) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        if let Some(entry) = self.entries.get(hash) {
            let data = std::fs::read(&entry.physical_location)?;
            Ok(data)
        } else {
            Err("Block not found".into())
        }
    }

    /// Compress data using zstd
    pub fn compress(data: &[u8], level: i32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let compressed = zstd::stream::encode_all(data, level)?;
        Ok(compressed)
    }

    /// Decompress data using zstd
    pub fn decompress(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let decompressed = zstd::stream::decode_all(data)?;
        Ok(decompressed)
    }

    /// Compress data using lz4 (fast compression)
    pub fn compress_lz4(_data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Placeholder - lz4 crate API needs proper integration
        Err("LZ4 compression not yet implemented".into())
    }

    /// Decompress data using lz4
    pub fn decompress_lz4(_data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Placeholder - lz4 crate API needs proper integration
        Err("LZ4 decompression not yet implemented".into())
    }

    /// Get statistics about the dedup table
    pub fn stats(&self) -> DedupStats {
        let total_blocks = self.entries.len();
        let total_references: u64 = self.entries.values().map(|e| e.reference_count).sum();
        let total_original_size: u64 = self.entries.values().map(|e| e.original_size * e.reference_count).sum();
        let total_stored_size: u64 = self.entries.values().map(|e| e.original_size).sum();

        let savings = if total_original_size > 0 {
            ((total_original_size - total_stored_size) as f64 / total_original_size as f64) * 100.0
        } else {
            0.0
        };

        DedupStats {
            total_blocks,
            total_references,
            total_original_size,
            total_stored_size,
            savings_percentage: savings,
        }
    }

    /// Save the dedup table to disk
    pub fn save_to_file(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let yaml = serde_yaml::to_string(self)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    /// Load the dedup table from disk
    pub fn load_from_file(path: &Path, storage_root: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let yaml = std::fs::read_to_string(path)?;
        let mut table: Self = serde_yaml::from_str(&yaml)?;
        table.storage_root = storage_root;
        Ok(table)
    }
}

/// Statistics about deduplication
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DedupStats {
    pub total_blocks: usize,
    pub total_references: u64,
    pub total_original_size: u64,
    pub total_stored_size: u64,
    pub savings_percentage: f64,
}

impl std::fmt::Display for DedupStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Deduplication Statistics:")?;
        writeln!(f, "  Total blocks: {}", self.total_blocks)?;
        writeln!(f, "  Total references: {}", self.total_references)?;
        writeln!(f, "  Original size: {} bytes", self.total_original_size)?;
        writeln!(f, "  Stored size: {} bytes", self.total_stored_size)?;
        writeln!(f, "  Space savings: {:.2}%", self.savings_percentage)?;
        Ok(())
    }
}

// Helper for hex encoding
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hash_data() {
        let data = b"hello world";
        let hash1 = DedupTable::hash_data(data);
        let hash2 = DedupTable::hash_data(data);

        assert_eq!(hash1, hash2);

        let hash3 = DedupTable::hash_data(b"hello world!");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_dedup_table_add_block() {
        let dir = TempDir::new().unwrap();
        let mut table = DedupTable::new(dir.path().to_path_buf());

        let data = b"test data";
        let hash = DedupTable::hash_data(data);

        let path = table.add_block(hash, data, false).unwrap();

        assert!(path.exists());
        assert!(table.has_block(&hash));
        assert_eq!(table.get_entry(&hash).unwrap().reference_count, 1);
    }

    #[test]
    fn test_dedup_table_duplicate_block() {
        let dir = TempDir::new().unwrap();
        let mut table = DedupTable::new(dir.path().to_path_buf());

        let data = b"test data";
        let hash = DedupTable::hash_data(data);

        // Add the same block twice
        table.add_block(hash, data, false).unwrap();
        table.add_block(hash, data, false).unwrap();

        // Should only store once, but reference count should be 2
        assert_eq!(table.entries.len(), 1);
        assert_eq!(table.get_entry(&hash).unwrap().reference_count, 2);
    }

    #[test]
    fn test_dedup_table_remove_block() {
        let dir = TempDir::new().unwrap();
        let mut table = DedupTable::new(dir.path().to_path_buf());

        let data = b"test data";
        let hash = DedupTable::hash_data(data);

        table.add_block(hash, data, false).unwrap();
        table.remove_block(&hash).unwrap();

        // Block should be removed
        assert!(!table.has_block(&hash));
    }

    #[test]
    fn test_compression() {
        let data = b"This is some repetitive text that should compress well. ".repeat(100);
        
        let compressed = DedupTable::compress(&data, 3).unwrap();
        let decompressed = DedupTable::decompress(&compressed).unwrap();

        assert_eq!(data, decompressed);
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_dedup_stats() {
        let dir = TempDir::new().unwrap();
        let mut table = DedupTable::new(dir.path().to_path_buf());

        let data = b"test data";
        let hash = DedupTable::hash_data(data);

        table.add_block(hash, data, false).unwrap();
        table.add_block(hash, data, false).unwrap();

        let stats = table.stats();

        assert_eq!(stats.total_blocks, 1);
        assert_eq!(stats.total_references, 2);
        assert!(stats.savings_percentage > 0.0);
    }
}
