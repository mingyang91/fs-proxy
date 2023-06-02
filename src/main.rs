mod args;
mod mapping;
mod inode;

use std::cell::RefCell;
use std::cmp::min;
use std::collections::BTreeMap;
use clap::{Parser};
use fuser::{FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request, ReplyOpen, ReplyEmpty};
use libc;
use std::ffi::OsStr;
use std::io::{Error, SeekFrom};
use std::ops::{Add, Deref};
use std::os::unix::fs::MetadataExt;
use std::process::exit;
use std::rc::Rc;
use std::sync::{Arc};
use std::time::{Duration, UNIX_EPOCH};
use std::fmt::{Display, Formatter};
use slog::{error, debug, info, Logger, o, Drain};
use slog_term;
use slog_async::{Async};
use crate::args::Args;
use tokio;
use tokio::io::{AsyncSeekExt, AsyncReadExt};
use tokio::runtime::{Runtime};
use tokio::sync::Mutex;
use tokio::fs::File;
use crate::inode::{INode, INodeOps, INodeTable};
use crate::mapping::Path;
use lazy_static::lazy_static;

const TTL: Duration = Duration::from_secs(0); // 1 second
lazy_static! {
  static ref LOG: Logger = {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::CompactFormat::new(decorator).build().fuse();
    let drain = Async::new(drain).build().fuse();
    Logger::root(drain, o!())
  };
}


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

struct Inner {
  file_handles: BTreeMap<u64, Arc<Mutex<File>>>,
  counter: u64,
}

impl Inner {
  fn inc_counter(&mut self) -> u64 {
    self.counter += 1;
    self.counter
  }
}

struct MappingFS {
  runtime: Runtime,
  inode_table: INodeTable,
  inner: Arc<Mutex<Inner>>
}

impl MappingFS {
  fn new(runtime: Runtime, mapping: Path) -> Self {
    let root: Rc<RefCell<INode>> = Rc::new(RefCell::new(mapping.into()));
    Self {
      runtime,
      inode_table: INodeTable::from(root),
      inner: Arc::new(Mutex::new(Inner {
        file_handles: Default::default(),
        counter: 0
      }))
    }
  }

  async fn getattr(ino: u64, name: &String) -> Result<FileAttr, Error> {
    let file = File::open(name).await?;
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
      ino,
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
    debug!(LOG, "lookup called with parent={}, name={:?}", parent, name);
    let Some(filename) = name.to_str() else {
      reply.error(libc::ENOENT);
      return;
    };
    let res = self.inode_table.lookup(parent, filename.to_string());
    match res {
      Some(inode) => {
        match inode.borrow().deref() {
          INode::File { target, .. } => {
            debug!(LOG, "lookup: found file {} -> {}", filename, target);
            let binding = target.clone();
            let ino = inode.borrow().get_ino();
            self.runtime.spawn(async move {
              match Self::getattr(ino, &binding).await {
                Ok(attr) => {
                  debug!(LOG, "lookup: got attr for {}: {:?}", binding, attr);
                  reply.entry(&TTL, &attr, 0);
                }
                Err(err) => {
                  error!(LOG, "Failed to get attr for {}: {}", binding, err);
                  reply.error(libc::EIO)
                }
              }
            });
          }
          INode::Folder { .. } => {
            debug!(LOG, "lookup: found folder {}", filename);
            let attr = make_folder_attr(&*inode.borrow());
            reply.entry(&TTL, &attr, 0);
          }
        }
      }
      None => {
        reply.error(libc::ENOENT);
      }
    }
  }

  fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
    let Some(inode) = self.inode_table.get_by_ino(ino) else {
      reply.error(libc::ENOENT);
      return;
    };

