//! Estates of extinct POPs (DECISIONS.md D7, MILESTONE_1 task T6), and the births
//! band-aid (D26).

mod common;

use std::sync::Arc;

use common::{d, rules};
use pax_engine::defs::{Defs, GoodDef, ProducerTypeDef, ProfessionDef};
use pax_engine::systems::demographics::update_population;
use pax_engine::world::{Geography, NewProducer};
use pax_engine::{Fixed, World};

/// One good, one profession, `provinces` provinces in one market.
fn world(provinces: usize) -> World {
    let mut r = rules();
    r.demographics.starvation_rate = d("1"); // zero life needs => whole POP dies
    let defs = Defs {
        goods: vec![GoodDef { key: "grain".into(), base_price: d("1") }],
        professions: vec![ProfessionDef {
            key: "farmer".into(),
            spend_rate: d("0.5"),
            subsistence: vec![d("0.01")],
            preference: vec![Fixed::ONE],
        }],
        producer_types: vec![],
        rules: r,
    };
    let geography = Geography {
        province_keys: (0..provinces).map(|p| format!("p{p}")).collect(),
        province_market: vec![0; provinces],
        market_keys: vec!["m".into()],
        market_nation: Vec::new(),
    };
    World::new(Arc::new(defs), geography, 1)
}

#[test]
fn many_simultaneous_extinctions_pass_estates_to_the_largest_survivor() {
    let mut w = world(2);
    let starving = 20_000;
    for _ in 0..starving {
        let i = w.push_pop(0, 0, 3, d("2.5"));
        w.pops.life_needs[i] = Fixed::ZERO;
    }
    let small_heir = w.push_pop(0, 0, 500, d("1"));
    let big_heir = w.push_pop(0, 0, 900, d("1"));
    let other_province = w.push_pop(1, 0, 10_000, d("7"));
    let before = w.total_money();

    update_population(&mut w);

    assert_eq!(w.total_money(), before, "estates must not create or destroy money");
    assert_eq!(w.pops.cash[big_heir], d("1") + d("2.5").mul_int(starving as i64));
    assert_eq!(w.pops.cash[small_heir], d("1"));
    assert_eq!(w.pops.cash[other_province], d("7"), "estates never cross provinces");
    assert!(w.pops.size[..starving].iter().all(|&s| s == 0));
    assert!(w.pops.cash[..starving].iter().all(|c| c.is_zero()));
}

#[test]
fn ties_go_to_the_lowest_row() {
    let mut w = world(1);
    let dying = w.push_pop(0, 0, 1, d("5"));
    w.pops.life_needs[dying] = Fixed::ZERO;
    let first = w.push_pop(0, 0, 100, Fixed::ZERO);
    let second = w.push_pop(0, 0, 100, Fixed::ZERO);

    update_population(&mut w);

    assert_eq!(w.pops.cash[first], d("5"));
    assert_eq!(w.pops.cash[second], Fixed::ZERO);
}

#[test]
fn estate_stays_put_when_the_province_is_empty() {
    let mut w = world(1);
    let dying = w.push_pop(0, 0, 1, d("5"));
    w.pops.life_needs[dying] = Fixed::ZERO;

    update_population(&mut w);

    assert_eq!(w.pops.size[dying], 0);
    assert_eq!(w.pops.cash[dying], d("5"), "no heir: the empty row keeps the cash (D7)");
}

/// D26: with `births_need_employment`, a fed worker POP grows only on its pool's
/// employed share; an owner POP, and the rule switched off, grow at the full rate.
#[test]
fn births_follow_the_employed_share_when_the_band_aid_is_on() {
    let run = |band_aid: bool| {
        let mut r = rules();
        r.demographics.growth_rate = d("0.01");
        r.demographics.births_need_employment = band_aid;
        let profession = |key: &str| ProfessionDef {
            key: key.into(),
            spend_rate: d("0.5"),
            subsistence: vec![d("0.01")],
            preference: vec![Fixed::ONE],
        };
        let defs = Defs {
            goods: vec![GoodDef { key: "grain".into(), base_price: d("1") }],
            professions: vec![profession("farmer"), profession("landlord")],
            producer_types: vec![ProducerTypeDef {
                key: "farm".into(),
                output: 0,
                output_per_worker: d("0.02"),
                inputs: vec![],
                worker: 0,
                owner: 1,
                labor_share: d("0.7"),
                input_spend_rate: Fixed::ZERO,
            }],
            rules: r,
        };
        let geography = Geography {
            province_keys: vec!["p".into()],
            province_market: vec![0],
            market_keys: vec!["m".into()],
            market_nation: Vec::new(),
        };
        let mut w = World::new(Arc::new(defs), geography, 1);
        let farmers = w.push_pop(0, 0, 1000, d("10"));
        let landlords = w.push_pop(0, 1, 1000, d("10"));
        for i in [farmers, landlords] {
            w.pops.life_needs[i] = Fixed::ONE;
        }
        // 500 jobs for 1,000 farmers: half are employed.
        w.push_producer(NewProducer {
            kind: 0,
            province: 0,
            capacity: 500,
            cash: d("50"),
            wage: d("0.01"),
            output_stock: Fixed::ZERO,
        });
        update_population(&mut w);
        (w.pops.size[farmers], w.pops.size[landlords])
    };
    assert_eq!(run(false), (1010, 1010), "D7 as written: +1% for every fed POP");
    assert_eq!(run(true), (1005, 1010), "half the farmers have jobs: half the births; owners unchanged");
}
