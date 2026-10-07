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

/// Number of labour pools.
pub fn pool_count(world: &World) -> usize {
    world.geography.province_count() * world.defs.professions.len()
}

/// POP rows grouped by labour pool.
pub fn pop_pools(world: &World) -> Groups {
    let keys: Vec<usize> = (0..world.pops.len())
        .map(|i| pool_key(world, world.pops.province[i], world.pops.profession[i] as usize))
        .collect();
    Groups::build(pool_count(world), &keys)
}

/// Sets `producers.employed` for every producer.
pub fn assign_employment(world: &mut World, pools: &Groups) {
    let producer_keys: Vec<usize> = (0..world.producers.len())
        .map(|i| {
            let def = &world.defs.producer_types[world.producers.kind[i] as usize];
            pool_key(world, world.producers.province[i], def.worker)
        })
        .collect();
    let by_pool = Groups::build(pool_count(world), &producer_keys);

    for pool in 0..by_pool.key_count() {
        let employers = by_pool.members(pool);
        if employers.is_empty() {
            continue;
        }
        let supply: u64 = pools.members(pool).iter().map(|&r| world.pops.size[r as usize] as u64).sum();
        let caps: Vec<i64> = employers.iter().map(|&r| world.producers.capacity[r as usize] as i64).collect();
        let demand: i64 = caps.iter().sum();
        let hired: Vec<i64> = if demand as u64 <= supply {
            caps
        } else {
            allocate_raw(supply as i64, &caps).expect("demand > supply >= 0 implies positive weights")
        };
        for (&r, h) in employers.iter().zip(hired) {
            world.producers.employed[r as usize] = h as u32;
        }
    }
}
