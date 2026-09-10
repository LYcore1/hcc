//! HCC Core Library - The Brain of the High Capacity Colorful File System
//! 
//! This module handles all logic that does not directly access the file system.
//! It includes color assignment, graph building, and community detection.

pub mod color;
pub mod fingerprint;
pub mod graph;
pub mod dedup;

#[cfg(feature = "fuse")]
pub mod fuse;

pub use color::{Color, ColorId, ColorTable};
pub use fingerprint::{Fingerprint, FileType, fingerprint};
pub use graph::{FileGraph, Node, Edge, EdgeSource, build_graph, detect_communities};
pub use dedup::{DedupTable, DedupEntry, Hash, BLOCK_SIZE};

#[cfg(feature = "fuse")]
pub use fuse::HccFs;

/// Auto-color pipeline: builds graph, detects communities, assigns colors
pub fn auto_color(root: &std::path::Path) -> Result<ColorTable, Box<dyn std::error::Error>> {
    use std::path::PathBuf;
    
    log::info!("Building file graph for {:?}", root);
    let graph = build_graph(root)?;
    
    log::info!("Detecting communities using Louvain algorithm");
    let communities = detect_communities(&graph)?;
    
    log::info!("Assigning colors to {} communities", communities.len());
    let mut table = ColorTable::new();
    
    for (idx, community) in communities.iter().enumerate() {
        // Collect file paths for this community
        let files: Vec<PathBuf> = community
            .iter()
            .filter_map(|&node_idx| graph.nodes.get(node_idx).map(|n| n.path.clone()))
            .collect();
        
        // Generate meaningful name based on files
        let name = Color::generate_name(&files, idx);
        
        // Calculate confidence based on community cohesion
        let confidence = calculate_community_confidence(&graph, community);
        
        let color = Color::new(
            idx as ColorId,
            name,
            confidence,
        );
        
        for &node_idx in community {
            if let Some(node) = graph.nodes.get(node_idx) {
                table.assign_color(node.path.clone(), color.clone());
            }
        }
    }
    
    Ok(table)
}

/// Calculate confidence score for a community based on edge weights
fn calculate_community_confidence(graph: &FileGraph, community: &[usize]) -> f64 {
    if community.len() <= 1 {
        return 0.5; // Single file gets medium confidence
    }
    
    let mut total_weight = 0.0;
    let mut edge_count = 0;
    
    for i in 0..community.len() {
        for j in (i + 1)..community.len() {
            // Find edge between these nodes
            for edge in &graph.edges {
                if (edge.from == community[i] && edge.to == community[j]) ||
                   (edge.from == community[j] && edge.to == community[i]) {
                    total_weight += edge.weight as f64 / 1000.0;
                    edge_count += 1;
                    break;
                }
            }
        }
    }
    
    if edge_count == 0 {
        return 0.5;
    }
    
    // Average weight normalized to 0-1 range
    let avg_weight = total_weight / edge_count as f64;
    avg_weight.min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_fingerprint_python() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.py");
        fs::write(&file_path, "print('hello')").unwrap();
        
        let result = fingerprint(&file_path).unwrap();
        assert_eq!(result.file_type, FileType::Python);
    }

    #[test]
    fn test_fingerprint_image() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.png");
        // PNG magic bytes
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        fs::write(&file_path, png_data).unwrap();
        
        let result = fingerprint(&file_path).unwrap();
        assert_eq!(result.file_type, FileType::Image);
        assert_eq!(result.confidence, 1.0);
    }
}
