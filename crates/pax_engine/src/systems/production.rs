//! Daily production with fixed-coefficient (Leontief) recipes.
//!
//! For a producer with `E` workers, productivity `π` and input coefficients
//! `aⱼ`, output is limited by labour, by the scarcest input, and by an
//! inventory target of `T` days of output (`target_stock_days`):
//!
//! `output = min(E π, minⱼ stockⱼ / aⱼ, max(0, T E π − unsold))`
//!
//! The inventory target is the quantity-adjustment half of firm behaviour
//! (prices are the other half, see `market.rs`): a producer whose goods are
//! not selling cuts output instead of piling up stock, which would otherwise
//! push its price, and with it wages, into a downward spiral.
//!
//! Inputs consumed are `aⱼ × output`, rounded *up* (but never beyond the
//! stock) so rounding can never manufacture goods out of nothing. Output is
//! added to the producer's stock and offered on the market the same day.

use crate::defs::GoodId;
use crate::fixed::Fixed;
use crate::world::World;

pub fn produce(world: &mut World) {
    let goods = world.defs.good_count();
    let defs = world.defs.clone();
    let target_days = defs.rules.firms.target_stock_days as i64;
    let p = &mut world.producers;
    for i in 0..p.len() {
        let def = &defs.producer_types[p.kind[i] as usize];
        let stock = &mut p.input_stock[i * goods..(i + 1) * goods];
        let capacity = def.output_per_worker.mul_int(p.employed[i] as i64);
        let mut output = capacity.min((capacity.mul_int(target_days) - p.output_stock[i]).max(Fixed::ZERO));
        for &(good, coeff) in &def.inputs {
            output = output.min(stock[good].div(coeff));
        }
        if output <= Fixed::ZERO {
            continue;
        }
        for &(good, coeff) in &def.inputs {
            let used = coeff.mul_ceil(output).min(stock[good]);
            stock[good] -= used;
        }
        p.output_stock[i] += output;
    }
}

/// Inputs producer `i` plans to buy today: `(good, quantity)` for one day of
/// output at today's workforce, minus the input stock it already holds.
///
/// Empty when the producer has no inputs or no workers, or when the **shutdown
/// rule** applies: the output price does not exceed the input cost per unit, so
/// buying inputs would only turn money into loss-making output (D6). This is the
/// single definition used by input orders (market) and by the working-capital
/// reserve (firms), so the two can never disagree.
pub fn planned_inputs(world: &World, i: usize) -> Vec<(GoodId, Fixed)> {
    let p = &world.producers;
    let def = &world.defs.producer_types[p.kind[i] as usize];
    if def.inputs.is_empty() || p.employed[i] == 0 {
        return Vec::new();
    }
    let market = world.market_of_province(p.province[i]);
    let unit_input_cost: Fixed = def.inputs.iter().map(|&(g, a)| a.mul_ceil(world.price(market, g))).sum();
    if world.price(market, def.output) <= unit_input_cost {
        return Vec::new();
    }
    let goods = world.defs.good_count();
    let target = def.output_per_worker.mul_int(p.employed[i] as i64);
    def.inputs.iter().map(|&(g, a)| (g, (a.mul_ceil(target) - p.input_stock[i * goods + g]).max(Fixed::ZERO))).collect()
}
