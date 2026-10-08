//! A blocking test client for `pax_server`: real TCP, real frames.
#![allow(dead_code)]

#[path = "../../src/noise.rs"]
pub mod noise;

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
        player: u16,
        nation: Option<u32>,
        day: u64,
        provinces: Vec<String>,
        nations: Vec<String>,
        content_hash: u64,
        map_hash: Option<u64>,
    },
    Rejected(String),
    Goodbye(String),
    Pong(u64),
    DayUpdate {
        day: u64,
        population: u64,
        map_values: Option<usize>,
        market: Option<u32>,
        province: Option<u32>,
        skipped: u32,
        tax_rates: Vec<i64>,
        state_hash: u64,
    },
    CommandResult {
        client_seq: u32,
        error: CommandError,
        applies_on_day: u64,
    },
    ServerState {
        day: u64,
        speed: Speed,
        changed_by: u16,
        /// Players a fairness pause waits for (D24).
        waiting_for: Vec<u16>,
    },
    SaveResult {
        name: String,
        error: String,
    },
    SaveList(Vec<String>),
    /// `LobbyState` (M4-2): each player's `(player, nation, ready)`, whether the game
    /// started, and the notice.
    Lobby {
        players: Vec<(u16, Option<u32>, bool)>,
        started: bool,
        notice: Option<String>,
    },
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
        self.send_raw(&hello_frame(major, nation));
    }

    pub fn hello(&mut self, nation: Option<u32>) {
        self.hello_with(pax_protocol::PROTOCOL_MAJOR, nation);
    }

    pub fn subscribe(&mut self, map_mode: MapMode, map_good: u16, market: Option<u32>, province: Option<u32>) {
        let mut b = FlatBufferBuilder::new();
        let s = Subscribe::create(&mut b, &SubscribeArgs { map_mode, map_good, market, province });
        self.send(&mut b, ClientPayload::Subscribe, s.as_union_value());
    }

    /// Gives up the decoder, for tests that write raw bytes without reading.
    pub fn into_stream(self) -> TcpStream {
        self.stream
    }

    pub fn set_speed(&mut self, speed: Speed) {
        let mut b = FlatBufferBuilder::new();
        let s = SetSpeed::create(&mut b, &SetSpeedArgs { speed });
        self.send(&mut b, ClientPayload::SetSpeed, s.as_union_value());
    }

    pub fn ack(&mut self, day: u64) {
        let mut b = FlatBufferBuilder::new();
        let a = Ack::create(&mut b, &AckArgs { day });
        self.send(&mut b, ClientPayload::Ack, a.as_union_value());
    }

    pub fn set_income_tax(&mut self, client_seq: u32, nation: u32, rate_raw: i64) {
        let mut b = FlatBufferBuilder::new();
        let rate = Fixed::new(rate_raw);
        let c = SetIncomeTax::create(&mut b, &SetIncomeTaxArgs { nation, rate: Some(&rate) });
        let s = SubmitCommand::create(
            &mut b,
            &SubmitCommandArgs { client_seq, command_type: Command::SetIncomeTax, command: Some(c.as_union_value()) },
        );
        self.send(&mut b, ClientPayload::SubmitCommand, s.as_union_value());
    }

    pub fn save_game(&mut self, name: &str) {
        let mut b = FlatBufferBuilder::new();
        let name = b.create_string(name);
        let s = SaveGame::create(&mut b, &SaveGameArgs { name: Some(name) });
        self.send(&mut b, ClientPayload::SaveGame, s.as_union_value());
    }

    pub fn load_game(&mut self, name: &str) {
        let mut b = FlatBufferBuilder::new();
        let name = b.create_string(name);
        let l = LoadGame::create(&mut b, &LoadGameArgs { name: Some(name) });
        self.send(&mut b, ClientPayload::LoadGame, l.as_union_value());
    }

    pub fn list_saves(&mut self) {
        let mut b = FlatBufferBuilder::new();
        let l = ListSaves::create(&mut b, &ListSavesArgs {});
        self.send(&mut b, ClientPayload::ListSaves, l.as_union_value());
    }

    pub fn claim_nation(&mut self, nation: Option<u32>) {
        let mut b = FlatBufferBuilder::new();
        let c = ClaimNation::create(&mut b, &ClaimNationArgs { nation });
        self.send(&mut b, ClientPayload::ClaimNation, c.as_union_value());
    }

    pub fn set_ready(&mut self, ready: bool) {
        let mut b = FlatBufferBuilder::new();
        let r = SetReady::create(&mut b, &SetReadyArgs { ready });
        self.send(&mut b, ClientPayload::SetReady, r.as_union_value());
    }

    pub fn start_game(&mut self) {
        let mut b = FlatBufferBuilder::new();
        let s = StartGame::create(&mut b, &StartGameArgs {});
        self.send(&mut b, ClientPayload::StartGame, s.as_union_value());
    }

    pub fn kick(&mut self, player: u16) {
        let mut b = FlatBufferBuilder::new();
        let k = Kick::create(&mut b, &KickArgs { player });
        self.send(&mut b, ClientPayload::Kick, k.as_union_value());
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
            player: w.player(),
            nation: w.nation(),
            day: w.day(),
            provinces: strings(defs.provinces()),
            nations: defs
                .nations()
                .map(|n| n.iter().map(|n| n.key().unwrap_or_default().to_owned()).collect())
                .unwrap_or_default(),
            content_hash: w.content_hash(),
            map_hash: defs.map_hash(),
        };
    }
    if let Some(r) = msg.payload_as_rejected() {
        return Got::Rejected(r.reason().unwrap_or_default().to_owned());
    }
    if let Some(g) = msg.payload_as_goodbye() {
        return Got::Goodbye(g.reason().unwrap_or_default().to_owned());
    }
    if let Some(u) = msg.payload_as_day_update() {
        return Got::DayUpdate {
            day: u.day(),
            population: u.world().map_or(0, |w| w.population()),
            map_values: u.map().and_then(|m| m.values()).map(|v| v.len()),
            market: u.market().map(|m| m.market()),
            province: u.province().map(|p| p.province()),
            skipped: u.skipped(),
            tax_rates: u
                .nations()
                .and_then(|n| n.income_tax_rate())
                .map(|r| r.iter().map(|f| f.raw()).collect())
                .unwrap_or_default(),
            state_hash: u.state_hash(),
        };
    }
    if let Some(r) = msg.payload_as_command_result() {
        return Got::CommandResult { client_seq: r.client_seq(), error: r.error(), applies_on_day: r.applies_on_day() };
    }
    if let Some(r) = msg.payload_as_save_result() {
        return Got::SaveResult {
            name: r.name().unwrap_or_default().to_owned(),
            error: r.error().unwrap_or_default().to_owned(),
        };
    }
    if let Some(l) = msg.payload_as_save_list() {
        return Got::SaveList(l.names().map(|n| n.iter().map(str::to_owned).collect()).unwrap_or_default());
    }
    if let Some(s) = msg.payload_as_server_state() {
        return Got::ServerState {
            day: s.day(),
            speed: s.speed(),
            changed_by: s.changed_by(),
            waiting_for: s.waiting_for().map(|w| w.iter().collect()).unwrap_or_default(),
        };
    }
    if let Some(l) = msg.payload_as_lobby_state() {
        return Got::Lobby {
            players: l
                .players()
                .map(|p| p.iter().map(|p| (p.player(), p.nation(), p.ready())).collect())
                .unwrap_or_default(),
            started: l.started(),
            notice: l.notice().map(str::to_owned),
        };
    }
    if let Some(p) = msg.payload_as_pong() {
        return Got::Pong(p.nonce());
    }
    Got::Other(format!("{:?}", msg.payload_type()))
}

