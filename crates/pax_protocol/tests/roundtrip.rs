//! Every message kind is built, framed, read back through `read_*_message`, and
//! compared field by field (NETWORK_PROTOCOL §9), including schema defaults.

mod common;

use common::*;
use flatbuffers::FlatBufferBuilder;
use pax_protocol::wire::*;
use pax_protocol::{Direction, FrameDecoder, read_client_message, read_server_message};

/// The frame survives the decoder unchanged, and reads as a client message.
fn client(frame: &[u8]) -> ClientMessage<'_> {
    let mut d = FrameDecoder::new(Direction::ClientToServer);
    d.push(frame);
    assert_eq!(d.next_frame().unwrap().as_deref(), Some(frame));
    read_client_message(frame).expect("valid client message")
}

fn server(frame: &[u8]) -> ServerMessage<'_> {
    let mut d = FrameDecoder::new(Direction::ServerToClient);
    d.push(frame);
    assert_eq!(d.next_frame().unwrap().as_deref(), Some(frame));
    read_server_message(frame).expect("valid server message")
}

#[test]
fn hello_and_its_defaults() {
    let mut b = FlatBufferBuilder::new();
    let name = b.create_string("Bismarck");
    let hello = Hello::create(
        &mut b,
        &HelloArgs {
            protocol_major: 1,
            protocol_minor: 0,
            client_name: Some(name),
            requested_nation: Some(1),
            resume_token: 0,
        },
    );
    let frame = client_frame(&mut b, ClientPayload::Hello, hello.as_union_value());
    let h = client(&frame).payload_as_hello().unwrap();
    assert_eq!((h.protocol_major(), h.client_name(), h.requested_nation()), (1, Some("Bismarck"), Some(1)));

    // Absent fields: no nation (sandbox), no resume token, no name.
    let mut b = FlatBufferBuilder::new();
    let hello = Hello::create(&mut b, &HelloArgs { protocol_major: 1, ..Default::default() });
    let frame = client_frame(&mut b, ClientPayload::Hello, hello.as_union_value());
    let h = client(&frame).payload_as_hello().unwrap();
    assert_eq!((h.requested_nation(), h.resume_token(), h.client_name()), (None, 0, None));
}

#[test]
fn every_command_round_trips() {
    for (kind, nation, raw) in [
        (Command::SetIncomeTax, 1, 120_000),
        (Command::SetTransferRate, 0, 50_000),
        (Command::SetConsumptionRate, 2, 0),
    ] {
        let mut b = FlatBufferBuilder::new();
        let rate = Fixed::new(raw);
        let command = match kind {
            Command::SetIncomeTax => {
                SetIncomeTax::create(&mut b, &SetIncomeTaxArgs { nation, rate: Some(&rate) }).as_union_value()
            }
            Command::SetTransferRate => {
                SetTransferRate::create(&mut b, &SetTransferRateArgs { nation, rate: Some(&rate) }).as_union_value()
            }
            Command::SetConsumptionRate => {
                SetConsumptionRate::create(&mut b, &SetConsumptionRateArgs { nation, rate: Some(&rate) })
                    .as_union_value()
            }
            other => unreachable!("{other:?}"),
        };
        let submit = SubmitCommand::create(
            &mut b,
            &SubmitCommandArgs { client_seq: 9, command_type: kind, command: Some(command) },
        );
        let frame = client_frame(&mut b, ClientPayload::SubmitCommand, submit.as_union_value());
        let s = client(&frame).payload_as_submit_command().unwrap();
        assert_eq!((s.client_seq(), s.command_type()), (9, kind));
        let got = match kind {
            Command::SetIncomeTax => s.command_as_set_income_tax().map(|c| (c.nation(), c.rate().map(|r| r.raw()))),
            Command::SetTransferRate => {
                s.command_as_set_transfer_rate().map(|c| (c.nation(), c.rate().map(|r| r.raw())))
            }
            _ => s.command_as_set_consumption_rate().map(|c| (c.nation(), c.rate().map(|r| r.raw()))),
        };
        assert_eq!(got, Some((nation, Some(raw))));
    }
}

#[test]
fn a_command_without_its_rate_is_detectable() {
    // The struct field is optional on the wire; the server must answer `Malformed`.
    let mut b = FlatBufferBuilder::new();
    let command = SetIncomeTax::create(&mut b, &SetIncomeTaxArgs { nation: 0, rate: None });
    let submit = SubmitCommand::create(
        &mut b,
        &SubmitCommandArgs {
            client_seq: 1,
            command_type: Command::SetIncomeTax,
            command: Some(command.as_union_value()),
        },
    );
    let frame = client_frame(&mut b, ClientPayload::SubmitCommand, submit.as_union_value());
    let s = client(&frame).payload_as_submit_command().unwrap();
    assert!(s.command_as_set_income_tax().unwrap().rate().is_none());
}

