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
use crate::world::World;

/// Dense key of the labour pool `(province, profession)`.
pub fn pool_key(world: &World, province: u32, profession: usize) -> usize {
    province as usize * world.defs.professions.len() + profession
}

/// Inverse of [`pool_key`]: the `(province, profession)` of a labour pool key.
/// Keep both functions together; they define the pool layout.
pub fn pool_of_key(world: &World, key: usize) -> (u32, u16) {
    let professions = world.defs.professions.len();
    ((key / professions) as u32, (key % professions) as u16)
}

/// Number of labour pools.
pub fn pool_count(world: &World) -> usize {
    world.geography.province_count() * world.defs.professions.len()
}

/// Employment in one labour pool `(province, profession)` on one day.
///
/// Diagnostics only: nothing here is simulation state. `jobs` is the pool's
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
