//! Color module - Defines color data structures and color table

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Unique identifier for a color
pub type ColorId = u32;

/// A semantic color label assigned to files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Color {
    pub id: ColorId,
    pub name: String,
    pub confidence: f64,
}

impl Color {
    pub fn new(id: ColorId, name: String, confidence: f64) -> Self {
        Self { id, name, confidence }
    }
    
    /// Generate a meaningful name based on files in the community
    pub fn generate_name(files: &[PathBuf], idx: usize) -> String {
        if files.is_empty() {
            return format!("Branch_{}", idx);
        }
        
        // Try to extract a meaningful name from file paths
        // Look for common directory names or file prefixes
        let mut dir_counts: HashMap<String, usize> = HashMap::new();
        let mut prefix_counts: HashMap<String, usize> = HashMap::new();
        
        for path in files {
            // Count directory names
            if let Some(parent) = path.parent() {
                if let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
                    *dir_counts.entry(dir_name.to_string()).or_insert(0) += 1;
                }
            }
            
            // Count file name prefixes (before extension)
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                // Use first 5 chars as prefix
                let prefix = stem.chars().take(5).collect::<String>();
                if !prefix.is_empty() {
                    *prefix_counts.entry(prefix).or_insert(0) += 1;
                }
            }
        }
        
        // Find most common directory name
        let best_dir = dir_counts.into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(name, _)| name);
        
        // Find most common prefix
        let best_prefix = prefix_counts.into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(name, _)| name);
        
        // Construct name
        if let Some(dir) = best_dir {
            // Capitalize first letter
            let mut chars = dir.chars();
            match chars.next() {
                None => format!("Branch_{}", idx),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        } else if let Some(prefix) = best_prefix {
            let mut chars = prefix.chars();
            match chars.next() {
                None => format!("Branch_{}", idx),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        } else {
            format!("Branch_{}", idx)
        }
    }
}

/// Maps files to colors and colors to files
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ColorTable {
    mapping: HashMap<PathBuf, Color>,
    reverse: HashMap<ColorId, Vec<PathBuf>>,
}

impl ColorTable {
    pub fn new() -> Self {
        Self {
            mapping: HashMap::new(),
            reverse: HashMap::new(),
        }
    }

    /// Assign a color to a file path
    pub fn assign_color(&mut self, path: PathBuf, color: Color) {
        let color_id = color.id;
        
        // Remove old color assignment if exists
        if let Some(old_color) = self.mapping.get(&path) {
            if let Some(files) = self.reverse.get_mut(&old_color.id) {
                files.retain(|p| p != &path);
            }
        }
        
        // Add to reverse mapping
        self.reverse
            .entry(color_id)
            .or_default()
            .push(path.clone());
        
        // Add to forward mapping
        self.mapping.insert(path, color);
    }

    /// Get the color for a file path
    pub fn get(&self, path: &PathBuf) -> Option<&Color> {
        self.mapping.get(path)
    }

    /// Get all files with a specific color
    pub fn get_files_by_color(&self, color_id: ColorId) -> Option<&Vec<PathBuf>> {
        self.reverse.get(&color_id)
    }

    /// Get all colors
    pub fn all_colors(&self) -> impl Iterator<Item = &Color> {
        self.mapping.values()
    }

    /// Get the number of files in the table
    pub fn len(&self) -> usize {
        self.mapping.len()
    }

    /// Check if the table is empty
    pub fn is_empty(&self) -> bool {
        self.mapping.is_empty()
    }

    /// Save the color table to a YAML file
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let yaml = serde_yaml::to_string(self)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    /// Load the color table from a YAML file
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let yaml = std::fs::read_to_string(path)?;
        let table: Self = serde_yaml::from_str(&yaml)?;
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_creation() {
        let color = Color::new(1, "Payment".to_string(), 0.9);
        assert_eq!(color.id, 1);
        assert_eq!(color.name, "Payment");
        assert_eq!(color.confidence, 0.9);
    }

    #[test]
    fn test_color_table_assignment() {
        let mut table = ColorTable::new();
        let color = Color::new(1, "Auth".to_string(), 0.8);
        
        table.assign_color(PathBuf::from("/src/auth.py"), color.clone());
        
        assert_eq!(table.len(), 1);
        assert!(table.get(&PathBuf::from("/src/auth.py")).is_some());
        assert_eq!(table.get_files_by_color(1).unwrap().len(), 1);
    }

    #[test]
    fn test_color_table_reassignment() {
        let mut table = ColorTable::new();
        let color1 = Color::new(1, "Auth".to_string(), 0.8);
        let color2 = Color::new(2, "Payment".to_string(), 0.9);
        
        table.assign_color(PathBuf::from("/src/file.py"), color1);
        table.assign_color(PathBuf::from("/src/file.py"), color2);
        
        // Should only have one entry
        assert_eq!(table.len(), 1);
        assert_eq!(table.get(&PathBuf::from("/src/file.py")).unwrap().id, 2);
    }
}
