use std::cell::RefCell;
use std::collections::{BTreeMap, LinkedList};
use std::ops::Deref;
use std::rc::Rc;
use fuser::FileType;


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
}

struct INodeTable {
  root: Rc<RefCell<INode>>,
  table: Vec<Rc<RefCell<INode>>>,
}

impl INodeTable {
  fn new() -> Self {
    let root = Rc::new(RefCell::new(INode::Folder {
      ino: 0,
      name: String::from("/"),
      entries: BTreeMap::new(),
    }));
    INodeTable {
      root: root.clone(),
      table: vec![root],
    }
  }

  fn from(root: Rc<RefCell<INode>>) -> Self {
    let mut stack = LinkedList::new();
    stack.push_back(root.clone());
    let mut collect = LinkedList::new();
    while let Some(current) = stack.pop_back() {
      collect.push_back(current.clone());
      match &*current.borrow() {
        INode::File { .. } => {}
        INode::Folder { entries, .. } => {
          for (_, entry) in entries {
            stack.push_back(entry.clone());
          }
        }
      }
    }
    INodeTable {
      root: root.clone(),
      table: collect.into_iter().collect(),
    }
  }
}

fn fake_inode_tree() -> INode {
  let mut hosts = Rc::new(RefCell::new(INode::File {
    ino: 3,
    name: String::from("hosts"),
    target: String::from("/etc/hosts"),
  }));
  let mut passwd = Rc::new(RefCell::new(INode::File {
    ino: 4,
    name: String::from("passwd"),
    target: String::from("/etc/passwd"),
  }));
  let mut shadow = Rc::new(RefCell::new(INode::File {
    ino: 5,
    name: String::from("shadow"),
    target: String::from("/etc/shadow"),
  }));
  let mut group = Rc::new(RefCell::new(INode::File {
    ino: 6,
    name: String::from("group"),
    target: String::from("/etc/group"),
  }));
  let mut subfile = Rc::new(RefCell::new(INode::File {
    ino: 8,
    name: String::from("subfile"),
    target: String::from("/etc/subfile"),
  }));
  let mut subfolder = Rc::new(RefCell::new(INode::Folder {
    ino: 7,
    name: String::from("subfolder"),
    entries: {
      let mut es = BTreeMap::new();
      es.insert(String::from("subfile"), subfile);
      es
    },
  }));

  let mut etc = Rc::new(RefCell::new(INode::Folder {
    ino: 2,
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
    ino: 1,
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

}