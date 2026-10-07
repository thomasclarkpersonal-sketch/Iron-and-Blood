//! Groupings derived from the POP table's layout, cached across ticks.
//!
//! Several systems need "the POPs of profession *c* in province *p*" (labour
//! pools), "… in market *m*" (dividend owners), and each POP's market. These
//! depend only on the `province`/`profession` columns and the province→market
//! map. In M1 those never change after loading, so rebuilding them every tick
//! (three O(N) passes, ~8 ms at 1M POPs) was pure overhead (MILESTONE_1 T4).
//!
//! **Invalidation contract:** any code that adds POP rows or changes a POP's
//! `province` or `profession`, or the province→market map, must call
//! [`World::invalidate_pop_layout`]. `World::push_pop` does this itself. Debug
//! builds verify the cache against a fresh build on every use, so a missed
//! invalidation fails the tests instead of silently corrupting a run.

use std::fmt;
use std::sync::Arc;

use crate::groups::Groups;
use crate::systems::labor::{pool_count, pool_key};
use crate::world::World;

/// Groupings derived from the POP table layout.
#[derive(Debug, PartialEq, Eq)]
pub struct PopLayout {
    /// POP rows by labour pool `(province, profession)` (see `labor::pool_key`).
    pub labour: Groups,
    /// POP rows by `(market, profession)`: the dividend owner pools.
    pub owners: Groups,
    /// Market row of each POP.
    pub market: Vec<u32>,
}

impl PopLayout {
    pub fn build(world: &World) -> PopLayout {
        let pops = &world.pops;
        let profs = world.defs.professions.len();
        let market: Vec<u32> = pops.province.iter().map(|&p| world.geography.province_market[p as usize]).collect();
        let labour_keys: Vec<usize> =
            (0..pops.len()).map(|i| pool_key(world, pops.province[i], pops.profession[i] as usize)).collect();
        let owner_keys: Vec<usize> =
            (0..pops.len()).map(|i| market[i] as usize * profs + pops.profession[i] as usize).collect();
        PopLayout {
            labour: Groups::build(pool_count(world), &labour_keys),
            owners: Groups::build(world.geography.market_count() * profs, &owner_keys),
            market,
        }
    }
}

/// Cache slot for [`PopLayout`] inside [`World`].
///
/// Derived data, not simulation state: equality ignores it and `state_hash`
/// never reads it. Two worlds with identical state compare equal whether or
/// not their caches are warm. Cloning shares the `Arc`.
#[derive(Clone, Default)]
pub struct LayoutCache(pub(crate) Option<Arc<PopLayout>>);

impl PartialEq for LayoutCache {
    fn eq(&self, _: &LayoutCache) -> bool {
        true
    }
}

impl Eq for LayoutCache {}

impl fmt::Debug for LayoutCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_some() { "LayoutCache(warm)" } else { "LayoutCache(cold)" })
    }
}
