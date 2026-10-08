//! Hostile input (M3-10, D22, NETWORK_PROTOCOL §9): a peer controls every byte and
//! every value it sends, so nothing it sends may panic the server.
//!
//! These are deterministic, seeded tests that run on stable Rust in every CI run:
//! * the request decoder gets noise, and every request type with bytes flipped and
//!   truncated;
//! * the sim thread gets long random sequences of well-formed requests with hostile
//!   values (unknown enums, out-of-range ids and rates, path-like save names, any
//!   session id), interleaved with ticks, connections and disconnections.
//!
//! The `fuzz/` directory adds a coverage-guided `cargo fuzz` target for the frame
//! reader (nightly only).

use std::path::PathBuf;

use flatbuffers::{FlatBufferBuilder, UnionWIPOffset, WIPOffset};
use pax_protocol::{PROTOCOL_MAJOR, wire};
use tokio::sync::mpsc::Receiver;

use crate::net::{ConnHandle, Inbound, Outbound};
use crate::request::{self, Request, WireCommand};
use crate::sim::Sim;

use crate::noise::Noise;

/// What a hostile client would send, on top of the shared generator.
impl Noise {
    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }

    /// A value a hostile client would try: edges, near-valid, or anything.
    fn int(&mut self, valid_below: u64) -> u64 {
        match self.below(5) {
            0 => 0,
            1 => valid_below.saturating_sub(1),
            2 => valid_below,
            3 => u64::MAX,
            _ => self.next(),
        }
    }

    fn rate(&mut self) -> i64 {
        [i64::MIN, -1, 0, 1, 500_000, 1_000_000, 1_000_001, i64::MAX][self.below(8) as usize]
    }

    fn name(&mut self) -> Option<String> {
        const NAMES: [&str; 8] = ["ok", "", "../escape", "/abs", "a.b", "con", "üñï", "save-1_A"];
        match self.below(10) {
            0 => None,
            1 => Some("x".repeat(65)),
            i => Some(NAMES[i as usize - 2].to_owned()),
        }
    }
}

fn frame<'a>(b: &mut FlatBufferBuilder<'a>, kind: wire::ClientPayload, payload: WIPOffset<UnionWIPOffset>) -> Vec<u8> {
    let msg = wire::ClientMessage::create(b, &wire::ClientMessageArgs { payload_type: kind, payload: Some(payload) });
    wire::finish_size_prefixed_client_message_buffer(b, msg);
    b.finished_data().to_vec()
}

/// One valid frame of every client message type that carries fields.
fn valid_frames() -> Vec<Vec<u8>> {
    use wire::ClientPayload as P;
    let mut frames = Vec::new();
    let mut b = FlatBufferBuilder::new();
    let name = b.create_string("client");
    let args = wire::HelloArgs {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: 0,
        client_name: Some(name),
        requested_nation: Some(1),
        resume_token: 7,
    };
    let h = wire::Hello::create(&mut b, &args);
    frames.push(frame(&mut b, P::Hello, h.as_union_value()));

    let mut b = FlatBufferBuilder::new();
    let rate = wire::Fixed::new(150_000);
    let c = wire::SetIncomeTax::create(&mut b, &wire::SetIncomeTaxArgs { nation: 1, rate: Some(&rate) });
    let s = wire::SubmitCommand::create(
        &mut b,
        &wire::SubmitCommandArgs {
            client_seq: 3,
            command_type: wire::Command::SetIncomeTax,
            command: Some(c.as_union_value()),
        },
    );
    frames.push(frame(&mut b, P::SubmitCommand, s.as_union_value()));

    let mut b = FlatBufferBuilder::new();
    let args = wire::SubscribeArgs { map_mode: wire::MapMode::Price, map_good: 2, market: Some(0), province: Some(3) };
    let s = wire::Subscribe::create(&mut b, &args);
    frames.push(frame(&mut b, P::Subscribe, s.as_union_value()));

    let mut b = FlatBufferBuilder::new();
    let name = b.create_string("autumn");
    let s = wire::SaveGame::create(&mut b, &wire::SaveGameArgs { name: Some(name) });
    frames.push(frame(&mut b, P::SaveGame, s.as_union_value()));

    let mut b = FlatBufferBuilder::new();
    let a = wire::Ack::create(&mut b, &wire::AckArgs { day: 12 });
    frames.push(frame(&mut b, P::Ack, a.as_union_value()));
    frames
}

