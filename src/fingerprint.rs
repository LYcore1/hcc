//! Fingerprint module - Detects file types by reading magic bytes and extensions

use std::path::Path;
use serde::{Deserialize, Serialize};

/// Supported file types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FileType {
    Python,
    JavaScript,
    TypeScript,
    Rust,
    C,
    Cpp,
    Go,
    Java,
    Ruby,
    Php,
    SQL,
    HTML,
    CSS,
    JSON,
    YAML,
    Xml,
    Shell,
    Markdown,
    Image,
    Binary,
    Config,
    Text,
    Data,
    Archive,
    GitMetadata,
    Unknown,
}

impl std::fmt::Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileType::Python => write!(f, "Python"),
            FileType::JavaScript => write!(f, "JavaScript"),
            FileType::TypeScript => write!(f, "TypeScript"),
            FileType::Rust => write!(f, "Rust"),
            FileType::C => write!(f, "C"),
            FileType::Cpp => write!(f, "Cpp"),
            FileType::Go => write!(f, "Go"),
            FileType::Java => write!(f, "Java"),
            FileType::Ruby => write!(f, "Ruby"),
            FileType::Php => write!(f, "Php"),
            FileType::SQL => write!(f, "SQL"),
            FileType::HTML => write!(f, "HTML"),
            FileType::CSS => write!(f, "CSS"),
            FileType::JSON => write!(f, "JSON"),
            FileType::YAML => write!(f, "YAML"),
            FileType::Xml => write!(f, "Xml"),
            FileType::Shell => write!(f, "Shell"),
            FileType::Markdown => write!(f, "Markdown"),
            FileType::Image => write!(f, "Image"),
            FileType::Binary => write!(f, "Binary"),
            FileType::Config => write!(f, "Config"),
            FileType::Text => write!(f, "Text"),
            FileType::Data => write!(f, "Data"),
            FileType::Archive => write!(f, "Archive"),
            FileType::GitMetadata => write!(f, "GitMetadata"),
            FileType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Result of fingerprinting a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    pub file_type: FileType,
    pub confidence: f64,
    pub extension: Option<String>,
}

/// Detect file type by reading the first 256 bytes
pub fn fingerprint(path: &Path) -> Result<Fingerprint, Box<dyn std::error::Error>> {
    // Check if this is a git metadata file
    if path
        .components()
        .any(|c| c.as_os_str().to_string_lossy() == ".git")
    {
        return Ok(Fingerprint {
            file_type: FileType::GitMetadata,
            confidence: 1.0,
            extension: None,
        });
    }
    
    let metadata = std::fs::metadata(path)?;
    
    // Check if it's a directory
    if metadata.is_dir() {
        return Ok(Fingerprint {
            file_type: FileType::Unknown,
            confidence: 0.0,
            extension: None,
        });
    }

    // Read first 256 bytes for magic number detection
    let mut file = std::fs::File::open(path)?;
    let mut buffer = [0u8; 256];
    let bytes_read = std::io::Read::read(&mut file, &mut buffer)?;
    let data = &buffer[..bytes_read];

    // Step 1: Check magic bytes for images and binaries
    if let Some(file_type) = check_magic_bytes(data) {
        return Ok(Fingerprint {
            file_type,
            confidence: 1.0,
            extension: path.extension().map(|e| e.to_string_lossy().to_string()),
        });
    }

    // Step 2: Check file extension
    if let Some(ext) = path.extension() {
        let ext_str = ext.to_string_lossy().to_lowercase();
        if let Some(file_type) = check_extension(&ext_str) {
            return Ok(Fingerprint {
                file_type,
                confidence: 0.8,
                extension: Some(ext_str),
            });
        }
    }

    // Step 2.5: Check filename (for files without extension)
    if path.extension().is_none() {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(file_type) = check_filename(name) {
                return Ok(Fingerprint {
                    file_type,
                    confidence: 0.8,
                    extension: None,
                });
            }
        }
    }

    // Step 3: Default to unknown
    Ok(Fingerprint {
        file_type: FileType::Unknown,
        confidence: 0.1,
        extension: path.extension().map(|e| e.to_string_lossy().to_string()),
    })
}

/// Check magic bytes for common file formats
fn check_magic_bytes(data: &[u8]) -> Option<FileType> {
    if data.len() < 8 {
        return None;
    }

    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(FileType::Image);
    }

    // JPEG: FF D8 FF
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(FileType::Image);
    }

    // GIF: 47 49 46 38
    if data.starts_with(&[0x47, 0x49, 0x46, 0x38]) {
        return Some(FileType::Image);
    }

    // PDF: 25 50 44 46
    if data.starts_with(&[0x25, 0x50, 0x44, 0x46]) {
        return Some(FileType::Binary);
    }

    // ZIP/JAR: 50 4B 03 04
    if data.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        return Some(FileType::Binary);
    }

    // ELF binary: 7F 45 4C 46
    if data.starts_with(&[0x7F, 0x45, 0x4C, 0x46]) {
        return Some(FileType::Binary);
    }

    None
}

