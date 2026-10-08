//! Saving and loading over real TCP (M3-6, D23).

mod common;

use common::*;
use pax_protocol::wire::Speed;
use pax_server::Config;

/// Plays at Fastest, acknowledging, until at least `day`; returns the day reached.
fn play_until(c: &mut Client, day: u64) -> u64 {
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
            Got::ServerState { speed: Speed::Paused, day } => return day,
            Got::DayUpdate { day, .. } => c.ack(day),
            _ => {}
        }
    }
}

#[test]
fn save_list_and_load_over_the_wire() {
    let saves = std::env::temp_dir().join(format!("pax-saves-tcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&saves);
    let mut config = Config::local(scenario("two_states"));
    config.saves_dir = saves.clone();
    let server = start(config);
    let mut c = Client::connect(server.local_addr());
    c.hello(Some(0));
    assert!(matches!(c.next(), Got::Welcome { .. }));

    c.set_income_tax(1, 0, 140_000);
    assert!(matches!(c.next(), Got::CommandResult { client_seq: 1, .. }));
    let saved_day = play_until(&mut c, 40);
    c.save_game("first_war");
    assert_eq!(c.next(), Got::SaveResult { name: "first_war".into(), error: String::new() });
    c.list_saves();
    assert_eq!(c.next(), Got::SaveList(vec!["first_war".into()]));

    let later = play_until(&mut c, saved_day + 20);
    assert!(later > saved_day);
    c.load_game("first_war");
    match c.next() {
        Got::Welcome { day, nation, .. } => assert_eq!((day, nation), (saved_day, Some(0))),
        other => panic!("a load answers with Welcome, got {other:?}"),
    }
    c.load_game("missing");
    assert!(matches!(c.next(), Got::SaveResult { error, .. } if error.contains("there is no save")));
    server.shutdown().expect("clean shutdown");
    std::fs::remove_dir_all(&saves).unwrap();
}
