//! Labour pool reporting (MILESTONE_1 task T7).

mod common;

use common::random_world;
use pax_engine::step;

#[test]
fn labour_report_matches_employment() {
    for seed in 2000..2100 {
        let mut world = random_world(seed);
        for day in 0..20 {
            let report = step(&mut world);
            let mut reported_employed = 0u64;
            let mut last_key = None;
            for pool in &report.labour {
                assert!(pool.workforce > 0 || pool.jobs > 0, "seed {seed} day {day}: empty pool reported");
                assert_eq!(pool.employed, pool.workforce.min(pool.jobs), "seed {seed} day {day}: {pool:?}");
                assert_eq!(pool.unemployed(), pool.workforce - pool.employed);
                let key = (pool.province, pool.profession);
                assert!(last_key < Some(key), "seed {seed} day {day}: pools out of order");
                last_key = Some(key);
                reported_employed += pool.employed;
            }
            let actual: u64 = world.producers.employed.iter().map(|&e| e as u64).sum();
            assert_eq!(reported_employed, actual, "seed {seed} day {day}: report disagrees with producers.employed");
        }
    }
}
