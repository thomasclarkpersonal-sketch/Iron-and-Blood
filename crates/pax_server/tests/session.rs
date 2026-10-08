//! Session rules over real TCP (M3-2, D22, NETWORK_PROTOCOL §2–§3).

mod common;

use std::time::Duration;

use common::*;
use pax_server::Config;

fn assert_goodbye(got: Got, contains: &str) {
    match got {
        Got::Goodbye(reason) => {
            assert!(reason.contains(contains), "Goodbye reason {reason:?} should mention {contains:?}")
        }
        other => panic!("expected Goodbye mentioning {contains:?}, got {other:?}"),
    }
}

fn assert_rejected(got: Got, contains: &str) {
    match got {
        Got::Rejected(reason) => {
            assert!(reason.contains(contains), "Rejected reason {reason:?} should mention {contains:?}")
        }
        other => panic!("expected Rejected mentioning {contains:?}, got {other:?}"),
    }
}

#[test]
fn hello_gets_welcome_with_the_scenario_tables() {
    let server = two_states();
    let mut c = Client::connect(server.local_addr());
    c.hello(Some(1));
    let expected_hash = pax_data::load_scenario(&scenario("two_states")).unwrap().content_hash;
    match c.next() {
        Got::Welcome { nation, day, provinces, nations, content_hash, map_hash } => {
            assert_eq!((nation, day, content_hash), (Some(1), 0, expected_hash));
            let expected_map = pax_data::load_scenario(&scenario("two_states")).unwrap().map.map(|m| m.map.map_hash);
            assert_eq!(map_hash, expected_map);
            assert!(map_hash.is_some(), "two_states has a map");
            assert_eq!(provinces.len(), 4);
            assert_eq!(nations, ["lowland_kingdom", "highland_republic"]);
        }
        other => panic!("expected Welcome, got {other:?}"),
    }
    server.shutdown().expect("clean shutdown");
}

#[test]
fn hello_without_a_nation_is_sandbox() {
    let server = two_states();
    let mut c = Client::connect(server.local_addr());
    c.hello(None);
    assert!(matches!(c.next(), Got::Welcome { nation: None, .. }));
    server.shutdown().expect("clean shutdown");
}

#[test]
fn a_second_client_is_refused_while_the_first_plays() {
    let server = two_states();
    let mut first = Client::connect(server.local_addr());
    first.hello(None);
    assert!(matches!(first.next(), Got::Welcome { .. }));
    let mut second = Client::connect(server.local_addr());
    second.hello(None);
    assert_rejected(second.next(), "server full");
    assert_eq!(second.next(), Got::Closed);
    // The first session is unaffected.
    first.ping(7);
    assert_eq!(first.next(), Got::Pong(7));
    server.shutdown().expect("clean shutdown");
}

#[test]
fn the_seat_frees_up_when_the_player_leaves() {
    let server = two_states();
    let mut first = Client::connect(server.local_addr());
    first.hello(None);
    assert!(matches!(first.next(), Got::Welcome { .. }));
    drop(first);
    // The sim thread hears about the close asynchronously: retry briefly.
    for attempt in 0.. {
        let mut next = Client::connect(server.local_addr());
        next.hello(None);
        match next.next() {
            Got::Welcome { .. } => break,
            Got::Rejected(_) if attempt < 50 => std::thread::sleep(Duration::from_millis(20)),
            other => panic!("expected Welcome after the first player left, got {other:?}"),
        }
    }
    server.shutdown().expect("clean shutdown");
}

#[test]
fn wrong_protocol_major_and_unknown_nation_are_rejected() {
    let server = two_states();
    let mut c = Client::connect(server.local_addr());
    c.hello_with(pax_protocol::PROTOCOL_MAJOR + 1, None);
    assert_rejected(c.next(), "not supported");
    let mut c = Client::connect(server.local_addr());
    c.hello(Some(2));
    assert_rejected(c.next(), "unknown nation 2");
    server.shutdown().expect("clean shutdown");
}

#[test]
fn anything_before_hello_is_a_protocol_error() {
    let server = two_states();
    let mut c = Client::connect(server.local_addr());
    c.ping(1);
    assert_goodbye(c.next(), "first message must be Hello");
    assert_eq!(c.next(), Got::Closed);
    server.shutdown().expect("clean shutdown");
}

#[test]
fn hello_twice_is_a_protocol_error() {
    let server = two_states();
    let mut c = Client::connect(server.local_addr());
    c.hello(None);
    assert!(matches!(c.next(), Got::Welcome { .. }));
    c.hello(None);
    assert_goodbye(c.next(), "Hello sent twice");
    server.shutdown().expect("clean shutdown");
}

