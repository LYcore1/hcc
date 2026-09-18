//! Louvain Community Detection Algorithm
//! 
//! This module implements the Louvain algorithm for community detection
//! using the graphrs crate.

use std::collections::HashMap;
use graphrs::{Graph, GraphSpecs, Edge, Node};
use graphrs::algorithms::community::louvain;
use crate::graph::FileGraph;

/// Detect communities using the Louvain algorithm
pub fn detect_communities_louvain(graph: &FileGraph) -> Result<Vec<Vec<usize>>, Box<dyn std::error::Error>> {
    let n = graph.nodes.len();
    
    if n == 0 {
        return Ok(Vec::new());
    }
    
    if n == 1 {
        return Ok(vec![vec![0]]);
    }
    
    log::info!("Running Louvain algorithm on {} nodes and {} edges", n, graph.edges.len());
    
    // Build graph specs - use undirected graph with auto-create missing nodes
    let mut specs = GraphSpecs::undirected_create_missing();
    specs.edge_dedupe_strategy = graphrs::EdgeDedupeStrategy::KeepFirst;
    
    // Create nodes as Arc<Node>
    let nodes: Vec<std::sync::Arc<Node<String, f64>>> = (0..n)
        .map(|i| Node::from_name(i.to_string()))
        .collect();
    
    // Create edges as Arc<Edge>
    let edges: Vec<std::sync::Arc<Edge<String, f64>>> = graph.edges.iter()
        .filter_map(|e| {
            let weight = e.weight as f64 / 1000.0;
            if weight > 0.0 {
                Some(Edge::with_weight(
                    e.from.to_string(),
                    e.to.to_string(),
                    weight,
                ))
            } else {
                None
            }
        })
        .collect();
    
    log::info!("Created graph with {} nodes and {} edges for Louvain", nodes.len(), edges.len());
    
    // Build the graph
    let g_result = Graph::<String, f64>::new_from_nodes_and_edges(nodes, edges, specs);
    
    let g = match g_result {
        Ok(graph) => graph,
        Err(e) => {
            log::warn!("Failed to build graph: {}. Using fallback.", e);
            return detect_communities_fallback(graph);
        }
    };
    
    // Run Louvain algorithm with parameters
    let communities_result = louvain::louvain_communities(
        &g,
        true,       // weighted = true
        Some(0.5),  // resolution parameter
        None,       // default threshold
        Some(42)    // seed for reproducibility
    );
    
    match communities_result {
        Ok(communities) => {
            // Convert communities to Vec<Vec<usize>>
            // communities is Vec<HashSet<String>> based on graphrs API
            let result: Vec<Vec<usize>> = communities
                .into_iter()
                .filter_map(|community_set| {
                    let indices: Vec<usize> = community_set
                        .into_iter()
                        .filter_map(|name_str| name_str.parse::<usize>().ok())
                        .collect();
                    
                    if !indices.is_empty() {
                        Some(indices)
                    } else {
                        None
                    }
                })
                .collect();
            
            log::info!("Louvain detected {} communities", result.len());
            Ok(result)
        }
        Err(e) => {
            log::warn!("Louvain failed: {}. Using fallback algorithm.", e);
            detect_communities_fallback(graph)
        }
    }
}

/// Fallback community detection using connected components
pub fn detect_communities_fallback(graph: &FileGraph) -> Result<Vec<Vec<usize>>, Box<dyn std::error::Error>> {
    let n = graph.nodes.len();
    
    if n == 0 {
        return Ok(Vec::new());
    }
    
    // Union-Find data structure
    let mut parent: Vec<usize> = (0..n).collect();
    
    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }
    
    fn union(parent: &mut [usize], x: usize, y: usize) {
        let px = find(parent, x);
        let py = find(parent, y);
        if px != py {
            parent[px] = py;
        }
    }
    
    // Union nodes connected by edges
    for edge in &graph.edges {
        union(&mut parent, edge.from, edge.to);
    }
    
    // Group nodes by their root parent
    let mut communities: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        communities.entry(root).or_default().push(i);
    }
    
    let result: Vec<Vec<usize>> = communities.into_values().collect();
    log::info!("Fallback detected {} communities", result.len());
    
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{FileGraph, EdgeSource};
    use crate::fingerprint::FileType;
    use std::path::PathBuf;
    
    #[test]
    fn test_louvain_single_node() {
        let mut graph = FileGraph::new();
        graph.add_node(PathBuf::from("/test/file.py"), FileType::Python);
        
        let communities = detect_communities_louvain(&graph).unwrap();
        assert_eq!(communities.len(), 1);
        assert_eq!(communities[0].len(), 1);
    }
    
    #[test]
    fn test_louvain_two_connected() {
        let mut graph = FileGraph::new();
        graph.add_node(PathBuf::from("/test/a.py"), FileType::Python);
        graph.add_node(PathBuf::from("/test/b.py"), FileType::Python);
        graph.add_edge(0, 1, 0.8, EdgeSource::Directory);
        
        let communities = detect_communities_louvain(&graph).unwrap();
        assert_eq!(communities.len(), 1); // Should be in same community
        assert_eq!(communities[0].len(), 2);
    }
    
    #[test]
    fn test_louvain_two_disconnected() {
        let mut graph = FileGraph::new();
        graph.add_node(PathBuf::from("/test/a.py"), FileType::Python);
        graph.add_node(PathBuf::from("/other/b.py"), FileType::Python);
        // No edges between them
        
        let communities = detect_communities_louvain(&graph).unwrap();
        assert_eq!(communities.len(), 2); // Should be separate communities
    }
}
