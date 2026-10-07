//! Militancy dynamics (DECISIONS.md D19).

mod common;

use common::{d, random_world};
use pax_engine::systems::politics::update_militancy;
use pax_engine::{Fixed, step};

#[test]
fn hunger_and_taxes_raise_militancy_then_it_fades() {
    let mut world = random_world(21);
    let rows: Vec<usize> = (0..world.pops.len()).filter(|&i| world.pops.size[i] > 0).collect();
    assert!(!rows.is_empty());
    for &i in &rows {
        world.pops.life_needs[i] = Fixed::ZERO;
    }
    update_militancy(&mut world);
    for &i in &rows {
        // At least the hunger term (rise = 0.05); taxes can only add to it.
        assert!(world.pops.militancy[i] >= d("0.05"), "row {i}: {}", world.pops.militancy[i]);
    }
    let raised: Vec<Fixed> = rows.iter().map(|&i| world.pops.militancy[i]).collect();
    // Needs met, no tax: militancy decays by 10% a month.
    for &i in &rows {
        world.pops.life_needs[i] = Fixed::ONE;
    }
    world.nations.income_tax_rate.fill(Fixed::ZERO);
    update_militancy(&mut world);
    for (&i, &before) in rows.iter().zip(&raised) {
        assert!(world.pops.militancy[i] < before, "row {i} did not decay");
    }
}

#[test]
fn militancy_stays_in_unit_interval() {
    for seed in 5000..5080 {
        let mut world = random_world(seed);
        for _ in 0..90 {
            step(&mut world);
            assert!(world.pops.militancy.iter().all(|&m| m >= Fixed::ZERO && m <= Fixed::ONE), "seed {seed}");
        }
    }
}
