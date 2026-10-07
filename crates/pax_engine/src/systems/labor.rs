//! Daily employment: matches producers' labour demand to the POPs of the
//! required profession living in the producer's province.
//!
//! Labour pool `(province, profession)` supplies `L = Σ size`. Producers
//! drawing on the pool demand their capacities `kᵢ`. If `Σ kᵢ ≤ L` everybody is
//! fully staffed; otherwise the pool is split pro rata to capacity with the
//! largest-remainder method, so `Σ employedᵢ = L` exactly and no producer is
//! favoured by its position in the table. Unemployment is shared evenly across
//! the pool's POPs (wages are later distributed by size).

use crate::alloc::allocate_raw;
use crate::groups::Groups;
use crate::layout::{pool_count, pool_key, pool_of_key};
use crate::world::World;

/// Employment in one labour pool `(province, profession)` on one day.
///
/// Not simulation state: it is derived from state each tick. Besides
/// diagnostics it is the input to month-end labour mobility (D18), so its
/// numbers are exactly what mobility acts on. `jobs` is the pool's
/// total producer capacity; a pool with `jobs == 0` has no employer at all
/// (e.g. owner professions), which reports may want to exclude from
/// unemployment figures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LabourReport {
    pub province: u32,
    pub profession: u16,
    /// People in the pool (`Σ size` of its POPs).
    pub workforce: u64,
    /// Total capacity of the producers hiring from this pool.
    pub jobs: u64,
    /// People employed: `min(workforce, jobs)`.
    pub employed: u64,
}

impl LabourReport {
    pub fn unemployed(&self) -> u64 {
        self.workforce - self.employed
    }
}

/// Sets `producers.employed` for every producer, and reports employment for
/// every labour pool that has people or jobs, in `(province, profession)` order.
pub fn assign_employment(world: &mut World, pools: &Groups) -> Vec<LabourReport> {
    let producer_keys: Vec<usize> = (0..world.producers.len())
        .map(|i| {
            let def = &world.defs.producer_types[world.producers.kind[i] as usize];
            pool_key(world, world.producers.province[i], def.worker)
        })
        .collect();
    let by_pool = Groups::build(pool_count(world), &producer_keys);
    let mut report = Vec::new();

    for pool in 0..by_pool.key_count() {
        let employers = by_pool.members(pool);
        let supply: u64 = pools.members(pool).iter().map(|&r| world.pops.size[r as usize] as u64).sum();
        if employers.is_empty() {
            if supply > 0 {
                let (province, profession) = pool_of_key(world, pool);
                report.push(LabourReport { province, profession, workforce: supply, jobs: 0, employed: 0 });
            }
            continue;
        }
        let caps: Vec<i64> = employers.iter().map(|&r| world.producers.capacity[r as usize] as i64).collect();
        let demand: i64 = caps.iter().sum();
        let hired: Vec<i64> = if demand as u64 <= supply {
            caps
        } else {
            allocate_raw(supply as i64, &caps).expect("demand > supply >= 0 implies positive weights")
        };
        let mut employed = 0u64;
        for (&r, h) in employers.iter().zip(hired) {
            world.producers.employed[r as usize] = h as u32;
            employed += h as u64;
        }
        let (province, profession) = pool_of_key(world, pool);
        report.push(LabourReport { province, profession, workforce: supply, jobs: demand as u64, employed });
    }
    report
}

/// Unemployment among *worker professions* (D18): `(unemployed, workforce)`
/// summed over the labour pools of professions some producer type employs.
/// Owner-only professions are outside the labour force. The single definition,
/// used by reports and tests.
pub fn unemployment(defs: &crate::defs::Defs, labour: &[LabourReport]) -> (u64, u64) {
    let worker = defs.worker_professions();
    labour
        .iter()
        .filter(|p| worker[p.profession as usize])
        .fold((0, 0), |(u, w), p| (u + p.unemployed(), w + p.workforce))
}
