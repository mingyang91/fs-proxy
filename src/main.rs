mod args;
mod mapping;
mod inode;

use std::collections::{HashMap, LinkedList};
use clap::{Parser};
use fuser::{
  FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
  Request,
};
use libc::ENOENT;
use std::ffi::OsStr;
use std::ops::Add;
use std::os::unix::fs::MetadataExt;
use std::time;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crate::args::Args;
use tokio;
use crate::mapping::Path;

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
  mapping: Path,
}

impl MappingFS {
  fn new(mapping: Path) -> Self {
    Self { mapping }
  }

  async fn getattr(name: String) -> Result<FileAttr, std::io::Error> {
    let file = tokio::fs::File::open(name).await?;
    let metadata = file.metadata().await?;
    let kind = if metadata.is_dir() {
      FileType::Directory
    } else if metadata.is_file() {
      FileType::RegularFile
    } else if metadata.is_symlink() {
      FileType::Symlink
    } else {
      FileType::NamedPipe
    };

    let attr = FileAttr {
      ino: metadata.ino(),
      size: metadata.size(),
      blocks: metadata.blocks(),
      atime: UNIX_EPOCH.add(Duration::from_secs(metadata.atime() as u64)),
      mtime: UNIX_EPOCH.add(Duration::from_secs(metadata.mtime() as u64)),
      ctime: UNIX_EPOCH.add(Duration::from_secs(metadata.ctime() as u64)),
      crtime: UNIX_EPOCH,
      kind,
      perm: metadata.mode() as u16,
      nlink: metadata.nlink() as u32,
      uid: metadata.uid(),
      gid: metadata.gid(),
      rdev: metadata.rdev() as u32,
      blksize: metadata.blksize() as u32,
      flags: 0,
    };
    Ok(attr)
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
    let Some(filename) = name.to_str() else {
      reply.error(ENOENT);
      return;
    };
    let Path::Folder { paths } = &self.mapping else {
      reply.error(ENOENT);
      return;
    };
    let Some(destination) = paths.get(filename) else {
      reply.error(ENOENT);
      return;
    };
    match destination {
      Path::File { path } => {
        reply.entry(&TTL, &HELLO_TXT_ATTR, 0);
      }
      Path::Folder { paths } => {
        reply.entry(&TTL, &HELLO_DIR_ATTR, 0);
      }
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

    // for (i, entry) in self.mapping.into_iter().enumerate().skip(offset as usize) {
    //   // i + 1 means the index of the next entry
    //   let (name, dest) = entry;
    //   let kind = match dest {
    //     Path::File { path } => FileType::RegularFile,
    //     Path::Folder { paths } => FileType::Directory,
    //   };
    //   if reply.add(1, (i + 1) as i64, kind, name) {
    //     break;
    //   }
    // }
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