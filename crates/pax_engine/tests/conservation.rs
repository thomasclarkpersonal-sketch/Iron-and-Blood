//! Stock-flow consistency under randomised economies (AGENTS.md §6).
//!
//! `tick::step` asserts that total money is unchanged every day; these tests
//! drive it through many random worlds (deprived POPs, rationing, idle
//! producers, extinct POPs, multiple markets) so that any leak in any system
//! trips the assertion.

mod common;

use common::random_world;
use pax_engine::step;

#[test]
fn money_conserved_in_random_economies() {
    for seed in 0..200 {
        let mut world = random_world(seed);
        let money = world.total_money();
        for _ in 0..60 {
            // `step` panics on any leak; the explicit check documents intent.
            assert_eq!(step(&mut world).total_money, money, "seed {seed}");
        }
        let all_cash = world.pops.cash.iter().chain(&world.producers.cash);
        assert!(all_cash.into_iter().all(|c| !c.is_negative()), "seed {seed}: negative cash");
    }
}

#[test]
fn goods_and_prices_stay_valid() {
    for seed in 200..260 {
        let mut world = random_world(seed);
        for _ in 0..60 {
            step(&mut world);
        }
        let p = &world.producers;
        assert!(p.output_stock.iter().chain(&p.input_stock).all(|q| !q.is_negative()), "seed {seed}: negative stock");
        assert!(world.markets.price.iter().all(|p| p.is_positive()), "seed {seed}: non-positive price");
    }
}

#[test]
fn random_worlds_are_deterministic() {
    for seed in 300..320 {
        let a = pax_engine::tick::run(&mut random_world(seed), 30);
        let b = pax_engine::tick::run(&mut random_world(seed), 30);
        assert_eq!(a, b, "seed {seed}");
    }
}
