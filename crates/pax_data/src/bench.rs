//! Benchmark worlds (D13): a scenario scaled up to target sizes, shared by
//! `pax_cli bench` and the server's view benchmark (`view_building_budget`, an ignored
//! test in `pax_server/src/view.rs`).

use pax_engine::World;

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
    let total_nations = base.nations.len() as u32 * regions;
    let cap = max_nations.map_or(total_nations, |c| c.clamp(1, total_nations.max(1)));
    use pax_engine::world::{Geography, NewProducer};
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