/// Check file extension for text-based formats
fn check_extension(ext: &str) -> Option<FileType> {
    match ext {
        "py" => Some(FileType::Python),
        "js" | "mjs" | "cjs" => Some(FileType::JavaScript),
        "ts" | "tsx" => Some(FileType::TypeScript),
        "rs" => Some(FileType::Rust),
        "c" | "h" => Some(FileType::C),
        "cpp" | "cc" | "cxx" | "hpp" => Some(FileType::Cpp),
        "go" => Some(FileType::Go),
        "java" => Some(FileType::Java),
        "rb" => Some(FileType::Ruby),
        "php" => Some(FileType::Php),
        "sql" => Some(FileType::SQL),
        "html" | "htm" => Some(FileType::HTML),
        "css" | "scss" | "sass" => Some(FileType::CSS),
        "json" => Some(FileType::JSON),
        "gitignore" | "gitattributes" | "dockerignore" => Some(FileType::Config),
        "yaml" | "yml" => Some(FileType::YAML),
        "xml" => Some(FileType::Xml),
        "sh" | "bash" | "zsh" | "fish" | "awk" | "sed" => Some(FileType::Shell),
        "md" | "markdown" => Some(FileType::Markdown),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" | "ico" => Some(FileType::Image),
        "exe" | "dll" | "so" | "bin" | "o" | "a" => Some(FileType::Binary),
        // Programming languages (all map to C-like for now)
        "swift" | "scala" | "sml" | "tcl" | "tf" | "sol" | "typ" | "r" | "re" | "pp" | "pl" | "pm"
        | "pas" | "odin" | "nim" | "nims" | "lua" | "lisp" | "el" | "ml" | "mli" | "mm"
        | "m" | "matlab" | "rkt" | "purs" | "rego" | "proto" | "qml" | "v" | "sv" | "svh"
        | "vhd" | "vhdl" | "zig" | "dart" | "cbl" | "cob" | "hs" | "lhs" => Some(FileType::C),
        // Web
        "svelte" | "sls" | "slnx" | "vue" => Some(FileType::JavaScript),
        // Docs and text
        "rst" | "tex" | "textile" | "org" | "mediawiki" | "ndjson" | "list" | "less"
        | "sum" | "tab" | "targets" | "strace" | "syslog" | "meminfo" => Some(FileType::Text),
        // Config and data
        "gitkeep" | "gitattributes" | "gitconfig" | "props" | "namelist" | "ninja"
        | "nsi" | "nse" | "mod" | "orig~" | "old" | "rpmsave" | "rpmorig" | "rpmnew"
        | "ucf-new" | "ucf-dist" | "ucf-old" | "sublime-syntax" | "tmTheme" => Some(FileType::Config),
        // Stylesheets
        "styl" | "slim" => Some(FileType::CSS),
        // Backup
        "rs~" => Some(FileType::Rust),
        "orig" | "bak" | "old~" => Some(FileType::Text),
        // Misc
        "ls" | "ll" => Some(FileType::Text),
        "robot" => Some(FileType::Text),
        // Programming languages
        "swift" | "scala" | "sml" | "tcl" | "tf" | "sol" | "typ" => Some(FileType::C),
        "svelte" | "sls" | "slnx" => Some(FileType::JavaScript),
        "rst" | "tex" | "textile" => Some(FileType::Markdown),
        "sv" | "svh" => Some(FileType::C),  // SystemVerilog
        // Config and data
        "gitkeep" | "gitattributes" | "gitconfig" => Some(FileType::Config),
        "syslog" | "sum" | "tab" | "targets" | "strace" => Some(FileType::Text),
        "ucf-new" | "ucf-dist" | "ucf-old" => Some(FileType::Config),
        "rpmsave" | "rpmorig" | "rpmnew" => Some(FileType::Config),
        // Stylesheets
        "styl" | "slim" => Some(FileType::CSS),
        // Backup files
        "rs~" => Some(FileType::Rust),
        // Misc
        "tmTheme" => Some(FileType::Config),
        "sublime-syntax" => Some(FileType::Config),
        // Syntax highlighting and editor files
        "sublime-syntax" | "tmTheme" | "tmLanguage" | "tmPreferences" => Some(FileType::Config),
        "vim" | "vimrc" | "el" => Some(FileType::Config),
        // Patch and diff files
        "patch" | "diff" | "rej" | "orig" => Some(FileType::Text),
        // Man pages
        "man" | "in" => Some(FileType::Text),
        // Data files
        "csv" | "tsv" => Some(FileType::Data),
        // Version control
        "gitkeep" | "gitconfig" | "gitattributes" | "gitignore" | "gitmodules" => Some(FileType::Config),
        // Shell scripts
        "bat" | "cmd" | "ps1" | "psm1" => Some(FileType::Shell),
        // Programming languages
        "dart" => Some(FileType::C),  // Dart uses C-like syntax
        "cbl" | "cob" => Some(FileType::C),  // COBOL
        "nix" => Some(FileType::Config),  // Nix
        "hs" | "lhs" => Some(FileType::Text),  // Haskell
        "zig" => Some(FileType::C),  // Zig
        "wgsl" => Some(FileType::Text),  // WebGPU Shading Language
        "v" | "vhdl" | "vhd" => Some(FileType::Text),  // Verilog/VHDL
        "vy" | "varlink" => Some(FileType::Text),
        // Web
        "vue" => Some(FileType::JavaScript),  // Vue
        "xaml" => Some(FileType::Xml),  // XAML
        // Misc
        "dot" => Some(FileType::Text),  // Graphviz
        "suffix" => Some(FileType::Text),
        "ucf-old" => Some(FileType::Text),
        "wrong_ext" => Some(FileType::Text),
        "unknown~" => Some(FileType::Text),
        "cpuinfo" => Some(FileType::Text),
        "toml" | "ini" | "conf" | "cfg" | "lock" => Some(FileType::Config),
        "txt" | "log" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => Some(FileType::Text),
        _ => None,
    }
}

