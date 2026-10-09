//! Client → server frames (D22): every message the client sends, as size-prefixed
//! bytes ready for the socket. Plain Rust, unit-tested without Godot.
//!
//! Commands carry rates as `Fixed` raw integers (value × 10⁶), never floats: what the
//! client sends is simulation input (D3).

use flatbuffers::{FlatBufferBuilder, UnionWIPOffset, WIPOffset};
use pax_protocol::{PROTOCOL_MAJOR, PROTOCOL_MINOR, wire};

/// Finishes `payload` as one size-prefixed `ClientMessage` frame.
fn frame(mut b: FlatBufferBuilder<'_>, kind: wire::ClientPayload, payload: WIPOffset<UnionWIPOffset>) -> Vec<u8> {
    let msg =
        wire::ClientMessage::create(&mut b, &wire::ClientMessageArgs { payload_type: kind, payload: Some(payload) });
    wire::finish_size_prefixed_client_message_buffer(&mut b, msg);
    b.finished_data().to_vec()
}

/// Opens the session. `nation: None` asks for sandbox (M3, D24), or in a lobby for
/// no claim yet. A non-zero `resume_token` reclaims a kept seat instead (D24).
/// `password`: the server's or the admin's (protocol 1.6), if it needs one.
pub fn hello(client_name: &str, nation: Option<u32>, resume_token: u64, password: Option<&str>) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let name = b.create_string(client_name);
    let password = password.map(|p| b.create_string(p));
    let args = wire::HelloArgs {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        client_name: Some(name),
        requested_nation: nation,
        resume_token,
        password,
    };
    let h = wire::Hello::create(&mut b, &args);
    frame(b, wire::ClientPayload::Hello, h.as_union_value())
}

/// The three policy commands (D15, D16, D21), by the rate they set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    IncomeTax,
    Transfer,
    Consumption,
}

impl Policy {
    pub const ALL: [Policy; 3] = [Policy::IncomeTax, Policy::Transfer, Policy::Consumption];

    /// The name GDScript passes (`PaxKeys.POLICY_*`, generated from this).
    pub fn name(self) -> &'static str {
        match self {
            Policy::IncomeTax => "income_tax",
            Policy::Transfer => "transfer",
            Policy::Consumption => "consumption",
        }
    }

    pub fn from_name(name: &str) -> Option<Policy> {
        Policy::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// One policy command: `rate_raw` is the `Fixed` raw value (0.15 is 150 000).
pub fn submit_command(client_seq: u32, policy: Policy, nation: u32, rate_raw: i64) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let rate = wire::Fixed::new(rate_raw);
    let (kind, command) = match policy {
        Policy::IncomeTax => (
            wire::Command::SetIncomeTax,
            wire::SetIncomeTax::create(&mut b, &wire::SetIncomeTaxArgs { nation, rate: Some(&rate) }).as_union_value(),
        ),
        Policy::Transfer => (
            wire::Command::SetTransferRate,
            wire::SetTransferRate::create(&mut b, &wire::SetTransferRateArgs { nation, rate: Some(&rate) })
                .as_union_value(),
        ),
        Policy::Consumption => (
            wire::Command::SetConsumptionRate,
            wire::SetConsumptionRate::create(&mut b, &wire::SetConsumptionRateArgs { nation, rate: Some(&rate) })
                .as_union_value(),
        ),
    };
    let args = wire::SubmitCommandArgs { client_seq, command_type: kind, command: Some(command) };
    let s = wire::SubmitCommand::create(&mut b, &args);
    frame(b, wire::ClientPayload::SubmitCommand, s.as_union_value())
}

pub fn set_speed(speed: wire::Speed) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let s = wire::SetSpeed::create(&mut b, &wire::SetSpeedArgs { speed });
    frame(b, wire::ClientPayload::SetSpeed, s.as_union_value())
}

/// The views this session wants. `market`/`province`: `None` for no panel.
pub fn subscribe(map_mode: wire::MapMode, map_good: u16, market: Option<u32>, province: Option<u32>) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let s = wire::Subscribe::create(&mut b, &wire::SubscribeArgs { map_mode, map_good, market, province });
    frame(b, wire::ClientPayload::Subscribe, s.as_union_value())
}

