//! Graph module - Builds file relationship graphs and detects communities

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::fingerprint::{fingerprint, FileType};

pub mod louvain;

/// A node in the file graph (represents one file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub path: PathBuf,
    pub file_type: FileType,
}

/// Source of an edge (relationship) between files
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EdgeSource {
    Directory,      // Files in same directory
    Prefix,         // Similar file names
    Import,         // One file imports another
    CoCommit,       // Files changed together in git
}

/// An edge in the file graph (represents a relationship)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub weight: u32,  // Use integer weights for graphrs compatibility
    pub source: EdgeSource,
}

/// The file relationship graph
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FileGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl FileGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, path: PathBuf, file_type: FileType) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(Node { path, file_type });
        idx
    }

    /// Add an edge to the graph
    pub fn add_edge(&mut self, from: usize, to: usize, weight: f64, source: EdgeSource) {
        // Convert f64 weight to u32 (scale by 1000 for precision)
        let int_weight = (weight * 1000.0) as u32;
        self.edges.push(Edge {
            from,
            to,
            weight: int_weight,
            source,
        });
    }

    /// Get the number of nodes
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the graph is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Build a file relationship graph from a root directory
pub fn build_graph(root: &Path) -> Result<FileGraph, Box<dyn std::error::Error>> {
    let mut graph = FileGraph::new();
    let mut path_to_idx: HashMap<PathBuf, usize> = HashMap::new();
    let mut files_by_dir: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    // Walk the directory tree
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path().to_path_buf();

        // Skip hidden files/directories EXCEPT .git (which we classify as GitMetadata)
        if path
            .components()
            .any(|c| {
                let s = c.as_os_str().to_string_lossy();
                s.starts_with('.') && s != ".git"
            })
        {
            continue;
        }

        // Skip directories (we only index files)
        if path.is_dir() {
            continue;
        }

        // Fingerprint the file
        let fp = match fingerprint(&path) {
            Ok(fp) => fp,
            Err(_) => continue,
        };

        // Skip unknown file types
        if fp.file_type == FileType::Unknown {
            continue;
        }

        // Add node to graph
        let idx = graph.add_node(path.clone(), fp.file_type);
        path_to_idx.insert(path.clone(), idx);

        // Group by directory for later edge creation
        if let Some(parent) = path.parent() {
            files_by_dir
                .entry(parent.to_path_buf())
                .or_default()
                .push(path);
        }
    }

    log::info!("Built graph with {} nodes", graph.len());

    // Create edges based on directory co-occurrence
    for (_dir, files) in files_by_dir {
        create_directory_edges(&mut graph, &files, &path_to_idx);
    }

    // Create edges based on name prefix similarity
    // DISABLED: O(n^2) explosion on large projects (Issue C)
    // create_prefix_edges(&mut graph, &path_to_idx);

    log::info!("Created {} edges", graph.edges.len());

    Ok(graph)
}

/// Create edges between files in the same directory
fn create_directory_edges(
    graph: &mut FileGraph,
    files: &[PathBuf],
    path_to_idx: &HashMap<PathBuf, usize>,
) {
    // Files in the same directory get a base weight
    let base_weight = 0.5;

    // Limit to first 100 files per directory to avoid O(n^2) explosion
    let limit = files.len().min(500);

    for i in 0..limit {
        for j in (i + 1)..limit {
            if let (Some(&idx1), Some(&idx2)) =
                (path_to_idx.get(&files[i]), path_to_idx.get(&files[j]))
            {
                graph.add_edge(idx1, idx2, base_weight, EdgeSource::Directory);
            }
        }
    }
}

/// Create edges between files with similar names
fn create_prefix_edges(graph: &mut FileGraph, path_to_idx: &HashMap<PathBuf, usize>) {
    let paths: Vec<&PathBuf> = path_to_idx.keys().take(1000).collect();

    for i in 0..paths.len() {
        for j in (i + 1)..paths.len() {
            let name1 = paths[i].file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let name2 = paths[j].file_stem().and_then(|s| s.to_str()).unwrap_or("");

            // Calculate prefix similarity (how many characters match from the start)
            let common_prefix_len = name1
                .chars()
                .zip(name2.chars())
                .take_while(|(a, b)| a == b)
                .count();

            if common_prefix_len > 0 {
                let max_len = name1.len().max(name2.len());
                let similarity = common_prefix_len as f64 / max_len as f64;

                // Only create edge if similarity is significant
                if similarity >= 0.3 {
                    if let (Some(&idx1), Some(&idx2)) =
                        (path_to_idx.get(paths[i]), path_to_idx.get(paths[j]))
                    {
                        let weight = similarity * 0.3; // Prefix contributes 30% max
                        graph.add_edge(idx1, idx2, weight, EdgeSource::Prefix);
                    }
                }
            }
        }
    }
}

/// Detect communities using a hybrid approach:
/// - Louvain for small projects (< 5000 files): semantic clustering
/// - Connected Components for large projects (>= 5000 files): fast clustering
pub fn detect_communities(graph: &FileGraph) -> Result<Vec<Vec<usize>>, Box<dyn std::error::Error>> {
    let node_count = graph.nodes.len();
    
    if node_count < 5000 {
        log::info!("Using Louvain for {} nodes (small project)", node_count);
        louvain::detect_communities_louvain(graph)
    } else {
        log::info!("Using connected components for {} nodes (large project)", node_count);
        louvain::detect_communities_fallback(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;


    #[test]
    fn test_graph_add_node() {
        let mut graph = FileGraph::new();
        let idx = graph.add_node(
            PathBuf::from("/test/file.py"),
            FileType::Python,
        );

        assert_eq!(idx, 0);
        assert_eq!(graph.len(), 1);
        assert_eq!(graph.nodes[0].file_type, FileType::Python);
    }

    #[test]
    fn test_graph_add_edge() {
        let mut graph = FileGraph::new();
        graph.add_node(PathBuf::from("/test/a.py"), FileType::Python);
        graph.add_node(PathBuf::from("/test/b.py"), FileType::Python);
        graph.add_edge(0, 1, 0.5, EdgeSource::Directory);

        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].weight, 500); // 0.5 * 1000
    }
}
