//! Per-province views (D22) must agree with the tick's own whole-world summaries:
//! summing a view over provinces gives the engine's total, in random economies.

mod common;

use common::random_world;
use pax_engine::step;
use pax_engine::systems::labor;
use pax_engine::systems::market::MilitancySummary;
use pax_engine::views::ProvinceStats;

#[test]
fn province_stats_sum_to_the_world_totals() {
    for seed in 0..60 {
        let mut world = random_world(seed);
        let mut report = step(&mut world);
        for _ in 0..40 {
            report = step(&mut world);
        }
        let s = ProvinceStats::of(&world, Some(&report.labour));

        let population: u64 = world.pops.size.iter().map(|&n| n as u64).sum();
        assert_eq!(s.population.iter().sum::<u64>(), population, "seed {seed}");

        let whole = MilitancySummary::of(&world.pops);
        let people: u64 = s.militancy.iter().map(|m| m.people).sum();
        let weighted: i128 = s.militancy.iter().map(|m| m.weighted_raw).sum();
        assert_eq!((people, weighted), (whole.people, whole.weighted_raw), "seed {seed}");

        let (unemployed, workforce) = labor::unemployment(&world.defs, &report.labour);
        let by_province = s.unemployment.iter().fold((0, 0), |(u, w), &(pu, pw)| (u + pu, w + pw));
        assert_eq!(by_province, (unemployed, workforce), "seed {seed}");
        for p in 0..s.population.len() {
            assert!(s.unemployment_rate(p) <= pax_engine::Fixed::ONE, "seed {seed}");
            assert!(s.life_needs[p].deprived <= s.life_needs[p].people, "seed {seed}");
        }
    }
}

#[test]
fn without_a_labour_report_unemployment_is_zero() {
    let world = random_world(7);
    let s = ProvinceStats::of(&world, None);
    assert!(s.unemployment.iter().all(|&u| u == (0, 0)));
}
