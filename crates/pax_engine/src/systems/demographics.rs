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
    let province_count = world.geography.province_count();
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
    if died_out.is_empty() {
        return;
    }

    // Heir of each province: its largest living POP, lowest row on ties. Sizes do
    // not change while estates are settled, so one O(N) pass serves every extinct
    // POP (previously each extinction rescanned the whole table: O(N²)).
    let mut heir: Vec<Option<usize>> = vec![None; province_count];
    for j in 0..pops.len() {
        if pops.size[j] == 0 {
            continue;
        }
        let slot = &mut heir[pops.province[j] as usize];
        // Ascending rows + strict ">" keeps the lowest row among equal sizes.
        if slot.is_none_or(|h| pops.size[j] > pops.size[h]) {
            *slot = Some(j);
        }
    }
    for i in died_out {
        if let Some(h) = heir[pops.province[i] as usize] {
            let estate = std::mem::take(&mut pops.cash[i]);
            pops.cash[h] += estate;
        }
    }
}
