extern crate reqwest;
extern crate torrent;

mod bencode;

use std::net::{IpAddr, SocketAddr, TcpListener};
use std::str::{self, FromStr};
use std::{env, fs, process};
use torrent::download::{self, Peer};
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

    let mut pieces = vec![];
    let mut total_length = 0;
    if let Some(length) = metainfo.info.length {
        total_length = length;
        let mut index = 0;
        for _ in 0..total_length / metainfo.info.piece_length {
            pieces.push(download::Piece::new(
                index,
                metainfo.info.piece_length,
                metainfo.info.pieces[index as usize],
            ));
            index += 1;
        }
        let remainder = total_length % metainfo.info.piece_length;
        if remainder != 0 {
            pieces.push(download::Piece::new(
                index,
                remainder,
                metainfo.info.pieces[index as usize],
            ));
        }
    } else if let Some(files) = metainfo.info.files {
        for file in files {
            total_length += file.length;
            let mut index = 0;
            for _ in 0..file.length / metainfo.info.piece_length {
                pieces.push(download::Piece::new(
                    index,
                    metainfo.info.piece_length,
                    metainfo.info.pieces[index as usize],
                ));
                index += 1;
            }
            let remainder = file.length % metainfo.info.piece_length;
            if remainder != 0 {
                pieces.push(download::Piece::new(
                    index,
                    remainder,
                    metainfo.info.pieces[index as usize],
                ));
            }
        }
    }
    println!("piece length: {}", metainfo.info.piece_length);
    println!("total_length: {}", total_length);

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
    let compact = "compact=1".to_string();

    let params = vec![
        info_hash, peer_id, port, uploaded, downloaded, left, compact,
    ];
    let params = params.join("&");

    let mut url = Url::parse(metainfo.announce).unwrap();
    url.set_query(Some(&params));

    let mut body = reqwest::get(url).unwrap();
    let mut body_buf: Vec<u8> = vec![];
    body.copy_to(&mut body_buf).unwrap();

    let peers = match parse_response(&body_buf) {
        Ok(peers) => peers,
        Err(e) => {
            println!("{}", e);
            return Ok(());
        }
    };

    torrent::download::download_from(
        &mut pieces,
        peers,
        metainfo.info.hash,
        metainfo.info.piece_length,
    );

    Ok(())
}

fn parse_response(
    response: &[u8],
) -> Result<Vec<Peer>, Box<std::error::Error>> {
    let dict = bencode::decode(response)?;
    let dict = match dict {
        bencode::BencodeVal::Dict {
            index: _,
            size: _,
            dict,
        } => dict,
        _ => return Err(From::from("response should be a dictionary")),
    };

    if let Some(reason) = dict.get("failure reason".as_bytes()) {
        match reason {
            bencode::BencodeVal::Str {
                index: _,
                size: _,
                byte_str,
            } => {
                let failure_reason = str::from_utf8(byte_str).unwrap();
                return Err(From::from(format!("failed: {}", failure_reason)));
            }
            _ => return Err(From::from("failure reason should be a byte str")),
        }
    }

    let peers_list = match dict.get("peers".as_bytes()) {
        Some(bencode::BencodeVal::List {
            index: _,
            size: _,
            list,
        }) => list,
        _ => return Err(From::from("should be a list")),
    };

    let mut peers: Vec<Peer> = Vec::new();
    for peer_dict in peers_list {
        if let bencode::BencodeVal::Dict {
            index: _,
            size: _,
            dict,
        } = peer_dict
        {
            let id = match dict.get("peer id".as_bytes()) {
                Some(&bencode::BencodeVal::Str {
                    index: _,
                    size: _,
                    byte_str,
                }) => {
                    let mut array = [0u8; 20];
                    array.clone_from_slice(&byte_str[0..20]);
                    array
                }
                _ => return Err(From::from("peer id should be a byte string")),
            };

            let ip = match dict.get("ip".as_bytes()) {
                Some(bencode::BencodeVal::Str {
                    index: _,
                    size: _,
                    byte_str,
                }) => {
                    IpAddr::from_str(str::from_utf8(byte_str).unwrap()).unwrap()
                }
                _ => return Err(From::from("peer id should be a byte string")),
            };

            let port = match dict.get("port".as_bytes()) {
                Some(bencode::BencodeVal::Int {
                    index: _,
                    size: _,
                    int,
                }) => *int as u16,
                _ => return Err(From::from("port should be an integer")),
            };

            peers.push(Peer {
                id: id,
                socket: SocketAddr::new(ip, port),
            });
        }
    }

    Ok(peers)
}
