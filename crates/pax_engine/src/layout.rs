//! POP groupings derived from the table layout, cached across ticks.
//!
//! Several systems need "the POPs of profession *c* in province *p*" (labour
//! pools), "… in market *m*" (dividend owner pools) and each POP's market. These
//! depend only on:
//!
//! * `pops.province` and `pops.profession` (and the number of rows),
//! * `geography.province_market` (and so the number of provinces), and
//! * the number of professions and markets, which size the key spaces.
//!
//! In M1 none of these change after loading, so rebuilding the groupings every
//! tick (three O(N) passes, ~8 ms at 1M POPs) was pure overhead (MILESTONE_1 T4).
//!
//! # Why this does not break D7
//!
//! DECISIONS.md D7 says derived values are never stored *as state*. This cache is
//! not state. It is excluded from equality and `World::state_hash`, and it
//! **validates itself**: every [`World::pop_layout`] call computes an O(N)
//! fingerprint of all the inputs above (~1 ms per 1M rows) and rebuilds on any
//! mismatch. A system that changes a POP's province or profession, or the
//! province→market map, therefore can never read stale groupings, even if it
//! forgets to call [`World::invalidate_pop_layout`], which remains available as an
//! explicit hint. Debug builds additionally compare the cache with a fresh build.
//!
//! **Snapshot validity:** a system receives the layout as an `Arc` snapshot taken
//! at the start of the tick, alongside `&mut World`. Within a tick no system
//! changes POP provinces or professions or the market map; systems that will
//! (M2 migration, promotion, conquest) must run *after* the systems that use the
//! snapshot, or fetch `World::pop_layout` again afterwards.
//!
//! This module also owns the **key encodings** of both groupings
//! ([`pool_key`], [`owner_key`]). Systems import them from here, so the
//! dependency direction is systems → layout → world.

use std::fmt;
use std::sync::Arc;

use crate::groups::Groups;
use crate::world::World;

/// Dense key of the labour pool `(province, profession)`.
pub fn pool_key(world: &World, province: u32, profession: usize) -> usize {
    province as usize * world.defs.professions.len() + profession
}

/// Inverse of [`pool_key`]: the `(province, profession)` of a labour pool key.
pub fn pool_of_key(world: &World, key: usize) -> (u32, u16) {
    let professions = world.defs.professions.len();
    ((key / professions) as u32, (key % professions) as u16)
}

/// Number of labour pools.
pub fn pool_count(world: &World) -> usize {
    world.geography.province_count() * world.defs.professions.len()
}

/// Dense key of the owner pool `(market, profession)`: the POPs that receive the
/// dividends of producers in `market` whose owner profession is `profession`.
pub fn owner_key(world: &World, market: usize, profession: usize) -> usize {
    market * world.defs.professions.len() + profession
}

/// Number of owner pools.
pub fn owner_count(world: &World) -> usize {
    world.geography.market_count() * world.defs.professions.len()
}

/// Groupings derived from the POP table layout. Obtain through
/// [`World::pop_layout`], which caches it; [`PopLayout::build`] is O(N).
#[derive(Debug, PartialEq, Eq)]
pub struct PopLayout {
    /// POP rows by labour pool (see [`pool_key`]).
    pub labour: Groups,
    /// POP rows by owner pool (see [`owner_key`]).
    pub owners: Groups,
    /// Market row of each POP.
    pub market: Vec<u32>,
    /// Fingerprint of the inputs this layout was built from (see [`fingerprint`]).
    /// Private so nothing outside this module can make a stale layout look valid.
    fingerprint: u64,
}

impl PopLayout {
    /// Fingerprint of the inputs this layout was built from.
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Builds the groupings from scratch: O(N). Systems should use
    /// [`World::pop_layout`] instead, which reuses a valid cached layout.
    pub fn build(world: &World) -> PopLayout {
        let pops = &world.pops;
        let market: Vec<u32> = pops.province.iter().map(|&p| world.geography.province_market[p as usize]).collect();
        let labour_keys: Vec<usize> =
            (0..pops.len()).map(|i| pool_key(world, pops.province[i], pops.profession[i] as usize)).collect();
        let owner_keys: Vec<usize> =
            (0..pops.len()).map(|i| owner_key(world, market[i] as usize, pops.profession[i] as usize)).collect();
        PopLayout {
            labour: Groups::build(pool_count(world), &labour_keys),
            owners: Groups::build(owner_count(world), &owner_keys),
            market,
            fingerprint: fingerprint(world),
        }
    }
}

/// 64-bit fingerprint of every input the layout depends on: row count, each
/// POP's province and profession, the province→market map (whose length is
/// the province count), and the profession and market counts that size the
/// pool key spaces. Word-at-a-time multiply–rotate mixing keeps it at ~1 ns per row.
/// A collision (≈2⁻⁶⁴) would be needed to miss a change.
pub fn fingerprint(world: &World) -> u64 {
    const K: u64 = 0x9E37_79B9_7F4A_7C15;
    let mix = |h: u64, v: u64| (h ^ v).wrapping_mul(K).rotate_left(29);
    let mut h = mix(K, world.pops.len() as u64);
    h = mix(h, world.defs.professions.len() as u64);
    h = mix(h, world.geography.market_count() as u64);
    for (&province, &profession) in world.pops.province.iter().zip(&world.pops.profession) {
        h = mix(h, ((province as u64) << 16) | profession as u64);
    }
    h = mix(h, world.geography.province_market.len() as u64);
    for &m in &world.geography.province_market {
        h = mix(h, m as u64);
    }
    h
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
