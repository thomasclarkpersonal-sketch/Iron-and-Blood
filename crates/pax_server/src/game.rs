//! The game the sim thread runs: the scenario's world, the log of every command
//! that applied to it (D21, D23), and everything the views derive from it (D22).
//!
//! [`Game`] owns the world, which is read-only from outside. Every method that
//! changes the world appends what applied to the log and rebuilds [`Today`] in the
//! same step. So a world change that isn't logged, or that leaves the views stale,
//! can't be written (AGENTS.md §1, D7). Saves are built from this log (D23).

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

/// A command that applied: the game's history (D21). Saves are the scenario plus
/// this log (D23).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Logged {
    /// The day at whose start it applied.
    pub day: u64,
    /// `None` for the scenario's own scripted commands (`commands.toml`).
    pub player: Option<u16>,
    pub command: Command,
}

/// The scenario being played, its history, and its derived [`Today`].
pub(crate) struct Game {
    scenario: Scenario,
    /// Every command that applied, in application order.
    log: Vec<Logged>,
    today: Today,
}

impl Game {
    pub(crate) fn new(scenario: Scenario) -> Game {
        let today = Today::of(&scenario.world, None);
        Game { scenario, log: Vec::new(), today }
    }

    pub(crate) fn world(&self) -> &World {
        &self.scenario.world
    }

    pub(crate) fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    /// Every command that applied so far, in application order. (Saves read it from
    /// M3-6 on.)
    #[cfg(test)]
    pub(crate) fn log(&self) -> &[Logged] {
        &self.log
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
                Ok(()) => self.log.push(Logged { day, player: None, command }),
                Err(e) => tracing::warn!(day, ?command, %e, "scripted command rejected"),
            }
        }
        let mut rejected = Vec::new();
        for (p, result) in players.into_iter().zip(outcomes.players) {
            match result {
                Ok(()) => self.log.push(Logged { day, player: Some(p.player), command: p.command }),
                Err(e) => rejected.push((p, e)),
            }
        }
        self.today = Today::of(&self.scenario.world, Some(report));
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
}
