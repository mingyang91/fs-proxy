use clap::{crate_version, Parser};

#[derive(Debug, Parser)]
#[clap(name = "fs-proxy")]
#[clap(author = "mingyang91 <mingyang91@qq.com>")]
#[clap(version = crate_version!())]
#[clap(about = "A proxy filesystem")]
pub(crate) struct Args {
    #[clap(help = "Mountpoint", index = 1)]
    pub mountpoint: String,
    #[clap(long = "auto-unmount", action, help = "Automatically unmount on process exit")]
    pub auto_unmount: bool,
    #[clap(long = "allow-root", action, help = "Allow root user to access filesystem")]
    pub allow_root: bool,
    #[clap(long = "mapping-file", help = "Mapping file, a json file that maps file path to destination. e.g. {\"/tmp/hello.txt\": {\"type\": \"File\", \"path\": \"/tmp/hello.txt\"}, \"/tmp/hello\": {\"type\": \"Folder\"}}")]
    pub mapping_file: String,
}

#[test]
fn test() {
    let args = Args::parse_from(vec![
        "fs-proxy",
        "/tmp/hello",
        "--auto-unmount",
        "--allow-root",
        "--mapping-file",
        "/tmp/mapping.json"
    ]);
    println!("args = {:?}", args);
}