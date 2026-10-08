//! Several players on one server, over real TCP (M4-1, D24).

mod common;

use common::*;
use pax_protocol::wire::{CommandError, MapMode, Speed};
use pax_server::Config;

fn server(players: u16) -> pax_server::Server {
    let mut config = Config::local(scenario("two_states"));
    config.max_players = players;
    start(config)
}

/// The next message that isn't a DayUpdate, acknowledging any updates on the way.
fn next_non_update(c: &mut Client) -> Got {
    loop {
        match c.next() {
            Got::DayUpdate { day, .. } => c.ack(day),
            other => return other,
        }
    }
}

/// The next DayUpdate, acknowledging it.
fn next_update(c: &mut Client) -> Got {
    loop {
        match c.next() {
            update @ Got::DayUpdate { day, .. } => {
                c.ack(day);
                return update;
            }
            Got::Closed => panic!("the server closed the connection"),
            _ => {}
        }
    }
}

#[test]
fn two_players_each_with_their_own_nation_view_and_permissions() {
    let server = server(2);
    let mut a = Client::connect(server.local_addr());
    a.hello(Some(0));
    assert!(matches!(a.next(), Got::Welcome { player: 0, nation: Some(0), .. }));
    let mut b = Client::connect(server.local_addr());
    b.hello(Some(0));
    assert!(matches!(b.next(), Got::Rejected(r) if r.contains("is taken by player 0")));
    let mut b = Client::connect(server.local_addr());
    b.hello(Some(1));
    assert!(matches!(b.next(), Got::Welcome { player: 1, nation: Some(1), .. }));
    let mut c = Client::connect(server.local_addr());
    c.hello(None);
    assert!(matches!(c.next(), Got::Rejected(r) if r.contains("server full: all 2 players")));

    // Each player's subscription is their own.
    a.subscribe(MapMode::Population, 0, None, Some(0));
    assert!(matches!(a.next(), Got::DayUpdate { province: Some(0), .. }));
    // Each commands only their own nation.
    b.set_income_tax(1, 0, 150_000);
    assert!(matches!(b.next(), Got::CommandResult { client_seq: 1, error: CommandError::NotPermitted, .. }));
    b.set_income_tax(2, 1, 150_000);
    assert!(matches!(b.next(), Got::CommandResult { client_seq: 2, error: CommandError::None, .. }));

    // B starts the clock: both hear it, with who did it.
    b.set_speed(Speed::Fastest);
    for c in [&mut a, &mut b] {
        assert_eq!(next_non_update(c), Got::ServerState { day: 0, speed: Speed::Fastest, changed_by: 1 });
    }
    let (Got::DayUpdate { province: pa, .. }, Got::DayUpdate { province: pb, tax_rates, .. }) =
        (next_update(&mut a), next_update(&mut b))
    else {
        unreachable!()
    };
    assert_eq!((pa, pb), (Some(0), None), "each update carries its own player's panels");
    assert_eq!(tax_rates[1], 150_000, "B's command applied");

    // B leaves; A's game goes on.
    drop(b);
    let day = match next_update(&mut a) {
        Got::DayUpdate { day, .. } => day,
        _ => unreachable!(),
    };
    for _ in 0..20 {
        next_update(&mut a);
    }
    assert!(matches!(next_update(&mut a), Got::DayUpdate { day: later, .. } if later > day));
    server.shutdown().expect("clean shutdown");
}
