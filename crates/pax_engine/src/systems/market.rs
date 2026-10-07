//! Daily market clearing: order formation → price discovery → settlement.
//!
//! This implements DECISIONS.md D1 (bounded tâtonnement + pro-rata rationing)
//! and D2 (Linear Expenditure System demand). The phases follow the Map-Reduce
//! rule in `AGENTS.md`:
//!
//! 1. **Map (parallel over POPs):** each POP is classified as *comfortable* or
//!    *deprived* at the opening prices and its size and budget are summed into
//!    per-`(market, profession)` aggregates. LES demand is linear in `N` and
//!    `Y` within a regime, so these few numbers reproduce the demand of
//!    millions of POPs exactly at any price vector — price discovery never
//!    touches the POP table.
//! 2. **Reduce / discover (parallel over markets):** iterate
//!    `pᵢ ← pᵢ (1 + λₖ zᵢ)` with `zᵢ = (Dᵢ − Sᵢ) / (Dᵢ + Sᵢ) ∈ [−1, 1]` until
//!    every `|zᵢ|` is within tolerance or the iteration budget is spent, then
//!    limit the move from yesterday's price to `±max_daily_change`.
//! 3. **Settle (parallel over POPs, then sequential over sellers):** every
//!    buyer recomputes its exact demand at the final prices; where demand
//!    exceeds supply each buyer receives the same fraction `S / D` (no queue,
//!    no priority by nation rank). Buyers pay `q × p`; the market's total
//!    receipts are split among sellers pro rata to the quantity they offered
//!    at the final price, with the largest-remainder method. Money paid equals
//!    money received exactly, and goods delivered equal goods sold exactly.
//!
//! **Supply** is price-responsive: a seller with stock `Q` and reservation
//! price `r` offers `S(p) = Q × min(1, p / r)`. The reservation price is
//! cost-plus (mark-up pricing, as in Delli Gatti et al.):
//! `r = Σⱼ aⱼ pⱼ + (w / π) / labor_share`, i.e. input cost plus the labour
//! cost per unit grossed up so that, at `p = r`, wages are exactly
//! `labor_share` of value added. Unsold stock is kept, not destroyed.
//!
//! The regime split in step 1 is the only approximation: a POP that crosses
//! the subsistence line during discovery is aggregated in its opening regime.
//! Settlement always uses each POP's exact regime at the final prices.

use rayon::prelude::*;

use crate::alloc::allocate;
use crate::defs::{Defs, MarketRules, ProfessionDef};
use crate::fixed::Fixed;
use crate::groups::Groups;
use crate::layout::PopLayout;
use crate::world::World;

/// Market statistics for one good in one market on one day.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GoodReport {
    pub price: Fixed,
    pub demand: Fixed,
    pub supply: Fixed,
    pub traded: Fixed,
}

/// Result of [`clear_markets`].
#[derive(Clone, Debug, Default)]
pub struct MarketOutcome {
    /// Sales revenue per producer row.
    pub revenue: Vec<Fixed>,
    /// Spending on input goods per producer row.
    pub input_cost: Vec<Fixed>,
    /// Row-major `[market * goods + good]`.
    pub goods: Vec<GoodReport>,
    /// Tâtonnement iterations used, per market.
    pub iterations: Vec<u32>,
    /// Total paid by households (POPs) for consumption: final demand.
    pub household_spending: Fixed,
    /// Total paid by producers for input goods: intermediate consumption.
    pub input_spending: Fixed,
}

/// A producer's input purchase order: `D(p) = min(need, budget / p)`.
#[derive(Clone, Copy, Debug)]
struct InputOrder {
    producer: usize,
    market: usize,
    good: usize,
    need: Fixed,
    budget: Fixed,
}

impl InputOrder {
    fn demand(&self, price: Fixed) -> Fixed {
        self.need.min(self.budget.div(price))
    }
}

/// A producer's sell offer: `S(p) = stock × min(1, p / reservation)`.
#[derive(Clone, Copy, Debug)]
struct SellOffer {
    producer: usize,
    market: usize,
    good: usize,
    stock: Fixed,
    reservation: Fixed,
}

impl SellOffer {
    fn supply(&self, price: Fixed) -> Fixed {
        if !self.reservation.is_positive() || price >= self.reservation {
            self.stock
        } else {
            self.stock.mul_div(price, self.reservation)
        }
    }
}