    match inode.borrow().deref() {
      INode::File { target, .. } => {
        let bind = target.clone();
        self.runtime.spawn(async move {
          match Self::getattr(ino, &bind).await {
            Ok(attr) => {
              reply.attr(&TTL, &attr);
            }
            Err(err) => {
              error!(LOG, "Failed to get attr for {}: {}", bind, err);
              reply.error(libc::EIO)
            }
          }
        });
      }
      INode::Folder { .. } => {
        reply.attr(&TTL, &HELLO_DIR_ATTR);
      }
    };
  }

  fn open(&mut self, _req: &Request<'_>, ino: u64, _flags: i32, reply: ReplyOpen) {
    let Some(inode) = self.inode_table.get_by_ino(ino) else {
      reply.error(libc::ENOENT);
      return;
    };

    let inode_borrow = inode.borrow();
    let INode::File { target, .. } = inode_borrow.deref() else {
      reply.error(libc::ENFILE);
      return;
    };

    let send_inner = self.inner.clone();
    let binding = target.clone();
    self.runtime.spawn(async move {
      match File::open(&binding).await {
        Ok(file) => {
          let arc_file = Arc::new(Mutex::new(file));
          let mut inner = send_inner.lock().await;
          let fh = inner.inc_counter();
          inner.file_handles.insert(fh, arc_file);
          reply.opened(fh, 0);
        }
        Err(err) => {
          info!(LOG, "Failed to open file {}: {}", binding, err);
          reply.error(libc::EIO);
        }
      }
    });
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
    debug!(LOG, "read(ino={}, offset={})", ino, offset);
    let send_inner = self.inner.clone();
    self.runtime.spawn(async move {
      let inner = send_inner.lock().await;

      let Some(arc_file) = inner.file_handles.get(&_fh) else {
        error!(LOG, "Failed to find file handle {}", _fh);
        reply.error(libc::EBADF);
        return;
      };

      let file_clone = arc_file.clone();
      let mut file = file_clone.lock().await;

      let file_size = match file.metadata().await {
        Ok(metadata) => metadata.len(),
        Err(err) => {
          error!(LOG, "Failed to get metadata for file handle {}: {}", _fh, err);
          reply.error(libc::EIO);
          return;
        }
      };

      let read_size = min(size, file_size.saturating_sub(offset as u64) as u32);

      if let Err(err) = file.seek(SeekFrom::Start(offset as u64)).await {
        error!(LOG, "Failed to seek file handle {}: {}", _fh, err);
        reply.error(libc::EIO);
        return;
      };

      let mut buf = vec![0; read_size as usize];
      if let Err(err) = file.read_exact(&mut buf).await {
        error!(LOG, "Failed to read file handle {}: {}", _fh, err);
        reply.error(libc::EIO);
        return;
      };

      reply.data(&buf);
    });
  }

  fn release(&mut self, _req: &Request<'_>, _ino: u64, fh: u64, _flags: i32, _lock_owner: Option<u64>, _flush: bool, reply: ReplyEmpty) {
    debug!(LOG, "release(fh={})", fh);
    let send_inner = self.inner.clone();
    self.runtime.spawn(async move {
      let mut inner = send_inner.lock().await;
      let Some(_file) = inner.file_handles.remove(&fh) else {
        error!(LOG, "Failed to find file handle {}", fh);
        reply.error(libc::ENOENT);
        return;
      };
      reply.ok();
      info!(LOG, "Closing file handle {}", fh);
    });
  }

  fn readdir(
    &mut self,
    _req: &Request,
    ino: u64,
    _fh: u64,
    offset: i64,
    mut reply: ReplyDirectory,
  ) {
    debug!(LOG, "readdir(ino={}, offset={})", ino, offset);
    let Some(inode) = self.inode_table.get_by_ino(ino) else {
      debug!(LOG, "readdir(ino={}): libc::ENOENT", ino);
      reply.error(libc::ENOENT);
      return;
    };

    debug!(LOG, "readdir(ino={}): {:?}", ino, inode.clone().borrow().get_name());

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
          debug!(LOG, "file[{}]: {:?}", i, entry.borrow().get_name());
          reply.add(file.get_ino(), (i + 1) as i64, FileType::RegularFile, name)
        }
        INode::Folder { name, .. } => {
          debug!(LOG, "folder[{}]: {:?}", i, entry.borrow().get_name());
          reply.add(file.get_ino(), (i + 1) as i64, FileType::Directory, name)
        }
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

  let config = match read_mapping_file(&args) {
    Ok(cfg) => cfg,
    Err(err) => {
      error!(LOG, "Failed to read mapping file: {}", err);
      exit(exitcode::CONFIG);
    }
  };

  let runtime = match Runtime::new() {
    Ok(runtime) => runtime,
    Err(err) => {
      error!(LOG, "Failed to create tokio runtime: {}", err);
      exit(exitcode::SOFTWARE);
    }
  };

  let mapping_fs = MappingFS::new(runtime, config);
  if let Err(err) = fuser::mount2(mapping_fs, args.mountpoint, &options) {
    error!(LOG, "Failed to mount filesystem: {}", err);
    exit(exitcode::SOFTWARE);
  }
}

fn read_mapping_file(args: &Args) -> Result<Path, StartError> {
  let mapping_file = std::fs::File::open(&args.mapping_file)
    .map_err(|e| StartError::Io(e))?;
  let rdr = std::io::BufReader::new(mapping_file);
  serde_json::from_reader(rdr)
    .map_err(|e| StartError::Serde(e))
}

enum StartError {
  Io(Error),
  Serde(serde_json::Error),
}

impl Display for StartError {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    match self {
      StartError::Io(err) => write!(f, "IO error: {}", err),
      StartError::Serde(err) => write!(f, "Serde error: {}", err),
    }
  }
}