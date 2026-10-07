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