/// Sufficient statistics of all POPs of one profession in one market.
#[derive(Clone, Copy, Debug, Default)]
struct ConsumerAggregate {
    comfortable_size: i64,
    comfortable_budget: Fixed,
    deprived_budget: Fixed,
}

/// Per-capita subsistence cost `C = Σ pₖ γₖ`, rounded up so that spending the
/// computed demand can never exceed the budget.
fn subsistence_cost(prof: &ProfessionDef, prices: &[Fixed]) -> Fixed {
    prof.subsistence.iter().zip(prices).filter(|(g, _)| g.is_positive()).map(|(g, p)| g.mul_ceil(*p)).sum()
}

/// Daily consumption budget of a POP.
fn budget(prof: &ProfessionDef, cash: Fixed) -> Fixed {
    cash.mul(prof.spend_rate)
}

/// LES demand of `size` people with total `budget` in a known regime, written
/// into `out`. Linear in `(size, budget)`, which is what makes aggregation exact.
fn les_demand(
    prof: &ProfessionDef,
    size: i64,
    budget: Fixed,
    comfortable: bool,
    cost: Fixed,
    prices: &[Fixed],
    out: &mut [Fixed],
) {
    if comfortable {
        // Clamped at zero: an aggregate classified at opening prices may sit
        // below the subsistence line at trial prices.
        let discretionary = (budget - cost.mul_int(size)).max(Fixed::ZERO);
        for (g, x) in out.iter_mut().enumerate() {
            let beta = prof.preference[g];
            let extra = if beta.is_positive() { beta.mul_div(discretionary, prices[g]) } else { Fixed::ZERO };
            *x = prof.subsistence[g].mul_int(size) + extra;
        }
    } else {
        for (x, gamma) in out.iter_mut().zip(&prof.subsistence) {
            *x = if cost.is_positive() { gamma.mul_div(budget, cost) } else { Fixed::ZERO };
        }
    }
}

/// Exact demand of a single POP at `prices`.
fn pop_demand(prof: &ProfessionDef, size: u32, cash: Fixed, cost: Fixed, prices: &[Fixed], out: &mut [Fixed]) {
    if size == 0 {
        out.fill(Fixed::ZERO);
        return;
    }
    let size = size as i64;
    let y = budget(prof, cash);
    let comfortable = cost.is_zero() || y >= cost.mul_int(size);
    les_demand(prof, size, y, comfortable, cost, prices, out);
}

/// Runs the whole market phase for every market.
pub fn clear_markets(world: &mut World, layout: &PopLayout) -> MarketOutcome {
    let defs = world.defs.clone();
    let goods = defs.good_count();
    let profs = defs.professions.len();
    let markets = world.geography.market_count();
    let pop_market: &[u32] = &layout.market;

    let orders = input_orders(world);
    let offers = sell_offers(world);
    let aggregates = aggregate_consumers(world, pop_market);

    // --- Price discovery, independent per market. ---
    let opening = world.markets.price.clone();
    let discovered: Vec<(Vec<Fixed>, u32)> = (0..markets)
        .into_par_iter()
        .map(|m| {
            let market_orders: Vec<InputOrder> = orders.iter().copied().filter(|o| o.market == m).collect();
            let market_offers: Vec<SellOffer> = offers.iter().copied().filter(|o| o.market == m).collect();
            discover_prices(
                &defs,
                &opening[m * goods..(m + 1) * goods],
                &aggregates[m * profs..(m + 1) * profs],
                &market_orders,
                &market_offers,
            )
        })
        .collect();
    let mut iterations = Vec::with_capacity(markets);
    for (m, (prices, iters)) in discovered.into_iter().enumerate() {
        world.markets.price[m * goods..(m + 1) * goods].copy_from_slice(&prices);
        iterations.push(iters);
    }

    settle(world, pop_market, &orders, &offers, iterations)
}

