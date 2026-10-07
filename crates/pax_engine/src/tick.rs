//! The daily tick: runs every system in a fixed order.
//!
//! Order matters and is part of the design contract (docs/ARCHITECTURE.md):
//!
//! | # | System | Cadence |
//! |---|--------|---------|
//! | 1 | labour: assign employment | daily |
//! | 2 | production | daily |
//! | 3 | market: orders → price discovery → settlement | daily |
//! | 4 | firms: wages and dividends, income tax withheld | daily |
//! | 4b | government: transfers from treasuries to POPs | daily |
//! | 5 | mobility: unemployed workers move to vacancies (D18), then migrate within their market (D20) | month end |
//! | 6 | politics: militancy (D19) | month end |
//! | 7 | demographics, then POP row compaction (D7) | month end |
//!
//! Weekly systems (promotion, migration) and monthly politics slot in after
//! step 4 when they are implemented.

use crate::fixed::Fixed;
use crate::systems::firms::Payouts;
use crate::systems::labor::LabourReport;
use crate::systems::market::{GoodReport, LifeNeedsSummary};
use crate::systems::{demographics, firms, government, labor, market, mobility, politics, production};
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
    /// Life-needs coverage at today's market.
    pub life_needs: LifeNeedsSummary,
    /// Paid from treasuries to POPs today (D15).
    pub transfers: Fixed,
    /// Paid from treasuries for government consumption today (D16).
    pub government_spending: Fixed,
    /// People who changed profession today (month end only, D18).
    pub moved: u64,
    /// People who moved to another province of their market today (month end only, D20).
    pub migrated: u64,
    /// POP rows removed by month-end compaction (D7).
    pub compacted: usize,
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
    let transfers = government::pay_transfers(world, &layout);
    let (mut moved, mut migrated, mut compacted) = (0, 0, 0);
    if demographics::is_month_end(world) {
        moved = mobility::reassign_workers(world, &layout, &labour);
        // Mobility may have appended rows, so the tick's layout snapshot is stale
        // from here on. Migration re-fetches it; later month-end systems (politics,
        // demographics, compaction) do not read the layout.
        let regrouped = world.pop_layout();
        migrated = mobility::migrate_within_markets(world, &regrouped);
        politics::update_militancy(world);
        demographics::update_population(world);
        compacted = world.compact_pops();
    }

    let money_after = world.total_money();
    assert_eq!(money_before, money_after, "money not conserved on day {}", world.day);
    let day = world.day;
    world.day += 1;
    DayReport {
        day,
        input_spending: outcome.input_spending,
        household_spending: outcome.household_spending,
        life_needs: outcome.life_needs,
        transfers,
        government_spending: outcome.government_spending,
        moved,
        migrated,
        compacted,
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
