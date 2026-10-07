//! Monthly population change (DECISIONS.md D7).
//!
//! Monthly rate for a POP with life-needs satisfaction `s ∈ [0, 1]`:
//!
//! * `s = 1`: `+growth_rate`
//! * `s < 1`: `−starvation_rate × (1 − s)`
//!
//! `ΔN = ⌊N × rate⌋`. A POP's cash is total holdings, so it stays with the
//! survivors. If a POP dies out entirely, its cash passes to the largest
//! living POP in the same province (lowest row on ties) — money is never
//! destroyed. If nobody lives in the province the empty row keeps the cash
//! until someone migrates in (migration arrives in M2).

use crate::fixed::Fixed;
use crate::world::World;

/// True on the last day of each month.
pub fn is_month_end(world: &World) -> bool {
    let days = world.defs.rules.days_per_month.max(1) as u64;
    (world.day + 1).is_multiple_of(days)
}

pub fn update_population(world: &mut World) {
    let rules = world.defs.rules.demographics.clone();
    let pops = &mut world.pops;
    let mut died_out = Vec::new();
    for i in 0..pops.len() {
        let size = pops.size[i];
        if size == 0 {
            continue;
        }
        let s = pops.life_needs[i];
        let rate = if s >= Fixed::ONE { rules.growth_rate } else { -rules.starvation_rate.mul(Fixed::ONE - s) };
        let delta = Fixed::from_int(size as i64).mul(rate).floor_int();
        let new_size = (size as i64 + delta).clamp(0, u32::MAX as i64) as u32;
        pops.size[i] = new_size;
        if new_size == 0 {
            died_out.push(i);
        }
    }
    for i in died_out {
        let province = pops.province[i];
        let heir = (0..pops.len())
            .filter(|&j| pops.province[j] == province && pops.size[j] > 0)
            .max_by(|&a, &b| pops.size[a].cmp(&pops.size[b]).then(b.cmp(&a)));
        if let Some(heir) = heir {
            let estate = std::mem::take(&mut pops.cash[i]);
            pops.cash[heir] += estate;
        }
    }
}
