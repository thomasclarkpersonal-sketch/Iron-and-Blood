//! Monthly population change (DECISIONS.md D7).
//!
//! Monthly rate for a POP with life-needs satisfaction `s ∈ [0, 1]`:
//!
//! * `s = 1`: `+growth_rate`
//! * `s < 1`: `−starvation_rate × (1 − s)`
//!
//! `ΔN = ⌊N × rate⌋`. A POP's cash is total holdings, so it stays with the
//! survivors. Each month end, the cash of every empty row (a POP that died
//! out, this month or earlier) passes to the largest living POP in the same
//! province (lowest row on ties). Money is never destroyed. If nobody lives in
//! the province, the empty row keeps the cash until someone does.

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
    }
    // Every empty row still holding cash has an estate to settle: rows that
    // died out this month, and rows that died out earlier with no heir in the
    // province at the time (D7). Those pass on once anyone lives there.
    let estates: Vec<usize> = (0..pops.len()).filter(|&i| pops.size[i] == 0 && pops.cash[i].is_positive()).collect();
    if estates.is_empty() {
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
    for i in estates {
        if let Some(h) = heir[pops.province[i] as usize] {
            let estate = std::mem::take(&mut pops.cash[i]);
            pops.cash[h] += estate;
        }
    }
}
