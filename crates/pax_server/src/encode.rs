//! Server→client frames (D22), built from engine state. Each function returns one
//! complete size-prefixed frame (`PAXS`), ready to write to the socket.

use flatbuffers::FlatBufferBuilder;
use pax_engine::World;
use pax_protocol::wire::{self, ServerPayload};

fn finish(
    mut b: FlatBufferBuilder<'_>,
    kind: ServerPayload,
    payload: flatbuffers::WIPOffset<flatbuffers::UnionWIPOffset>,
) -> Vec<u8> {
    let msg =
        wire::ServerMessage::create(&mut b, &wire::ServerMessageArgs { payload_type: kind, payload: Some(payload) });
    wire::finish_size_prefixed_server_message_buffer(&mut b, msg);
    b.finished_data().to_vec()
}

/// The session's index tables: every id in the protocol indexes one of these (D22).
fn static_data<'a>(b: &mut FlatBufferBuilder<'a>, world: &World) -> flatbuffers::WIPOffset<wire::StaticData<'a>> {
    let defs = &world.defs;
    let geo = &world.geography;
    let mut keys = |keys: Vec<&str>| {
        let offsets: Vec<_> = keys.into_iter().map(|k| b.create_string(k)).collect();
        b.create_vector(&offsets)
    };
    let goods = keys(defs.goods.iter().map(|g| g.key.as_str()).collect());
    let professions = keys(defs.professions.iter().map(|p| p.key.as_str()).collect());
    let producer_types = keys(defs.producer_types.iter().map(|t| t.key.as_str()).collect());
    let provinces = keys(geo.province_keys.iter().map(String::as_str).collect());
    let markets = keys(geo.market_keys.iter().map(String::as_str).collect());
    let province_market = b.create_vector(&geo.province_market);
    // Ownership travels as each nation's list of markets; a market no nation lists is
    // stateless, so no sentinel is needed (D22).
    let nations: Vec<_> = (0..world.nations.key.len())
        .map(|n| {
            let owned: Vec<u32> = (0..geo.market_keys.len() as u32)
                .filter(|&m| geo.market_nation[m as usize] == Some(n as u32))
                .collect();
            let key = b.create_string(&world.nations.key[n]);
            let markets = b.create_vector(&owned);
            wire::NationDef::create(b, &wire::NationDefArgs { key: Some(key), markets: Some(markets) })
        })
        .collect();
    let nations = b.create_vector(&nations);
    wire::StaticData::create(
        b,
        &wire::StaticDataArgs {
            goods: Some(goods),
            professions: Some(professions),
            producer_types: Some(producer_types),
            provinces: Some(provinces),
            province_market: Some(province_market),
            markets: Some(markets),
            nations: Some(nations),
        },
    )
}

/// What a `Welcome` says about the session, besides the world's tables.
pub struct WelcomeInfo<'s> {
    pub player: u16,
    pub resume_token: u64,
    /// `None`: sandbox, the session may command every nation (M3 only).
    pub nation: Option<u32>,
    pub scenario: &'s str,
    pub content_hash: u64,
    pub speed: wire::Speed,
}

pub fn welcome(world: &World, info: &WelcomeInfo<'_>) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let defs = static_data(&mut b, world);
    let scenario = b.create_string(info.scenario);
    let w = wire::Welcome::create(
        &mut b,
        &wire::WelcomeArgs {
            protocol_major: pax_protocol::PROTOCOL_MAJOR,
            protocol_minor: pax_protocol::PROTOCOL_MINOR,
            player: info.player,
            resume_token: info.resume_token,
            nation: info.nation,
            scenario: Some(scenario),
            content_hash: info.content_hash,
            day: world.day,
            speed: info.speed,
            defs: Some(defs),
        },
    );
    finish(b, ServerPayload::Welcome, w.as_union_value())
}

pub fn rejected(reason: &str) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let reason = b.create_string(reason);
    let r = wire::Rejected::create(&mut b, &wire::RejectedArgs { reason: Some(reason) });
    finish(b, ServerPayload::Rejected, r.as_union_value())
}

pub fn goodbye(reason: &str) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let reason = b.create_string(reason);
    let g = wire::Goodbye::create(&mut b, &wire::GoodbyeArgs { reason: Some(reason) });
    finish(b, ServerPayload::Goodbye, g.as_union_value())
}

pub fn pong(nonce: u64) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let p = wire::Pong::create(&mut b, &wire::PongArgs { nonce });
    finish(b, ServerPayload::Pong, p.as_union_value())
}
