mod args;
mod mapping;

use std::collections::HashMap;
use clap::{Parser};
use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
    Request,
};
use libc::ENOENT;
use std::ffi::OsStr;
use std::time::{Duration, UNIX_EPOCH};
use crate::args::Args;
use tokio;
use crate::mapping::Destination;

const TTL: Duration = Duration::from_secs(1); // 1 second

const HELLO_DIR_ATTR: FileAttr = FileAttr {
    ino: 1,
    size: 0,
    blocks: 0,
    atime: UNIX_EPOCH, // 1970-01-01 00:00:00
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    crtime: UNIX_EPOCH,
    kind: FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 501,
    gid: 20,
    rdev: 0,
    flags: 0,
    blksize: 512,
};

const HELLO_TXT_CONTENT: &str = "Hello World!\n";

const HELLO_TXT_ATTR: FileAttr = FileAttr {
    ino: 2,
    size: 13,
    blocks: 1,
    atime: UNIX_EPOCH, // 1970-01-01 00:00:00
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    crtime: UNIX_EPOCH,
    kind: FileType::RegularFile,
    perm: 0o644,
    nlink: 1,
    uid: 501,
    gid: 20,
    rdev: 0,
    flags: 0,
    blksize: 512,
};

struct MappingFS {
    mapping: HashMap<String, Destination>,
}

impl MappingFS {
    fn new(mapping: HashMap<String, Destination>) -> Self {
        Self { mapping }
    }
}

impl From<mapping::MappingConfig> for MappingFS {
    fn from(mapping: mapping::MappingConfig) -> Self {
        let mut mapping_fs = Self::new(mapping.mapping);
        mapping_fs
    }
}

impl Filesystem for MappingFS {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        if parent == 1 && name.to_str() == Some("hello.txt") {
            reply.entry(&TTL, &HELLO_TXT_ATTR, 0);
        } else {
            reply.error(ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        match ino {
            1 => reply.attr(&TTL, &HELLO_DIR_ATTR),
            2 => reply.attr(&TTL, &HELLO_TXT_ATTR),
            _ => reply.error(ENOENT),
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        _size: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyData,
    ) {
        if ino == 2 {
            reply.data(&HELLO_TXT_CONTENT.as_bytes()[offset as usize..]);
        } else {
            reply.error(ENOENT);
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
        if ino != 1 {
            reply.error(ENOENT);
            return;
        }

        let entries = vec![
            (1, FileType::Directory, "."),
            (1, FileType::Directory, ".."),
            (2, FileType::RegularFile, "hello.txt"),
        ];

        for (i, entry) in self.mapping.iter().enumerate().skip(offset as usize) {
            // i + 1 means the index of the next entry
            let (name, dest) = entry;
            let kind = match dest {
                Destination::File { path } => FileType::RegularFile,
                Destination::Folder => FileType::Directory,
            };
            if reply.add(i as u64, (i + 1) as i64, kind, name) {
                break;
            }
        }
        reply.ok();
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();

    let mut options = vec![MountOption::RO, MountOption::FSName("hello".to_string())];
    if args.auto_unmount {
        options.push(MountOption::AutoUnmount);
    }
    if args.allow_root {
        options.push(MountOption::AllowRoot);
    }

    let mapping_file = tokio::fs::File::open(args.mapping_file).await.unwrap();
    let rdr = std::io::BufReader::new(mapping_file.into_std().await);
    let mut config: mapping::MappingConfig = serde_json::from_reader(rdr).unwrap();
    let mapping_fs = MappingFS::from(config);
    fuser::mount2(mapping_fs, args.mountpoint, &options).unwrap();
}