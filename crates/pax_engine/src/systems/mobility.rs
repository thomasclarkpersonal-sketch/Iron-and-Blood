//! Monthly labour mobility within a province (DECISIONS.md D18).
//!
//! A deterministic flow, not dice rolls (D7). For every province:
//!
//! 1. **Sources:** worker professions (those some producer type employs) with
//!    unemployed people. **Destinations:** worker professions with vacancies
//!    (jobs − workforce > 0), ordered by vacancy (descending), then profession.
//! 2. Each source sends `⌊unemployed × mobility_rate⌋` people, filling the
//!    destinations in that order, never beyond a destination's vacancies.
//! 3. Movers leave their POP rows pro rata to row size (largest remainder) and
//!    take their share of the row's cash (`cash × movers / size`, split exactly).
//!    They join the first POP row of the destination profession in the province,
//!    or a new row if none exists. The destination's `life_needs` becomes the
//!    size-weighted mean.
//!
//! People and money are moved, never created: population and total cash are
//! unchanged. Employment from the day's labour report is used, so mobility
//! responds to the same numbers players see.

use crate::alloc::allocate_raw;
use crate::fixed::Fixed;
use crate::systems::labor::LabourReport;
use crate::world::World;

/// Moves workers between professions within provinces. Returns the number of
/// people moved.
pub fn reassign_workers(world: &mut World, labour: &[LabourReport]) -> u64 {
    let rate = world.defs.rules.demographics.mobility_rate;
    if !rate.is_positive() {
        return 0;
    }
    let profs = world.defs.professions.len();
    let is_worker: Vec<bool> = (0..profs).map(|c| world.defs.producer_types.iter().any(|t| t.worker == c)).collect();
    let mut moved_total = 0u64;
    // The report is ordered by (province, profession); walk it province by province.
    let mut start = 0;
    while start < labour.len() {
        let province = labour[start].province;
        let end = start + labour[start..].iter().take_while(|p| p.province == province).count();
        let pools: Vec<&LabourReport> =
            labour[start..end].iter().filter(|p| is_worker[p.profession as usize]).collect();
        start = end;

        let mut vacancies: Vec<(u16, u64)> =
            pools.iter().filter(|p| p.jobs > p.workforce).map(|p| (p.profession, p.jobs - p.workforce)).collect();
        vacancies.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        if vacancies.is_empty() {
            continue;
        }
        for source in pools.iter().filter(|p| p.unemployed() > 0) {
            let mut movers = Fixed::from_int(source.unemployed() as i64).mul(rate).floor_int() as u64;
            for (dest, open) in vacancies.iter_mut() {
                if movers == 0 {
                    break;
                }
                if *dest == source.profession || *open == 0 {
                    continue;
                }
                let n = movers.min(*open);
                move_people(world, province, source.profession as usize, *dest as usize, n);
                *open -= n;
                movers -= n;
                moved_total += n;
            }
        }
    }
    moved_total
}

/// Moves `n` people of `from` to `to` within `province`, with their cash.
fn move_people(world: &mut World, province: u32, from: usize, to: usize, n: u64) {
    let rows: Vec<usize> = (0..world.pops.len())
        .filter(|&i| world.pops.province[i] == province && world.pops.profession[i] as usize == from)
        .collect();
    let sizes: Vec<i64> = rows.iter().map(|&r| world.pops.size[r] as i64).collect();
    let Some(take) = allocate_raw(n as i64, &sizes) else { return };

    let dest = (0..world.pops.len())
        .find(|&i| world.pops.province[i] == province && world.pops.profession[i] as usize == to)
        .unwrap_or_else(|| world.push_pop(province, to, 0, Fixed::ZERO));

    for (&r, m) in rows.iter().zip(take) {
        if m == 0 {
            continue;
        }
        let size = world.pops.size[r] as i64;
        let split = allocate_raw(world.pops.cash[r].raw(), &[size - m, m]).expect("size > 0");
        let cash = Fixed::from_raw(split[1]);
        let p = &mut world.pops;
        p.size[r] -= m as u32;
        p.cash[r] -= cash;
        let (dest_size, mover_life) = (p.size[dest] as i64, p.life_needs[r]);
        p.life_needs[dest] = if dest_size + m > 0 {
            Fixed::from_raw(
                ((p.life_needs[dest].raw() as i128 * dest_size as i128 + mover_life.raw() as i128 * m as i128)
                    / (dest_size + m) as i128) as i64,
            )
        } else {
            mover_life
        };
        p.size[dest] += m as u32;
        p.cash[dest] += cash;
    }
}
