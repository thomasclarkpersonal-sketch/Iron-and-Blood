//! Wages, wage adjustment and dividends (DECISIONS.md D6).
//!
//! Runs after the market so producers pay out of today's receipts.
//!
//! * **Value added:** `V = revenue − input purchases`, smoothed as
//!   `V̄ ← V̄ + (V − V̄) / h` (`h` = `revenue_smoothing_days`).
//! * **Sticky wages:** target `w* = max(labor_share × max(V̄, 0) / E, m × C)`, where
//!   `C` is a worker's daily subsistence cost and `m` = `subsistence_wage_multiple`
//!   (the wage floor that anchors prices to the cost of labour); the wage closes
//!   `1 / wage_stickiness_days` of the gap each day, so a bad day does not
//!   crash wages (MACROECONOMICS.md §4).
//! * **Wage bill:** `min(w × E, cash − input reserve)`, where the reserve is one
//!   day of missing inputs at today's prices (liquidity rule). It is paid into the labour pool
//!   `(province, worker profession)` and split across its POPs by size.
//! * **Dividends:** cash above `reserve_days × wage bill` is paid out at
//!   `dividend_payout_rate` per day to owner POPs in the same market, split by
//!   size. With no owners present the profit is retained.
//!
//! * **Income tax (D15):** a flat share of every wage and dividend payment is
//!   withheld at source and credited to the treasury of the producer's nation;
//!   the pool receives the net. Stateless markets are untaxed.
//!
//! Every transfer is a debit from one column and a credit to another of
//! exactly the same amount, so total money is unchanged.

use crate::alloc::allocate_raw;
use crate::fixed::Fixed;
use crate::groups::Groups;
use crate::layout::{PopLayout, owner_key, pool_key};
use crate::systems::production::input_requirements;
use crate::world::World;

/// Totals paid out by [`pay_wages_and_dividends`] (diagnostics only).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Payouts {
    /// Gross wages paid by producers (before income tax).
    pub wages: Fixed,
    /// Gross dividends paid by producers (before income tax).
    pub dividends: Fixed,
    /// Income tax withheld from wages and dividends and paid to treasuries (D15).
    pub taxes: Fixed,
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

    // Daily subsistence cost of one person, per (market, profession): the wage floor's base.
    let (markets, profs) = (world.geography.market_count(), defs.professions.len());
    let subsistence_cost: Vec<Fixed> =
        (0..markets * profs).map(|k| defs.professions[k % profs].subsistence_cost(world.prices(k / profs))).collect();

    let mut tax_income = vec![Fixed::ZERO; world.nations.len()];
    let (mut gross_wages, mut gross_dividends) = (Fixed::ZERO, Fixed::ZERO);
    let mut wage_income = vec![Fixed::ZERO; pools.key_count()];
    let mut dividend_income = vec![Fixed::ZERO; owners.key_count()];

    for i in 0..world.producers.len() {
        let def = &defs.producer_types[world.producers.kind[i] as usize];
        let market = world.market_of_province(world.producers.province[i]);
        let wage_pool = pool_key(world, world.producers.province[i], def.worker);
        let owner_pool = owner_key(world, market, def.owner);
        // Working capital (restart reserve): the cost of the inputs still missing
        // for one day of output, kept *even while shut down*. A shut-down producer
        // that paid out its last cash could never restart once prices recovered,
        // which is the trap the liquidity rule exists to prevent (D6).
        let input_reserve: Fixed =
            input_requirements(world, i).iter().map(|&(g, need)| need.mul_ceil(world.price(market, g))).sum();
        let has_owners = owners.members(owner_pool).iter().any(|&r| world.pops.size[r as usize] > 0);

        // Income tax of the market's nation (D15); stateless markets are untaxed.
        let nation = world.geography.nation_of_market(market);
        let tax_rate = nation.map_or(Fixed::ZERO, |n| world.nations.income_tax_rate[n]);
        let p = &mut world.producers;
        let employed = p.employed[i] as i64;
        let avg = p.value_added_avg[i];
        p.value_added_avg[i] =
            avg + (revenue[i] - input_cost[i] - avg).div_int(rules.revenue_smoothing_days.max(1) as i64);
        if employed > 0 {
            let floor = rules.subsistence_wage_multiple.mul(subsistence_cost[market * profs + def.worker]);
            let target = def.labor_share.mul(p.value_added_avg[i].max(Fixed::ZERO)).div_int(employed).max(floor);
            let wage = p.wage[i];
            p.wage[i] = (wage + (target - wage).div_int(rules.wage_stickiness_days.max(1) as i64)).max(Fixed::ZERO);
        }

        let bill = p.wage[i].mul_int(employed);
        // Liquidity rule (D6): wages come only from cash above the working-capital
        // reserve, so a struggling producer can still buy inputs, produce and sell.
        // A firm paying out its last cash in wages could never buy inputs again:
        // no output, no revenue, a permanent trap.
        let paid = bill.min((p.cash[i] - input_reserve).max(Fixed::ZERO));
        p.cash[i] -= paid;
        // Withheld at source: the pool receives the net, the treasury the tax.
        let wage_tax = paid.mul(tax_rate);
        wage_income[wage_pool] += paid - wage_tax;
        gross_wages += paid;
        if let Some(n) = nation {
            tax_income[n] += wage_tax;
        }

        if has_owners {
            let reserve = bill.mul_int(rules.reserve_days as i64);
            let surplus = p.cash[i] - reserve;
            if surplus.is_positive() {
                let dividend = surplus.mul(rules.dividend_payout_rate);
                p.cash[i] -= dividend;
                let dividend_tax = dividend.mul(tax_rate);
                dividend_income[owner_pool] += dividend - dividend_tax;
                gross_dividends += dividend;
                if let Some(n) = nation {
                    tax_income[n] += dividend_tax;
                }
            }
        }
    }

    distribute(world, pools, &wage_income);
    distribute(world, owners, &dividend_income);
    for (treasury, &tax) in world.nations.treasury.iter_mut().zip(&tax_income) {
        *treasury += tax;
    }
    Payouts { wages: gross_wages, dividends: gross_dividends, taxes: tax_income.iter().copied().sum() }
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
