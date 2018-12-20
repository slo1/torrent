use std::collections::HashMap;
use std::fmt;
use std::str;

pub enum BencodeVal<'a> {
    Int(i64),
    Str(&'a [u8]),
    List(Vec<BencodeVal<'a>>),
    Dict(HashMap<&'a [u8], BencodeVal<'a>>),
}

impl<'a> fmt::Debug for BencodeVal<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BencodeVal::Int(i) => write!(f, "{}", i),
            BencodeVal::Str(s) => match str::from_utf8(s) {
                Ok(s) => write!(f, "{:?}", s),
                _ => write!(f, "{:?}", s),
            },
            BencodeVal::List(l) => write!(f, "{:?}", l),
            BencodeVal::Dict(d) => {
                write!(f, "{{")?;
                let mut first = true;
                for (key, val) in d.iter() {
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

struct BdecodeContext<'a> {
    val: BencodeVal<'a>,
    index: usize,
}

pub fn decode(bytes: &[u8]) -> Result<BencodeVal, Box<std::error::Error>> {
    match bytes.iter().next() {
        Some(&c) => match c {
            b'i' => Ok(decode_int(bytes)?.val),
            b'l' => Ok(decode_list(bytes)?.val),
            b'd' => Ok(decode_dict(bytes)?.val),
            _ if c.is_ascii_digit() => Ok(decode_str(bytes)?.val),
            _ => Err(From::from(format!("unexpected char: {}", c as char))),
        },
        None => Err(From::from("reached eof")),
    }
}

fn decode_int(bytes: &[u8]) -> Result<BdecodeContext, Box<std::error::Error>> {
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
        return Ok(BdecodeContext {
            val: BencodeVal::Int(0),
            index: 3,
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
        return Ok(BdecodeContext {
            val: BencodeVal::Int(integer),
            index: index + 3,
        });
    }

    Err(From::from("reached eof"))
}

fn decode_str(bytes: &[u8]) -> Result<BdecodeContext, Box<std::error::Error>> {
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

        return Ok(BdecodeContext {
            val: BencodeVal::Str(&bytes[index + 1..index + len + 1]),
            index: index + len + 1,
        });
    }

    Err(From::from("reached eof"))
}

fn decode_list(bytes: &[u8]) -> Result<BdecodeContext, Box<std::error::Error>> {
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
        return Ok(BdecodeContext {
            val: BencodeVal::List(Vec::new()),
            index: 2,
        });
    }

    let mut v: Vec<BencodeVal> = Vec::new();
    let mut index: usize = 1;
    loop {
        if index >= bytes.len() {
            return Err(From::from("reached eof"));
        }

        let context = match bytes[index] {
            b'i' => decode_int(&bytes[index..])?,
            b'l' => decode_list(&bytes[index..])?,
            b'd' => decode_dict(&bytes[index..])?,
            c if c.is_ascii_digit() => decode_str(&bytes[index..])?,
            b'e' => {
                return Ok(BdecodeContext {
                    val: BencodeVal::List(v),
                    index: index + 1,
                });
            }
            _ => {
                return Err(From::from(format!(
                    "unexpected char: {}",
                    bytes[index] as char
                )))
            }
        };

        v.push(context.val);
        index += context.index;
    }
}

fn decode_dict(bytes: &[u8]) -> Result<BdecodeContext, Box<std::error::Error>> {
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
        return Ok(BdecodeContext {
            val: BencodeVal::Dict(HashMap::new()),
            index: 2,
        });
    }

    let mut d: HashMap<&[u8], BencodeVal> = HashMap::new();
    let mut index: usize = 1;

    loop {
        if index >= bytes.len() {
            return Err(From::from("reached eof"));
        }

        if bytes[index] == b'e' {
            return Ok(BdecodeContext {
                val: BencodeVal::Dict(d),
                index: index + 1,
            });
        }

        if !bytes[index].is_ascii_digit() {
            return Err(From::from(format!(
                "unexpected char: {}",
                bytes[index] as char
            )));
        }

        let context = decode_str(&bytes[index..])?;
        let key = match context.val {
            BencodeVal::Str(s) => s,
            _ => return Err(From::from("not possible")),
        };

        index += context.index;

        if index >= bytes.len() {
            return Err(From::from("reached eof"));
        }

        let context = match bytes[index] {
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

        d.insert(key, context.val);
        index += context.index;
    }
}
