//! Determinism harness (DECISIONS.md D3, D11).
//!
//! These tests are the gate that keeps lockstep multiplayer and save/load
//! replay possible: identical inputs must give bit-identical state on every
//! day, independent of thread count, of process restarts, and of time.

use std::path::PathBuf;

use pax_data::golden;
use pax_engine::{World, tick};

fn scenario_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mini_valley")
}

fn load() -> World {
    pax_data::load_scenario(&scenario_dir()).expect("mini_valley loads").world
}

fn run_with_threads(threads: usize, days: u64) -> Vec<u64> {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
    pool.install(|| tick::run(&mut load(), days))
}

#[test]
fn same_input_same_hashes() {
    assert_eq!(tick::run(&mut load(), 365), tick::run(&mut load(), 365));
}

#[test]
fn independent_of_thread_count() {
    let one = run_with_threads(1, 120);
    for threads in [2, 3, 8] {
        assert_eq!(one, run_with_threads(threads, 120), "{threads} threads diverged");
    }
}

#[test]
fn snapshot_resume_matches_continuous_run() {
    let mut continuous = load();
    tick::run(&mut continuous, 100);
    let mut resumed = continuous.clone();
    assert_eq!(tick::run(&mut continuous, 100), tick::run(&mut resumed, 100));
    assert_eq!(continuous, resumed);
}

#[test]
fn two_states_matches_golden_hashes() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
    let expected = golden::read(&dir.join("golden.hashes")).expect("two_states golden.hashes present");
    let scenario = pax_data::load_scenario(&dir).expect("two_states loads");
    assert!(!scenario.commands.is_empty(), "two_states replays a command log");
    assert!(expected.len() > 720, "the golden replay must cover the whole command log (last command: day 720)");
    let mut world = scenario.world;
    let actual =
        pax_data::run_logged(&mut world, &scenario.commands, expected.len() as u64).expect("logged commands accepted");
    assert_eq!(actual, expected, "two_states results changed; re-record if intended");
}

#[test]
fn matches_golden_hashes() {
    let path = scenario_dir().join("golden.hashes");
    let expected = golden::read(&path)
        .expect("golden.hashes present; record with `cargo run -p pax_cli -- record scenarios/mini_valley`");
    let actual = tick::run(&mut load(), expected.len() as u64);
    if let Some(day) = expected.iter().zip(&actual).position(|(a, b)| a != b) {
        panic!(
            "simulation results changed on day {day}. If intended, re-record with \
             `cargo run --release -p pax_cli -- record scenarios/mini_valley` and commit golden.hashes."
        );
    }
}

#[test]
fn money_is_conserved_for_a_year() {
    let mut world = load();
    let start = world.total_money();
    for _ in 0..365 {
        assert_eq!(pax_engine::step(&mut world).total_money, start);
    }
}
