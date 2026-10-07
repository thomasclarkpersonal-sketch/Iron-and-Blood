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
//!    or a new row if none exists. The destination's `life_needs` and
//!    `militancy` become size-weighted means.
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
    // Planning invariant: a move never exceeds the people it was planned from
    // (unemployed ≤ workforce, surplus ≤ workforce). A violation is a planning bug.
    assert!(n as i64 <= available, "planned move of {n} exceeds the {available} people available");
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
        p.absorb(dest, r, m as u32, cash);
    }
    n
}

/// Month-end migration between provinces of the same market (D20).
///
/// For each market and worker profession, provinces whose workforce exceeds
/// their jobs send `⌊surplus × migration_rate⌋` people to provinces of the same
/// market with vacancies for that profession (largest vacancy first, then lowest
/// province), never beyond a destination's vacancies. Migrants keep their
/// profession and take their cash, through the same `execute_moves` as labour
/// mobility, after which it runs on the workforce as it stands then. `layout`
/// must be current (the tick passes a fresh one). Returns the number of people moved.
pub fn migrate_within_markets(world: &mut World, layout: &PopLayout) -> u64 {
    let rate = world.defs.rules.demographics.migration_rate;
    if !rate.is_positive() {
        return 0;
    }
    let profs = world.defs.professions.len();
    let provinces = world.geography.province_count();
    let mut workforce = vec![0u64; provinces * profs];
    for i in 0..world.pops.len() {
        workforce[world.pops.province[i] as usize * profs + world.pops.profession[i] as usize] +=
            world.pops.size[i] as u64;
    }
    let mut jobs = vec![0u64; provinces * profs];
    for i in 0..world.producers.len() {
        let worker = world.defs.producer_types[world.producers.kind[i] as usize].worker;
        jobs[world.producers.province[i] as usize * profs + worker] += world.producers.capacity[i] as u64;
    }
    let is_worker = world.defs.worker_professions();
    let mut moves = Vec::new();
    for market in 0..world.geography.market_count() {
        let members: Vec<u32> =
            (0..provinces as u32).filter(|&p| world.geography.province_market[p as usize] as usize == market).collect();
        if members.len() < 2 {
            continue;
        }
        for c in (0..profs).filter(|&c| is_worker[c]) {
            let key = |p: u32| p as usize * profs + c;
            let mut vacancies: Vec<(u32, u64)> = members
                .iter()
                .filter(|&&p| jobs[key(p)] > workforce[key(p)])
                .map(|&p| (p, jobs[key(p)] - workforce[key(p)]))
                .collect();
            vacancies.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            for &source in &members {
                let surplus = workforce[key(source)].saturating_sub(jobs[key(source)]);
                let mut movers = Fixed::from_int(surplus as i64).mul(rate).floor_int() as u64;
                for (dest, open) in vacancies.iter_mut() {
                    if movers == 0 {
                        break;
                    }
                    if *dest == source || *open == 0 {
                        continue;
                    }
                    let n = movers.min(*open);
                    moves.push(Move { from: (source, c), to: (*dest, c), n });
                    *open -= n;
                    movers -= n;
                }
            }
        }
    }
    execute_moves(world, layout, &moves)
}
