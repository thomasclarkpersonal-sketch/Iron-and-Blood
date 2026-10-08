//! A blocking test client for `pax_server`: real TCP, real frames.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use flatbuffers::FlatBufferBuilder;
use pax_protocol::wire::*;
use pax_protocol::{Direction, FrameDecoder, read_server_message};
use pax_server::{Config, Server};

pub fn scenario(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios").join(name)
}

pub fn start(config: Config) -> Server {
    Server::start(config).expect("server starts")
}

pub fn two_states() -> Server {
    start(Config::local(scenario("two_states")))
}

/// What the server sent, decoded into what the tests check.
#[derive(Debug, PartialEq)]
pub enum Got {
    Welcome {
        nation: Option<u32>,
        day: u64,
        provinces: Vec<String>,
        nations: Vec<String>,
        content_hash: u64,
    },
    Rejected(String),
    Goodbye(String),
    Pong(u64),
    Other(String),
    /// The server closed the connection.
    Closed,
}

pub struct Client {
    stream: TcpStream,
    decoder: FrameDecoder,
}

impl Client {
    pub fn connect(addr: SocketAddr) -> Client {
        let stream = TcpStream::connect(addr).expect("connects");
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        Client { stream, decoder: FrameDecoder::new(Direction::ServerToClient) }
    }

    pub fn send_raw(&mut self, bytes: &[u8]) {
        let _ = self.stream.write_all(bytes);
    }

    fn send(
        &mut self,
        b: &mut FlatBufferBuilder<'_>,
        kind: ClientPayload,
        payload: flatbuffers::WIPOffset<flatbuffers::UnionWIPOffset>,
    ) {
        let msg = ClientMessage::create(b, &ClientMessageArgs { payload_type: kind, payload: Some(payload) });
        finish_size_prefixed_client_message_buffer(b, msg);
        let frame = b.finished_data().to_vec();
        self.send_raw(&frame);
    }

    pub fn hello_with(&mut self, major: u16, nation: Option<u32>) {
        let mut b = FlatBufferBuilder::new();
        let name = b.create_string("test client");
        let h = Hello::create(
            &mut b,
            &HelloArgs {
                protocol_major: major,
                protocol_minor: 0,
                client_name: Some(name),
                requested_nation: nation,
                resume_token: 0,
            },
        );
        self.send(&mut b, ClientPayload::Hello, h.as_union_value());
    }

    pub fn hello(&mut self, nation: Option<u32>) {
        self.hello_with(pax_protocol::PROTOCOL_MAJOR, nation);
    }

    pub fn ping(&mut self, nonce: u64) {
        let mut b = FlatBufferBuilder::new();
        let p = Ping::create(&mut b, &PingArgs { nonce });
        self.send(&mut b, ClientPayload::Ping, p.as_union_value());
    }

    /// The next message, or `Closed` once the server has closed the connection.
    pub fn next(&mut self) -> Got {
        let mut buf = [0u8; 64 * 1024];
        loop {
            match self.decoder.next_frame().expect("server frames are well-formed") {
                Some(frame) => return decode(&frame),
                None => match self.stream.read(&mut buf) {
                    Ok(0) => return Got::Closed,
                    Ok(n) => self.decoder.push(&buf[..n]),
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                        ) =>
                    {
                        return Got::Closed;
                    }
                    Err(e) => panic!("no message from the server: {e}"),
                },
            }
        }
    }
}

fn decode(frame: &[u8]) -> Got {
    let msg = read_server_message(frame).expect("verified server frame");
    let strings = |v: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<&str>>>| {
        v.map(|v| v.iter().map(str::to_owned).collect()).unwrap_or_default()
    };
    if let Some(w) = msg.payload_as_welcome() {
        let defs = w.defs().expect("Welcome carries StaticData");
        return Got::Welcome {
            nation: w.nation(),
            day: w.day(),
            provinces: strings(defs.provinces()),
            nations: defs
                .nations()
                .map(|n| n.iter().map(|n| n.key().unwrap_or_default().to_owned()).collect())
                .unwrap_or_default(),
            content_hash: w.content_hash(),
        };
    }
    if let Some(r) = msg.payload_as_rejected() {
        return Got::Rejected(r.reason().unwrap_or_default().to_owned());
    }
    if let Some(g) = msg.payload_as_goodbye() {
        return Got::Goodbye(g.reason().unwrap_or_default().to_owned());
    }
    if let Some(p) = msg.payload_as_pong() {
        return Got::Pong(p.nonce());
    }
    Got::Other(format!("{:?}", msg.payload_type()))
}