#[test]
fn the_request_decoder_survives_noise_flipped_bytes_and_truncation() {
    let valid = valid_frames();
    for f in &valid {
        assert!(request::decode(f).is_ok(), "the unmodified frames decode");
    }
    let mut noise = Noise::new(0x9E37_79B9_7F4A_7C15);
    let mut decoded = 0;
    for round in 0..40_000u64 {
        let base = &valid[(round % valid.len() as u64) as usize];
        let mut f = base.clone();
        match round % 4 {
            // Pure noise behind a matching length prefix.
            0 => {
                let len = 8 + noise.below(120) as usize;
                f = (len as u32).to_le_bytes().to_vec();
                f.extend((0..len).map(|_| noise.next() as u8));
            }
            // A few bytes of a valid frame changed (prefix and identifier kept, so the
            // verifier and the decoder see the damage).
            1 | 2 => {
                for _ in 0..1 + noise.below(4) {
                    let at = 12 + noise.below(f.len() as u64 - 12) as usize;
                    f[at] = noise.next() as u8;
                }
            }
            // Truncated, with the prefix rewritten to match.
            _ => {
                f.truncate(4 + noise.below(f.len() as u64 - 4) as usize);
                let len = (f.len() - 4) as u32;
                f[..4].copy_from_slice(&len.to_le_bytes());
            }
        }
        if request::decode(&f).is_ok() {
            decoded += 1;
        }
    }
    assert!(decoded > 1_000, "flipped bytes often still verify, so the decoder sees odd values ({decoded})");
}

/// A well-formed request with hostile values.
fn hostile_request(n: &mut Noise) -> Request {
    let nation = |n: &mut Noise| n.int(2) as u32;
    let opt = |n: &mut Noise, below: u64| (!n.chance(3)).then(|| n.int(below) as u32);
    match n.below(11) {
        0 => Request::Hello {
            major: if n.chance(2) { PROTOCOL_MAJOR } else { n.next() as u16 },
            minor: n.next() as u16,
            name: n.name(),
            requested_nation: opt(n, 2),
            resume_token: n.next(),
        },
        1..=3 => {
            let rate_raw = (!n.chance(6)).then(|| n.rate());
            let command = match n.below(4) {
                0 => None,
                1 => Some(WireCommand::SetIncomeTax { nation: nation(n), rate_raw }),
                2 => Some(WireCommand::SetTransferRate { nation: nation(n), rate_raw }),
                _ => Some(WireCommand::SetConsumptionRate { nation: nation(n), rate_raw }),
            };
            Request::SubmitCommand { client_seq: n.next() as u32, command }
        }
        4 => Request::SetSpeed { speed: wire::Speed(n.below(9) as u8) },
        5 => Request::Subscribe {
            map_mode: wire::MapMode(n.below(10) as u8),
            map_good: n.int(4) as u16,
            market: opt(n, 2),
            province: opt(n, 4),
        },
        6 => Request::Ack { day: n.int(400) },
        7 => Request::SaveGame { name: n.name() },
        8 => Request::LoadGame { name: n.name() },
        9 => Request::Kick { player: n.int(4) as u16 },
        _ => Request::ListSaves,
    }
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Thousands of hostile requests from several sessions, with ticks in between. The
/// sim thread must never panic, and every tick still checks money conservation (D5).
#[test]
fn the_sim_thread_survives_hostile_requests() {
    let root = TempDir(std::env::temp_dir().join(format!("pax-hostile-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&root.0);
    std::fs::create_dir_all(&root.0).unwrap();
    let saves = root.0.join("saves");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
    let mut config = crate::Config::local(&dir);
    config.saves_dir = saves.clone();
    // Several players, so taken nations, the host's rules and kicks are exercised too.
    config.max_players = 3;
    let mut sim = Sim::new(pax_data::load_scenario(&dir).unwrap(), &config);
    let mut receivers: Vec<Receiver<Outbound>> = Vec::new();
    let mut noise = Noise::new(0xC0FF_EE00_DEAD_BEEF);
    // Sessions as `net` produces them: a fresh id, `Connected` first, `Closed` last.
    // Within that, anything goes: `net` normally stops a repeated or missing `Hello`,
    // but the sim thread is tested without relying on it.
    let mut open: Vec<u64> = Vec::new();
    let mut next_session = 1;
    for round in 0..6_000u64 {
        match noise.below(40) {
            0 if open.len() < 4 => {
                let (conn, rx) = ConnHandle::for_test();
                receivers.push(rx);
                sim.handle(Inbound::Connected { session: next_session, conn });
                open.push(next_session);
                next_session += 1;
            }
            1 if !open.is_empty() => {
                let session = open.swap_remove(noise.index(open.len()));
                sim.handle(Inbound::Closed { session });
            }
            2..=5 => sim.tick(),
            _ if !open.is_empty() => {
                let session = open[noise.index(open.len())];
                let request = hostile_request(&mut noise);
                sim.handle(Inbound::Request { session, request });
            }
            _ => {}
        }
        // Keep the test connections' queues from filling.
        if round % 64 == 0 {
            for rx in &mut receivers {
                while rx.try_recv().is_ok() {}
            }
        }
    }
    assert!(sim.world_day() > 100, "the game kept running");
    // The run reached the deep paths: commands applied, and games saved.
    assert!(sim.log().iter().any(|l| l.player.is_some()), "some hostile session's commands applied");
    let saved = std::fs::read_dir(&saves).map_or(0, |d| d.count());
    assert!(saved > 0, "some saves were written");
    // Path-like names never wrote outside the saves directory.
    for entry in std::fs::read_dir(&root.0).unwrap() {
        assert_eq!(entry.unwrap().path(), saves, "only the saves directory was created");
    }
}
