extern crate torrent;

use std::{env, fs, process};
use torrent::TorrentMetaInfo;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Please pass a torrent filename");
        process::exit(1);
    }

    if let Err(e) = run(&args[1]) {
        println!("{}", e);
    }
}

fn run(s: &str) -> std::io::Result<()> {
    let contents = fs::read(s)?;
    let metainfo = match TorrentMetaInfo::new(&contents) {
        Ok(m) => m,
        Err(e) => {
            println!("{}", e);
            return Ok(());
        }
    };

    println!("Announce: {}", metainfo.announce);
    println!("Name: {}", metainfo.info.name);
    println!("Piece Length: {}", metainfo.info.piece_length);

    if let Some(length) = metainfo.info.length {
        println!("Length: {}", length);
    }

    if let Some(files) = metainfo.info.files {
        for file in files {
            println!("{:?}", file);
        }
    }

    Ok(())
}
