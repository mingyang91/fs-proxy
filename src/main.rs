mod args;
mod mapping;
mod inode;

use std::cell::RefCell;
use clap::{Parser};
use fuser::{
  FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
  Request,
};
use libc::{EIO, ENOENT};
use std::ffi::OsStr;
use std::io::Error;
use std::ops::{Add, Deref};
use std::os::unix::fs::MetadataExt;
use std::rc::Rc;
use std::time::{Duration, UNIX_EPOCH};
use crate::args::Args;
use tokio;
use tokio::runtime::{Runtime};
use log::{debug, error};
use crate::inode::{INode, INodeOps, INodeTable};
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
  runtime: Runtime,
  inode_table: INodeTable,
}

impl MappingFS {
  fn new(runtime: Runtime, mapping: Path) -> Self {
    let root: Rc<RefCell<INode>> = Rc::new(RefCell::new(mapping.into()));
    Self {
      runtime,
      inode_table: INodeTable::from(root),
    }
  }

  async fn getattr(name: &String) -> Result<FileAttr, std::io::Error> {
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

  fn getattr_sync(&self, name: &String) -> Result<FileAttr, Error> {
    self.runtime.block_on(Self::getattr(name))
  }
}

impl Filesystem for MappingFS {
  fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
    debug!("lookup called with parent={}, name={:?}", parent, name);
    let Some(filename) = name.to_str() else {
      reply.error(ENOENT);
      return;
    };
    let res = self.inode_table.lookup(parent, filename.to_string());
    match res {
      Some(inode) => {
        match inode.borrow().deref() {
          INode::File { target, .. } => {
            debug!("lookup: found file {} -> {}", filename, target);
            match self.getattr_sync(target) {
              Ok(attr) => {
                reply.entry(&TTL, &attr, 0);
              }
              Err(err) => {
                error!("Failed to get attr for {}: {}", target, err);
                reply.error(EIO)
              }
            }

          }
          INode::Folder { .. } => {
            debug!("lookup: found folder {}", filename);
            reply.entry(&TTL, &HELLO_DIR_ATTR, 0);
          }
        }
      }
      None => {
        reply.error(ENOENT);
      }
    }
  }

  fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
    let Some(inode) = self.inode_table.get_by_ino(ino) else {
      reply.error(ENOENT);
      return;
    };

    match inode.borrow().deref() {
      INode::File { target, .. } => {
        match self.getattr_sync(target) {
          Ok(attr) => {
            reply.attr(&TTL, &attr);
          }
          Err(err) => {
            error!("Failed to get attr for {}: {}", target, err);
            reply.error(EIO)
          }
        }
      }
      INode::Folder { .. } => {
        reply.attr(&TTL, &HELLO_DIR_ATTR);
      }
    };
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
    debug!("readdir(ino={}, offset={})", ino, offset);
    let Some(inode) = self.inode_table.get_by_ino(ino) else {
      debug!("readdir(ino={}): ENOENT", ino);
      reply.error(ENOENT);
      return;
    };

    debug!("readdir(ino={}): {:?}", ino, inode.borrow().get_name());

    for (i, entry) in inode.list_current().iter().enumerate().skip(offset as usize) {
      let file = entry.borrow();
      let ret = match file.deref() {
        INode::File { name, .. } => {
          debug!("file[{}]: {:?}", i, entry.borrow().get_name());
          reply.add(file.get_ino(), (i + 1) as i64, FileType::RegularFile, name)
        },
        INode::Folder { name, .. } => {
          debug!("folder[{}]: {:?}", i, entry.borrow().get_name());
          reply.add(file.get_ino(), (i + 1) as i64, FileType::Directory, name)
        },
      };

      if ret {
        break;
      }
    }

    reply.ok();
  }
}

fn main() {
  env_logger::init();
  let args = Args::parse();

  let mut options = vec![MountOption::RO, MountOption::FSName("hello".to_string())];
  if args.auto_unmount {
    options.push(MountOption::AutoUnmount);
  }
  if args.allow_root {
    options.push(MountOption::AllowRoot);
  }

  let config = read_mapping_file(&args);
  let mapping_fs = MappingFS::new(Runtime::new().unwrap(), config);
  fuser::mount2(mapping_fs, args.mountpoint, &options).unwrap();
}

fn read_mapping_file(args: &Args) -> Path {
  let mapping_file = std::fs::File::open(&args.mapping_file).unwrap();
  let rdr = std::io::BufReader::new(mapping_file);
  serde_json::from_reader(rdr).unwrap()
}