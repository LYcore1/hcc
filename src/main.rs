//! HCC - High Capacity Colorful Semantic File System
//! 
//! Main CLI entry point for mounting and managing HCC file systems.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "hcc")]
#[command(author = "HCC Team")]
#[command(version = "0.1.0")]
#[command(about = "High Capacity Colorful Semantic File System", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Mount a directory with HCC semantic layer
    Mount {
        /// Source directory to mount
        source: PathBuf,

        /// Mount point for the FUSE filesystem
        mount_point: PathBuf,

        /// Enable debug output
        #[arg(short, long)]
        debug: bool,
    },

    /// Unmount an HCC filesystem
    Unmount {
        /// Mount point to unmount
        mount_point: PathBuf,
    },

    /// Analyze a directory and show color assignments
    Analyze {
        /// Directory to analyze
        path: PathBuf,

        /// Output format (text, json, yaml)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Show deduplication statistics
    Stats {
        /// HCC storage directory
        path: PathBuf,
    },

    /// Run the background daemon for prefetching
    Daemon {
        /// Root directory to watch
        root: PathBuf,

        /// Color table file
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
}

fn main() {
    env_logger::init();
    
    let cli = Cli::parse();

    match &cli.command {
        Commands::Mount { source, mount_point, debug: _ } => {
            log::info!("Mounting {:?} at {:?}", source, mount_point);
            
            // Build color table
            let table = match hcc_core::auto_color(source) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Error building color table: {}", e);
                    std::process::exit(1);
                }
            };
            
            // Create dedup table
            let dedup = hcc_core::DedupTable::new(source.join(".hcc/blocks"));
            
            // Create FUSE filesystem
            #[cfg(feature = "fuse")]
            {
                use fuser::Session;
                
                let fs = hcc_core::HccFs::new(source.clone(), table, dedup);
                
                let mut session = Session::new(fs, mount_point, &[]).expect("Failed to create FUSE session");
                
                println!("Mounted {} at {}", source.display(), mount_point.display());
                println!("Use 'hcc unmount {}' to unmount.", mount_point.display());
                
                session.run().expect("FUSE session failed");
            }
            
            #[cfg(not(feature = "fuse"))]
            {
                eprintln!("FUSE support not enabled. Rebuild with --features fuse");
                std::process::exit(1);
            }
        }

        Commands::Unmount { mount_point } => {
            println!("Unmounting {:?}", mount_point);
            
            unmount_filesystem(mount_point);
        }

        Commands::Analyze { path, format } => {
            println!("Analyzing directory: {:?}", path);
            
            match analyze_directory(path, format) {
                Ok(_) => println!("Analysis complete"),
                Err(e) => eprintln!("Error analyzing directory: {}", e),
            }
        }

        Commands::Stats { path } => {
            println!("Getting stats for: {:?}", path);
            
            show_stats(path);
        }

        Commands::Daemon { root, config } => {
            println!("Starting daemon for: {:?}", root);
            if let Some(cfg) = config {
                println!("Using config: {:?}", cfg);
            }
            
            run_daemon(root);
        }
    }
}

fn unmount_filesystem(mount_point: &PathBuf) {
    // Use fuser to unmount
    let status = std::process::Command::new("fusermount")
        .arg("-u")
        .arg(mount_point)
        .status();

    match status {
        Ok(exit_status) => {
            if exit_status.success() {
                println!("Successfully unmounted {:?}", mount_point);
            } else {
                eprintln!("Failed to unmount: {:?}", mount_point);
            }
        }
        Err(e) => {
            eprintln!("Error running fusermount: {}", e);
            // Try alternative unmount method
            let _ = std::process::Command::new("umount")
                .arg(mount_point)
                .status();
        }
    }
}