/// Producers' input orders, priced at the opening prices.
///
/// Each producer aims to hold one day's worth of inputs for today's labour.
/// Its input budget (`cash × input_spend_rate`) is split across inputs in
/// proportion to the cost of what it needs.
///
/// **Shutdown rule:** a producer whose output price does not cover the cost of
/// the inputs per unit (`p_out ≤ Σⱼ aⱼ pⱼ`) places no orders, so it stops
/// turning money into loss-making output; it resumes when prices recover.
fn input_orders(world: &World) -> Vec<InputOrder> {
    let defs = &world.defs;
    let goods = defs.good_count();
    let p = &world.producers;
    let mut orders = Vec::new();
    for i in 0..p.len() {
        let def = &defs.producer_types[p.kind[i] as usize];
        if def.inputs.is_empty() || p.employed[i] == 0 {
            continue;
        }
        let market = world.market_of_province(p.province[i]);
        let unit_input_cost: Fixed = def.inputs.iter().map(|&(g, a)| a.mul_ceil(world.price(market, g))).sum();
        if world.price(market, def.output) <= unit_input_cost {
            continue;
        }
        let target = def.output_per_worker.mul_int(p.employed[i] as i64);
        let needs: Vec<Fixed> = def
            .inputs
            .iter()
            .map(|&(g, a)| (a.mul_ceil(target) - p.input_stock[i * goods + g]).max(Fixed::ZERO))
            .collect();
        let costs: Vec<Fixed> =
            def.inputs.iter().zip(&needs).map(|(&(g, _), n)| n.mul(world.price(market, g))).collect();
        let Some(budgets) = allocate(p.cash[i].mul(def.input_spend_rate), &costs) else { continue };
        for ((&(good, _), need), budget) in def.inputs.iter().zip(needs).zip(budgets) {
            if need.is_positive() && budget.is_positive() {
                orders.push(InputOrder { producer: i, market, good, need, budget });
            }
        }
    }
    orders
}

/// Sell offers of every producer holding output stock, with cost-plus
/// reservation prices at the opening prices (see the module docs).
fn sell_offers(world: &World) -> Vec<SellOffer> {
    let p = &world.producers;
    let mut offers = Vec::new();
    for i in 0..p.len() {
        if !p.output_stock[i].is_positive() {
            continue;
        }
        let def = &world.defs.producer_types[p.kind[i] as usize];
        let market = world.market_of_province(p.province[i]);
        let input_cost: Fixed = def.inputs.iter().map(|&(g, a)| a.mul_ceil(world.price(market, g))).sum();
        let unit_labor = p.wage[i].div(def.output_per_worker);
        let labor = if def.labor_share.is_positive() { unit_labor.div(def.labor_share) } else { unit_labor };
        offers.push(SellOffer {
            producer: i,
            market,
            good: def.output,
            stock: p.output_stock[i],
            reservation: input_cost + labor,
        });
    }
    offers
}

/// Map phase: `[market * professions + profession]` consumer aggregates.
fn aggregate_consumers(world: &World, pop_market: &[u32]) -> Vec<ConsumerAggregate> {
    let defs = &world.defs;
    let goods = defs.good_count();
    let profs = defs.professions.len();
    let markets = world.geography.market_count();
    // Opening subsistence cost per (market, profession).
    let costs: Vec<Fixed> = (0..markets * profs)
        .map(|k| {
            let (m, c) = (k / profs, k % profs);
            subsistence_cost(&defs.professions[c], &world.markets.price[m * goods..(m + 1) * goods])
        })
        .collect();
    let pops = &world.pops;
    (0..pops.len())
        .into_par_iter()
        .with_min_len(4096)
        .fold(
            || vec![ConsumerAggregate::default(); markets * profs],
            |mut acc, i| {
                let size = pops.size[i];
                if size == 0 {
                    return acc;
                }
                let c = pops.profession[i] as usize;
                let key = pop_market[i] as usize * profs + c;
                let y = budget(&defs.professions[c], pops.cash[i]);
                let cost = costs[key];
                let a = &mut acc[key];
                if cost.is_zero() || y >= cost.mul_int(size as i64) {
                    a.comfortable_size += size as i64;
                    a.comfortable_budget += y;
                } else {
                    a.deprived_budget += y;
                }
                acc
            },
        )
        .reduce(
            || vec![ConsumerAggregate::default(); markets * profs],
            |mut a, b| {
                for (x, y) in a.iter_mut().zip(b) {
                    x.comfortable_size += y.comfortable_size;
                    x.comfortable_budget += y.comfortable_budget;
                    x.deprived_budget += y.deprived_budget;
                }
                a
            },
        )
}

