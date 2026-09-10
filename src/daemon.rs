//! HCC Daemon - Background worker for prefetching and color updates

use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use clap::Parser;

#[derive(Parser)]
struct Cli {
    /// Root directory to watch
    #[arg(short, long)]
    root: PathBuf,

    /// Color table file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Enable debug output
    #[arg(short, long)]
    debug: bool,
}

fn main() {
    env_logger::init();
    
    let cli = Cli::parse();
    
    log::info!("Starting HCC daemon");
    log::info!("Watching directory: {:?}", cli.root);
    
    if let Some(config) = &cli.config {
        log::info!("Using config file: {:?}", config);
    }
    
    // Load or create color table
    let color_table = Arc::new(RwLock::new(
        load_color_table(&cli.root, cli.config.as_ref())
    ));
    
    // Start file watcher
    let watcher_root = cli.root.clone();
    let watcher_table = Arc::clone(&color_table);
    
    std::thread::spawn(move || {
        watch_directory(watcher_root, watcher_table);
    });
    
    // Start prefetch listener
    let listener_table = Arc::clone(&color_table);
    
    std::thread::spawn(move || {
        listen_for_access(listener_table);
    });
    
    log::info!("Daemon started. Press Ctrl+C to stop.");
    
    // Keep running until interrupted
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
        
        // Periodic color re-assignment could happen here
        if let Err(e) = update_colors(&cli.root, &color_table) {
            log::warn!("Failed to update colors: {}", e);
        }
    }
}

fn load_color_table(root: &PathBuf, config: Option<&PathBuf>) -> hcc_core::ColorTable {
    if let Some(cfg_path) = config {
        if cfg_path.exists() {
            match hcc_core::ColorTable::load_from_file(cfg_path) {
                Ok(table) => {
                    log::info!("Loaded color table from {:?}", cfg_path);
                    return table;
                }
                Err(e) => {
                    log::warn!("Failed to load color table: {}", e);
                }
            }
        }
    }
    
    // Build new color table
    log::info!("Building new color table for {:?}", root);
    match hcc_core::auto_color(root) {
        Ok(table) => {
            log::info!("Built color table with {} files", table.len());
            table
        }
        Err(e) => {
            log::error!("Failed to build color table: {}", e);
            hcc_core::ColorTable::new()
        }
    }
}

fn watch_directory(root: PathBuf, table: Arc<RwLock<hcc_core::ColorTable>>) {
    use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::channel;
    
    let (tx, rx) = channel();
    
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                tx.send(event).ok();
            }
        },
        notify::Config::default(),
    ).expect("Failed to create watcher");
    
    watcher.watch(&root, RecursiveMode::Recursive)
        .expect("Failed to watch directory");
    
    log::info!("File watcher started for {:?}", root);
    
    for event in rx {
        match event.kind {
            EventKind::Create(_) => {
                log::debug!("File created: {:?}", event.paths);
                // Could trigger color re-assignment
            }
            EventKind::Modify(_) => {
                log::debug!("File modified: {:?}", event.paths);
                // Could trigger color re-assignment
            }
            EventKind::Remove(_) => {
                log::debug!("File removed: {:?}", event.paths);
                // Remove from color table
            }
            _ => {}
        }
    }
}

fn listen_for_access(table: Arc<RwLock<hcc_core::ColorTable>>) {
    log::info!("Prefetch listener started");
    
    // In a real implementation, this would:
    // 1. Listen for file access events (via inotify or similar)
    // 2. When a file is accessed, find its color
    // 3. Prefetch all related files into the page cache
    
    // This is a placeholder for the actual implementation
    loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        
        // Placeholder: just log that we're still running
        log::debug!("Prefetch listener active");
    }
}

fn update_colors(root: &PathBuf, table: &Arc<RwLock<hcc_core::ColorTable>>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Updating color assignments...");
    
    let new_table = hcc_core::auto_color(root)?;
    
    let mut current_table = table.write();
    *current_table = new_table;
    
    log::info!("Color assignments updated");
    Ok(())
}

/// Prefetch related files when a file is accessed
fn prefetch_related(file: &PathBuf, table: &hcc_core::ColorTable) {
    if let Some(color) = table.get(file) {
        if let Some(related_files) = table.get_files_by_color(color.id) {
            for related in related_files {
                if related != file {
                    // Read file into kernel page cache
                    // Using posix_fadvise would be more efficient
                    let _ = std::fs::read(related);
                    log::debug!("Prefetched: {:?}", related);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefetch_related() {
        let mut table = hcc_core::ColorTable::new();
        let color = hcc_core::Color::new(1, "Test".to_string(), 0.9);
        
        let file1 = PathBuf::from("/tmp/test1.py");
        let file2 = PathBuf::from("/tmp/test2.js");
        
        table.assign_color(file1.clone(), color.clone());
        table.assign_color(file2.clone(), color);
        
        // This would normally prefetch file2 when file1 is accessed
        prefetch_related(&file1, &table);
    }
}
