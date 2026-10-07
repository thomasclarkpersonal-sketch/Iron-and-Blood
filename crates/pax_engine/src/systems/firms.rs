//! Wages, wage adjustment and dividends (DECISIONS.md D6).
//!
//! Runs after the market so producers pay out of today's receipts.
//!
//! * **Value added:** `V = revenue − input purchases`, smoothed as
//!   `V̄ ← V̄ + (V − V̄) / h` (`h` = `revenue_smoothing_days`).
//! * **Sticky wages:** target `w* = labor_share × max(V̄, 0) / E`; the wage closes
//!   `1 / wage_stickiness_days` of the gap each day, so a bad day does not
//!   crash wages (MACROECONOMICS.md §4).
//! * **Wage bill:** `min(w × E, cash)`, paid into the labour pool
//!   `(province, worker profession)` and split across its POPs by size.
//! * **Dividends:** cash above `reserve_days × wage bill` is paid out at
//!   `dividend_payout_rate` per day to owner POPs in the same market, split by
//!   size. With no owners present the profit is retained.
//!
//! Every transfer is a debit from one column and a credit to another of
//! exactly the same amount, so total money is unchanged.

use crate::alloc::allocate_raw;
use crate::fixed::Fixed;
use crate::groups::Groups;
use crate::layout::{PopLayout, owner_key, pool_key};
use crate::world::World;

/// Totals paid out by [`pay_wages_and_dividends`] (diagnostics only).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Payouts {
    pub wages: Fixed,
    pub dividends: Fixed,
}

pub fn pay_wages_and_dividends(
    world: &mut World,
    layout: &PopLayout,
    revenue: &[Fixed],
    input_cost: &[Fixed],
) -> Payouts {
    let defs = world.defs.clone();
    let rules = &defs.rules.firms;
    let (pools, owners) = (&layout.labour, &layout.owners);

    let mut wage_income = vec![Fixed::ZERO; pools.key_count()];
    let mut dividend_income = vec![Fixed::ZERO; owners.key_count()];

    for i in 0..world.producers.len() {
        let def = &defs.producer_types[world.producers.kind[i] as usize];
        let market = world.market_of_province(world.producers.province[i]);
        let wage_pool = pool_key(world, world.producers.province[i], def.worker);
        let owner_pool = owner_key(world, market, def.owner);
        let has_owners = owners.members(owner_pool).iter().any(|&r| world.pops.size[r as usize] > 0);

        let p = &mut world.producers;
        let employed = p.employed[i] as i64;
        let avg = p.value_added_avg[i];
        p.value_added_avg[i] =
            avg + (revenue[i] - input_cost[i] - avg).div_int(rules.revenue_smoothing_days.max(1) as i64);
        if employed > 0 {
            let target = def.labor_share.mul(p.value_added_avg[i].max(Fixed::ZERO)).div_int(employed);
            let wage = p.wage[i];
            p.wage[i] = (wage + (target - wage).div_int(rules.wage_stickiness_days.max(1) as i64)).max(Fixed::ZERO);
        }

        let bill = p.wage[i].mul_int(employed);
        let paid = bill.min(p.cash[i]);
        p.cash[i] -= paid;
        wage_income[wage_pool] += paid;

        if has_owners {
            let reserve = bill.mul_int(rules.reserve_days as i64);
            let surplus = p.cash[i] - reserve;
            if surplus.is_positive() {
                let dividend = surplus.mul(rules.dividend_payout_rate);
                p.cash[i] -= dividend;
                dividend_income[owner_pool] += dividend;
            }
        }
    }

    distribute(world, pools, &wage_income);
    distribute(world, owners, &dividend_income);
    Payouts { wages: wage_income.iter().copied().sum(), dividends: dividend_income.iter().copied().sum() }
}

/// Credits each group's income to its member POPs pro rata to size.
fn distribute(world: &mut World, groups: &Groups, income: &[Fixed]) {
    for (k, &amount) in income.iter().enumerate() {
        if amount.is_zero() {
            continue;
        }
        let rows = groups.members(k);
        let sizes: Vec<i64> = rows.iter().map(|&r| world.pops.size[r as usize] as i64).collect();
        let shares = allocate_raw(amount.raw(), &sizes).expect("income is only routed to groups with living members");
        for (&r, s) in rows.iter().zip(shares) {
            world.pops.cash[r as usize] += Fixed::from_raw(s);
        }
    }
}