#[test]
fn small_client_messages() {
    let mut b = FlatBufferBuilder::new();
    let m = SetSpeed::create(&mut b, &SetSpeedArgs { speed: Speed::Fastest });
    let f = client_frame(&mut b, ClientPayload::SetSpeed, m.as_union_value());
    assert_eq!(client(&f).payload_as_set_speed().unwrap().speed(), Speed::Fastest);

    let mut b = FlatBufferBuilder::new();
    let m = Subscribe::create(&mut b, &SubscribeArgs { map_mode: MapMode::Price, map_good: 4, ..Default::default() });
    let f = client_frame(&mut b, ClientPayload::Subscribe, m.as_union_value());
    let s = client(&f).payload_as_subscribe().unwrap();
    assert_eq!((s.map_mode(), s.map_good(), s.market(), s.province()), (MapMode::Price, 4, None, None));

    let mut b = FlatBufferBuilder::new();
    let m = Ack::create(&mut b, &AckArgs { day: 77 });
    let f = client_frame(&mut b, ClientPayload::Ack, m.as_union_value());
    assert_eq!(client(&f).payload_as_ack().unwrap().day(), 77);

    let mut b = FlatBufferBuilder::new();
    let m = Ping::create(&mut b, &PingArgs { nonce: u64::MAX });
    let f = client_frame(&mut b, ClientPayload::Ping, m.as_union_value());
    assert_eq!(client(&f).payload_as_ping().unwrap().nonce(), u64::MAX);

    let mut b = FlatBufferBuilder::new();
    let name = b.create_string("autumn_1848");
    let m = SaveGame::create(&mut b, &SaveGameArgs { name: Some(name) });
    let f = client_frame(&mut b, ClientPayload::SaveGame, m.as_union_value());
    assert_eq!(client(&f).payload_as_save_game().unwrap().name(), Some("autumn_1848"));

    let mut b = FlatBufferBuilder::new();
    let name = b.create_string("autumn_1848");
    let m = LoadGame::create(&mut b, &LoadGameArgs { name: Some(name) });
    let f = client_frame(&mut b, ClientPayload::LoadGame, m.as_union_value());
    assert_eq!(client(&f).payload_as_load_game().unwrap().name(), Some("autumn_1848"));

    let mut b = FlatBufferBuilder::new();
    let m = ListSaves::create(&mut b, &ListSavesArgs {});
    let f = client_frame(&mut b, ClientPayload::ListSaves, m.as_union_value());
    assert!(client(&f).payload_as_list_saves().is_some());
}

#[test]
fn welcome_carries_static_data() {
    let mut b = FlatBufferBuilder::new();
    let goods = ["grain", "coal"].map(|s| b.create_string(s));
    let goods = b.create_vector(&goods);
    let provinces = ["riverlands", "peaks"].map(|s| b.create_string(s));
    let provinces = b.create_vector(&provinces);
    let province_market = b.create_vector(&[0u32, 1]);
    // Market 0 belongs to the only nation; market 1 is stateless.
    let key = b.create_string("lowland_kingdom");
    let markets = b.create_vector(&[0u32]);
    let nation = NationDef::create(&mut b, &NationDefArgs { key: Some(key), markets: Some(markets) });
    let nations = b.create_vector(&[nation]);
    let defs = StaticData::create(
        &mut b,
        &StaticDataArgs {
            goods: Some(goods),
            provinces: Some(provinces),
            province_market: Some(province_market),
            nations: Some(nations),
            ..Default::default()
        },
    );
    let scenario = b.create_string("two_states");
    let welcome = Welcome::create(
        &mut b,
        &WelcomeArgs {
            protocol_major: 1,
            player: 0,
            nation: Some(0),
            scenario: Some(scenario),
            content_hash: 0x1234,
            day: 360,
            speed: Speed::Paused,
            defs: Some(defs),
            ..Default::default()
        },
    );
    let frame = server_frame(&mut b, ServerPayload::Welcome, welcome.as_union_value());
    let w = server(&frame).payload_as_welcome().unwrap();
    assert_eq!(
        (w.nation(), w.day(), w.speed(), w.scenario(), w.content_hash()),
        (Some(0), 360, Speed::Paused, Some("two_states"), 0x1234)
    );
    let d = w.defs().unwrap();
    assert_eq!(d.goods().unwrap().iter().collect::<Vec<_>>(), ["grain", "coal"]);
    let n = d.nations().unwrap().get(0);
    assert_eq!((n.key(), n.markets().unwrap().iter().collect::<Vec<_>>()), (Some("lowland_kingdom"), vec![0]));
    assert!(d.professions().is_none());
}

