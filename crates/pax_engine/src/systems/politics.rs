//! Monthly militancy update (DECISIONS.md D19, POLITICS_SYSTEM.md).
//!
//! For each POP with people, with life-needs satisfaction `s` and its nation's
//! income tax rate `t` (0 in stateless markets):
//!
//! `m ← clamp(m + rise × (1 − s) + tax_weight × t − decay × m, 0, 1)`
//!
//! So militancy climbs while a POP goes hungry or is taxed, and fades
//! geometrically once conditions improve. All terms are `Fixed` (D3). Militancy
//! has no effects yet; rebellions come in a later milestone.

use crate::fixed::Fixed;
use crate::world::World;

pub fn update_militancy(world: &mut World) {
    let rules = world.defs.rules.politics.clone();
    for i in 0..world.pops.len() {
        if world.pops.size[i] == 0 {
            continue;
        }
        let market = world.market_of_province(world.pops.province[i]);
        let tax = world.geography.nation_of_market(market).map_or(Fixed::ZERO, |n| world.nations.income_tax_rate[n]);
        let p = &mut world.pops;
        let m = p.militancy[i];
        let pressure = rules.militancy_rise.mul(Fixed::ONE - p.life_needs[i]) + rules.militancy_tax_weight.mul(tax);
        p.militancy[i] = (m + pressure - rules.militancy_decay.mul(m)).clamp(Fixed::ZERO, Fixed::ONE);
    }
}
