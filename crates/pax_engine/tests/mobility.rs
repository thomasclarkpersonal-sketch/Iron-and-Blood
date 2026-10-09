//! Monthly labour mobility (DECISIONS.md D18), migration (D20) and occupational
//! migration (D25).

mod common;

use std::sync::Arc;

use common::{d, random_world, rules};
use pax_engine::defs::{Defs, GoodDef, ProducerTypeDef, ProfessionDef};
use pax_engine::systems::labor::assign_employment;
use pax_engine::systems::mobility::{migrate_within_markets, reassign_workers, retrain_within_markets};
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

    let moved = reassign_workers(&mut world, &layout, &labour);

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
    assert_eq!(reassign_workers(&mut world, &layout, &labour), 0);
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
            moves += reassign_workers(&mut world, &layout, &labour);
            assert_eq!((world.population(), world.total_money()), (people, money), "seed {seed}");
            assert!(world.pops.cash.iter().all(|c| !c.is_negative()), "seed {seed}");
        }
    }
    assert!(moves > 0, "random worlds should exercise mobility");
}

/// Two provinces of one market: 1,000 farmers and 100 farm jobs in province 0;
/// a farm with 300 jobs and nobody in province 1.
fn two_province_world() -> World {
    let mut world = mismatched_world();
    world.geography.province_keys.push("q".into());
    world.geography.province_market.push(0);
    world.producers.capacity[1] = 0; // no labourer jobs: farmers must migrate, not switch
    world.push_producer(NewProducer {
        kind: 0,
        province: 1,
        capacity: 300,
        cash: d("50"),
        wage: d("0.01"),
        output_stock: Fixed::ZERO,
    });
    world
}

#[test]
fn surplus_workers_migrate_to_vacancies_in_their_market() {
    let mut world = two_province_world();
    let (people, money) = (world.population(), world.total_money());
    let layout = world.pop_layout();
    let moved = migrate_within_markets(&mut world, &layout);
    // Surplus 900 farmers × migration_rate 0.1 = 90, well within 300 vacancies.
    assert_eq!(moved, 90);
    let dest = (0..world.pops.len()).find(|&i| world.pops.province[i] == 1).expect("a farmer row in province 1");
    assert_eq!(world.pops.profession[dest], 0, "migrants keep their profession");
    assert_eq!(world.pops.size[dest], 90);
    assert_eq!(world.pops.cash[dest], d("9"), "migrants take 90/1000 of the cash");
    assert_eq!((world.population(), world.total_money()), (people, money));
}

#[test]
fn migration_never_crosses_markets() {
    let mut world = two_province_world();
    world.geography.market_keys.push("other".into());
    world.geography.province_market[1] = 1;
    let goods = world.defs.good_count();
    world.markets.price.extend(std::iter::repeat_n(Fixed::ONE, goods));
    let layout = world.pop_layout();
    assert_eq!(migrate_within_markets(&mut world, &layout), 0);
}

#[test]
fn migration_conserves_people_and_money_in_random_worlds() {
    let mut moves = 0;
    for seed in 4100..4200 {
        let mut world = random_world(seed);
        for _ in 0..3 {
            let (people, money) = (world.population(), world.total_money());
            let layout = world.pop_layout();
            moves += migrate_within_markets(&mut world, &layout);
            assert_eq!((world.population(), world.total_money()), (people, money), "seed {seed}");
        }
    }
    assert!(moves > 0, "random worlds should exercise migration");
}

/// Two provinces of one market, D25's case: 1,000 farmers and 100 farm jobs in
/// province 0, 300 labourer jobs and nobody in province 1. D18 can't help (no other
/// jobs in province 0), nor can D20 (no farm jobs in province 1).
fn farm_and_camp_world(rate: &str) -> World {
    let mut world = two_province_world();
    world.producers.kind[2] = 1; // province 1's 300 jobs hire labourers, not farmers
    let mut defs = (*world.defs).clone();
    defs.rules.demographics.occupational_migration_rate = d(rate);
    world.defs = Arc::new(defs);
    world
}