#[test]
fn ping_gets_pong() {
    let server = two_states();
    let mut c = Client::connect(server.local_addr());
    c.hello(None);
    assert!(matches!(c.next(), Got::Welcome { .. }));
    for nonce in [0, 1, u64::MAX] {
        c.ping(nonce);
        assert_eq!(c.next(), Got::Pong(nonce));
    }
    server.shutdown().expect("clean shutdown");
}

#[test]
fn oversized_and_garbage_frames_end_the_session_without_a_panic() {
    let server = two_states();
    let mut c = Client::connect(server.local_addr());
    c.send_raw(&(pax_protocol::MAX_CLIENT_FRAME as u32 + 1).to_le_bytes());
    assert_goodbye(c.next(), "exceeds");
    let mut c = Client::connect(server.local_addr());
    c.send_raw(&[12, 0, 0, 0, 0xde, 0xad, 0xbe, 0xef, b'P', b'A', b'X', b'C', 1, 2, 3, 4]);
    assert_goodbye(c.next(), "verification");
    let mut c = Client::connect(server.local_addr());
    c.send_raw(&[8, 0, 0, 0, 1, 2, 3, 4, b'P', b'A', b'X', b'S']);
    assert_goodbye(c.next(), "identifier PAXC");
    // The server still serves well-behaved clients.
    let mut c = Client::connect(server.local_addr());
    c.hello(None);
    assert!(matches!(c.next(), Got::Welcome { .. }));
    server.shutdown().expect("clean shutdown");
}

#[test]
fn a_silent_client_times_out() {
    let mut config = Config::local(scenario("two_states"));
    config.idle_timeout = Duration::from_millis(300);
    let server = start(config);
    let mut c = Client::connect(server.local_addr());
    c.hello(None);
    assert!(matches!(c.next(), Got::Welcome { .. }));
    assert_goodbye(c.next(), "no message for");
    assert_eq!(c.next(), Got::Closed);
    server.shutdown().expect("clean shutdown");
}

#[test]
fn exit_when_idle_stops_the_server_when_the_player_leaves() {
    let mut config = Config::local(scenario("two_states"));
    config.exit_when_idle = true;
    let server = start(config);
    let mut c = Client::connect(server.local_addr());
    c.hello(None);
    assert!(matches!(c.next(), Got::Welcome { .. }));
    drop(c);
    // wait() returns only because the server stopped by itself.
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        server.wait().expect("clean exit");
        let _ = done_tx.send(());
    });
    done_rx.recv_timeout(Duration::from_secs(5)).expect("server exits after its only player leaves");
}

#[test]
fn a_rejected_client_cannot_act_even_if_it_sends_requests_at_once() {
    let server = two_states();
    let mut first = Client::connect(server.local_addr());
    first.hello(None);
    assert!(matches!(first.next(), Got::Welcome { .. }));
    // Hello and Subscribe in one burst: the Subscribe reaches the sim thread before
    // the rejection closes the connection, and must be ignored there.
    let mut second = Client::connect(server.local_addr());
    second.hello(None);
    second.subscribe(pax_protocol::wire::MapMode::Population, 0, None, None);
    assert!(matches!(second.next(), Got::Rejected(_)));
    assert_eq!(second.next(), Got::Closed);
    server.shutdown().expect("clean shutdown");
}

#[test]
fn a_client_that_never_reads_is_disconnected_instead_of_queued_forever() {
    use std::io::Write;
    let server = two_states();
    let mut c = Client::connect(server.local_addr());
    c.hello(None);
    assert!(matches!(c.next(), Got::Welcome { .. }));
    // Ping without reading: once the socket buffers fill, Pongs back up in the
    // server's bounded queue, and the server closes the connection.
    let mut stream = c.into_stream();
    let mut batch = Vec::new();
    for nonce in 0..1_000u64 {
        batch.extend(ping_frame(nonce));
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let closed = loop {
        if stream.write_all(&batch).is_err() {
            break true;
        }
        if std::time::Instant::now() > deadline {
            break false;
        }
    };
    assert!(closed, "the server kept accepting pings from a client that never reads");
    // The server is still healthy for others.
    let mut other = Client::connect(server.local_addr());
    other.hello(None);
    let got = other.next();
    assert!(matches!(got, Got::Welcome { .. } | Got::Rejected(_)), "{got:?}");
    server.shutdown().expect("clean shutdown");
}
