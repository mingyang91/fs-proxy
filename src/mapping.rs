#![feature(generators, generator_trait)]

use std::collections::{HashMap, LinkedList};
use std::collections::hash_map::Iter;
use std::ops::Index;
use std::vec::IntoIter;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub(crate) enum Path {
  File { path: String },
  Folder { paths: HashMap<String, Path> },
}

struct PathIter<'a> {
  stack: LinkedList<Iter<'a, String, Path>>,
}

impl Path {
  fn iter(&self) -> PathIter {
    match self {
      Path::File { .. } => {
        PathIter {
          stack: LinkedList::new(),
        }
      }
      Path::Folder { paths } => {
        PathIter {
          stack: {
            let mut list = LinkedList::new();
            list.push_back(paths.iter());
            list
          },
        }
      }
    }

  }
  fn lookup(&self, name: &Vec<String>) -> Option<String> {
    match self {
      Path::File { path } => {
        if name.is_empty() {
          Some(path.clone())
        } else {
          None
        }
      }
      Path::Folder { paths } => {
        if let Some(path) = paths.get(&name[0]) {
          let tail = name[1..].to_vec();
          path.lookup(&tail)
        } else {
          None
        }
      }
    }
  }
}


#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct MappingConfig {
  pub(crate) mapping: Path,
}

fn fake_mapping() -> MappingConfig {
  MappingConfig {
    mapping: {
      let mut root = HashMap::new();
      root.insert("folder1".to_string(), Path::Folder {
        paths: {
          let mut folder1 = HashMap::new();
          folder1.insert("d1f1.txt".to_string(), Path::File { path: "/tmp/hello.txt".to_string() });
          folder1
        }
      });
      root.insert("folder2".to_string(), Path::Folder {
        paths: {
          let mut folder2 = HashMap::new();
          folder2.insert("d2f1.txt".to_string(), Path::File { path: "/tmp/hello.txt".to_string() });
          folder2.insert("d2f2.txt".to_string(), Path::File { path: "/tmp/hello.txt".to_string() });
          folder2
        }
      });
      root.insert("file1.txt".to_string(), Path::File { path: "/tmp/hello.txt".to_string() });
      Path::Folder { paths: root }
    }
  }
}

#[test]
fn test_serde() {
  let mapping = fake_mapping();
  let serialized = serde_json::to_string(&mapping).unwrap();
  println!("serialized = {}", serialized);
  let deserialized: MappingConfig = serde_json::from_str(&serialized).unwrap();
  println!("deserialized = {:?}", deserialized);
}

#[test]
fn test_lookup() {
  let mapping = fake_mapping();
  let path = mapping.mapping.lookup(&vec!["folder1".to_string(), "d1f1.txt".to_string()]);
  println!("path = {:?}", path);
  assert!(path.is_some(), "path should be Some");
  assert_eq!(path.unwrap(), "/tmp/hello.txt", "path should be /tmp/hello.txt");
  let path2 = mapping.mapping.lookup(&vec!["folder1".to_string(), "d1f2.txt".to_string()]);
  println!("path2 = {:?}", path2);
  assert!(path2.is_none(), "path2 should be None");
}

#[test]
fn test_iter() {
  let mapping = fake_mapping();
  let mut iter = mapping.mapping.iter();

}