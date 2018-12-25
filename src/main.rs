extern crate reqwest;
extern crate torrent;

use std::net::{SocketAddr, TcpListener};
use std::{env, fs, process};
use torrent::TorrentMetaInfo;
use url::percent_encoding::{percent_encode, DEFAULT_ENCODE_SET};
use url::Url;

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

    let mut total_length = 0;
    if let Some(length) = metainfo.info.length {
        total_length = length;
    } else if let Some(files) = metainfo.info.files {
        for file in files {
            total_length += file.length;
        }
    }

    let addrs: Vec<SocketAddr> = (6881..6889)
        .into_iter()
        .map(|x| SocketAddr::from(([127, 0, 0, 1], x)))
        .collect();
    let listener = TcpListener::bind(&addrs[..])?;
    let port = listener.local_addr().unwrap().port();

    let info_hash = format!(
        "info_hash={}",
        percent_encode(&metainfo.info.hash, DEFAULT_ENCODE_SET).to_string()
    );
    let peer_id = format!("peer_id={}", "01234567890123456789");
    let port = format!("port={}", port.to_string());
    let uploaded = format!("uploaded={}", "0");
    let downloaded = format!("downloaded={}", "0");
    let left = format!("left={}", total_length.to_string());

    let params = vec![info_hash, peer_id, port, uploaded, downloaded, left];
    let params = params.join("&");

    let mut url = Url::parse(metainfo.announce).unwrap();
    url.set_query(Some(&params));

    println!("{:?}", url);
    let body = reqwest::get(url).unwrap().text().unwrap();
    println!("{:?}", body);

    Ok(())
}
