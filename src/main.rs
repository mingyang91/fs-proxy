mod args;
mod mapping;
mod inode;

use std::cell::RefCell;
use clap::{Parser};
use fuser::{FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request, ReplyOpen};
use libc::{EIO, EISDIR, ENOENT};
use std::ffi::OsStr;
use std::io::{Error, SeekFrom};
use std::ops::{Add, Deref};
use std::os::unix::fs::MetadataExt;
use std::rc::Rc;
use std::time::{Duration, UNIX_EPOCH};
use crate::args::Args;
use tokio;
use tokio::io::{AsyncSeekExt, AsyncReadExt};
use tokio::runtime::{Runtime};
use log::{debug, error, info};
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

  async fn getattr(name: &String) -> Result<FileAttr, Error> {
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

  async fn read(name: &String, offset: i64, size: u32) -> Result<Vec<u8>, Error> {
    let mut file = tokio::fs::File::open(name).await?;
    let mut buf = vec![0; size as usize];
    file.seek(SeekFrom::Start(offset as u64)).await?;
    file.read_exact(&mut buf).await?;
    Ok(buf)
  }

}

fn make_folder_attr(inode: &INode) -> FileAttr {
  FileAttr {
    ino: inode.get_ino(),
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
            let binding = target.clone();
            self.runtime.spawn(async move {
              match Self::getattr(&binding).await {
                Ok(attr) => {
                  reply.entry(&TTL, &attr, 0);
                }
                Err(err) => {
                  error!("Failed to get attr for {}: {}", binding, err);
                  reply.error(EIO)
                }
              }
            });
          }
          INode::Folder { .. } => {
            debug!("lookup: found folder {}", filename);
            let attr = make_folder_attr(&*inode.borrow());
            reply.entry(&TTL, &attr, 0);
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
        let bind = target.clone();
        self.runtime.spawn(async move {
          match Self::getattr(&bind).await {
            Ok(attr) => {
              reply.attr(&TTL, &attr);
            }
            Err(err) => {
              error!("Failed to get attr for {}: {}", bind, err);
              reply.error(EIO)
            }
          }
        });
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
    size: u32,
    _flags: i32,
    _lock: Option<u64>,
    reply: ReplyData,
  ) {
    debug!("read(ino={}, offset={})", ino, offset);
    let Some (inode) = self.inode_table.get_by_ino(ino) else {
      debug!("read: {} not found", ino);
      reply.error(ENOENT);
      return;
    };

    match inode.borrow().deref() {
      INode::Folder { .. } => {
        debug!("read: {} is a folder", ino);
        reply.error(EISDIR);
        return;
      }
      INode::File { target, .. } => {
        debug!("follow read request to {}", target);
        let binding = target.clone();
        self.runtime.spawn(async move {
          match Self::read(&binding, offset, size).await {
            Ok(data) => {
              debug!("read: {} bytes read", data.len());
              reply.data(&data);
            }
            Err(err) => {
              error!("Failed to read {}: {}", binding, err);
              reply.error(EIO)
            }
          }
        });
      }
    };
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

    debug!("readdir(ino={}): {:?}", ino, inode.clone().borrow().get_name());

    let curr = inode.borrow();
    let mut files = vec![
      Rc::new(RefCell::new(INode::Folder {
        ino: curr.get_ino(),
        parent: curr.get_parent(),
        name: ".".to_string(),
        entries: Default::default(),
      }))
    ];

    if let Some(parent) = self.inode_table.get_by_ino(inode.borrow().get_parent()) {
      let parent_ref = parent.borrow();
      let parent_folder = Rc::new(RefCell::new(INode::Folder {
        ino: parent_ref.get_ino(),
        parent: parent_ref.get_parent(),
        name: "..".to_string(),
        entries: Default::default(),
      }));
      files.push(parent_folder);
    }

    files.append(&mut inode.list_current());

    for (i, entry) in files.iter().enumerate().skip(offset as usize) {
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