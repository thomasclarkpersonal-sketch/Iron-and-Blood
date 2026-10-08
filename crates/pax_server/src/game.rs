//! The game the sim thread runs: the scenario's world, the log of every command
//! that applied to it (D21, D23), and everything the views derive from it (D22).
//!
//! [`Game`] owns the world, which is read-only from outside. Every method that
//! changes the world appends what applied to the log, records the D23 checkpoints
//! and rebuilds [`Today`] in the same step. So a world change that isn't logged, or
//! that leaves the views stale, can't be written (AGENTS.md §1, D7). Saves are built
//! from this log (D23).

use pax_data::save::{CHECKPOINT_DAYS, Checkpoint, SaveData, SavedCommand};
use pax_data::{DayStep, Scenario};
use pax_engine::views::ProvinceStats;
use pax_engine::{Command, CommandError, DayReport, World};

use crate::queue::Pending;
use crate::view::DayViews;

/// Everything derived from the world's current state that every session's update
/// shares: the report of the day that produced it, the per-province stats, and the
/// state hash (D10, D22). Computed once per change to the world, never per request.
/// The hash costs about 28 ms at 1M POP rows (M3-3).
struct Today {
    /// The world's day this was computed for: checked in debug builds on every use.
    day: u64,
    /// `None` before the first tick, and after a load (a report isn't state).
    report: Option<DayReport>,
    stats: ProvinceStats,
    state_hash: u64,
}

impl Today {
    fn of(world: &World, report: Option<DayReport>) -> Today {
        let stats = ProvinceStats::of(world, report.as_ref().map(|r| r.labour.as_slice()));
        Today { day: world.day, stats, state_hash: world.state_hash(), report }
    }
}

/// The scenario being played, its history, and its derived [`Today`].
pub(crate) struct Game {
    scenario: Scenario,
    /// Every command that applied, in application order: the game's history (D21),
    /// and what a save is built from (D23).
    log: Vec<SavedCommand>,
    /// The state hash each time `world.day` reached a multiple of `CHECKPOINT_DAYS`:
    /// what saves are verified against (D23).
    checkpoints: Vec<Checkpoint>,
    today: Today,
}

impl Game {
    pub(crate) fn new(scenario: Scenario) -> Game {
        let today = Today::of(&scenario.world, None);
        Game { scenario, log: Vec::new(), checkpoints: Vec::new(), today }
    }

    /// A loaded save (D23): its world, history and checkpoints, with [`Today`]
    /// rebuilt. `report` is the last replayed day's, or `None` when loaded from a
    /// snapshot.
    pub(crate) fn resume(scenario: Scenario, save: SaveData, report: Option<DayReport>) -> Game {
        let today = Today::of(&scenario.world, report);
        Game { scenario, log: save.commands, checkpoints: save.checkpoints, today }
    }

    pub(crate) fn world(&self) -> &World {
        &self.scenario.world
    }

    pub(crate) fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    /// Every command that applied so far, in application order.
    #[cfg(test)]
    pub(crate) fn log(&self) -> &[SavedCommand] {
        &self.log
    }

    #[cfg(test)]
    pub(crate) fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// The game as a save (D23): the scenario, as `scenario_dir` names it, plus its
    /// history. Commands queued for the next tick haven't applied, so they aren't in it.
    pub(crate) fn save_data(&self, scenario_dir: &std::path::Path) -> SaveData {
        SaveData {
            scenario: scenario_dir.to_path_buf(),
            content_hash: self.scenario.content_hash,
            day: self.scenario.world.day,
            checkpoints: self.checkpoints.clone(),
            commands: self.log.clone(),
        }
    }

