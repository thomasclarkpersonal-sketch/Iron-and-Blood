//! The daily tick: runs every system in a fixed order.
//!
//! Order matters and is part of the design contract (docs/ARCHITECTURE.md):
//!
//! | # | System | Cadence |
//! |---|--------|---------|
//! | 1 | labour: assign employment | daily |
//! | 2 | production | daily |
//! | 3 | market: orders → price discovery → settlement | daily |
//! | 4 | firms: wages and dividends | daily |
//! | 5 | demographics | month end |
//!
//! Weekly systems (promotion, migration) and monthly politics slot in after
//! step 4 when they are implemented.

use crate::fixed::Fixed;
use crate::systems::market::GoodReport;
use crate::systems::{demographics, firms, labor, market, production};
use crate::world::World;

/// Diagnostics produced by one day. Not part of the simulation state.
#[derive(Clone, Debug)]
pub struct DayReport {
    /// The day that was simulated.
    pub day: u64,
    /// Row-major `[market * goods + good]`.
    pub goods: Vec<GoodReport>,
    pub iterations: Vec<u32>,
    pub total_money: Fixed,
}

/// Advances the world by one day.
///
/// # Panics
/// If total money changes. Until a minting mechanic exists money must be
/// exactly conserved (DECISIONS.md D5); a failure here is always a bug in a
/// system, and continuing would corrupt every later tick.
pub fn step(world: &mut World) -> DayReport {
    let money_before = world.total_money();
    let pools = labor::pop_pools(world);

    labor::assign_employment(world, &pools);
    production::produce(world);
    let outcome = market::clear_markets(world);
    firms::pay_wages_and_dividends(world, &pools, &outcome.revenue, &outcome.input_cost);
    if demographics::is_month_end(world) {
        demographics::update_population(world);
    }

    let money_after = world.total_money();
    assert_eq!(money_before, money_after, "money not conserved on day {}", world.day);
    let day = world.day;
    world.day += 1;
    DayReport { day, goods: outcome.goods, iterations: outcome.iterations, total_money: money_after }
}

/// Runs `days` ticks and returns the state hash after each one.
///
/// Hashing walks every column, so it is done here for the determinism harness
/// rather than inside [`step`].
pub fn run(world: &mut World, days: u64) -> Vec<u64> {
    (0..days)
        .map(|_| {
            step(world);
            world.state_hash()
        })
        .collect()
}
