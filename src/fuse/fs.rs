//! FUSE filesystem implementation for HCC
//!
//! This module presents a read-only semantic view of a source directory.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
    ReplyOpen, Request, FUSE_ROOT_ID,
};
use parking_lot::RwLock;

use crate::{fingerprint, ColorTable, DedupTable};

const TTL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct VirtualNode {
    pub vpath: PathBuf,
    pub real_path: Option<PathBuf>,
    pub is_dir: bool,
    pub parent: u64,
    pub name: String,
}

pub struct HccFs {
    pub source: PathBuf,
    pub color_table: Arc<RwLock<ColorTable>>,
    pub dedup: Arc<DedupTable>,
    pub inodes: RwLock<HashMap<u64, VirtualNode>>,
    pub vpath_to_ino: RwLock<HashMap<PathBuf, u64>>,
    pub next_ino: AtomicU64,
}

impl HccFs {
    pub fn new(source: PathBuf, color_table: ColorTable, dedup: DedupTable) -> Self {
        let mut inodes = HashMap::new();
        let mut vpath_to_ino = HashMap::new();

        inodes.insert(
            FUSE_ROOT_ID,
            VirtualNode {
                vpath: PathBuf::from("/"),
                real_path: None,
                is_dir: true,
                parent: FUSE_ROOT_ID,
                name: String::from(""),
            },
        );
        vpath_to_ino.insert(PathBuf::from("/"), FUSE_ROOT_ID);

        let fs = Self {
            source,
            color_table: Arc::new(RwLock::new(color_table)),
            dedup: Arc::new(dedup),
            inodes: RwLock::new(inodes),
            vpath_to_ino: RwLock::new(vpath_to_ino),
            next_ino: AtomicU64::new(FUSE_ROOT_ID + 1),
        };

        fs.register_dir("/by-type", "by-type", FUSE_ROOT_ID);
        fs.register_dir("/by-color", "by-color", FUSE_ROOT_ID);

        fs
    }

    fn alloc_inode(&self, node: VirtualNode) -> u64 {
        let ino = self.next_ino.fetch_add(1, Ordering::SeqCst);
        self.vpath_to_ino.write().insert(node.vpath.clone(), ino);
        self.inodes.write().insert(ino, node);
        ino
    }

    fn register_dir(&self, vpath: &str, name: &str, parent: u64) -> u64 {
        let vpath_buf = PathBuf::from(vpath);
        if let Some(&ino) = self.vpath_to_ino.read().get(&vpath_buf) {
            return ino;
        }
        let node = VirtualNode {
            vpath: vpath_buf,
            real_path: None,
            is_dir: true,
            parent,
            name: name.to_string(),
        };
        self.alloc_inode(node)
    }

    fn register_file(&self, vpath: PathBuf, real_path: PathBuf, name: &str, parent: u64) -> u64 {
        if let Some(&ino) = self.vpath_to_ino.read().get(&vpath) {
            return ino;
        }
        let node = VirtualNode {
            vpath,
            real_path: Some(real_path),
            is_dir: false,
            parent,
            name: name.to_string(),
        };
        self.alloc_inode(node)
    }

    fn make_attr(&self, ino: u64, node: &VirtualNode) -> FileAttr {
        if node.is_dir {
            FileAttr {
                ino,
                size: 4096,
                blocks: 8,
                atime: UNIX_EPOCH,
                mtime: UNIX_EPOCH,
                ctime: UNIX_EPOCH,
                crtime: UNIX_EPOCH,
                kind: FileType::Directory,
                perm: 0o555,
                nlink: 2,
                uid: 1000,
                gid: 1000,
                rdev: 0,
                flags: 0,
                blksize: 512,
            }
        } else if let Some(ref real) = node.real_path {
            match std::fs::metadata(real) {
                Ok(meta) => FileAttr {
                    ino,
                    size: meta.len(),
                    blocks: meta.len().div_ceil(512),
                    atime: meta.accessed().unwrap_or(UNIX_EPOCH),
                    mtime: meta.modified().unwrap_or(UNIX_EPOCH),
                    ctime: meta.created().unwrap_or(UNIX_EPOCH),
                    crtime: UNIX_EPOCH,
                    kind: FileType::RegularFile,
                    perm: 0o444,
                    nlink: 1,
                    uid: 1000,
                    gid: 1000,
                    rdev: 0,
                    flags: 0,
                    blksize: 512,
                },
                Err(_) => FileAttr {
                    ino,
                    size: 0,
                    blocks: 0,
                    atime: UNIX_EPOCH,
                    mtime: UNIX_EPOCH,
                    ctime: UNIX_EPOCH,
                    crtime: UNIX_EPOCH,
                    kind: FileType::RegularFile,
                    perm: 0o444,
                    nlink: 1,
                    uid: 1000,
                    gid: 1000,
                    rdev: 0,
                    flags: 0,
                    blksize: 512,
                },
            }
        } else {
            FileAttr {
                ino,
                size: 0,
                blocks: 0,
                atime: UNIX_EPOCH,
                mtime: UNIX_EPOCH,
                ctime: UNIX_EPOCH,
                crtime: UNIX_EPOCH,
                kind: FileType::RegularFile,
                perm: 0o444,
                nlink: 1,
                uid: 1000,
                gid: 1000,
                rdev: 0,
                flags: 0,
                blksize: 512,
            }
        }
    }