    /// Runs one day through the shared day step ([`pax_data::step_day`]): the
    /// scenario's scripted commands, then `players`, already in stamp order. Logs
    /// every command that applied and rebuilds [`Today`], in the same step.
    ///
    /// Returns the players' commands that failed when applied (none can today), for
    /// their late error `CommandResult`. A rejected scripted command is logged as a
    /// warning: a running game can't stop for a scenario's mistake (`step_day`).
    pub(crate) fn step(&mut self, players: Vec<Pending>) -> Vec<(Pending, CommandError)> {
        let day = self.scenario.world.day;
        let commands: Vec<Command> = players.iter().map(|p| p.command).collect();
        let DayStep { report, outcomes } =
            pax_data::step_day(&mut self.scenario.world, &self.scenario.commands, &commands);
        for (command, result) in outcomes.scripted {
            match result {
                Ok(()) => self.log.push(SavedCommand { day, player: None, command }),
                Err(e) => tracing::warn!(day, ?command, %e, "scripted command rejected"),
            }
        }
        // `step_day` returns one outcome per player command, index-aligned.
        assert_eq!(outcomes.players.len(), players.len(), "one outcome per player command");
        let mut rejected = Vec::new();
        for (p, result) in players.into_iter().zip(outcomes.players) {
            match result {
                Ok(()) => self.log.push(SavedCommand { day, player: Some(p.player), command: p.command }),
                Err(e) => rejected.push((p, e)),
            }
        }
        self.today = Today::of(&self.scenario.world, Some(report));
        let day = self.scenario.world.day;
        if day.is_multiple_of(CHECKPOINT_DAYS) {
            // The hash Today just computed (D10): no second pass.
            self.checkpoints.push(Checkpoint { day, state_hash: self.today.state_hash });
        }
        rejected
    }

    /// The views of the current state, shared by every session.
    pub(crate) fn views(&self) -> DayViews<'_> {
        debug_assert_eq!(
            self.today.day, self.scenario.world.day,
            "Today is from another day: a world change skipped Today::of"
        );
        DayViews {
            world: &self.scenario.world,
            report: self.today.report.as_ref(),
            stats: &self.today.stats,
            state_hash: self.today.state_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The views always describe the world as it is now, through every step.
    #[test]
    fn views_track_the_world_through_every_step() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        let mut game = Game::new(pax_data::load_scenario(&dir).unwrap());
        for _ in 0..40 {
            let views = game.views();
            assert_eq!(views.state_hash, game.world().state_hash());
            assert_eq!(views.stats, &ProvinceStats::of(game.world(), views.report.map(|r| r.labour.as_slice())));
            game.step(Vec::new());
        }
        assert!(game.views().report.is_some());
    }

    /// M3's definition of done (item 4): the server's day costs little beyond the
    /// tick, at D13's long-term scale on 8 threads. It reports the tick, the state hash
    /// (per day, D10/D22) and everything else the server adds (stats and one full
    /// update). It asserts that the server's own work stays within 10% of the tick:
    /// the server's share only. Whether the tick itself fits D13's 100 ms is D13's
    /// "Measured" column, and at `two_states` content it doesn't (MILESTONE_3, item 4).
    /// The hash is reported, not asserted: the owner kept it per day, outside the
    /// budget (MILESTONE_3's risks). Run by hand:
    /// `cargo test -p pax_server --release -- --ignored server_day_budget --nocapture`
    #[test]
    #[ignore]
    fn server_day_budget() {
        use crate::view::{self, Subscription};
        use pax_protocol::wire;
        // Timed days; with the warm-up day they must end before the first month end.
        const DAYS: u32 = 10;
        let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap();
        pool.install(|| {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
            let mut scenario = pax_data::load_scenario(&dir).unwrap();
            let regions = 1_500;
            let scale = pax_data::bench::scale_for_rows(&scenario.world, 1_000_000, regions);
            // The warm-up day plus DAYS must end before the first month end, when
            // compaction merges the `scale` copies (D7) and the world shrinks.
            assert!(
                u64::from(DAYS) < pax_data::bench::days_before_compaction(&scenario.world),
                "server_day_budget must finish before the first month end"
            );
            scenario.world = pax_data::bench::replicate_with_nations(&scenario.world, scale, regions, Some(200));
            let rows = scenario.world.pops.size.len();
            let mut bare = scenario.world.clone();
            let mut game = Game::new(scenario);
            let sub = Subscription { map_mode: wire::MapMode::LifeNeeds, map_good: 0, market: Some(7), province: Some(11) }
                .checked(game.world())
                .unwrap();
            // Warm up both, then time.
            pax_engine::step(&mut bare);
            game.step(Vec::new());
            let per_day = |start: std::time::Instant| start.elapsed().as_secs_f64() * 1e3 / f64::from(DAYS);
            let start = std::time::Instant::now();
            for _ in 0..DAYS {
                pax_engine::step(&mut bare);
            }
            let tick_ms = per_day(start);
            let start = std::time::Instant::now();
            for _ in 0..DAYS {
                let _ = bare.state_hash();
            }
            let hash_ms = per_day(start);
            let start = std::time::Instant::now();
            for _ in 0..DAYS {
                game.step(Vec::new());
                let _ = view::day_update(&game.views(), &sub, wire::Speed::Fastest, 0, true);
            }
            let day_ms = per_day(start);
            let server_ms = day_ms - tick_ms - hash_ms;
            println!(
                "{rows} POP rows, 8 threads: tick {tick_ms:.1} ms + state hash {hash_ms:.1} ms + stats and one update {server_ms:.1} ms = {day_ms:.1} ms per day"
            );
            assert!(server_ms <= tick_ms * 0.1, "stats and the update took {server_ms:.1} ms, over 10% of the tick");
        });
    }

