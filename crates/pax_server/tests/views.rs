//! Subscriptions over real TCP (M3-3, D22): an immediate DayUpdate with exactly the
//! views asked for, and a protocol error for ids that don't exist.

mod common;

use common::*;
use pax_protocol::wire::MapMode;

fn welcomed() -> (pax_server::Server, Client) {
    let server = two_states();
    let mut c = Client::connect(server.local_addr());
    c.hello(None);
    assert!(matches!(c.next(), Got::Welcome { .. }));
    (server, c)
}

#[test]
fn subscribe_gets_an_immediate_update_with_the_requested_views() {
    let (server, mut c) = welcomed();
    c.subscribe(MapMode::Population, 0, Some(1), Some(2));
    match c.next() {
        Got::DayUpdate { day, population, map_values, market, province } => {
            assert_eq!(day, 0, "the server is paused at day 0 until M3-5");
            assert!(population > 0);
            assert_eq!((map_values, market, province), (Some(4), Some(1), Some(2)));
        }
        other => panic!("expected DayUpdate, got {other:?}"),
    }
    // A new subscription replaces the old one completely.
    c.subscribe(MapMode::None, 0, None, None);
    assert!(matches!(c.next(), Got::DayUpdate { map_values: None, market: None, province: None, .. }));
    server.shutdown().expect("clean shutdown");
}

#[test]
fn subscribing_to_a_missing_id_is_a_protocol_error() {
    let (server, mut c) = welcomed();
    c.subscribe(MapMode::None, 0, Some(9), None);
    match c.next() {
        Got::Goodbye(reason) => assert!(reason.contains("market 9"), "{reason}"),
        other => panic!("expected Goodbye, got {other:?}"),
    }
    assert_eq!(c.next(), Got::Closed);
    server.shutdown().expect("clean shutdown");
}
