use std::collections::HashMap;
use std::fmt;
use std::str;

pub enum BencodeVal<'a> {
    Int {
        index: usize,
        int: i64,
        size: usize,
    },
    Str {
        index: usize,
        byte_str: &'a [u8],
        size: usize,
    },
    List {
        index: usize,
        list: Vec<BencodeVal<'a>>,
        size: usize,
    },
    Dict {
        index: usize,
        dict: HashMap<&'a [u8], BencodeVal<'a>>,
        size: usize,
    },
}

impl<'a> fmt::Debug for BencodeVal<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BencodeVal::Int {
                index: _,
                size: _,
                int,
            } => write!(f, "{}", int),
            BencodeVal::Str {
                index: _,
                size: _,
                byte_str,
            } => match str::from_utf8(byte_str) {
                Ok(byte_str) => write!(f, "{:?}", byte_str),
                _ => write!(f, "{:?}", byte_str),
            },
            BencodeVal::List {
                index: _,
                size: _,
                list,
            } => write!(f, "{:?}", list),
            BencodeVal::Dict {
                index: _,
                size: _,
                dict,
            } => {
                write!(f, "{{")?;
                let mut first = true;
                for (key, val) in dict.iter() {
                    if first {
                        first = false;
                    } else {
                        write!(f, ",")?;
                    }
                    write!(f, "{:?}: {:?}", str::from_utf8(key).unwrap(), val)?;
                }
                write!(f, "}}")
            }
        }
    }
}

pub fn decode(bytes: &[u8]) -> Result<BencodeVal, Box<std::error::Error>> {
    match bytes.iter().next() {
        Some(&c) => match c {
            b'i' => Ok(decode_int(bytes)?),
            b'l' => Ok(decode_list(bytes)?),
            b'd' => Ok(decode_dict(bytes)?),
            _ if c.is_ascii_digit() => Ok(decode_str(bytes)?),
            _ => Err(From::from(format!("unexpected char: {}", c as char))),
        },
        None => Err(From::from("reached eof")),
    }
}

fn decode_int(bytes: &[u8]) -> Result<BencodeVal, Box<std::error::Error>> {
    if bytes.len() < 3 {
        return Err(From::from("reached eof"));
    }

    if bytes[0] != b'i' {
        return Err(From::from(format!(
            "unexpected delimiter: {}",
            bytes[0] as char
        )));
    }

    if &bytes[1..3] == b"0e" {
        return Ok(BencodeVal::Int {
            index: 0,
            int: 0,
            size: 3,
        });
    }

    if bytes[1] == b'0' {
        return Err(From::from("no leading zeroes allowed"));
    }

    if &bytes[1..3] == b"-0" {
        return Err(From::from("negative zero not allowed"));
    }

    if let Some(index) = bytes[2..].iter().position(|&x| !x.is_ascii_digit()) {
        if bytes[index + 2] != b'e' {
            return Err(From::from(format!(
                "unexpected char: {}",
                bytes[index + 2] as char
            )));
        }

        let integer = str::from_utf8(&bytes[1..index + 2])
            .unwrap()
            .parse::<i64>()
            .unwrap();
        return Ok(BencodeVal::Int {
            index: 0,
            int: integer,
            size: index + 3,
        });
    }

    Err(From::from("reached eof"))
}

fn decode_str(bytes: &[u8]) -> Result<BencodeVal, Box<std::error::Error>> {
    if bytes.len() < 2 {
        return Err(From::from("reached eof"));
    }

    if let Some(index) = bytes.iter().position(|&x| !x.is_ascii_digit()) {
        if bytes[index] != b':' {
            return Err(From::from(format!(
                "unexpected char: {}",
                bytes[index] as char
            )));
        }

        let len: usize =
            str::from_utf8(&bytes[..index]).unwrap().parse().unwrap();
        if len + index >= bytes.len() {
            return Err(From::from("reached eof"));
        }

        return Ok(BencodeVal::Str {
            index: 0,
            byte_str: &bytes[index + 1..index + len + 1],
            size: index + len + 1,
        });
    }

    Err(From::from("reached eof"))
}