/// Aggregate demand of one market at `prices`.
fn market_demand(
    defs: &Defs,
    prices: &[Fixed],
    consumers: &[ConsumerAggregate],
    orders: &[InputOrder],
    scratch: &mut [Fixed],
) -> Vec<Fixed> {
    let mut demand = vec![Fixed::ZERO; prices.len()];
    for (prof, agg) in defs.professions.iter().zip(consumers) {
        let cost = subsistence_cost(prof, prices);
        for (comfortable, size, budget) in
            [(true, agg.comfortable_size, agg.comfortable_budget), (false, 0, agg.deprived_budget)]
        {
            if (comfortable && size == 0) || (!comfortable && budget.is_zero()) {
                continue;
            }
            les_demand(prof, size, budget, comfortable, cost, prices, scratch);
            for (d, x) in demand.iter_mut().zip(scratch.iter()) {
                *d += *x;
            }
        }
    }
    for o in orders {
        demand[o.good] += o.demand(prices[o.good]);
    }
    demand
}

/// Normalised excess demand `(D − S) / (D + S)`, defined as 0 when both are 0.
fn excess(demand: Fixed, supply: Fixed) -> Fixed {
    let total = demand + supply;
    if total.is_zero() { Fixed::ZERO } else { (demand - supply).div(total) }
}

/// Total quantity offered per good at `prices`.
fn market_supply(prices: &[Fixed], offers: &[SellOffer]) -> Vec<Fixed> {
    let mut supply = vec![Fixed::ZERO; prices.len()];
    for o in offers {
        supply[o.good] += o.supply(prices[o.good]);
    }
    supply
}

/// Bounded tâtonnement for one market. Returns the executed prices and the
/// number of iterations used.
fn discover_prices(
    defs: &Defs,
    opening: &[Fixed],
    consumers: &[ConsumerAggregate],
    orders: &[InputOrder],
    offers: &[SellOffer],
) -> (Vec<Fixed>, u32) {
    let rules: &MarketRules = &defs.rules.market;
    let mut prices = opening.to_vec();
    let mut scratch = vec![Fixed::ZERO; prices.len()];
    let mut stock = vec![Fixed::ZERO; prices.len()];
    for o in offers {
        stock[o.good] += o.stock;
    }
    let decay = Fixed::from_int(rules.step_decay_iterations.max(1) as i64);
    let mut used = 0;
    for k in 0..rules.max_iterations {
        let demand = market_demand(defs, &prices, consumers, orders, &mut scratch);
        let supply = market_supply(&prices, offers);
        let z: Vec<Fixed> = demand.iter().zip(&supply).map(|(&d, &s)| excess(d, s)).collect();
        if z.iter().all(|z| z.abs() <= rules.tolerance) {
            break;
        }
        used = k + 1;
        // λₖ = λ · decay / (decay + k)
        let step = rules.step.mul_div(decay, decay + Fixed::from_int(k as i64));
        for (p, z) in prices.iter_mut().zip(z) {
            *p = (*p + p.mul(step.mul(z))).clamp(rules.price_floor, rules.price_ceiling);
        }
    }
    // Daily movement limit relative to yesterday, then the technical bounds.
    // With no stock anywhere there is no trade to discover a price from, so
    // the quote is held; scarcity remains visible as unmet demand in the report.
    for ((p, &open), &s) in prices.iter_mut().zip(opening).zip(&stock) {
        if s.is_zero() {
            *p = open;
            continue;
        }
        let band = open.mul(rules.max_daily_change);
        *p = (*p).clamp(open - band, open + band).clamp(rules.price_floor, rules.price_ceiling);
    }
    (prices, used)
}

