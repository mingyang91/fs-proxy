use std::cell::{RefCell};
use std::collections::{BTreeMap};
use std::ops::{Deref};
use std::rc::Rc;
use crate::mapping::Path;

#[derive(Debug)]
pub enum INode {
  File {
    ino: u64,
    parent: u64,
    name: String,
    target: String,
  },
  Folder {
    ino: u64,
    parent: u64,
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

  pub fn get_name(&self) -> &String {
    match self {
      INode::File { name, .. } => name,
      INode::Folder { name, .. } => name,
    }
  }

  pub fn get_ino(&self) -> u64 {
    match self {
      INode::File { ino, .. } => *ino,
      INode::Folder { ino, .. } => *ino,
    }
  }

  fn set_ino(&mut self, ino: u64) {
    match self {
      INode::File { ino: i, .. } => *i = ino,
      INode::Folder { ino: i, .. } => *i = ino,
    }
  }

  fn set_parent(&mut self, parent: u64) {
    match self {
      INode::File { parent: p, .. } => *p = parent,
      INode::Folder { parent: p, .. } => *p = parent,
    }
  }

  pub fn get_parent(&self) -> u64 {
    match self {
      INode::File { parent, .. } => *parent,
      INode::Folder { parent, .. } => *parent,
    }
  }

  fn auto_set_parent(&mut self, parent: u64) {
    self.set_parent(parent);
    match self {
      INode::File { .. } => {}
      INode::Folder { ino, entries, .. } => {
        for (_, entry) in entries {
          entry.borrow_mut().auto_set_parent(*ino);
        }
      }
    }
  }
}

impl From<Path> for INode {
  fn from(value: Path) -> Self {
    match value {
      Path::File { name, path } => INode::File {
        ino: 0,
        parent: 0,
        name,
        target: path,
      },
      Path::Folder { name, paths } => {
        let mut entries = BTreeMap::new();
        for (name, path) in paths {
          entries.insert(name, Rc::new(RefCell::new(path.into())));
        }
        INode::Folder {
          ino: 0,
          parent: 0,
          name,
          entries,
        }
      }
    }
  }
}

pub trait INodeOps {
  fn list_recursively(&self) -> Vec<Rc<RefCell<INode>>>;
  fn list_current(&self) -> Vec<Rc<RefCell<INode>>>;
}

impl INodeOps for Rc<RefCell<INode>> {
  fn list_recursively(&self) -> Vec<Rc<RefCell<INode>>> {
    match self.borrow().deref() {
      INode::File { .. } => vec![self.clone()],
      INode::Folder { entries, .. } => {
        let mut res = vec![self.clone()];
        let mut deep: Vec<Rc<RefCell<INode>>> = entries.values()
          .flat_map(|entry| entry.list_recursively())
          .collect();
        res.append(&mut deep);
        res
      }
    }
  }

  fn list_current(&self) -> Vec<Rc<RefCell<INode>>> {
    match self.borrow().deref() {
      INode::File { .. } => vec![],
      INode::Folder { entries , .. } => {
        entries.values().cloned().collect()
      }
    }
  }
}


#[derive(Debug)]
pub struct INodeTable {
  table: Vec<Rc<RefCell<INode>>>,
  root: Rc<RefCell<INode>>,
}

impl INodeTable {
  pub fn lookup(&self, ino: u64, name: String) -> Option<Rc<RefCell<INode>>> {
    self.table.get(ino as usize - 1)
      .and_then(|inode| inode.borrow().lookup(name))
  }

  pub fn get_by_ino(&self, ino: u64) -> Option<Rc<RefCell<INode>>> {
    self.table.get(ino as usize - 1).cloned()
  }
}

impl From<Rc<RefCell<INode>>> for INodeTable {
  fn from(root: Rc<RefCell<INode>>) -> Self {
    let table = root.list_recursively();
    for (idx, inode) in table.iter().enumerate() {
      inode.borrow_mut().set_ino(idx as u64 + 1)
    }

    root.borrow_mut().auto_set_parent(0);

    INodeTable {
      root: root.clone(),
      table,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;


  fn fake_inode_tree() -> INode {
    let hosts = Rc::new(RefCell::new(INode::File {
      ino: 0,
      parent: 0,
      name: String::from("hosts"),
      target: String::from("/etc/hosts"),
    }));
    let passwd = Rc::new(RefCell::new(INode::File {
      ino: 0,
      parent: 0,
      name: String::from("passwd"),
      target: String::from("/etc/passwd"),
    }));
    let shadow = Rc::new(RefCell::new(INode::File {
      ino: 0,
      parent: 0,
      name: String::from("shadow"),
      target: String::from("/etc/shadow"),
    }));
    let group = Rc::new(RefCell::new(INode::File {
      ino: 0,
      parent: 0,
      name: String::from("group"),
      target: String::from("/etc/group"),
    }));
    let subfile = Rc::new(RefCell::new(INode::File {
      ino: 0,
      parent: 0,
      name: String::from("subfile"),
      target: String::from("/etc/subfile"),
    }));
    let subfolder = Rc::new(RefCell::new(INode::Folder {
      ino: 0,
      parent: 0,
      name: String::from("subfolder"),
      entries: {
        let mut es = BTreeMap::new();
        es.insert(String::from("subfile"), subfile);
        es
      },
    }));

    let etc = Rc::new(RefCell::new(INode::Folder {
      ino: 0,
      parent: 0,
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
      parent: 0,
      name: String::from("/"),
      entries: {
        let mut es = BTreeMap::new();
        es.insert(String::from("etc"), etc);
        es
      },
    }
  }

  #[test]
  fn test() {
    let root = Rc::new(RefCell::new(fake_inode_tree()));

    let table = INodeTable::from(root);
    println!("list_files: {:?}", table);
  }

  #[test]
  fn test_mapping_file() {
    let mapping_tree = fs::read_to_string("res/mapping-tree.json").unwrap();
    let path: Path = serde_json::from_str(&mapping_tree).unwrap();
    let root = Rc::new(RefCell::new(path.into()));
    let table = INodeTable::from(root);
    let root_ino = table.root.borrow().get_ino();
    let file = table.lookup(root_ino, String::from("Surge.app"));

    assert!(file.is_some());
  }
}
