//! The game the sim thread runs: the scenario's world, and everything the views
//! derive from it (D22).
//!
//! Derived data can't go stale here because there is no cache to invalidate.
//! [`Game`] owns the world, which is read-only from outside. Every method that
//! changes the world rebuilds [`Today`] in the same step, so a world change without
//! fresh views can't be written (AGENTS.md §1, D7).

use pax_data::Scenario;
use pax_engine::views::ProvinceStats;
use pax_engine::{Command, CommandError, DayReport, World};

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

/// The scenario being played, with its derived [`Today`].
pub(crate) struct Game {
    scenario: Scenario,
    today: Today,
}

impl Game {
    pub(crate) fn new(scenario: Scenario) -> Game {
        let today = Today::of(&scenario.world, None);
        Game { scenario, today }
    }

    pub(crate) fn world(&self) -> &World {
        &self.scenario.world
    }

    pub(crate) fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    /// Runs one day with `commands` applied at its start (D21) and rebuilds
    /// [`Today`] from the result in the same step. Returns each command's outcome.
    pub(crate) fn step(&mut self, commands: &[Command]) -> Vec<Result<(), CommandError>> {
        let (report, results) = pax_engine::tick::step_with(&mut self.scenario.world, commands);
        self.today = Today::of(&self.scenario.world, Some(report));
        results
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
            game.step(&[]);
        }
        assert!(game.views().report.is_some());
    }
}
