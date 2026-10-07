//! Edge cases found by the overnight engine audit (2026-10-08).

mod common;

use std::sync::Arc;

use common::{d, rules};
use pax_engine::defs::{Defs, GoodDef, ProfessionDef};
use pax_engine::world::Geography;
use pax_engine::{Fixed, World, step};

/// D1: prices anywhere inside [price_floor, price_ceiling] must not overflow the
/// fixed-point maths, even for very large POPs with a large subsistence need.
#[test]
fn huge_subsistence_cost_times_huge_pop_does_not_overflow() {
    let defs = Defs {
        goods: vec![GoodDef { key: "grain".into(), base_price: d("950000") }],
        professions: vec![ProfessionDef {
            key: "farmer".into(),
            spend_rate: d("0.2"),
            subsistence: vec![d("1")],
            preference: vec![Fixed::ONE],
        }],
        producer_types: vec![],
        rules: rules(),
    };
    let geography = Geography {
        province_keys: vec!["p".into()],
        province_market: vec![0],
        market_keys: vec!["m".into()],
        market_nation: Vec::new(),
    };
    let mut world = World::new(Arc::new(defs), geography, 1);
    world.push_pop(0, 0, 10_000_000, d("1000"));
    world.push_pop(0, 0, 100_000_000, d("1000"));
    for _ in 0..3 {
        step(&mut world); // must not panic
    }
}

/// D7: an empty row's cash passes to an heir as soon as anyone lives in the
/// province, not only in the month the row died out.
#[test]
fn stranded_estates_pass_to_a_later_heir() {
    let defs = Defs {
        goods: vec![GoodDef { key: "grain".into(), base_price: d("1") }],
        professions: ["farmer", "labourer"]
            .map(|k| ProfessionDef {
                key: k.into(),
                spend_rate: d("0.2"),
                subsistence: vec![Fixed::ZERO],
                preference: vec![Fixed::ONE],
            })
            .to_vec(),
        producer_types: vec![],
        rules: rules(),
    };
    let geography = Geography {
        province_keys: vec!["p".into()],
        province_market: vec![0],
        market_keys: vec!["m".into()],
        market_nation: Vec::new(),
    };
    let mut world = World::new(Arc::new(defs), geography, 1);
    let empty = world.push_pop(0, 0, 0, d("500")); // died out earlier with no heir
    let living = world.push_pop(0, 1, 1000, d("10"));
    pax_engine::systems::demographics::update_population(&mut world);
    assert_eq!(world.pops.cash[empty], Fixed::ZERO);
    assert_eq!(world.pops.cash[living], d("510"));
}
