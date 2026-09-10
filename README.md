# HCC - High Capacity Colorful

A semantic filesystem layer for Linux that groups files by meaning, not just location.

## What It Does

HCC presents a read-only semantic view of any directory. Instead of browsing by folder structure, you browse by:

- by-type/ - Files grouped by language (Python, Rust, Markdown, ...)
- by-color/ - Files grouped by meaning using Louvain community detection

## Current Status

| Feature | Status |
|---------|--------|
| File type detection (25+ types) | Working |
| Git metadata detection | Working |
| Louvain community detection | Working |
| FUSE read-only mount | Working |
| by-type navigation | Working |
| by-color navigation | Working |
| Semantic file reading | Working |
| 22 unit tests | Passing |
| Block deduplication | In development |
| Compression | In development |
| Background daemon | In development |

## Requirements

Install FUSE development libraries:

    sudo apt install libfuse3-dev pkg-config

## Build

    git clone https://github.com/LYcore1/HCC.git
    cd HCC
    cargo build --release

## Usage

Analyze a directory:

    ./target/release/hcc analyze ~/my-project

Mount the semantic view:

    mkdir -p /tmp/hcc-mount
    ./target/release/hcc mount ~/my-project /tmp/hcc-mount

Browse semantically:

    ls /tmp/hcc-mount/by-type/
    ls /tmp/hcc-mount/by-color/
    cat /tmp/hcc-mount/by-color/Src/main.rs

Unmount:

    ./target/release/hcc unmount /tmp/hcc-mount

## How It Works

1. Walk directory (classify hidden dirs like .git as GitMetadata)
2. Fingerprint each file (magic bytes + extension + filename)
3. Build weighted graph (nodes = files, edges = relationships)
4. Run Louvain community detection
5. Assign colors to communities
6. Present via FUSE as read-only semantic view

## Architecture

    src/
      main.rs          CLI
      lib.rs           auto_color pipeline
      color.rs         Color, ColorTable
      fingerprint.rs   File type detection (25+ types)
      graph.rs         File relationship graph
        graph/louvain.rs   Louvain community detection
      dedup.rs         BLAKE3 dedup (in development)
      daemon.rs        Background prefetch (in development)
      fuse/
        fs.rs          FUSE filesystem
        mod.rs
        ops.rs

## Supported File Types

Python, JavaScript, TypeScript, Rust, C, Cpp, Go, Java, Ruby, Php, SQL, HTML, CSS, JSON, YAML, XML, Shell, Markdown, Image, Binary, Config, Text, GitMetadata, Unknown.

## Limitations

- Read-only
- Heuristic clustering (by directory and name, not content)
- FUSE overhead (~20% slower than ext4)

## Roadmap

- [x] v0.1: FUSE mount and Louvain clustering
- [ ] v0.2: Integration tests
- [ ] v0.3: Content-based clustering
- [ ] v0.4: Deduplication and compression
- [ ] v0.5: Background daemon
- [ ] v1.0: Stable release

## License

MIT

## Acknowledgments

- Inspired by the Golgi apparatus in cell biology
- Uses graphrs, fuser, BLAKE3, walkdir
