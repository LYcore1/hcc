//! FUSE operations helper functions

use std::path::PathBuf;
use crate::ColorTable;

/// Get files grouped by type
pub fn get_files_by_type(source: &PathBuf, _color_table: &ColorTable) -> std::collections::HashMap<String, Vec<PathBuf>> {
    use crate::fingerprint;
    
    let mut by_type: std::collections::HashMap<String, Vec<PathBuf>> = std::collections::HashMap::new();
    
    for entry in walkdir::WalkDir::new(source)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path().to_path_buf();
        if path.is_file() {
            if let Ok(fp) = fingerprint(&path) {
                let type_name = format!("{:?}", fp.file_type);
                by_type.entry(type_name).or_default().push(path);
            }
        }
    }
    
    by_type
}

/// Get files grouped by color
pub fn get_files_by_color(color_table: &ColorTable) -> std::collections::HashMap<String, Vec<PathBuf>> {
    let mut by_color: std::collections::HashMap<String, Vec<PathBuf>> = std::collections::HashMap::new();
    
    for color in color_table.all_colors() {
        if let Some(files) = color_table.get_files_by_color(color.id) {
            by_color.insert(color.name.clone(), files.clone());
        }
    }
    
    by_color
}
