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
use crate::layout::{PopLayout, pool_count, pool_key};
use crate::systems::labor::LabourReport;
use crate::world::World;

/// A planned flow of `n` people from identity `from` to identity `to`, each a
/// `(province, profession)` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Move {
    pub from: (u32, usize),
    pub to: (u32, usize),
    pub n: u64,
}

/// Moves workers between professions within provinces. Returns the number of
/// people actually moved.
pub fn reassign_workers(world: &mut World, layout: &PopLayout, labour: &[LabourReport]) -> u64 {
    let rate = world.defs.rules.demographics.mobility_rate;
    if !rate.is_positive() {
        return 0;
    }
    let is_worker = world.defs.worker_professions();
    let mut moves = Vec::new();
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
                moves.push(Move { from: (province, source.profession as usize), to: (province, *dest as usize), n });
                *open -= n;
                movers -= n;
            }
        }
    }
    execute_moves(world, layout, &moves)
}

/// Executes planned moves, in order, with the D7 split rules. Returns the
/// number of people actually moved (a move whose source rows are all empty
/// moves nobody and is not counted).
///
/// Rows are found through the tick's layout snapshot (`labour` groups), never
/// by scanning the table. Missing destination rows are created *first*, by
/// appending, so the snapshot stays valid for every pre-existing row while the
/// moves run. Cost: O(moves × rows per identity), not O(moves × table).
pub(crate) fn execute_moves(world: &mut World, layout: &PopLayout, moves: &[Move]) -> u64 {
    let mut created: Vec<Option<usize>> = vec![None; pool_count(world)];
    let mut destination = Vec::with_capacity(moves.len());
    for m in moves {
        let key = pool_key(world, m.to.0, m.to.1);
        let row = match layout.labour.members(key).first() {
            Some(&r) => r as usize,
            None => *created[key].get_or_insert_with(|| world.push_pop(m.to.0, m.to.1, 0, Fixed::ZERO)),
        };
        destination.push(row);
    }
    let mut moved = 0;
    for (m, &dest) in moves.iter().zip(&destination) {
        let rows = layout.labour.members(pool_key(world, m.from.0, m.from.1));
        moved += move_people(world, rows, dest, m.n);
    }
    moved
}

/// Moves `n` people from `rows` (pro rata to size) into row `dest`, with their
/// cash. Returns how many moved (0 if the rows are empty).
fn move_people(world: &mut World, rows: &[u32], dest: usize, n: u64) -> u64 {
    let sizes: Vec<i64> = rows.iter().map(|&r| world.pops.size[r as usize] as i64).collect();
    let available: i64 = sizes.iter().sum();
    let n = n.min(available.max(0) as u64);
    let Some(take) = allocate_raw(n as i64, &sizes) else { return 0 };
    for (&r, m) in rows.iter().zip(take) {
        let r = r as usize;
        if m == 0 || r == dest {
            continue;
        }
        let size = world.pops.size[r] as i64;
        let split = allocate_raw(world.pops.cash[r].raw(), &[size - m, m]).expect("size > 0");
        let cash = Fixed::from_raw(split[1]);
        let p = &mut world.pops;
        p.size[r] -= m as u32;
        p.cash[r] -= cash;
        let dest_size = p.size[dest] as i64;
        p.life_needs[dest] = weighted_mean(p.life_needs[dest], dest_size, p.life_needs[r], m);
        p.size[dest] += m as u32;
        p.cash[dest] += cash;
    }
    n
}

/// Size-weighted mean of two intensive values (D7 merge rule), rounded down.
fn weighted_mean(a: Fixed, a_size: i64, b: Fixed, b_size: i64) -> Fixed {
    if a_size + b_size == 0 {
        return b;
    }
    Fixed::from_raw(
        ((a.raw() as i128 * a_size as i128 + b.raw() as i128 * b_size as i128) / (a_size + b_size) as i128) as i64,
    )
}
