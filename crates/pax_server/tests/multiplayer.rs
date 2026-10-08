//! Several players on one server, over real TCP (M4-1 to M4-4, D24).

mod common;

use common::*;
use pax_protocol::wire::{CommandError, MapMode, Speed};
use pax_server::Config;
use std::time::{Duration, Instant};

fn server(players: u16) -> pax_server::Server {
    let mut config = Config::local(scenario("two_states"));
    config.max_players = players;
    start(config)
}

/// The next message that isn't a DayUpdate or a lobby update, acknowledging any
/// updates on the way.
fn next_non_update(c: &mut Client) -> Got {
    loop {
        match c.next() {
            Got::DayUpdate { day, .. } => c.ack(day),
            Got::Lobby { notice: None, .. } => {}
            other => return other,
        }
    }
}

/// The next message that isn't a lobby update.
fn next_game(c: &mut Client) -> Got {
    loop {
        match c.next() {
            Got::Lobby { notice: None, .. } => {}
            other => return other,
        }
    }
}

/// Reads until the lobby says the game started.
fn until_started(c: &mut Client) {
    loop {
        match c.next() {
            Got::Lobby { started: true, .. } => return,
            Got::Closed => panic!("the server closed the connection"),
            _ => {}
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

    // The lobby (M4-2): nothing plays until the host starts the game.
    b.set_income_tax(9, 1, 150_000);
    assert!(matches!(
        next_non_update(&mut b),
        Got::CommandResult { client_seq: 9, error: CommandError::NotStarted, .. }
    ));
    b.set_ready(true);
    a.set_ready(true);
    // The two connections race: start only once the lobby shows both ready.
    loop {
        if let Got::Lobby { players, .. } = a.next()
            && players.len() == 2
            && players.iter().all(|p| p.2)
        {
            break;
        }
    }
    a.start_game();
    until_started(&mut a);
    until_started(&mut b);

    // Each player's subscription is their own.
    a.subscribe(MapMode::Population, 0, None, Some(0));
    assert!(matches!(next_game(&mut a), Got::DayUpdate { province: Some(0), .. }));
    // Each commands only their own nation.
    b.set_income_tax(1, 0, 150_000);
    assert!(matches!(
        next_non_update(&mut b),
        Got::CommandResult { client_seq: 1, error: CommandError::NotPermitted, .. }
    ));
    b.set_income_tax(2, 1, 150_000);
    assert!(matches!(next_non_update(&mut b), Got::CommandResult { client_seq: 2, error: CommandError::None, .. }));

    // Only the host (A, the first player) starts the clock; B is told the speed is unchanged.
    b.set_speed(Speed::Fastest);
    assert!(matches!(next_non_update(&mut b), Got::ServerState { speed: Speed::Paused, .. }));
    a.set_speed(Speed::Fastest);
    for c in [&mut a, &mut b] {
        assert_eq!(
            next_non_update(c),
            Got::ServerState { day: 0, speed: Speed::Fastest, changed_by: 0, waiting_for: vec![] }
        );
    }
    let (Got::DayUpdate { province: pa, .. }, Got::DayUpdate { province: pb, tax_rates, .. }) =
        (next_update(&mut a), next_update(&mut b))
    else {
        unreachable!()
    };
    assert_eq!((pa, pb), (Some(0), None), "each update carries its own player's panels");
    assert_eq!(tax_rates[1], 150_000, "B's command applied");

    // A guest can't save.
    b.save_game("mine");
    assert!(matches!(next_non_update(&mut b), Got::SaveResult { error, .. } if error.contains("only the host")));
    // The host kicks B; A's game goes on.
    a.kick(1);
    assert_eq!(next_non_update(&mut b), Got::Goodbye("kicked by the host".into()));
    assert_eq!(next_non_update(&mut b), Got::Closed);
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

/// Two players in a started game, with short lag thresholds for the test.
fn started_pair(pause_after: Duration, drop_after: Duration) -> (pax_server::Server, Client, Client) {
    let mut config = Config::local(scenario("two_states"));
    config.max_players = 2;
    config.pause_after = Some(pause_after);
    config.idle_timeout = drop_after;
    let server = start(config);
    let mut a = Client::connect(server.local_addr());
    a.hello(Some(0));
    // A must be seated first, to be the host that starts the game.
    assert!(matches!(a.next(), Got::Welcome { player: 0, .. }));
    let mut b = Client::connect(server.local_addr());
    b.hello(Some(1));
    a.set_ready(true);
    b.set_ready(true);
    // The two connections race: start only once the lobby shows both ready.
    loop {
        if let Got::Lobby { players, .. } = a.next()
            && players.len() == 2
            && players.iter().all(|p| p.2)
        {
            break;
        }
    }
    a.start_game();
    until_started(&mut a);
    until_started(&mut b);
    (server, a, b)
}

/// Reads `a`'s messages, keeping it alive (acks and pings), until one matches.
fn keep_alive_until(a: &mut Client, what: &str, matches: impl Fn(&Got) -> bool) -> Got {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "no {what} within 10 s");
        a.ping(1);
        match a.next() {
            Got::DayUpdate { day, .. } => a.ack(day),
            Got::Closed => panic!("the server closed the connection"),
            got if matches(&got) => return got,
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// M4-4, with real time: a silent player pauses everyone, speaking again resumes the
/// game, and staying silent past the drop threshold drops them while the others
/// play on (D24).
#[test]
fn a_silent_player_pauses_everyone_then_is_dropped() {
    let (server, mut a, mut b) = started_pair(Duration::from_millis(600), Duration::from_millis(2500));
    a.set_speed(Speed::Fast);
    // B says nothing: the game waits for player 1.
    keep_alive_until(
        &mut a,
        "fairness pause",
        |g| matches!(g, Got::ServerState { speed: Speed::Paused, waiting_for, .. } if waiting_for == &[1]),
    );
    // B speaks: the game resumes at its speed.
    b.ping(2);
    keep_alive_until(
        &mut a,
        "resume",
        |g| matches!(g, Got::ServerState { speed: Speed::Fast, waiting_for, .. } if waiting_for.is_empty()),
    );
    // B goes silent for good: paused again, then dropped, and A's game resumes.
    keep_alive_until(&mut a, "second pause", |g| matches!(g, Got::ServerState { speed: Speed::Paused, .. }));
    keep_alive_until(
        &mut a,
        "resume after the drop",
        |g| matches!(g, Got::ServerState { speed: Speed::Fast, waiting_for, .. } if waiting_for.is_empty()),
    );
    let goodbye = loop {
        match b.next() {
            Got::Goodbye(reason) => break reason,
            Got::Closed => panic!("closed without a Goodbye"),
            _ => {}
        }
    };
    assert!(goodbye.contains("no message for 2.5 s"), "{goodbye}");
    server.shutdown().expect("clean shutdown");
}
