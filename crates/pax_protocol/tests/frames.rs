//! Framing under arbitrary chunking and hostile input (D22, NETWORK_PROTOCOL §2, §9).
//! A hostile peer controls every byte, so nothing here may panic.

mod common;

use common::*;
use flatbuffers::FlatBufferBuilder;
use pax_protocol::wire::*;
use pax_protocol::{Direction, FrameDecoder, FrameError, MAX_CLIENT_FRAME, read_client_message};

fn ping(nonce: u64) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let m = Ping::create(&mut b, &PingArgs { nonce });
    client_frame(&mut b, ClientPayload::Ping, m.as_union_value())
}

/// xorshift64*: deterministic noise for the hostile-input tests, without a dependency.
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

#[test]
fn frames_survive_any_chunking() {
    let stream: Vec<u8> = (0..50).flat_map(ping).collect();
    let mut noise = Noise(0x9E37_79B9_7F4A_7C15);
    for _ in 0..200 {
        let mut d = FrameDecoder::new(Direction::ClientToServer);
        let mut nonces = Vec::new();
        let mut at = 0;
        while at < stream.len() {
            let n = 1 + noise.below(97).min(stream.len() - at - 1);
            d.push(&stream[at..at + n]);
            at += n;
            while let Some(frame) = d.next_frame().unwrap() {
                nonces.push(read_client_message(&frame).unwrap().payload_as_ping().unwrap().nonce());
            }
        }
        assert_eq!(nonces, (0..50).collect::<Vec<u64>>());
        assert_eq!(d.buffered(), 0);
    }
}

#[test]
fn an_oversized_length_is_refused_before_its_body_arrives() {
    let mut d = FrameDecoder::new(Direction::ClientToServer);
    d.push(&(MAX_CLIENT_FRAME as u32 + 1).to_le_bytes());
    assert_eq!(d.next_frame(), Err(FrameError::TooLarge { len: MAX_CLIENT_FRAME + 1, limit: MAX_CLIENT_FRAME }));
    // Nothing is buffered, and the decoder stays failed: the session is closing.
    d.push(&ping(1));
    assert_eq!(d.buffered(), 0);
    assert!(d.next_frame().is_err());
}

#[test]
fn a_maximal_length_waits_for_its_body() {
    let mut d = FrameDecoder::new(Direction::ClientToServer);
    d.push(&(MAX_CLIENT_FRAME as u32).to_le_bytes());
    d.push(&[0; 100]);
    assert_eq!(d.next_frame(), Ok(None));
}

#[test]
fn an_empty_frame_is_refused() {
    let mut d = FrameDecoder::new(Direction::ClientToServer);
    d.push(&[0, 0, 0, 0]);
    assert_eq!(d.next_frame(), Err(FrameError::Empty));
}

#[test]
fn garbage_and_truncated_frames_are_errors_not_panics() {
    let valid = ping(42);
    let mut noise = Noise(0xD1B5_4A32_D192_ED03);
    let mut read_ok = 0;
    for round in 0..20_000 {
        // Mix of pure noise, a valid frame with flipped bytes, and truncated frames.
        let frame: Vec<u8> = match round % 3 {
            0 => {
                let len = noise.below(64);
                let mut f = (len as u32).to_le_bytes().to_vec();
                f.extend((0..len).map(|_| noise.next() as u8));
                f
            }
            1 => {
                let mut f = valid.clone();
                for _ in 0..1 + noise.below(4) {
                    let at = 4 + noise.below(f.len() - 4);
                    f[at] = noise.next() as u8;
                }
                f
            }
            _ => valid[..noise.below(valid.len())].to_vec(),
        };
        let mut d = FrameDecoder::new(Direction::ClientToServer);
        d.push(&frame);
        if let Ok(Some(frame)) = d.next_frame()
            && let Ok(msg) = read_client_message(&frame)
        {
            // A verified message may hold altered values, but reading it must be safe.
            let _ = msg.payload_as_ping().map(|p| p.nonce());
            read_ok += 1;
        }
    }
    assert!(read_ok > 0, "the flipped-byte cases should sometimes still verify");
}