    fn list_dir(&self, ino: u64) -> Option<Vec<(String, bool, Option<PathBuf>)>> {
        let node = self.inodes.read().get(&ino).cloned()?;

        if node.vpath == PathBuf::from("/") {
            return Some(vec![
                (".".to_string(), true, None),
                ("..".to_string(), true, None),
                ("by-type".to_string(), true, None),
                ("by-color".to_string(), true, None),
            ]);
        }

        if node.vpath == PathBuf::from("/by-type") {
            let mut entries = vec![
                (".".to_string(), true, None),
                ("..".to_string(), true, None),
            ];
            let mut seen = HashSet::new();
            for t in self.detect_types() {
                if seen.insert(t.clone()) {
                    entries.push((t, true, None));
                }
            }
            return Some(entries);
        }

        if node.vpath.starts_with("/by-type") && node.vpath.components().count() == 3 {
            let type_name = node.name.clone();
            let mut entries = vec![
                (".".to_string(), true, None),
                ("..".to_string(), true, None),
            ];
            for (name, real) in self.files_of_type(&type_name) {
                entries.push((name, false, Some(real)));
            }
            return Some(entries);
        }

        if node.vpath == PathBuf::from("/by-color") {
            let mut entries = vec![
                (".".to_string(), true, None),
                ("..".to_string(), true, None),
            ];
            let table = self.color_table.read();
            let mut seen: HashSet<String> = HashSet::new();
            for color in table.all_colors() {
                if seen.insert(color.name.clone()) {
                    entries.push((color.name.clone(), true, None));
                }
            }
            return Some(entries);
        }

        if node.vpath.starts_with("/by-color") && node.vpath.components().count() == 3 {
            let color_name = node.name.clone();
            let mut entries = vec![
                (".".to_string(), true, None),
                ("..".to_string(), true, None),
            ];
            let table = self.color_table.read();
            for color in table.all_colors() {
                if color.name == color_name {
                    if let Some(files) = table.get_files_by_color(color.id) {
                        for real in files {
                            let name = real
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            entries.push((name, false, Some(real.clone())));
                        }
                    }
                    break;
                }
            }
            return Some(entries);
        }

        None
    }

    fn detect_types(&self) -> Vec<String> {
        let mut set = HashSet::new();
        for entry in walkdir::WalkDir::new(&self.source)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Ok(fp) = fingerprint(entry.path()) {
                    set.insert(format!("{:?}", fp.file_type));
                }
            }
        }
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort();
        v
    }

    fn files_of_type(&self, type_name: &str) -> Vec<(String, PathBuf)> {
        let mut out = Vec::new();
        for entry in walkdir::WalkDir::new(&self.source)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Ok(fp) = fingerprint(entry.path()) {
                    if format!("{:?}", fp.file_type) == type_name {
                        let name = entry
                            .path()
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        out.push((name, entry.path().to_path_buf()));
                    }
                }
            }
        }
        out
    }

    fn resolve_child(&self, parent_ino: u64, name: &str) -> Option<u64> {
        let parent = self.inodes.read().get(&parent_ino).cloned()?;

        let vpath = if parent.vpath == PathBuf::from("/") {
            PathBuf::from("/").join(name)
        } else {
            parent.vpath.join(name)
        };

        if let Some(&ino) = self.vpath_to_ino.read().get(&vpath) {
            return Some(ino);
        }

        if parent.vpath == PathBuf::from("/") {
            if name == "by-type" || name == "by-color" {
                let ino = self.register_dir(vpath.to_str()?, name, parent_ino);
                return Some(ino);
            }
            return None;
        }

        if parent.vpath == PathBuf::from("/by-type") {
            let types = self.detect_types();
            if types.iter().any(|t| t == name) {
                let ino = self.register_dir(vpath.to_str()?, name, parent_ino);
                return Some(ino);
            }
            return None;
        }

        if parent.vpath.starts_with("/by-type") && parent.vpath.components().count() == 3 {
            let files = self.files_of_type(&parent.name);
            for (fname, real) in files {
                if fname == name {
                    let ino = self.register_file(vpath.clone(), real, &fname, parent_ino);
                    return Some(ino);
                }
            }
            return None;
        }

        if parent.vpath == PathBuf::from("/by-color") {
            let table = self.color_table.read();
            for color in table.all_colors() {
                if color.name == name {
                    let ino = self.register_dir(vpath.to_str()?, name, parent_ino);
                    return Some(ino);
                }
            }
            return None;
        }

        if parent.vpath.starts_with("/by-color") && parent.vpath.components().count() == 3 {
            let table = self.color_table.read();
            for color in table.all_colors() {
                if color.name == parent.name {
                    if let Some(files) = table.get_files_by_color(color.id) {
                        for real in files {
                            let fname = real
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            if fname == name {
                                let ino = self.register_file(vpath.clone(), real.clone(), &fname, parent_ino);
                                return Some(ino);
                            }
                        }
                    }
                    break;
                }
            }
            return None;
        }

        None
    }
}

