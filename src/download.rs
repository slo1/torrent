use std::cmp::{self, Ordering};
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpStream};
use std::panic;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

#[derive(Copy, Clone)]
pub struct Peer {
    pub id: [u8; 20],
    pub socket: SocketAddr,
}

enum WorkerMsg {
    Bitfield {
        socket: SocketAddr,
        bitfield: Vec<u8>,
    },
    Piece {
        socket: SocketAddr,
        index: u32,
        buffer: Vec<u8>,
    },
}

struct Job {
    index: u32,
    length: u64,
    downloaded_length: u64,
    hash: [u8; 20],
}

enum ManagerMsg {
    JobMsg {
        index: u32,
        length: u64,
        hash: [u8; 20],
    },
    Done,
}

struct Worker {
    peer: Peer,
    sender: Sender<ManagerMsg>,
    thread: thread::JoinHandle<()>,
}

struct PeerMsg {
    id: u8,
    payload: Option<Vec<u8>>,
}

impl Worker {
    pub fn new(
        peer: Peer,
        to_manager: Sender<WorkerMsg>,
        info_hash: [u8; 20],
    ) -> Option<Worker> {
        let (to_me, from_manager) = mpsc::channel();

        let (handshake_tx, handshake_rx) = mpsc::channel();

        let thread = thread::spawn(move || {
            let mut stream = match handshake(&peer, &info_hash) {
                Ok(stream) => stream,
                Err(e) => {
                    println!("Handshake failed: {}", e);
                    handshake_tx.send(false).unwrap();
                    return;
                }
            };
            handshake_tx.send(true).unwrap();

            let mut job_queue = VecDeque::new();
            let mut piece_buffer = vec![];

            loop {
                let peer_msg = match read_msg(&mut stream) {
                    Ok(Some(peer_msg)) => peer_msg,
                    Ok(None) => continue,
                    Err(e) => {
                        println!("{}", e);
                        continue;
                    }
                };

                if handle_peer_msg(
                    &mut stream,
                    &peer,
                    peer_msg,
                    &to_manager,
                    &from_manager,
                    &mut job_queue,
                    &mut piece_buffer,
                ) == ThreadState::Dead
                {
                    break;
                }
            }
        });

        if handshake_rx.recv().unwrap() == false {
            return None;
        }

        Some(Worker {
            peer,
            sender: to_me,
            thread,
        })
    }
}

#[derive(PartialEq)]
pub enum JobState {
    Done,
    Downloading,
    Available,
}

pub struct Piece {
    pub index: u32,
    pub peers: Vec<SocketAddr>,
    pub job_state: JobState,
    pub length: u64,
    pub hash: [u8; 20],
}

impl Ord for Piece {
    fn cmp(&self, other: &Piece) -> Ordering {
        self.peers.len().cmp(&other.peers.len())
    }
}

