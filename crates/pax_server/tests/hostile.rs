//! Hostile input over real TCP (M3-10, D22, NETWORK_PROTOCOL §9). Many connections
//! at once send noise and damaged frames, before and after a valid `Hello`, and then
//! vanish without closing. The server must end each session cleanly, never panic,
//! and keep serving well-behaved clients. `Server::shutdown` reports a sim-thread
//! panic as an error, so a clean shutdown proves there was none.
//!
//! The deeper per-request coverage is in `src/hostile.rs`.

mod common;

use std::io::Write;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use common::*;

/// xorshift64*: deterministic noise without a dependency.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A frame from `valid` with a few bytes after the length prefix changed.
fn damaged(noise: &mut Noise, valid: &[Vec<u8>]) -> Vec<u8> {
    let mut f = valid[noise.below(valid.len())].clone();
    for _ in 0..1 + noise.below(3) {
        let at = 4 + noise.below(f.len() - 4);
        f[at] = noise.next() as u8;
    }
    f
}

/// One hostile connection: maybe a valid `Hello`, then damaged frames, noise, or a
/// frame split mid-way, and then it vanishes without reading anything.
fn attack(addr: std::net::SocketAddr, seed: u64) {
    let valid = [hello_frame(pax_protocol::PROTOCOL_MAJOR, None), ping_frame(seed)];
    let mut noise = Noise(seed);
    let Ok(mut stream) = TcpStream::connect(addr) else { return };
    let mut bytes = Vec::new();
    if noise.below(2) == 0 {
        bytes.extend_from_slice(&valid[0]);
    }
    for _ in 0..1 + noise.below(6) {
        match noise.below(4) {
            0 => bytes.extend((0..noise.below(64)).map(|_| noise.next() as u8)),
            1 => bytes.extend_from_slice(&valid[1]),
            _ => bytes.extend(damaged(&mut noise, &valid)),
        }
    }
    // Writes may fail once the server has said Goodbye and closed: that's the point.
    let mut at = 0;
    while at < bytes.len() {
        let end = (at + 1 + noise.below(48)).min(bytes.len());
        if stream.write_all(&bytes[at..end]).is_err() {
            return;
        }
        at = end;
    }
}

#[test]
fn hostile_connections_never_take_the_server_down() {
    let server = two_states();
    let addr = server.local_addr();
    let threads: Vec<_> = (0..4u64)
        .map(|t| {
            std::thread::spawn(move || {
                for i in 0..40u64 {
                    attack(addr, 0x5EED_0000 + t * 1_000 + i);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }

    // A hostile session may still hold the single seat until the server sees its
    // connection close, so a well-behaved client retries briefly.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut c = Client::connect(addr);
        c.hello(Some(0));
        match c.next() {
            Got::Welcome { .. } => break,
            Got::Rejected(reason) if Instant::now() < deadline => {
                assert!(reason.contains("server full"), "unexpected rejection: {reason}");
                std::thread::sleep(Duration::from_millis(50));
            }
            other => panic!("the server should still serve a good client, got {other:?}"),
        }
    }
    server.shutdown().expect("no panic on the sim thread");
}
