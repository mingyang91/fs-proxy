use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub(crate) enum Path {
  File { name: String, path: String },
  Folder { name: String, paths: HashMap<String, Path> },
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct MappingConfig {
  pub(crate) mapping: Path,
}

fn fake_mapping() -> MappingConfig {
  MappingConfig {
    mapping: {
      let mut root = HashMap::new();
      let folder1_name = "folder1".to_string();
      root.insert(folder1_name.clone(), Path::Folder {
        name: folder1_name,
        paths: {
          let name = "d1f1.txt".to_string();
          let mut folder1 = HashMap::new();
          folder1.insert(name.clone(), Path::File {
            name,
            path: "/tmp/hello.txt".to_string()
          });
          folder1
        }
      });
      let folder2_name = "folder2".to_string();
      root.insert(folder2_name.clone(), Path::Folder {
        name: folder2_name,
        paths: {
          let mut folder2 = HashMap::new();
          let d2f1_name = "d2f1.txt".to_string();
          folder2.insert(d2f1_name.clone(), Path::File {
            name: d2f1_name,
            path: "/tmp/hello.txt".to_string()
          });
          let d2f2_name = "d2f2.txt".to_string();
          folder2.insert(d2f2_name.clone(), Path::File {
            name: d2f2_name,
            path: "/tmp/hello.txt".to_string()
          });
          folder2
        }
      });
      let file1_name = "file1.txt".to_string();
      root.insert(file1_name.clone(), Path::File {
        name: file1_name,
        path: "/tmp/hello.txt".to_string()
      });
      Path::Folder { name: "/".to_string(), paths: root }
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