impl Filesystem for HccFs {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name_str = name.to_string_lossy().to_string();

        if name_str == "." {
            if let Some(node) = self.inodes.read().get(&parent) {
                let attr = self.make_attr(parent, node);
                reply.entry(&TTL, &attr, 0);
                return;
            }
            reply.error(libc::ENOENT);
            return;
        }
        if name_str == ".." {
            let parent_of_parent = self
                .inodes
                .read()
                .get(&parent)
                .map(|n| n.parent)
                .unwrap_or(FUSE_ROOT_ID);
            if let Some(node) = self.inodes.read().get(&parent_of_parent) {
                let attr = self.make_attr(parent_of_parent, node);
                reply.entry(&TTL, &attr, 0);
                return;
            }
            reply.error(libc::ENOENT);
            return;
        }

        match self.resolve_child(parent, &name_str) {
            Some(ino) => {
                let node = self.inodes.read().get(&ino).cloned();
                if let Some(n) = node {
                    let attr = self.make_attr(ino, &n);
                    reply.entry(&TTL, &attr, 0);
                } else {
                    reply.error(libc::ENOENT);
                }
            }
            None => reply.error(libc::ENOENT),
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let node = self.inodes.read().get(&ino).cloned();
        match node {
            Some(n) => {
                let attr = self.make_attr(ino, &n);
                reply.attr(&TTL, &attr);
            }
            None => reply.error(libc::ENOENT),
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let node = self.inodes.read().get(&ino).cloned();
        let node = match node {
            Some(n) => n,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        if node.is_dir {
            reply.error(libc::EISDIR);
            return;
        }

        let real = match node.real_path {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        match std::fs::read(&real) {
            Ok(data) => {
                let offset = offset as usize;
                if offset >= data.len() {
                    reply.data(&[]);
                } else {
                    let end = (offset + size as usize).min(data.len());
                    reply.data(&data[offset..end]);
                }
            }
            Err(_) => reply.error(libc::EIO),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let entries = match self.list_dir(ino) {
            Some(e) => e,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let mut idx = 0i64;
        for (name, is_dir, real) in entries {
            if idx < offset {
                idx += 1;
                continue;
            }

            let entry_ino = if name == "." {
                ino
            } else if name == ".." {
                self.inodes
                    .read()
                    .get(&ino)
                    .map(|n| n.parent)
                    .unwrap_or(FUSE_ROOT_ID)
            } else if let Some(real_path) = real {
                let parent_node = self.inodes.read().get(&ino).cloned().unwrap();
                let vpath = if parent_node.vpath == PathBuf::from("/") {
                    PathBuf::from("/").join(&name)
                } else {
                    parent_node.vpath.join(&name)
                };
                self.register_file(vpath, real_path, &name, ino)
            } else {
                match self.resolve_child(ino, &name) {
                    Some(i) => i,
                    None => continue,
                }
            };

            let kind = if is_dir {
                FileType::Directory
            } else {
                FileType::RegularFile
            };

            if reply.add(entry_ino, idx + 1, kind, &name) {
                break;
            }
            idx += 1;
        }
        reply.ok();
    }

    fn open(&mut self, _req: &Request, _ino: u64, _flags: i32, reply: ReplyOpen) {
        reply.opened(0, 0);
    }
}
