//! Benchmark worlds (D13): a scenario scaled up to target sizes, shared by
//! `pax_cli bench` and the server's benchmarks (`view_building_budget` and
//! `server_day_budget`, ignored tests in `pax_server`).
//!
//! Two ways to grow a world, with different limits:
//! * `regions` copies the whole map, so every copy has its own provinces and
//!   markets and runs for any number of days;
//! * `scale` repeats POP rows **with the same identity** `(province, profession)`.
//!   Month-end compaction merges such rows back into one (D7), so a scaled world is
//!   only the world it claims to be until its first month end:
//!   [`days_before_compaction`]. Past it, the merged sizes can overflow `u32`.

use pax_engine::World;
use pax_engine::world::{Geography, NewProducer};

/// How many days a world with `scale > 1` can run before its first month end, when
/// compaction merges the repeated rows (D7). A benchmark that runs longer measures a
/// different, smaller world, or overflows, so `pax_cli bench` refuses it.
pub fn days_before_compaction(base: &World) -> u64 {
    let month = u64::from(base.defs.rules.days_per_month.max(1));
    month - 1 - base.day % month
}

/// The `scale` that brings `base`, copied `regions` times, to about `rows` POP rows.
/// One place for the arithmetic, so `pax_cli bench` callers and the server's budget
/// test build the same kind of world (D13's "Measured" rows).
pub fn scale_for_rows(base: &World, rows: u64, regions: u32) -> u32 {
    let per_copy = base.pops.len() as u64 * u64::from(regions);
    u32::try_from((rows / per_copy.max(1)).max(1)).expect("a scale that fits u32")
}

/// Builds a benchmark world: the scenario's whole map copied `regions` times
/// (separate provinces and markets), with every POP row repeated `scale` times.
/// Rows are pushed region by region, so they stay grouped by market as a loaded
/// scenario's are. Each region gets its own copy of the scenario's nations.
pub fn replicate(base: &World, scale: u32, regions: u32) -> World {
    replicate_with_nations(base, scale, regions, None)
}

/// [`replicate`], with at most `max_nations` nations. Region copies share nations
/// round-robin, so a 3,000-market world can have D13's few hundred nations instead
/// of one set per region. `None` behaves exactly like [`replicate`].
pub fn replicate_with_nations(base: &World, scale: u32, regions: u32, max_nations: Option<u32>) -> World {
    assert!(max_nations != Some(0), "a benchmark world needs at least one nation");
    let total_nations = base.nations.len() as u32 * regions;
    let cap = max_nations.map_or(total_nations, |c| c.min(total_nations.max(1)));
    let g = &base.geography;
    let (provinces, markets) = (g.province_count() as u32, g.market_count() as u32);
    let mut geography = Geography::default();
    for r in 0..regions {
        geography.province_keys.extend(g.province_keys.iter().map(|k| format!("{k}#{r}")));
        geography.province_market.extend(g.province_market.iter().map(|&m| m + r * markets));
        geography.market_keys.extend(g.market_keys.iter().map(|k| format!("{k}#{r}")));
        let nations = base.nations.len() as u32;
        geography
            .market_nation
            .extend((0..markets as usize).map(|m| g.nation_of_market(m).map(|n| (n as u32 + r * nations) % cap)));
    }
    let mut world = World::new(base.defs.clone(), geography, base.seed);
    for r in 0..regions {
        let n = &base.nations;
        for k in 0..n.len() {
            if r * n.len() as u32 + k as u32 >= cap {
                break;
            }
            world.push_nation(pax_engine::world::NewNation {
                key: format!("{}#{r}", n.key[k]),
                treasury: n.treasury[k],
                income_tax_rate: n.income_tax_rate[k],
                transfer_rate: n.transfer_rate[k],
                consumption_rate: n.consumption_rate[k],
                basket: n.basket[k * base.defs.good_count()..(k + 1) * base.defs.good_count()].to_vec(),
            });
        }
    }
    let (pops, producers) = (&base.pops, &base.producers);
    for r in 0..regions {
        for _ in 0..scale {
            for i in 0..pops.len() {
                world.push_pop(
                    pops.province[i] + r * provinces,
                    pops.profession[i] as usize,
                    pops.size[i],
                    pops.cash[i],
                );
            }
        }
        for i in 0..producers.len() {
            world.push_producer(NewProducer {
                kind: producers.kind[i] as usize,
                province: producers.province[i] + r * provinces,
                capacity: producers.capacity[i],
                cash: producers.cash[i],
                wage: producers.wage[i],
                output_stock: producers.output_stock[i],
            });
        }
    }
    world
}
