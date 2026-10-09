//! Monthly population change (DECISIONS.md D7).
//!
//! Monthly rate for a POP with life-needs satisfaction `s ∈ [0, 1]`:
//!
//! * `s = 1`: `+growth_rate`
//! * `s < 1`: `−starvation_rate × (1 − s)`
//!
//! `ΔN = ⌊N × rate⌋`.
//!
//! **Births band-aid (D26):** with `births_need_employment`, a worker POP's growth
//! is scaled by its pool's employed share, `min(1, jobs ÷ workforce)` for its
//! `(province, profession)`: people without an income don't raise children. Owner
//! professions, and starvation, are unchanged. It stands in for a dependents model
//! (POP_SYSTEM.md, "Future: dependents"), and goes when that arrives.
//!
//! A POP's cash is total holdings, so it stays with the
//! survivors. Each month end, the cash of every empty row (a POP that died
//! out, this month or earlier) passes to the largest living POP in the same
//! province (lowest row on ties). Money is never destroyed. If nobody lives in
//! the province, the empty row keeps the cash until someone does.

use crate::fixed::Fixed;
use crate::systems::labor::Pools;
use crate::world::World;

/// True on the last day of each month, when the month-end systems and POP
/// compaction run (D4, D7).
pub fn is_month_end(world: &World) -> bool {
    days_until_month_end(world) == 0
}

/// Days from today to the next month end: 0 on a month-end day. The one place the
/// month schedule lives; [`is_month_end`] and the benchmarks (`pax_data::bench`)
/// both read it.
pub fn days_until_month_end(world: &World) -> u64 {
    let days = world.defs.rules.days_per_month.max(1) as u64;
    days - 1 - world.day % days
}

/// With the births band-aid on (D26), each POP row's birth scale: its pool's
/// employed share `min(1, jobs ÷ workforce)` for a worker profession, 1 for an
/// owner profession. `None` when the band-aid is off (D7's rule as written). Counted
/// from the tables as they stand at month end, after mobility.
fn births_scale(world: &World) -> Option<Vec<Fixed>> {
    if !world.defs.rules.demographics.births_need_employment {
        return None;
    }
    let is_worker = world.defs.worker_professions();
    let pools = Pools::count(world);
    let pops = &world.pops;
    let share = |i: usize| {
        let profession = pops.profession[i] as usize;
        let (w, j) = pools.of(world, pops.province[i], profession);
        if !is_worker.contains(profession) || w <= j {
            Fixed::ONE
        } else {
            // j < w, so the share is below 1; `div` rounds down (fewer births).
            Fixed::from_int(j as i64).div(Fixed::from_int(w as i64))
        }
    };
    Some((0..pops.len()).map(share).collect())
}

pub fn update_population(world: &mut World) {
    let rules = world.defs.rules.demographics.clone();
    let province_count = world.geography.province_count();
    let employed_share = births_scale(world);
    let pops = &mut world.pops;
    for i in 0..pops.len() {
        let size = pops.size[i];
        if size == 0 {
            continue;
        }
        let s = pops.life_needs[i];
        let rate = if s >= Fixed::ONE {
            employed_share.as_ref().map_or(rules.growth_rate, |share| rules.growth_rate.mul(share[i]))
        } else {
            -rules.starvation_rate.mul(Fixed::ONE - s)
        };
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