/// Settlement at the executed prices. See the module docs, step 3.
fn settle(
    world: &mut World,
    pop_market: &[u32],
    orders: &[InputOrder],
    offers: &[SellOffer],
    iterations: Vec<u32>,
) -> MarketOutcome {
    let defs = world.defs.clone();
    let goods = defs.good_count();
    let profs = defs.professions.len();
    let markets = world.geography.market_count();
    let prices = world.markets.price.clone();
    let costs: Vec<Fixed> = (0..markets * profs)
        .map(|k| subsistence_cost(&defs.professions[k % profs], &prices[(k / profs) * goods..(k / profs + 1) * goods]))
        .collect();

    // Pass A: exact total demand at final prices.
    let pops = &world.pops;
    let mut demand = (0..pops.len())
        .into_par_iter()
        .with_min_len(4096)
        .fold(
            || (vec![Fixed::ZERO; markets * goods], vec![Fixed::ZERO; goods]),
            |(mut acc, mut x), i| {
                let (m, c) = (pop_market[i] as usize, pops.profession[i] as usize);
                let row = &prices[m * goods..(m + 1) * goods];
                pop_demand(&defs.professions[c], pops.size[i], pops.cash[i], costs[m * profs + c], row, &mut x);
                for (a, q) in acc[m * goods..(m + 1) * goods].iter_mut().zip(&x) {
                    *a += *q;
                }
                (acc, x)
            },
        )
        .map(|(acc, _)| acc)
        .reduce(|| vec![Fixed::ZERO; markets * goods], add_vectors);
    for o in orders {
        demand[o.market * goods + o.good] += o.demand(prices[o.market * goods + o.good]);
    }

    // Quantity offered at the executed prices.
    let offered: Vec<Fixed> = offers.iter().map(|o| o.supply(prices[o.market * goods + o.good])).collect();
    let mut supply = vec![Fixed::ZERO; markets * goods];
    for (o, &q) in offers.iter().zip(&offered) {
        supply[o.market * goods + o.good] += q;
    }

    // Rationing fraction per (market, good): 1 if supply suffices, else S / D.
    let ration: Vec<Fixed> =
        demand.iter().zip(&supply).map(|(&d, &s)| if d <= s { Fixed::ONE } else { s.div(d) }).collect();

    // Pass B: POPs buy, pay, and record life-needs satisfaction.
    let crate::world::Pops { size, cash, profession, life_needs, .. } = &mut world.pops;
    let (size, profession) = (&*size, &*profession);
    let (bought, paid) = cash
        .par_iter_mut()
        .zip(life_needs.par_iter_mut())
        .enumerate()
        .with_min_len(4096)
        .fold(
            || (vec![Fixed::ZERO; markets * goods], vec![Fixed::ZERO; markets * goods], vec![Fixed::ZERO; goods]),
            |(mut bought, mut paid, mut x), (i, (cash, life))| {
                let (m, c) = (pop_market[i] as usize, profession[i] as usize);
                let prof = &defs.professions[c];
                let row = m * goods..(m + 1) * goods;
                pop_demand(prof, size[i], *cash, costs[m * profs + c], &prices[row.clone()], &mut x);
                let mut spent = Fixed::ZERO;
                let mut satisfaction = Fixed::ONE;
                for (g, &wanted) in x.iter().enumerate() {
                    let k = m * goods + g;
                    let q = if ration[k] == Fixed::ONE { wanted } else { wanted.mul(ration[k]) };
                    // D1: rationing never gives a buyer more than it asked for.
                    assert!(q <= wanted, "market-good {k}: POP {i} received {q} > demanded {wanted}");
                    let cost = q.mul(prices[k]);
                    bought[k] += q;
                    paid[k] += cost;
                    spent += cost;
                    let need = prof.subsistence[g].mul_int(size[i] as i64);
                    if need.is_positive() {
                        satisfaction = satisfaction.min(q.div(need).min(Fixed::ONE));
                    }
                }
                assert!(spent <= *cash, "POP {i} overspent: {spent} > {cash}");
                *cash -= spent;
                *life = satisfaction;
                (bought, paid, x)
            },
        )
        .map(|(b, p, _)| (b, p))
        .reduce(
            || (vec![Fixed::ZERO; markets * goods], vec![Fixed::ZERO; markets * goods]),
            |(b1, p1), (b2, p2)| (add_vectors(b1, b2), add_vectors(p1, p2)),
        );
    let (mut bought, mut paid) = (bought, paid);
    let household_spending: Fixed = paid.iter().copied().sum();

    // Producers buy inputs.
    let producers = &mut world.producers;
    let mut input_cost = vec![Fixed::ZERO; producers.len()];
    for o in orders {
        let k = o.market * goods + o.good;
        let wanted = o.demand(prices[k]);
        let q = wanted.mul(ration[k]);
        assert!(q <= wanted, "market-good {k}: producer {} received {q} > demanded {wanted}", o.producer);
        let cost = q.mul(prices[k]);
        producers.cash[o.producer] -= cost;
        assert!(!producers.cash[o.producer].is_negative(), "producer {} overspent", o.producer);
        producers.input_stock[o.producer * goods + o.good] += q;
        input_cost[o.producer] += cost;
        bought[k] += q;
        paid[k] += cost;
    }

    // Sellers deliver and are paid pro rata to the quantity they offered.
    let seller_keys: Vec<usize> = offers.iter().map(|o| o.market * goods + o.good).collect();
    let sellers = Groups::build(markets * goods, &seller_keys);
    let mut revenue = vec![Fixed::ZERO; producers.len()];
    for k in 0..markets * goods {
        let rows = sellers.members(k);
        assert!(bought[k] <= supply[k], "market {k}: sold more than supplied");
        if bought[k].is_zero() {
            assert!(paid[k].is_zero(), "market {k}: payment without delivery");
            continue;
        }
        let weights: Vec<Fixed> = rows.iter().map(|&r| offered[r as usize]).collect();
        let delivered = allocate(bought[k], &weights).expect("goods sold implies positive offers");
        let receipts = allocate(paid[k], &weights).expect("goods sold implies positive offers");
        for (((&r, q), cash), &offered_qty) in rows.iter().zip(delivered).zip(receipts).zip(&weights) {
            // Largest-remainder allocation of sold ≤ offered never exceeds an offer.
            assert!(q <= offered_qty, "market {k}: seller delivered {q} > offered {offered_qty}");
            let producer = offers[r as usize].producer;
            producers.output_stock[producer] -= q;
            producers.cash[producer] += cash;
            revenue[producer] += cash;
        }
    }

    let reports = (0..markets * goods)
        .map(|k| GoodReport {
            price: prices[k],
            demand: demand[k],
            supply: supply[k],
            traded: std::mem::take(&mut bought[k]),
        })
        .collect();
    let input_spending = input_cost.iter().copied().sum();
    MarketOutcome { revenue, input_cost, household_spending, input_spending, goods: reports, iterations }
}

