//! Playing over real TCP (M3-4, M3-5): speed, daily updates with flow control, and
//! commands that apply on the day the server promised.

mod common;

use common::*;
use pax_protocol::wire::{CommandError, Speed};

fn welcomed(nation: Option<u32>) -> (pax_server::Server, Client) {
    let server = two_states();
    let mut c = Client::connect(server.local_addr());
    c.hello(nation);
    assert!(matches!(c.next(), Got::Welcome { .. }));
    (server, c)
}

/// The next DayUpdate, acknowledged; other messages are returned to the caller via `seen`.
fn next_update(c: &mut Client, seen: &mut Vec<Got>) -> (u64, u32, Vec<i64>) {
    loop {
        match c.next() {
            Got::DayUpdate { day, skipped, tax_rates, .. } => {
                c.ack(day);
                return (day, skipped, tax_rates);
            }
            Got::Closed => panic!("server closed the connection"),
            other => seen.push(other),
        }
    }
}

#[test]
fn days_advance_in_order_while_the_client_keeps_up() {
    let (server, mut c) = welcomed(None);
    c.set_speed(Speed::Fastest);
    assert!(matches!(c.next(), Got::ServerState { speed: Speed::Fastest, .. }));
    let mut seen = Vec::new();
    let mut last = 0;
    for _ in 0..50 {
        let (day, skipped, _) = next_update(&mut c, &mut seen);
        // Every day is accounted for: shown, or counted as skipped.
        assert_eq!(day, last + 1 + u64::from(skipped));
        last = day;
    }
    c.set_speed(Speed::Paused);
    server.shutdown().expect("clean shutdown");
}

#[test]
fn a_command_applies_on_the_day_the_server_promised() {
    let (server, mut c) = welcomed(Some(1));
    c.set_income_tax(42, 1, 175_000);
    let promised = match c.next() {
        Got::CommandResult { client_seq: 42, error: CommandError::None, applies_on_day } => applies_on_day,
        other => panic!("expected an accepted CommandResult, got {other:?}"),
    };
    // Someone else's nation is refused, and changes nothing.
    c.set_income_tax(43, 0, 175_000);
    assert!(matches!(c.next(), Got::CommandResult { client_seq: 43, error: CommandError::NotPermitted, .. }));
    c.set_speed(Speed::Fastest);
    let mut seen = Vec::new();
    loop {
        let (day, _, tax_rates) = next_update(&mut c, &mut seen);
        // The update for a day shows the state after it; the command applied at its start.
        if day > promised {
            assert_eq!(tax_rates[1], 175_000);
            assert_ne!(tax_rates[0], 175_000);
            break;
        }
    }
    c.set_speed(Speed::Paused);
    server.shutdown().expect("clean shutdown");
}

#[test]
fn paused_means_no_updates() {
    let (server, mut c) = welcomed(None);
    c.set_speed(Speed::Fastest);
    assert!(matches!(c.next(), Got::ServerState { .. }));
    c.set_speed(Speed::Paused);
    // Drain what was already in flight, up to the pause.
    let mut paused_at = None;
    while paused_at.is_none() {
        match c.next() {
            Got::DayUpdate { day, .. } => c.ack(day),
            Got::ServerState { day, speed: Speed::Paused } => paused_at = Some(day),
            other => panic!("unexpected {other:?}"),
        }
    }
    c.ping(1);
    // The next message is the Pong: no DayUpdate arrived while paused.
    loop {
        match c.next() {
            Got::Pong(1) => break,
            Got::DayUpdate { day, .. } => assert!(day <= paused_at.unwrap(), "an update for day {day} after pausing"),
            other => panic!("unexpected {other:?}"),
        }
    }
    server.shutdown().expect("clean shutdown");
}