fn decode_list(bytes: &[u8]) -> Result<BencodeVal, Box<std::error::Error>> {
    if bytes.len() < 2 {
        return Err(From::from("reached eof"));
    }

    if bytes[0] != b'l' {
        return Err(From::from(format!(
            "unexpected char: {}",
            bytes[0] as char
        )));
    }

    if bytes[1] == b'e' {
        return Ok(BencodeVal::List {
            index: 0,
            list: Vec::new(),
            size: 2,
        });
    }

    let mut v: Vec<BencodeVal> = Vec::new();
    let mut index: usize = 1;
    loop {
        if index >= bytes.len() {
            return Err(From::from("reached eof"));
        }

        let val = match bytes[index] {
            b'i' => decode_int(&bytes[index..])?,
            b'l' => decode_list(&bytes[index..])?,
            b'd' => decode_dict(&bytes[index..])?,
            c if c.is_ascii_digit() => decode_str(&bytes[index..])?,
            b'e' => {
                return Ok(BencodeVal::List {
                    index: 0,
                    list: v,
                    size: index + 1,
                });
            }
            _ => {
                return Err(From::from(format!(
                    "unexpected char: {}",
                    bytes[index] as char
                )))
            }
        };

        let (size, actual_val) = match val {
            BencodeVal::Int {
                index: _,
                int,
                size,
            } => (
                size,
                BencodeVal::Int {
                    index: index,
                    int: int,
                    size: size,
                },
            ),
            BencodeVal::Str {
                index: _,
                byte_str,
                size,
            } => (
                size,
                BencodeVal::Str {
                    index: index,
                    byte_str: byte_str,
                    size: size,
                },
            ),
            BencodeVal::List {
                index: _,
                list,
                size,
            } => (
                size,
                BencodeVal::List {
                    index: index,
                    list: list,
                    size: size,
                },
            ),
            BencodeVal::Dict {
                index: _,
                dict,
                size,
            } => (
                size,
                BencodeVal::Dict {
                    index: index,
                    dict: dict,
                    size: size,
                },
            ),
        };

        v.push(actual_val);
        index += size;
    }
}

fn decode_dict(bytes: &[u8]) -> Result<BencodeVal, Box<std::error::Error>> {
    if bytes.len() < 2 {
        return Err(From::from("reached eof"));
    }

    if bytes[0] != b'd' {
        return Err(From::from(format!(
            "unexpected char: {}",
            bytes[0] as char
        )));
    }

    if bytes[1] == b'e' {
        return Ok(BencodeVal::Dict {
            index: 0,
            dict: HashMap::new(),
            size: 2,
        });
    }

    let mut d: HashMap<&[u8], BencodeVal> = HashMap::new();
    let mut index: usize = 1;

    loop {
        if index >= bytes.len() {
            return Err(From::from("reached eof"));
        }

        if bytes[index] == b'e' {
            return Ok(BencodeVal::Dict {
                index: 0,
                dict: d,
                size: index + 1,
            });
        }

        if !bytes[index].is_ascii_digit() {
            return Err(From::from(format!(
                "unexpected char: {}",
                bytes[index] as char
            )));
        }

        let val = decode_str(&bytes[index..])?;
        let key = match val {
            BencodeVal::Str {
                index: _,
                size,
                byte_str,
            } => {
                index += size;
                byte_str
            }
            _ => return Err(From::from("not possible")),
        };

        if index >= bytes.len() {
            return Err(From::from("reached eof"));
        }

        let val = match bytes[index] {
            b'i' => decode_int(&bytes[index..])?,
            b'l' => decode_list(&bytes[index..])?,
            b'd' => decode_dict(&bytes[index..])?,
            c if c.is_ascii_digit() => decode_str(&bytes[index..])?,
            _ => {
                return Err(From::from(format!(
                    "unexpected char: {}",
                    bytes[index] as char
                )))
            }
        };

        let (size, actual_val) = match val {
            BencodeVal::Int {
                index: _,
                int,
                size,
            } => (
                size,
                BencodeVal::Int {
                    index: index,
                    int: int,
                    size: size,
                },
            ),
            BencodeVal::Str {
                index: _,
                byte_str,
                size,
            } => (
                size,
                BencodeVal::Str {
                    index: index,
                    byte_str: byte_str,
                    size: size,
                },
            ),
            BencodeVal::List {
                index: _,
                list,
                size,
            } => (
                size,
                BencodeVal::List {
                    index: index,
                    list: list,
                    size: size,
                },
            ),
            BencodeVal::Dict {
                index: _,
                dict,
                size,
            } => (
                size,
                BencodeVal::Dict {
                    index: index,
                    dict: dict,
                    size: size,
                },
            ),
        };

        d.insert(key, actual_val);
        index += size;
    }
}