#[test]
fn day_update_with_every_view() {
    let frame =
        day_update(&Scale { nations: 3, provinces: 5, goods: 4, pops: 2, map: true, market: true, province: true });
    let u = server(&frame).payload_as_day_update().unwrap();
    assert_eq!((u.day(), u.skipped(), u.speed(), u.state_hash()), (7_300, 2, Speed::Normal, 0xDEAD_BEEF_0BAD_F00D));
    let w = u.world().unwrap();
    assert_eq!((w.population(), w.unemployed(), w.life_needs().unwrap().raw()), (250_000_000, 6_000_000, 990_000));
    assert_eq!(u.nations().unwrap().treasury().unwrap().get(2).raw(), 2_000_006);
    let map = u.map().unwrap();
    assert_eq!((map.mode(), map.good(), map.values().unwrap().len()), (MapMode::Price, 3, 5));
    assert_eq!(u.market().unwrap().demand().unwrap().get(3).raw(), 51);
    let pops = u.province().unwrap().pops().unwrap();
    assert_eq!(pops.people().unwrap().iter().collect::<Vec<_>>(), [5_000, 5_000]);
}

#[test]
fn unsubscribed_views_are_absent() {
    let frame =
        day_update(&Scale { nations: 2, provinces: 0, goods: 0, pops: 0, map: false, market: false, province: false });
    let u = server(&frame).payload_as_day_update().unwrap();
    assert!(u.map().is_none() && u.market().is_none() && u.province().is_none());
    assert_eq!(u.nations().unwrap().population().unwrap().len(), 2);
}

#[test]
fn small_server_messages() {
    let mut b = FlatBufferBuilder::new();
    let m = CommandResult::create(
        &mut b,
        &CommandResultArgs { client_seq: 9, error: CommandError::RateOutOfRange, applies_on_day: 0 },
    );
    let f = server_frame(&mut b, ServerPayload::CommandResult, m.as_union_value());
    let r = server(&f).payload_as_command_result().unwrap();
    assert_eq!((r.client_seq(), r.error()), (9, CommandError::RateOutOfRange));

    let mut b = FlatBufferBuilder::new();
    let m = ServerState::create(&mut b, &ServerStateArgs { day: 12, speed: Speed::Paused, changed_by: 0 });
    let f = server_frame(&mut b, ServerPayload::ServerState, m.as_union_value());
    assert_eq!(server(&f).payload_as_server_state().unwrap().speed(), Speed::Paused);

    let mut b = FlatBufferBuilder::new();
    let m = Pong::create(&mut b, &PongArgs { nonce: 5 });
    let f = server_frame(&mut b, ServerPayload::Pong, m.as_union_value());
    assert_eq!(server(&f).payload_as_pong().unwrap().nonce(), 5);

    for (kind, text) in [(ServerPayload::Rejected, "server full"), (ServerPayload::Goodbye, "timed out")] {
        let mut b = FlatBufferBuilder::new();
        let reason = b.create_string(text);
        let m = match kind {
            ServerPayload::Rejected => {
                Rejected::create(&mut b, &RejectedArgs { reason: Some(reason) }).as_union_value()
            }
            _ => Goodbye::create(&mut b, &GoodbyeArgs { reason: Some(reason) }).as_union_value(),
        };
        let f = server_frame(&mut b, kind, m);
        let msg = server(&f);
        let got = match kind {
            ServerPayload::Rejected => msg.payload_as_rejected().and_then(|r| r.reason()),
            _ => msg.payload_as_goodbye().and_then(|g| g.reason()),
        };
        assert_eq!(got, Some(text));
    }

    let mut b = FlatBufferBuilder::new();
    let name = b.create_string("autumn_1848");
    let error = b.create_string("");
    let m = SaveResult::create(&mut b, &SaveResultArgs { name: Some(name), error: Some(error) });
    let f = server_frame(&mut b, ServerPayload::SaveResult, m.as_union_value());
    assert_eq!(server(&f).payload_as_save_result().unwrap().error(), Some(""));

    let mut b = FlatBufferBuilder::new();
    let names = ["a", "b"].map(|s| b.create_string(s));
    let names = b.create_vector(&names);
    let m = SaveList::create(&mut b, &SaveListArgs { names: Some(names) });
    let f = server_frame(&mut b, ServerPayload::SaveList, m.as_union_value());
    assert_eq!(server(&f).payload_as_save_list().unwrap().names().unwrap().len(), 2);
}

#[test]
fn each_direction_refuses_the_others_messages() {
    let to_client =
        day_update(&Scale { nations: 1, provinces: 0, goods: 0, pops: 0, map: false, market: false, province: false });
    assert!(matches!(
        read_client_message(&to_client),
        Err(pax_protocol::ProtocolError::WrongIdentifier { expected: Direction::ClientToServer })
    ));

    let mut b = FlatBufferBuilder::new();
    let m = Ping::create(&mut b, &PingArgs { nonce: 1 });
    let to_server = client_frame(&mut b, ClientPayload::Ping, m.as_union_value());
    assert!(matches!(
        read_server_message(&to_server),
        Err(pax_protocol::ProtocolError::WrongIdentifier { expected: Direction::ServerToClient })
    ));
}
