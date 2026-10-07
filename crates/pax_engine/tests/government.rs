//! Treasury bookkeeping (DECISIONS.md D15).

mod common;

use common::random_world;
use pax_engine::{Fixed, step};

#[test]
fn treasuries_change_by_taxes_minus_transfers() {
    let mut taxed_days = 0;
    for seed in 3000..3150 {
        let mut world = random_world(seed);
        for day in 0..60 {
            let before: Fixed = world.nations.treasury.iter().copied().sum();
            let report = step(&mut world);
            let after: Fixed = world.nations.treasury.iter().copied().sum();
            assert_eq!(after - before, report.payouts.taxes - report.transfers, "seed {seed} day {day}");
            assert!(world.nations.treasury.iter().all(|t| !t.is_negative()), "seed {seed} day {day}");
            if report.payouts.taxes.is_positive() {
                taxed_days += 1;
            }
        }
    }
    assert!(taxed_days > 100, "random worlds should exercise taxation ({taxed_days} taxed days)");
}

#[test]
fn stateless_markets_are_untaxed() {
    for seed in 3200..3260 {
        let mut world = random_world(seed);
        if world.nations.is_empty() {
            for day in 0..30 {
                let report = step(&mut world);
                assert!(report.payouts.taxes.is_zero() && report.transfers.is_zero(), "seed {seed} day {day}");
            }
        }
    }
}