pub fn ack(day: u64) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let a = wire::Ack::create(&mut b, &wire::AckArgs { day });
    frame(b, wire::ClientPayload::Ack, a.as_union_value())
}

pub fn ping(nonce: u64) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let p = wire::Ping::create(&mut b, &wire::PingArgs { nonce });
    frame(b, wire::ClientPayload::Ping, p.as_union_value())
}

pub fn save_game(name: &str) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let name = b.create_string(name);
    let s = wire::SaveGame::create(&mut b, &wire::SaveGameArgs { name: Some(name) });
    frame(b, wire::ClientPayload::SaveGame, s.as_union_value())
}

pub fn load_game(name: &str) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let name = b.create_string(name);
    let l = wire::LoadGame::create(&mut b, &wire::LoadGameArgs { name: Some(name) });
    frame(b, wire::ClientPayload::LoadGame, l.as_union_value())
}

/// Lobby (M4-2): claim a nation, or `None` to give up the claim.
pub fn claim_nation(nation: Option<u32>) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let c = wire::ClaimNation::create(&mut b, &wire::ClaimNationArgs { nation });
    frame(b, wire::ClientPayload::ClaimNation, c.as_union_value())
}

pub fn set_ready(ready: bool) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let r = wire::SetReady::create(&mut b, &wire::SetReadyArgs { ready });
    frame(b, wire::ClientPayload::SetReady, r.as_union_value())
}

pub fn start_game() -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let s = wire::StartGame::create(&mut b, &wire::StartGameArgs {});
    frame(b, wire::ClientPayload::StartGame, s.as_union_value())
}

/// Host only (D24): end player `player`'s session.
pub fn kick(player: u16) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let k = wire::Kick::create(&mut b, &wire::KickArgs { player });
    frame(b, wire::ClientPayload::Kick, k.as_union_value())
}

pub fn list_saves() -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let l = wire::ListSaves::create(&mut b, &wire::ListSavesArgs {});
    frame(b, wire::ClientPayload::ListSaves, l.as_union_value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_protocol::read_client_message;

    #[test]
    fn every_frame_verifies_as_the_message_it_claims() {
        let read = |f: &[u8]| read_client_message(f).expect("verifies").payload_type();
        assert_eq!(read(&hello("c", Some(1), 0, None)), wire::ClientPayload::Hello);
        assert_eq!(read(&set_speed(wire::Speed::Fast)), wire::ClientPayload::SetSpeed);
        assert_eq!(read(&subscribe(wire::MapMode::Price, 2, Some(0), None)), wire::ClientPayload::Subscribe);
        assert_eq!(read(&ack(9)), wire::ClientPayload::Ack);
        assert_eq!(read(&ping(1)), wire::ClientPayload::Ping);
        assert_eq!(read(&save_game("a")), wire::ClientPayload::SaveGame);
        assert_eq!(read(&load_game("a")), wire::ClientPayload::LoadGame);
        assert_eq!(read(&list_saves()), wire::ClientPayload::ListSaves);
        for policy in [Policy::IncomeTax, Policy::Transfer, Policy::Consumption] {
            assert_eq!(read(&submit_command(1, policy, 0, 150_000)), wire::ClientPayload::SubmitCommand);
        }
    }

    #[test]
    fn commands_carry_exact_raw_rates_and_absent_ids_stay_absent() {
        let f = submit_command(7, Policy::Transfer, 1, 123_457);
        let s = read_client_message(&f).unwrap().payload_as_submit_command().unwrap();
        let c = s.command_as_set_transfer_rate().unwrap();
        assert_eq!((s.client_seq(), c.nation(), c.rate().unwrap().raw()), (7, 1, 123_457));
        let f = hello("c", None, 0, None);
        assert_eq!(read_client_message(&f).unwrap().payload_as_hello().unwrap().requested_nation(), None);
    }
}