fn add_vectors(mut a: Vec<Fixed>, b: Vec<Fixed>) -> Vec<Fixed> {
    for (x, y) in a.iter_mut().zip(b) {
        *x += y;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Fixed {
        Fixed::parse_decimal(s).unwrap()
    }

    fn prof() -> ProfessionDef {
        ProfessionDef {
            key: "worker".into(),
            spend_rate: Fixed::ONE,
            subsistence: vec![d("1"), Fixed::ZERO],
            preference: vec![d("0.25"), d("0.75")],
        }
    }

    #[test]
    fn les_comfortable_spends_exactly_budget() {
        // N = 10, Y = 100, p = (2, 5): C = 2, discretionary = 80.
        // x0 = 10 + 0.25*80/2 = 20, x1 = 0.75*80/5 = 12. Spend = 40 + 60 = 100.
        let prices = [d("2"), d("5")];
        let mut x = [Fixed::ZERO; 2];
        let cost = subsistence_cost(&prof(), &prices);
        pop_demand(&prof(), 10, d("100"), cost, &prices, &mut x);
        assert_eq!(x, [d("20"), d("12")]);
    }

    #[test]
    fn les_deprived_buys_scaled_subsistence() {
        // N = 10 needs 10 units at p = 2 (cost 20) but has only 5.
        let prices = [d("2"), d("5")];
        let mut x = [Fixed::ZERO; 2];
        let cost = subsistence_cost(&prof(), &prices);
        pop_demand(&prof(), 10, d("5"), cost, &prices, &mut x);
        assert_eq!(x, [d("2.5"), Fixed::ZERO]);
    }

    #[test]
    fn aggregation_matches_individual_demand_within_regime() {
        let prices = [d("1.3"), d("0.7")];
        let p = prof();
        let cost = subsistence_cost(&p, &prices);
        let mut a = [Fixed::ZERO; 2];
        let mut b = [Fixed::ZERO; 2];
        let mut total = [Fixed::ZERO; 2];
        pop_demand(&p, 10, d("40"), cost, &prices, &mut a);
        pop_demand(&p, 30, d("90"), cost, &prices, &mut b);
        les_demand(&p, 40, d("130"), true, cost, &prices, &mut total);
        for g in 0..2 {
            // Equal up to one rounding unit per aggregated POP.
            assert!((total[g] - (a[g] + b[g])).abs() <= Fixed::from_raw(2), "good {g}");
        }
    }

    #[test]
    fn excess_is_bounded_and_safe() {
        assert_eq!(excess(Fixed::ZERO, Fixed::ZERO), Fixed::ZERO);
        assert_eq!(excess(d("5"), Fixed::ZERO), Fixed::ONE);
        assert_eq!(excess(Fixed::ZERO, d("5")), -Fixed::ONE);
    }
}
