use std::cell::{Ref, RefCell};
use std::collections::{BTreeMap, LinkedList};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use fuser::FileType;

#[derive(Debug)]
enum INode {
  File {
    ino: u64,
    name: String,
    target: String,
  },
  Folder {
    ino: u64,
    name: String,
    entries: BTreeMap<String, Rc<RefCell<INode>>>
  },
}

impl INode {
  fn lookup(&self, name: String) -> Option<Rc<RefCell<INode>>> {
    match self {
      INode::File { .. } => None,
      INode::Folder { entries, .. } => {
        if let Some(entry) = entries.get(&name) {
          Some(entry.clone())
        } else {
          None
        }
      }
    }
  }

  fn set_ino(&mut self, ino: u64) {
    match self {
      INode::File { ino: i, .. } => *i = ino,
      INode::Folder { ino: i, .. } => *i = ino,
    }
  }
}

trait INodeOps {
  fn list(&self) -> Vec<Rc<RefCell<INode>>>;
}

impl INodeOps for Rc<RefCell<INode>> {
  fn list(&self) -> Vec<Rc<RefCell<INode>>> {
    match self.borrow().deref() {
      INode::File { .. } => vec![self.clone()],
      INode::Folder { entries, .. } => {
        entries.values()
          .flat_map(|entry| match *entry.borrow() {
            INode::File { .. } => vec![entry.clone()],
            INode::Folder { .. } => {
              let mut files = entry.clone().list();
              files.push(entry.clone());
              files
            },
          })
          .collect()
      }
    }
  }
}


#[derive(Debug)]
struct INodeTable {
  table: Vec<Rc<RefCell<INode>>>,
  root: Rc<RefCell<INode>>,
}

impl INodeTable {
  fn new() -> Self {
    let root = Rc::new(RefCell::new(INode::Folder {
      ino: 0,
      name: String::from("/"),
      entries: BTreeMap::new(),
    }));
    INodeTable {
      table: vec![root.clone()],
      root,
    }
  }

  fn from(root: Rc<RefCell<INode>>) -> Self {
    let mut table = root.list();
    for (idx, inode) in table.iter().enumerate() {
      inode.borrow_mut().set_ino(idx as u64)
    }

    INodeTable {
      root: root.clone(),
      table: table,
    }
  }
}

fn fake_inode_tree() -> INode {
  let mut hosts = Rc::new(RefCell::new(INode::File {
    ino: 0,
    name: String::from("hosts"),
    target: String::from("/etc/hosts"),
  }));
  let mut passwd = Rc::new(RefCell::new(INode::File {
    ino: 0,
    name: String::from("passwd"),
    target: String::from("/etc/passwd"),
  }));
  let mut shadow = Rc::new(RefCell::new(INode::File {
    ino: 0,
    name: String::from("shadow"),
    target: String::from("/etc/shadow"),
  }));
  let mut group = Rc::new(RefCell::new(INode::File {
    ino: 0,
    name: String::from("group"),
    target: String::from("/etc/group"),
  }));
  let mut subfile = Rc::new(RefCell::new(INode::File {
    ino: 0,
    name: String::from("subfile"),
    target: String::from("/etc/subfile"),
  }));
  let mut subfolder = Rc::new(RefCell::new(INode::Folder {
    ino: 0,
    name: String::from("subfolder"),
    entries: {
      let mut es = BTreeMap::new();
      es.insert(String::from("subfile"), subfile);
      es
    },
  }));

  let mut etc = Rc::new(RefCell::new(INode::Folder {
    ino: 0,
    name: String::from("etc"),
    entries: {
      let mut es = BTreeMap::new();
      es.insert(String::from("hosts"), hosts);
      es.insert(String::from("passwd"), passwd);
      es.insert(String::from("shadow"), shadow);
      es.insert(String::from("group"), group);
      es.insert(String::from("subfolder"), subfolder);
      es
    },
  }));
  INode::Folder {
    ino: 0,
    name: String::from("/"),
    entries: {
      let mut es = BTreeMap::new();
      es.insert(String::from("etc"), etc);
      es
    },
  }
}

#[test]
fn test () {
  let root = Rc::new(RefCell::new(fake_inode_tree()));

  let table = INodeTable::from(root);
  println!("list_files: {:?}", table);
}