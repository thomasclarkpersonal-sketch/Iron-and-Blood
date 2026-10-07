//! Shared generator of random small economies for the engine's property tests.
//! Each integration test file includes it with `mod common;`.

#![allow(dead_code)] // each test crate uses a different subset

use std::sync::Arc;

use pax_engine::defs::*;
use pax_engine::world::{Geography, NewNation, NewProducer};
use pax_engine::{Fixed, World};

/// Xorshift64: a tiny deterministic generator for test inputs only.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// Random value in `[0, max_raw)` raw units.
    pub fn fixed(&mut self, max_raw: i64) -> Fixed {
        Fixed::from_raw((self.next() % max_raw as u64) as i64)
    }
}

pub fn d(s: &str) -> Fixed {
    Fixed::parse_decimal(s).unwrap()
}

pub fn rules() -> Rules {
    Rules {
        days_per_month: 10,
        market: MarketRules {
            max_iterations: 32,
            step: d("0.5"),
            step_decay_iterations: 8,
            tolerance: d("0.001"),
            max_daily_change: d("0.2"),
            min_stock: d("0.01"),
            price_floor: d("0.001"),
            price_ceiling: d("1000000"),
        },
        firms: FirmRules {
            wage_stickiness_days: 10,
            revenue_smoothing_days: 10,
            reserve_days: 5,
            dividend_payout_rate: d("0.1"),
            target_stock_days: 2,
            subsistence_wage_multiple: d("1.5"),
        },
        demographics: DemographicRules { growth_rate: d("0.01"), starvation_rate: d("0.3"), mobility_rate: d("0.2") },
    }
}

pub fn random_world(seed: u64) -> World {
    let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let goods = 2 + r.below(4) as usize;
    let profs = 1 + r.below(3) as usize;
    let defs = Defs {
        goods: (0..goods)
            .map(|g| GoodDef { key: format!("g{g}"), base_price: d("0.5") + r.fixed(5_000_000) })
            .collect(),
        professions: (0..profs)
            .map(|c| {
                let weights: Vec<Fixed> = (0..goods).map(|_| r.fixed(1_000_000) + Fixed::EPSILON).collect();
                ProfessionDef {
                    key: format!("c{c}"),
                    spend_rate: d("0.05") + r.fixed(900_000),
                    subsistence: (0..goods).map(|g| if g == 0 { r.fixed(50_000) } else { Fixed::ZERO }).collect(),
                    preference: pax_engine::alloc::allocate(Fixed::ONE, &weights).unwrap(),
                }
            })
            .collect(),
        producer_types: (0..goods)
            .map(|g| ProducerTypeDef {
                key: format!("t{g}"),
                output: g,
                output_per_worker: r.fixed(100_000) + Fixed::EPSILON,
                inputs: if g > 0 && r.below(2) == 0 { vec![(g - 1, d("0.5") + r.fixed(2_000_000))] } else { vec![] },
                worker: r.below(profs as u64) as usize,
                owner: r.below(profs as u64) as usize,
                labor_share: r.fixed(1_000_000),
                input_spend_rate: r.fixed(1_000_000),
            })
            .collect(),
        rules: rules(),
    };
    let markets = 1 + r.below(3) as usize;
    let provinces = markets + r.below(3) as usize;
    let geography = Geography {
        province_keys: (0..provinces).map(|p| format!("p{p}")).collect(),
        province_market: (0..provinces).map(|p| (p % markets) as u32).collect(),
        market_keys: (0..markets).map(|m| format!("m{m}")).collect(),
        market_nation: Vec::new(),
    };
    // 0–2 nations; each market belongs to a random nation or to none (stateless).
    let nations = r.below(3) as u32;
    let mut geography = geography;
    geography.market_nation = (0..markets)
        .map(|_| if nations == 0 || r.below(4) == 0 { None } else { Some(r.below(nations as u64) as u32) })
        .collect();
    let mut world = World::new(Arc::new(defs), geography, seed);
    for n in 0..nations {
        world.push_nation(NewNation {
            key: format!("n{n}"),
            treasury: r.fixed(1_000_000_000),
            income_tax_rate: r.fixed(500_000),
            transfer_rate: r.fixed(200_000),
            consumption_rate: r.fixed(200_000),
            basket: (0..world.defs.good_count()).map(|_| r.fixed(1_000_000) + Fixed::EPSILON).collect(),
        });
    }
    for _ in 0..(1 + r.below(30)) {
        let size = if r.below(10) == 0 { 0 } else { 1 + r.below(50_000) as u32 };
        world.push_pop(r.below(provinces as u64) as u32, r.below(profs as u64) as usize, size, r.fixed(10_000_000_000));
    }
    for _ in 0..(1 + r.below(10)) {
        world.push_producer(NewProducer {
            kind: r.below(goods as u64) as usize,
            province: r.below(provinces as u64) as u32,
            capacity: r.below(20_000) as u32,
            cash: r.fixed(5_000_000_000),
            wage: r.fixed(100_000),
            output_stock: r.fixed(500_000_000),
        });
    }
    world
}
