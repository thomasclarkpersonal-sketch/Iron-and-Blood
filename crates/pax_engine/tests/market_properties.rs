//! Market-clearing properties under randomised economies (MILESTONE_1 task T8).
//!
//! `conservation.rs` checks that money is conserved; these tests check that the
//! market's *rules* hold every day in every random world (DECISIONS.md D1):
//!
//! * nobody receives more than they asked for, and nothing sells that was not
//!   offered: `traded ≤ min(demand, supply)` for every market and good;
//! * the executed price moves at most `max_daily_change` from yesterday's
//!   price, and stays within the technical floor and ceiling;
//! * buyers never overspend (no negative cash) and life-needs satisfaction
//!   stays in `[0, 1]`.
//!
//! Per-buyer `received ≤ demanded` and per-seller `delivered ≤ offered` are
//! asserted inside settlement itself (`market.rs`): the per-agent quantities
//! are local to the parallel fold, and asserting them there means every test
//! that steps a world exercises them.

mod common;

use common::random_world;
use pax_engine::{Fixed, step};

const SEEDS: std::ops::Range<u64> = 1000..1150;
const DAYS: usize = 60;

#[test]
fn trades_never_exceed_demand_or_supply() {
    for seed in SEEDS {
        let mut world = random_world(seed);
        for day in 0..DAYS {
            let report = step(&mut world);
            for (k, g) in report.goods.iter().enumerate() {
                assert!(
                    g.traded <= g.demand,
                    "seed {seed} day {day} market-good {k}: traded {} > demand {}",
                    g.traded,
                    g.demand
                );
                assert!(
                    g.traded <= g.supply,
                    "seed {seed} day {day} market-good {k}: traded {} > supply {}",
                    g.traded,
                    g.supply
                );
                assert!(!g.traded.is_negative(), "seed {seed} day {day}: negative trade");
            }
        }
    }
}

#[test]
fn prices_respect_daily_band_and_technical_bounds() {
    for seed in SEEDS {
        let mut world = random_world(seed);
        let rules = world.defs.rules.market.clone();
        for day in 0..DAYS {
            let opening = world.markets.price.clone();
            step(&mut world);
            for (k, (&open, &now)) in opening.iter().zip(&world.markets.price).enumerate() {
                // Pins D1's band exactly as computed in discover_prices (rounded
                // down, D3). Change both together if the band rounding changes.
                let band = open.mul(rules.max_daily_change);
                let lo = (open - band).max(rules.price_floor);
                let hi = (open + band).min(rules.price_ceiling);
                assert!(
                    now >= lo && now <= hi,
                    "seed {seed} day {day} market-good {k}: price {open} -> {now} outside [{lo}, {hi}]"
                );
            }
        }
    }
}

#[test]
fn buyers_never_overspend_and_needs_stay_in_range() {
    for seed in SEEDS {
        let mut world = random_world(seed);
        for day in 0..DAYS {
            step(&mut world);
            for (i, (&cash, &life)) in world.pops.cash.iter().zip(&world.pops.life_needs).enumerate() {
                assert!(!cash.is_negative(), "seed {seed} day {day} pop {i}: negative cash {cash}");
                assert!(life >= Fixed::ZERO && life <= Fixed::ONE, "seed {seed} day {day} pop {i}: life_needs {life}");
            }
            for (i, &cash) in world.producers.cash.iter().enumerate() {
                assert!(!cash.is_negative(), "seed {seed} day {day} producer {i}: negative cash {cash}");
            }
        }
    }
}

#[test]
fn life_needs_summary_matches_the_market_snapshot() {
    for seed in SEEDS.take(40) {
        let mut world = random_world(seed);
        for day in 0..DAYS {
            // Sizes as the market sees them (demographics may change them after).
            let sizes = world.pops.size.clone();
            let report = step(&mut world);
            let (mut people, mut deprived, mut weighted) = (0u64, 0u64, 0i128);
            for (&n, &life) in sizes.iter().zip(&world.pops.life_needs) {
                if n > 0 {
                    people += n as u64;
                    weighted += n as i128 * life.raw() as i128;
                    if life < Fixed::ONE {
                        deprived += n as u64;
                    }
                }
            }
            let s = report.life_needs;
            assert_eq!((s.people, s.deprived, s.weighted_raw), (people, deprived, weighted), "seed {seed} day {day}");
        }
    }
}
