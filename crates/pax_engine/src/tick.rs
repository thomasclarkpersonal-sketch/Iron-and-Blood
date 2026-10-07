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
use crate::systems::firms::Payouts;
use crate::systems::labor::LabourReport;
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
    /// Employment per labour pool (see [`LabourReport`]).
    pub labour: Vec<LabourReport>,
    /// Household consumption spending: final demand, i.e. expenditure GDP
    /// in this closed economy without government or investment.
    pub household_spending: Fixed,
    /// Spending by producers on input goods (intermediate consumption).
    pub input_spending: Fixed,
    /// Wages and dividends paid out today.
    pub payouts: Payouts,
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
    let layout = world.pop_layout();

    let labour = labor::assign_employment(world, &layout.labour);
    production::produce(world);
    let outcome = market::clear_markets(world, &layout);
    let payouts = firms::pay_wages_and_dividends(world, &layout, &outcome.revenue, &outcome.input_cost);
    if demographics::is_month_end(world) {
        demographics::update_population(world);
    }

    let money_after = world.total_money();
    assert_eq!(money_before, money_after, "money not conserved on day {}", world.day);
    let day = world.day;
    world.day += 1;
    DayReport {
        day,
        input_spending: outcome.input_spending,
        household_spending: outcome.household_spending,
        goods: outcome.goods,
        iterations: outcome.iterations,
        labour,
        payouts,
        total_money: money_after,
    }
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