/// Detect file type by file name (for files without extension)
fn check_filename(name: &str) -> Option<FileType> {
    match name {
        "Makefile" | "makefile" | "GNUmakefile" => Some(FileType::Config),
        "Dockerfile" | "dockerfile" => Some(FileType::Config),
        "LICENSE" | "LICENSE-MIT" | "LICENSE-APACHE" | "COPYING" => Some(FileType::Text),
        "README" | "CHANGELOG" | "CONTRIBUTING" | "AUTHORS" => Some(FileType::Markdown),
        "CMakeLists.txt" => Some(FileType::Config),
        "_fd" | "_fdfind" | "_fd.zsh" | "_fdfind.zsh" => Some(FileType::Shell),
        "Gemfile" | "Rakefile" => Some(FileType::Ruby),
        "go.mod" | "go.sum" => Some(FileType::Config),
        "requirements.txt" | "setup.py" | "pyproject.toml" => Some(FileType::Config),
        "CMakeCache.txt" => Some(FileType::Config),
        "Vagrantfile" => Some(FileType::Ruby),
        "UNLICENSE" | "UNLICENCE" | "NOTICE" => Some(FileType::Text),
        "SETUP" | "INSTALL" => Some(FileType::Text),
        "benchsuite" | "copy-examples" | "configure" | "bootstrap" => Some(FileType::Shell),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_fingerprint_python_by_extension() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.py");
        fs::write(&file_path, "print('hello')").unwrap();

        let result = fingerprint(&file_path).unwrap();
        assert_eq!(result.file_type, FileType::Python);
        assert_eq!(result.confidence, 0.8);
    }

    #[test]
    fn test_fingerprint_png_by_magic() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.png");
        // PNG magic bytes
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        fs::write(&file_path, png_data).unwrap();

        let result = fingerprint(&file_path).unwrap();
        assert_eq!(result.file_type, FileType::Image);
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_fingerprint_jpeg_by_magic() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.jpg");
        // JPEG magic bytes: FF D8 FF E0 (need at least 4 bytes)
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
        fs::write(&file_path, jpeg_data).unwrap();

        let result = fingerprint(&file_path).unwrap();
        assert_eq!(result.file_type, FileType::Image);
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_fingerprint_javascript() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("app.js");
        fs::write(&file_path, "console.log('hello')").unwrap();

        let result = fingerprint(&file_path).unwrap();
        assert_eq!(result.file_type, FileType::JavaScript);
    }

    #[test]
    fn test_fingerprint_unknown() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("file.xyz");
        fs::write(&file_path, "unknown content").unwrap();

        let result = fingerprint(&file_path).unwrap();
        assert_eq!(result.file_type, FileType::Unknown);
        assert_eq!(result.confidence, 0.1);
    }
}
