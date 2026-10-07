//! POP layout cache invalidation (MILESTONE_1 task T4).

mod common;

use common::random_world;
use pax_engine::layout::PopLayout;
use pax_engine::{Fixed, step};

#[test]
fn cache_is_rebuilt_after_push_pop() {
    let mut world = random_world(7);
    let before = world.pop_layout();
    assert_eq!(before.market.len(), world.pops.len());
    world.push_pop(0, 0, 10, Fixed::ONE);
    let after = world.pop_layout();
    assert_eq!(after.market.len(), world.pops.len(), "push_pop must invalidate the cache");
    assert_eq!(*after, PopLayout::build(&world));
}

#[test]
fn cache_does_not_affect_equality_or_results() {
    let mut warm = random_world(8);
    warm.pop_layout();
    let cold = random_world(8);
    assert_eq!(warm, cold, "cache must not take part in equality");
    let (mut a, mut b) = (warm, cold);
    for _ in 0..30 {
        step(&mut a);
        step(&mut b);
        b.invalidate_pop_layout(); // force a rebuild every day on one side
    }
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn cache_rebuilds_itself_when_inputs_change_without_invalidation() {
    let mut world = random_world(9);
    world.pop_layout();
    let last = world.pops.len() - 1;
    let provinces = world.geography.province_count() as u32;
    assert!(provinces >= 2, "seed must give at least two provinces");
    // A system "forgets" to invalidate after moving a POP.
    world.pops.province[last] = (world.pops.province[last] + 1) % provinces;
    assert_eq!(*world.pop_layout(), PopLayout::build(&world), "stale layout served after a province change");
    // Same for the province -> market map.
    world.geography.province_market[0] =
        (world.geography.province_market[0] + 1) % world.geography.market_count() as u32;
    assert_eq!(*world.pop_layout(), PopLayout::build(&world), "stale layout served after a market map change");
}
