use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub(crate) enum Destination {
    File { path: String },
    Folder,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct MappingConfig {
    pub(crate) mapping: HashMap<String, Destination>,
}

#[test]
fn test() {
    let mapping = MappingConfig {
        mapping: {
            let mut mapping = HashMap::new();
            mapping.insert("hello.txt".to_string(), Destination::File { path: "/tmp/hello.txt".to_string() });
            mapping.insert("hello".to_string(), Destination::Folder);
            mapping
        }
    };
    let serialized = serde_json::to_string(&mapping).unwrap();
    println!("serialized = {}", serialized);
    let deserialized: MappingConfig = serde_json::from_str(&serialized).unwrap();
    println!("deserialized = {:?}", deserialized);
}