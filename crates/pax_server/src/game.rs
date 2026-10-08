//! The game the sim thread runs: the scenario's world, and everything the views
//! derive from it (D22).
//!
//! Derived data can't go stale here because there is no cache to invalidate.
//! [`Game`] owns the world, which is read-only from outside. Every method that
//! changes the world rebuilds [`Today`] in the same step, so a world change without
//! fresh views can't be written (AGENTS.md §1, D7).

use pax_data::Scenario;
use pax_engine::views::ProvinceStats;
use pax_engine::{DayReport, World};

use crate::view::DayViews;

/// Everything derived from the world's current state that every session's update
/// shares: the report of the day that produced it, the per-province stats, and the
/// state hash (D10, D22). Computed once per change to the world, never per request.
/// The hash costs about 28 ms at 1M POP rows (M3-3).
struct Today {
    /// `None` before the first tick, and after a load (a report isn't state).
    report: Option<DayReport>,
    stats: ProvinceStats,
    state_hash: u64,
}

impl Today {
    fn of(world: &World, report: Option<DayReport>) -> Today {
        let stats = ProvinceStats::of(world, report.as_ref().map(|r| r.labour.as_slice()));
        Today { stats, state_hash: world.state_hash(), report }
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

    /// The views of the current state, shared by every session.
    pub(crate) fn views(&self) -> DayViews<'_> {
        DayViews {
            world: &self.scenario.world,
            report: self.today.report.as_ref(),
            stats: &self.today.stats,
            state_hash: self.today.state_hash,
        }
    }
}