/// Plays at Fastest, acknowledging every update, until at least `day`, then pauses.
/// Returns the day the game paused on.
pub fn play_until(c: &mut Client, day: u64) -> u64 {
    c.set_speed(Speed::Fastest);
    let mut reached = 0;
    while reached < day {
        match c.next() {
            Got::DayUpdate { day, .. } => {
                c.ack(day);
                reached = day;
            }
            Got::Closed => panic!("server closed the connection"),
            _ => {}
        }
    }
    c.set_speed(Speed::Paused);
    // Drain to the pause confirmation, so later messages are the replies we expect.
    loop {
        match c.next() {
            Got::ServerState { speed: Speed::Paused, day, .. } => return day,
            Got::DayUpdate { day, .. } => c.ack(day),
            Got::Closed => panic!("server closed the connection"),
            _ => {}
        }
    }
}

/// One size-prefixed `Hello` frame.
pub fn hello_frame(major: u16, nation: Option<u32>) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let name = b.create_string("test client");
    let args = HelloArgs {
        protocol_major: major,
        protocol_minor: 0,
        client_name: Some(name),
        requested_nation: nation,
        resume_token: 0,
    };
    let h = Hello::create(&mut b, &args);
    let msg = ClientMessage::create(
        &mut b,
        &ClientMessageArgs { payload_type: ClientPayload::Hello, payload: Some(h.as_union_value()) },
    );
    finish_size_prefixed_client_message_buffer(&mut b, msg);
    b.finished_data().to_vec()
}

/// One size-prefixed `Ping` frame.
pub fn ping_frame(nonce: u64) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let p = Ping::create(&mut b, &PingArgs { nonce });
    let msg = ClientMessage::create(
        &mut b,
        &ClientMessageArgs { payload_type: ClientPayload::Ping, payload: Some(p.as_union_value()) },
    );
    finish_size_prefixed_client_message_buffer(&mut b, msg);
    b.finished_data().to_vec()
}
