extern crate bencode;

use std::fs;

fn main() -> std::io::Result<()> {
    let contents = fs::read("ubuntu.torrent")?;
    match bencode::decode(&contents[..]) {
        Ok(v) => println!("{:?}", v),
        Err(e) => println!("{}", e),
    }

    Ok(())
}
