//! Monthly labour mobility (DECISIONS.md D18).

mod common;

use std::sync::Arc;

use common::{d, random_world, rules};
use pax_engine::defs::{Defs, GoodDef, ProducerTypeDef, ProfessionDef};
use pax_engine::systems::labor::assign_employment;
use pax_engine::systems::mobility::reassign_workers;
use pax_engine::world::{Geography, NewProducer};
use pax_engine::{Fixed, World};

/// One province: 1,000 farmers but only 100 farm jobs; 500 labourer jobs and no labourers.
fn mismatched_world() -> World {
    let mut r = rules();
    r.demographics.mobility_rate = d("0.2");
    let profession = |key: &str| ProfessionDef {
        key: key.into(),
        spend_rate: d("0.2"),
        subsistence: vec![d("0.01")],
        preference: vec![Fixed::ONE],
    };
    let producer = |key: &str, worker| ProducerTypeDef {
        key: key.into(),
        output: 0,
        output_per_worker: d("0.02"),
        inputs: vec![],
        worker,
        owner: 0,
        labor_share: d("0.7"),
        input_spend_rate: Fixed::ZERO,
    };
    let defs = Defs {
        goods: vec![GoodDef { key: "grain".into(), base_price: d("1") }],
        professions: vec![profession("farmer"), profession("labourer")],
        producer_types: vec![producer("farm", 0), producer("camp", 1)],
        rules: r,
    };
    let geography = Geography {
        province_keys: vec!["p".into()],
        province_market: vec![0],
        market_keys: vec!["m".into()],
        market_nation: Vec::new(),
    };
    let mut world = World::new(Arc::new(defs), geography, 1);
    world.push_pop(0, 0, 1000, d("100"));
    for (kind, capacity) in [(0, 100), (1, 500)] {
        world.push_producer(NewProducer {
            kind,
            province: 0,
            capacity,
            cash: d("50"),
            wage: d("0.01"),
            output_stock: Fixed::ZERO,
        });
    }
    world
}

#[test]
fn unemployed_move_to_vacancies_with_their_cash() {
    let mut world = mismatched_world();
    let layout = world.pop_layout();
    let labour = assign_employment(&mut world, &layout.labour);
    let (people, money) = (world.population(), world.total_money());

    let moved = reassign_workers(&mut world, &labour);

    // 900 unemployed farmers × 0.2 = 180 move into the 500 labourer vacancies.
    assert_eq!(moved, 180);
    assert_eq!(world.pops.len(), 2, "a labourer POP row was created");
    assert_eq!((world.pops.size[0], world.pops.size[1]), (820, 180));
    assert_eq!(world.pops.profession[1], 1);
    assert_eq!(world.pops.cash[1], d("18"), "movers take 180/1000 of the cash");
    assert_eq!((world.population(), world.total_money()), (people, money));
}

#[test]
fn no_vacancies_means_no_moves() {
    let mut world = mismatched_world();
    world.producers.capacity[1] = 0; // no labourer jobs
    let layout = world.pop_layout();
    let labour = assign_employment(&mut world, &layout.labour);
    assert_eq!(reassign_workers(&mut world, &labour), 0);
    assert_eq!(world.pops.len(), 1);
}

#[test]
fn mobility_conserves_people_and_money_in_random_worlds() {
    let mut moves = 0;
    for seed in 4000..4100 {
        let mut world = random_world(seed);
        for _ in 0..3 {
            let layout = world.pop_layout();
            let labour = assign_employment(&mut world, &layout.labour);
            let (people, money) = (world.population(), world.total_money());
            moves += reassign_workers(&mut world, &labour);
            assert_eq!((world.population(), world.total_money()), (people, money), "seed {seed}");
            assert!(world.pops.cash.iter().all(|c| !c.is_negative()), "seed {seed}");
        }
    }
    assert!(moves > 0, "random worlds should exercise mobility");
}