#[test]
fn surplus_workers_retrain_for_vacancies_elsewhere_in_their_market() {
    let mut world = farm_and_camp_world("0.1");
    let (people, money) = (world.population(), world.total_money());
    let layout = world.pop_layout();
    let labour = assign_employment(&mut world, &layout.labour);
    assert_eq!(reassign_workers(&mut world, &layout, &labour), 0, "no other jobs in province 0: D18 can't help");
    let layout = world.pop_layout();
    assert_eq!(migrate_within_markets(&mut world, &layout), 0, "no farm jobs elsewhere: D20 can't help");
    let layout = world.pop_layout();
    let moved = retrain_within_markets(&mut world, &layout);
    // Surplus 900 farmers × 0.1 = 90, well within 300 vacancies.
    assert_eq!(moved, 90);
    let dest = (0..world.pops.len()).find(|&i| world.pops.province[i] == 1).expect("a row in province 1");
    assert_eq!(world.pops.profession[dest], 1, "movers take the vacancy's profession");
    assert_eq!(world.pops.size[dest], 90);
    assert_eq!(world.pops.cash[dest], d("9"), "movers take 90/1000 of the cash");
    assert_eq!((world.population(), world.total_money()), (people, money));
}

#[test]
fn retraining_needs_another_province_and_another_profession() {
    // Same province only (D18's case): nothing to do for D25.
    let mut world = mismatched_world();
    let mut defs = (*world.defs).clone();
    defs.rules.demographics.occupational_migration_rate = d("0.1");
    world.defs = Arc::new(defs);
    let layout = world.pop_layout();
    assert_eq!(retrain_within_markets(&mut world, &layout), 0, "one province: D18's job");
    // Same profession elsewhere (D20's case): nothing for D25 either.
    let mut world = two_province_world();
    let mut defs = (*world.defs).clone();
    defs.rules.demographics.occupational_migration_rate = d("0.1");
    world.defs = Arc::new(defs);
    let layout = world.pop_layout();
    assert_eq!(retrain_within_markets(&mut world, &layout), 0, "same profession: D20's job");
    // Rate 0 (mini_valley's frozen rules): off.
    let mut world = farm_and_camp_world("0");
    let layout = world.pop_layout();
    assert_eq!(retrain_within_markets(&mut world, &layout), 0);
}

#[test]
fn retraining_never_crosses_markets() {
    let mut world = farm_and_camp_world("0.1");
    world.geography.market_keys.push("other".into());
    world.geography.province_market[1] = 1;
    let goods = world.defs.good_count();
    world.markets.price.extend(std::iter::repeat_n(Fixed::ONE, goods));
    let layout = world.pop_layout();
    assert_eq!(retrain_within_markets(&mut world, &layout), 0);
}

#[test]
fn retraining_conserves_people_and_money_in_random_worlds() {
    let mut moves = 0;
    for seed in 4300..4400 {
        let mut world = random_world(seed);
        for _ in 0..3 {
            let (people, money) = (world.population(), world.total_money());
            let layout = world.pop_layout();
            moves += retrain_within_markets(&mut world, &layout);
            assert_eq!((world.population(), world.total_money()), (people, money), "seed {seed}");
        }
    }
    assert!(moves > 0, "random worlds should exercise occupational migration");
}

#[test]
fn compaction_merges_duplicates_and_drops_empty_rows() {
    let mut world = mismatched_world();
    world.push_pop(0, 0, 500, d("30")); // duplicate farmer identity
    let empty = world.push_pop(0, 1, 0, Fixed::ZERO); // empty, cashless labourer row
    world.pops.life_needs[empty] = Fixed::ZERO;
    world.pops.life_needs[1] = Fixed::ZERO; // the duplicate is starving
    let (people, money) = (world.population(), world.total_money());

    assert_eq!(world.compact_pops(), 2);
    assert_eq!(world.pops.len(), 1);
    assert_eq!((world.pops.size[0], world.pops.cash[0]), (1500, d("130")));
    // 1,000 people at 1.0 and 500 at 0.0 -> 0.666666
    assert_eq!(world.pops.life_needs[0], Fixed::ratio(2, 3));
    assert_eq!((world.population(), world.total_money()), (people, money));
    assert_eq!(world.compact_pops(), 0, "compaction is idempotent");
}

#[test]
fn identities_stay_unique_in_random_worlds() {
    for seed in 4300..4360 {
        let mut world = random_world(seed);
        for _ in 0..60 {
            pax_engine::step(&mut world);
        }
        world.compact_pops();
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..world.pops.len() {
            assert!(seen.insert((world.pops.province[i], world.pops.profession[i])), "seed {seed}: duplicate identity");
        }
    }
}