    /// M4's definition of done, item 5 (D24, M4-7): a remote client's bandwidth at
    /// speed 3 at D13's long-term scale, with every view subscribed. It encodes a real
    /// update with and without the `MapView`, then applies the throttle's policy: at
    /// most four updates a second, the map in every fifth. Run by hand:
    /// `cargo test -p pax_server --release -- --ignored remote_bandwidth --nocapture`
    #[test]
    #[ignore]
    fn remote_bandwidth_budget() {
        use crate::throttle::{MAP_EVERY, UPDATES_PER_SECOND};
        use crate::view::{self, Subscription};
        use pax_protocol::{Pacing, wire};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        let mut scenario = pax_data::load_scenario(&dir).unwrap();
        // two_states has 4 provinces: 2,500 copies give D24's 10,000.
        let regions = 2_500;
        let scale = pax_data::bench::scale_for_rows(&scenario.world, 1_000_000, regions);
        scenario.world = pax_data::bench::replicate_with_nations(&scenario.world, scale, regions, Some(200));
        let provinces = scenario.world.geography.province_count();
        let mut game = Game::new(scenario);
        game.step(Vec::new());
        let sub =
            Subscription { map_mode: wire::MapMode::Population, map_good: 0, market: Some(7), province: Some(11) }
                .checked(game.world())
                .unwrap();
        let views = game.views();
        let with_map = view::day_update(&views, &sub, wire::Speed::Normal, 0, true).len() as f64;
        let without = view::day_update(&views, &sub, wire::Speed::Normal, 0, false).len() as f64;
        let per_update = (without * f64::from(MAP_EVERY - 1) + with_map) / f64::from(MAP_EVERY);
        let rate = |speed: wire::Speed| -> f64 {
            let days_per_second = match pax_protocol::pacing(speed) {
                Pacing::Every(d) if !d.is_zero() => 1.0 / d.as_secs_f64(),
                _ => f64::INFINITY,
            };
            days_per_second.min(f64::from(UPDATES_PER_SECOND)) * per_update / 1000.0
        };
        // Speed 3 is `Normal`, two days a second (D23's pacing table).
        let (speed3, fastest) = (rate(wire::Speed::Normal), rate(wire::Speed::Fastest));
        println!(
            "{provinces} provinces: update {:.1} KB with the map, {:.1} KB without; remote client {speed3:.1} KB/s at speed 3, {fastest:.1} KB/s at Fastest",
            with_map / 1000.0,
            without / 1000.0
        );
        assert!(speed3 <= 100.0, "{speed3:.1} KB/s at speed 3, over D24's 100 KB/s");
    }
}
