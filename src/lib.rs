extern crate sha1;

use std::fmt;
use std::str;

mod bencode;

pub struct File {
    pub length: i64,
    pub path: String,
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "length: {}", self.length)?;
        write!(f, "path: {}", self.path)
    }
}

pub struct Info<'a> {
    pub name: &'a str,
    pub piece_length: i64,
    pub pieces: Vec<&'a [u8]>,
    pub length: Option<i64>,
    pub files: Option<Vec<File>>,
    pub hash: [u8; 20],
}

pub struct TorrentMetaInfo<'a> {
    pub announce: &'a str,
    pub info: Info<'a>,
}

impl<'a> TorrentMetaInfo<'a> {
    pub fn new(
        contents: &'a [u8],
    ) -> Result<TorrentMetaInfo, Box<std::error::Error>> {
        let dict = match bencode::decode(&contents)? {
            bencode::BencodeVal::Dict {
                index: _,
                size: _,
                dict,
            } => dict,
            _ => {
                return Err(From::from("should be a dictionary"));
            }
        };

        let announce = match dict.get("announce".as_bytes()) {
            Some(v) => match v {
                bencode::BencodeVal::Str {
                    index: _,
                    size: _,
                    byte_str,
                } => str::from_utf8(byte_str)?,
                _ => {
                    return Err(From::from(
                        "announce should be a UTF-8 encoded string",
                    ))
                }
            },
            None => {
                return Err(From::from(
                    "announce not found in metainfo dictionary",
                ))
            }
        };

        let (&info_dict_index, info_dict_size, info_dict) =
            match dict.get("info".as_bytes()) {
                Some(v) => match v {
                    bencode::BencodeVal::Dict { index, dict, size } => {
                        (index, size, dict)
                    }
                    _ => return Err(From::from("info should be a dictionary")),
                },
                None => {
                    return Err(From::from(
                        "info dict not found in metainfo dictionary",
                    ))
                }
            };

        let info_hash = sha1::Sha1::from(
            &contents[info_dict_index..info_dict_index + info_dict_size],
        )
        .digest()
        .bytes();

        let name = match info_dict.get("name".as_bytes()) {
            Some(v) => match v {
                bencode::BencodeVal::Str {
                    index: _,
                    byte_str,
                    size: _,
                } => str::from_utf8(byte_str)?,
                _ => {
                    return Err(From::from(
                        "name should be a UTF-8 encoded string",
                    ))
                }
            },
            None => {
                return Err(From::from("name not found in info_dict dictionary"))
            }
        };

        let piece_length = match info_dict.get("piece length".as_bytes()) {
            Some(v) => match v {
                bencode::BencodeVal::Int {
                    index: _,
                    int,
                    size: _,
                } => *int,
                _ => {
                    return Err(From::from("piece length should be an integer"))
                }
            },
            None => {
                return Err(From::from(
                    "piece length not found in info_dict dictionary",
                ))
            }
        };

        let pieces_byte_string = match info_dict.get("pieces".as_bytes()) {
            Some(v) => match v {
                bencode::BencodeVal::Str {
                    index: _,
                    byte_str,
                    size: _,
                } => byte_str,
                _ => return Err(From::from("pieces should be a byte string")),
            },
            None => {
                return Err(From::from(
                    "pieces not found in info_dict dictionary",
                ))
            }
        };

        let mut pieces = Vec::new();
        for i in 0..pieces_byte_string.len() / 20 {
            pieces.push(&pieces_byte_string[i * 20..(i + 1) * 20]);
        }

        let length = if let Some(bencode::BencodeVal::Int {
            index: _,
            size: _,
            int,
        }) = info_dict.get("length".as_bytes())
        {
            Some(*int)
        } else {
            None
        };

        let files = if let Some(bencode::BencodeVal::List {
            index: _,
            size: _,
            list,
        }) = info_dict.get("files".as_bytes())
        {
            let mut vector = Vec::new();
            for elem in list {
                if let bencode::BencodeVal::Dict {
                    index: _,
                    size: _,
                    dict,
                } = elem
                {
                    let file_length = match dict.get("length".as_bytes()) {
                        Some(v) => match v {
                            bencode::BencodeVal::Int {
                                index: _,
                                size: _,
                                int,
                            } => *int,
                            _ => {
                                return Err(From::from(
                                    "file length should be an integer",
                                ))
                            }
                        },
                        None => {
                            return Err(From::from(
                                "file length not found in dictionary",
                            ))
                        }
                    };

                    let file_path = match dict.get("path".as_bytes()) {
                        Some(v) => match v {
                            bencode::BencodeVal::List {
                                index: _,
                                size: _,
                                list,
                            } => list,
                            _ => {
                                return Err(From::from(
                                    "file path should be a list",
                                ))
                            }
                        },
                        None => {
                            return Err(From::from(
                                "file path not found in dictionary",
                            ))
                        }
                    };

                    let file_path: Vec<&str> = file_path
                        .iter()
                        .map(|x| match x {
                            bencode::BencodeVal::Str {
                                index: _,
                                size: _,
                                byte_str,
                            } => str::from_utf8(byte_str).unwrap_or(""),
                            _ => "",
                        })
                        .collect();

                    vector.push(File {
                        length: file_length,
                        path: file_path.join("/"),
                    });
                }
            }
            Some(vector)
        } else {
            None
        };

        Ok(TorrentMetaInfo {
            announce: announce,
            info: Info {
                name: name,
                piece_length: piece_length,
                pieces: pieces,
                length: length,
                files: files,
                hash: info_hash,
            },
        })
    }
}
