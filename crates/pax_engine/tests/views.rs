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

/// On a day without month-end demographics, POP sizes and rows at the end of the day
/// are the ones the market saw. The per-province life-needs summaries must then add up
/// to the tick's own summary exactly, `deprived` included: one rule, applied once.
#[test]
fn province_life_needs_add_up_to_the_ticks_own_summary() {
    for seed in 0..60 {
        let mut world = random_world(seed);
        // `is_month_end` asks about the day about to run; the day that just ran was a
        // month end when the day count reached a multiple of the month length.
        let month = world.defs.rules.days_per_month.max(1) as u64;
        let mut report = step(&mut world);
        while world.day.is_multiple_of(month) || world.day < 40 {
            report = step(&mut world);
        }
        let totals = ProvinceStats::of(&world, Some(&report.labour)).totals();
        assert_eq!(totals.life_needs, report.life_needs, "seed {seed}, day {}", world.day);
        assert_eq!(totals.militancy.people, report.militancy.people, "seed {seed}");
    }
}

/// The province panel's groups partition the province's POPs, one per profession.
#[test]
fn province_pops_partition_each_province() {
    for seed in 0..30 {
        let mut world = random_world(seed);
        for _ in 0..35 {
            step(&mut world);
        }
        let stats = ProvinceStats::of(&world, None);
        for province in 0..world.geography.province_count() as u32 {
            let groups = pax_engine::views::province_pops(&world, province);
            assert!(groups.windows(2).all(|w| w[0].profession < w[1].profession), "seed {seed}");
            let people: u64 = groups.iter().map(|g| g.people).sum();
            assert_eq!(people, stats.population[province as usize], "seed {seed}");
            let life = groups
                .iter()
                .fold(Default::default(), |t: pax_engine::systems::market::LifeNeedsSummary, g| t + g.life_needs);
            assert_eq!(life, stats.life_needs[province as usize], "seed {seed}");
        }
    }
}

#[test]
fn without_a_labour_report_unemployment_is_zero() {
    let world = random_world(7);
    let s = ProvinceStats::of(&world, None);
    assert!(s.unemployment.iter().all(|&u| u == (0, 0)));
}