impl PartialOrd for Piece {
    fn partial_cmp(&self, other: &Piece) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Piece {
    fn eq(&self, other: &Piece) -> bool {
        self.peers.len() == other.peers.len()
    }
}

impl Eq for Piece {}

impl Piece {
    pub fn new(index: u32, length: u64, hash: [u8; 20]) -> Piece {
        Piece {
            index,
            peers: vec![],
            job_state: JobState::Available,
            length,
            hash,
        }
    }
}

pub fn download_from(
    pieces: &mut Vec<Piece>,
    peers: Vec<Peer>,
    info_hash: [u8; 20],
    piece_len: u64,
) {
    let (to_me, from_worker) = mpsc::channel();
    let mut workers = vec![];
    for peer in peers {
        let to_me = to_me.clone();
        if let Some(worker) = Worker::new(peer, to_me, info_hash) {
            workers.push(worker);
        }
    }

    let mut num_bitfields = 0;
    let mut pieces_table = vec![];
    let mut size = 0;
    let mut workers_done = 0;
    for msg in from_worker {
        match msg {
            WorkerMsg::Bitfield { socket, bitfield } => {
                for (i, byte) in bitfield.iter().enumerate() {
                    for j in 0..8 {
                        if ((0b1000_0000 >> j) & byte) != 0 {
                            pieces[i * 8 + j].peers.push(socket);
                        }
                    }
                }

                num_bitfields += 1;
                if num_bitfields == workers.len() {
                    pieces.sort();
                    for worker in &workers {
                        if !give_out_job(pieces, worker) {
                            panic!("no more jobs. impossible");
                        }
                    }
                }
            }
            WorkerMsg::Piece {
                socket,
                index,
                buffer,
            } => {
                size += buffer.len();
                pieces_table.push((index, buffer));
                if size > (2 << 30) {
                    pieces_table.sort();
                    match write_pieces_table_to_file(&pieces_table, piece_len) {
                        Ok(_) => println!("wrote to file"),
                        _ => println!("failed to write to file"),
                    }
                    pieces_table.clear();
                }
                let worker =
                    workers.iter().find(|&x| x.peer.socket == socket).unwrap();
                let mut piece =
                    pieces.iter_mut().find(|x| x.index == index).unwrap();
                piece.job_state = JobState::Done;
                if !give_out_job(pieces, worker) {
                    workers_done += 1;
                    if workers_done == workers.len() {
                        for worker in &workers {
                            worker.sender.send(ManagerMsg::Done).unwrap();
                        }
                        break;
                    }
                }
            }
        }
    }

    match write_pieces_table_to_file(&pieces_table, piece_len) {
        Ok(_) => println!("wrote to file"),
        _ => println!("failed to write to file"),
    }
    pieces_table.clear();

    for worker in workers {
        let _ = worker.thread.join();
    }
}

fn write_pieces_table_to_file(
    pieces_table: &Vec<(u32, Vec<u8>)>,
    piece_length: u64,
) -> std::io::Result<()> {
    let mut f = OpenOptions::new().write(true).create(true).open("part")?;
    for (index, buffer) in pieces_table {
        let index: u64 = *index as u64;
        f.seek(SeekFrom::Start(index * piece_length))?;
        f.write_all(&buffer)?;
    }

    f.sync_all()?;
    Ok(())
}

fn read_msg(
    stream: &mut TcpStream,
) -> Result<Option<PeerMsg>, Box<std::error::Error>> {
    let mut len = [0u8; 4];
    let mut len_bytes_read = 0;
    while len_bytes_read < 4 {
        len_bytes_read += stream.read(&mut len[len_bytes_read..])?;
    }
    let len = unsafe { std::mem::transmute::<[u8; 4], u32>(len) }.to_be();
    if len == 0 {
        return Ok(None);
    }

    let len = len - 1;

    let mut id = [0u8];
    let mut id_bytes_read = 0;
    while id_bytes_read != 1 {
        id_bytes_read = stream.read(&mut id)?;
    }
    let id = id[0];

    let mut payload = None;
    if len != 0 {
        let mut buffer = vec![0u8; len as usize];
        let mut payload_bytes_read: usize = 0;
        while payload_bytes_read < len as usize {
            payload_bytes_read +=
                stream.read(&mut buffer[payload_bytes_read..])?;
        }
        payload = Some(buffer);
    }

    Ok(Some(PeerMsg { id, payload }))
}

fn handshake(
    peer: &Peer,
    info_hash: &[u8; 20],
) -> Result<TcpStream, Box<std::error::Error>> {
    let mut stream =
        TcpStream::connect_timeout(&peer.socket, Duration::from_secs(17))?;
    let pstr = "BitTorrent protocol".as_bytes();
    let pstrlen = pstr.len() as u8;
    let reserved = [0u8; 8];
    let id = "01234567890123456789".as_bytes();

    let mut buffer = vec![];
    buffer.push(pstrlen);
    buffer.extend(pstr);
    buffer.extend(&reserved);
    buffer.extend(info_hash);
    let id_offset = buffer.len();
    buffer.extend(id);

    stream.write(&buffer)?;

    let mut response = vec![0u8; buffer.len()];

    let mut n = 0;
    loop {
        let bytes_read = stream.read(&mut response[n..])?;
        if bytes_read == 0 {
            return Err(From::from("can't read any more bytes"));
        }

        n += bytes_read;

        if n == buffer.len() {
            let response_peer_id =
                &response[id_offset..id_offset + peer.id.len()];
            if response_peer_id != peer.id {
                return Err(From::from("ids don't match"));
            }

            println!("Completed handshake with {}", peer.socket.to_string());
            return Ok(stream);
        }
    }
}

fn handle_peer_msg(
    stream: &mut TcpStream,
    peer: &Peer,
    peer_msg: PeerMsg,
    to_manager: &Sender<WorkerMsg>,
    from_manager: &Receiver<ManagerMsg>,
    job_queue: &mut VecDeque<Job>,
    piece_buffer: &mut Vec<u8>,
) -> ThreadState {
    match peer_msg.id {
        1 => request_block(stream, job_queue),
        5 => handle_bitfield_msg(
            stream,
            peer,
            peer_msg,
            to_manager,
            from_manager,
            job_queue,
        ),
        7 => handle_piece_msg(
            stream,
            peer,
            peer_msg,
            job_queue,
            piece_buffer,
            from_manager,
            to_manager,
        ),
        id => {
            println!("unhandled request {}", id);
            return ThreadState::Alive;
        }
    }
}

fn request_block(
    stream: &mut TcpStream,
    job_queue: &mut VecDeque<Job>,
) -> ThreadState {
    if let Some(Job {
        index,
        length,
        downloaded_length,
        hash: _,
    }) = job_queue.front()
    {
        let mut buffer = vec![];
        let length_left = length - downloaded_length;
        let req_len: u32 = cmp::min(length_left as u32, 2 << 14);
        let begin = *downloaded_length as u32;

        let len: [u8; 4] = unsafe { std::mem::transmute(13_u32.to_be()) };
        let index: [u8; 4] = unsafe { std::mem::transmute(index.to_be()) };
        let begin: [u8; 4] = unsafe { std::mem::transmute(begin.to_be()) };
        let req_len: [u8; 4] = unsafe { std::mem::transmute(req_len.to_be()) };

        buffer.extend(&len);
        buffer.push(6u8);
        buffer.extend(&index);
        buffer.extend(&begin);
        buffer.extend(&req_len);
        stream.write(&buffer).unwrap();
    }

    ThreadState::Alive
}

fn handle_bitfield_msg(
    stream: &mut TcpStream,
    peer: &Peer,
    peer_msg: PeerMsg,
    to_manager: &Sender<WorkerMsg>,
    from_manager: &Receiver<ManagerMsg>,
    job_queue: &mut VecDeque<Job>,
) -> ThreadState {
    if let Some(payload) = peer_msg.payload {
        to_manager
            .send(WorkerMsg::Bitfield {
                socket: peer.socket,
                bitfield: payload,
            })
            .unwrap();
        match from_manager.recv() {
            Ok(ManagerMsg::JobMsg {
                index,
                length,
                hash,
            }) => {
                job_queue.push_back(Job {
                    index,
                    length,
                    downloaded_length: 0,
                    hash,
                });
            }
            _ => panic!("impossible"),
        }

        let interested = vec![0u8, 0u8, 0u8, 1u8, 2u8];
        stream.write(&interested).unwrap();
    }

    ThreadState::Alive
}

#[derive(PartialEq)]
enum ThreadState {
    Dead,
    Alive,
}

fn handle_piece_msg(
    stream: &mut TcpStream,
    peer: &Peer,
    peer_msg: PeerMsg,
    job_queue: &mut VecDeque<Job>,
    piece_buffer: &mut Vec<u8>,
    from_manager: &Receiver<ManagerMsg>,
    to_manager: &Sender<WorkerMsg>,
) -> ThreadState {
    if let Some(payload) = peer_msg.payload {
        let payload = &payload[8..];
        piece_buffer.extend(payload);
        if let Some(Job {
            index,
            length,
            downloaded_length,
            hash,
        }) = job_queue.front_mut()
        {
            *downloaded_length = *downloaded_length + payload.len() as u64;
            let length_left = *length - *downloaded_length;
            if length_left != 0 {
                request_block(stream, job_queue);
            } else {
                if *hash == sha1::Sha1::from(&piece_buffer).digest().bytes() {
                    to_manager
                        .send(WorkerMsg::Piece {
                            socket: peer.socket,
                            index: *index,
                            buffer: piece_buffer.to_vec(),
                        })
                        .unwrap();
                    piece_buffer.clear();
                    match from_manager.recv() {
                        Ok(ManagerMsg::JobMsg {
                            index,
                            length,
                            hash,
                        }) => {
                            job_queue.push_back(Job {
                                index,
                                length,
                                downloaded_length: 0,
                                hash,
                            });
                        }
                        Ok(ManagerMsg::Done) => {
                            return ThreadState::Dead;
                        }
                        _ => panic!("impossible"),
                    }
                    job_queue.pop_front();
                    request_block(stream, job_queue);
                } else {
                    *downloaded_length = 0;
                    request_block(stream, job_queue);
                }
            }
        }
    }

    ThreadState::Alive
}

fn give_out_job(pieces: &mut Vec<Piece>, worker: &Worker) -> bool {
    for piece in pieces.iter_mut() {
        if piece.job_state == JobState::Available {
            if piece.peers.iter().any(|&x| x == worker.peer.socket) {
                worker
                    .sender
                    .send(ManagerMsg::JobMsg {
                        index: piece.index,
                        length: piece.length,
                        hash: piece.hash,
                    })
                    .unwrap();
                piece.job_state = JobState::Downloading;
                return true;
            }
        }
    }

    false
}
