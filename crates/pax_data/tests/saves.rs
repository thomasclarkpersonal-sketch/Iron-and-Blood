//! Save files (D23, DATA_FORMAT "Save files"): the format round-trips, both loaders
//! agree, and a save that disagrees with itself is refused.

mod common;

use common::{TempDir, repo};
use pax_data::save::{Checkpoint, SaveData, SavedCommand, checkpoint_days, load, load_by_replay, snapshot_path};
use pax_data::{Scenario, step_day};
use pax_engine::{Command, Fixed};

fn two_states() -> Scenario {
    pax_data::load_scenario(&repo().join("scenarios/two_states")).unwrap()
}

/// Plays `days` days of two_states through the shared day step, logging history the
/// way the server does (`Game::step`): scripted commands, then the players' that
/// applied, and a checkpoint on every checkpoint day. A player command applies on
/// day 10.
fn played(days: u64) -> (Scenario, SaveData) {
    let mut s = two_states();
    let mut commands = Vec::new();
    let mut checkpoints = Vec::new();
    for _ in 0..days {
        let day = s.world.day;
        let players =
            if day == 10 { vec![Command::SetIncomeTax { nation: 1, rate: Fixed::from_raw(160_000) }] } else { vec![] };
        let step = step_day(&mut s.world, &s.commands, &players);
        for (command, result) in step.outcomes.scripted {
            result.expect("scripted commands apply");
            commands.push(SavedCommand { day, player: None, command });
        }
        for (&command, result) in players.iter().zip(step.outcomes.players) {
            result.expect("the player's command applies");
            commands.push(SavedCommand { day, player: Some(0), command });
        }
        if checkpoint_days(s.world.day).last() == Some(s.world.day) {
            checkpoints.push(Checkpoint { day: s.world.day, state_hash: s.world.state_hash() });
        }
    }
    let save = SaveData {
        scenario: repo().join("scenarios/two_states"),
        content_hash: s.content_hash,
        day: s.world.day,
        checkpoints,
        commands,
    };
    (s, save)
}

fn error(result: Result<pax_data::save::LoadedSave, pax_data::LoadError>) -> String {
    result.err().expect("the save should have been refused").to_string()
}

#[test]
fn a_save_round_trips_and_both_loaders_agree() {
    let dir = TempDir::new("save-round-trip");
    // Past day 360, so the log holds the scenario's scripted commands too.
    let (s, save) = played(370);
    assert!(save.commands.iter().any(|c| c.player.is_none()) && save.commands.iter().any(|c| c.player.is_some()));
    assert_eq!(save.checkpoints.len(), 12);
    let path = dir.0.join("g.toml");
    save.write(&path, &s.world).unwrap();

    let fast = load(&path).unwrap();
    let slow = load_by_replay(&path).unwrap();
    assert_eq!(fast.save, save, "the TOML writer and reader agree on every field");
    assert_eq!(slow.save, save);
    assert_eq!(fast.scenario.world.state_hash(), s.world.state_hash());
    assert_eq!(slow.scenario.world.state_hash(), s.world.state_hash());
}

#[test]
fn a_save_for_changed_content_is_refused() {
    let dir = TempDir::new("save-content");
    let (s, save) = played(0);
    let path = dir.0.join("changed.toml");
    SaveData { content_hash: save.content_hash ^ 1, ..save }.write(&path, &s.world).unwrap();
    assert!(error(load(&path)).contains("files changed since this game was saved"));
}

#[test]
fn a_snapshot_from_another_game_is_refused() {
    let dir = TempDir::new("save-swap");
    let (s, save) = played(95);
    let path = dir.0.join("g.toml");
    save.write(&path, &s.world).unwrap();
    let (s0, save0) = played(0);
    save0.write(&dir.0.join("other.toml"), &s0.world).unwrap();
    std::fs::copy(dir.0.join("other.world"), snapshot_path(&path)).unwrap();
    assert!(error(load(&path)).contains("not this save's snapshot"));
}

/// Checks both loaders make before choosing a path: the checkpoints are exactly the
/// checkpoint days, and on a checkpoint day the last one is the snapshot's state.
#[test]
fn history_that_disagrees_with_itself_is_refused_without_replaying() {
    let dir = TempDir::new("save-history");
    let (s, save) = played(90);
    let path = dir.0.join("g.toml");
    let write = |save: &SaveData| save.write(&path, &s.world).unwrap();

    let mut missing = save.clone();
    missing.checkpoints.remove(1);
    write(&missing);
    assert!(error(load(&path)).contains("checkpoints must be for days [30, 60, 90]"));

    let mut reordered = save.clone();
    reordered.checkpoints.swap(0, 1);
    write(&reordered);
    assert!(error(load(&path)).contains("checkpoints must be for days"));

    let mut wrong_last = save.clone();
    wrong_last.checkpoints[2].state_hash ^= 1;
    write(&wrong_last);
    assert!(error(load(&path)).contains("disagrees with snapshot_hash"));

    // A middle checkpoint can only be checked by replaying (the log is trusted until then).
    let mut wrong_middle = save.clone();
    wrong_middle.checkpoints[0].state_hash ^= 1;
    write(&wrong_middle);
    assert!(load(&path).is_ok());
    assert!(error(load_by_replay(&path)).contains("different state on day 30"));
}

/// The snapshot's file name comes from the save's own name: a save can't name
/// another file for the loader to open.
#[test]
fn a_save_cannot_name_its_snapshot_file() {
    let dir = TempDir::new("save-snapshot-name");
    let (s, save) = played(3);
    let path = dir.0.join("g.toml");
    save.write(&path, &s.world).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("snapshot_hash", "snapshot = \"/etc/passwd\"\nsnapshot_hash")).unwrap();
    assert!(error(load(&path)).contains("snapshot"), "an unknown field is refused");
}