fn analyze_directory(path: &PathBuf, format: &str) -> Result<(), Box<dyn std::error::Error>> {
    use hcc_core::{auto_color, fingerprint};
    
    log::info!("Building color table for {:?}", path);
    let table = auto_color(path)?;
    
    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&table)?;
            println!("{}", json);
        }
        "yaml" => {
            let yaml = serde_yaml::to_string(&table)?;
            println!("{}", yaml);
        }
        _ => {
            // Text format
            println!("\n=== HCC Color Analysis ===\n");
            println!("Total files analyzed: {}", table.len());
            println!("\nColors assigned:");
            
            let mut colors_seen = std::collections::HashSet::new();
            for color in table.all_colors() {
                if !colors_seen.contains(&color.id) {
                    colors_seen.insert(color.id);
                    println!("  Color {} ({}): confidence={:.2}", 
                             color.id, color.name, color.confidence);
                    
                    if let Some(files) = table.get_files_by_color(color.id) {
                        for file in files {
                            if let Ok(fp) = fingerprint(file) {
                                println!("    - {:?} ({})", file, fp.file_type);
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}

fn show_stats(path: &PathBuf) {
    use hcc_core::BLOCK_SIZE;
    
    // Check for .hcc/blocks directory
    let blocks_dir = path.join(".hcc").join("blocks");
    
    if !blocks_dir.exists() {
        println!("\n=== HCC Dedup Statistics ===\n");
        println!("No HCC dedup data found at {:?}", path);
        
        // Count files anyway
        let mut file_count = 0;
        let mut total_size = 0u64;
        
        for entry in walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| !e.path().starts_with(path.join(".hcc")))
            .filter(|e| e.file_type().is_file())
        {
            file_count += 1;
            if let Ok(meta) = entry.metadata() {
                total_size += meta.len();
            }
        }
        
        println!("Total files:        {}", file_count);
        println!("Total size:         {} bytes", total_size);
        println!("Dedup size:         {} bytes (no dedup yet)", total_size);
        println!("Space saved:        0 bytes (0.0%)");
        println!("Unique blocks:      0");
        println!("Block size:         {} bytes", BLOCK_SIZE);
        return;
    }
    
    // Count blocks and calculate statistics
    let mut total_blocks = 0;
    let mut stored_size = 0u64;
    
    for entry in walkdir::WalkDir::new(&blocks_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        total_blocks += 1;
        if let Ok(meta) = entry.metadata() {
            stored_size += meta.len();
        }
    }
    
    // Count files in source (excluding .hcc)
    let mut file_count = 0;
    let mut original_size = 0u64;
    
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !e.path().starts_with(path.join(".hcc")))
        .filter(|e| e.file_type().is_file())
    {
        file_count += 1;
        if let Ok(meta) = entry.metadata() {
            original_size += meta.len();
        }
    }
    
    let savings = if original_size > 0 {
        ((original_size as f64 - stored_size as f64) / original_size as f64) * 100.0
    } else {
        0.0
    };
    
    println!("\n=== HCC Dedup Statistics ===\n");
    println!("Total files:        {}", file_count);
    println!("Total size:         {} bytes", original_size);
    println!("Dedup size:         {} bytes", stored_size);
    println!("Space saved:        {:.0} bytes ({:.1}%)", original_size as f64 - stored_size as f64, savings);
    println!("Unique blocks:      {}", total_blocks);
    println!("Block size:         {} bytes", BLOCK_SIZE);
}

fn run_daemon(root: &PathBuf) {
    use std::sync::Arc;
    use parking_lot::RwLock;
    use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::channel;
    
    println!("Starting HCC daemon...");
    println!("Watching directory: {:?}\n", root);
    
    // Load or create color table
    let color_table = Arc::new(RwLock::new(
        match hcc_core::auto_color(root) {
            Ok(table) => {
                println!("Built color table with {} files", table.len());
                table
            }
            Err(e) => {
                eprintln!("Failed to build color table: {}", e);
                hcc_core::ColorTable::new()
            }
        }
    ));
    
    // Set up file watcher
    let (tx, rx) = channel();
    let mut watcher = match RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                tx.send(event).ok();
            }
        },
        notify::Config::default(),
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create watcher: {}", e);
            return;
        }
    };
    
    if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
        eprintln!("Failed to watch directory: {}", e);
        return;
    }
    
    println!("Daemon started. Watching {:?}\n", root);
    println!("Press Ctrl+C to stop.\n");
    
    let table_clone = Arc::clone(&color_table);
    
    // Handle events
    for event in rx {
        match event.kind {
            EventKind::Access(_) => {
                // File was accessed - trigger prefetch
                for path in &event.paths {
                    if let Some(color) = table_clone.read().get(path) {
                        let color_id = color.id;
                        let color_name = color.name.clone();
                        
                        if let Some(related) = table_clone.read().get_files_by_color(color_id) {
                            let mut prefetched = 0;
                            for related_path in related.iter() {
                                if related_path != path && related_path.is_file() {
                                    // Prefetch by reading into page cache
                                    let _ = std::fs::read(related_path);
                                    prefetched += 1;
                                    if prefetched >= 10 {
                                        break; // Limit prefetch
                                    }
                                }
                            }
                            
                            if prefetched > 0 {
                                println!("Prefetched {} files for color {}", prefetched, color_name);
                            }
                        }
                    }
                }
            }
            EventKind::Modify(_) => {
                log::debug!("File modified: {:?}", event.paths);
            }
            EventKind::Create(_) => {
                log::debug!("File created: {:?}", event.paths);
            }
            EventKind::Remove(_) => {
                log::debug!("File removed: {:?}", event.paths);
            }
            _ => {}
        }
    }
}
