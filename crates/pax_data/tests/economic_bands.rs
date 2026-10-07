//! Economic regression bands for `two_states` (MILESTONE_2 task N5).
//!
//! Golden hashes catch *any* change; this test says whether the economy still
//! *behaves* the same: year-20 aggregates must stay within bands around the
//! values measured when the bands were set (2026-10-08, after D21). When a PR
//! changes economic behaviour on purpose (investment will lower unemployment,
//! for instance), it updates these bands and says why.
//!
//! Integer and `Fixed` arithmetic only (no floats, D3). 20 years take ~1 s in
//! release; debug builds skip it (`cargo test --release` runs it, as CI does).

use std::path::PathBuf;

use pax_engine::Fixed;

#[test]
#[cfg_attr(debug_assertions, ignore = "20-year run; executed by `cargo test --release` (CI)")]
fn two_states_year_20_aggregates_stay_in_band() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
    let scenario = pax_data::load_scenario(&dir).expect("two_states loads");
    let (mut world, log) = (scenario.world, scenario.commands);
    let worker = world.defs.worker_professions();

    let (days, window) = (7200u64, 360u64);
    let (mut final_demand, mut taxes, mut gross_income) = (Fixed::ZERO, Fixed::ZERO, Fixed::ZERO);
    let mut last = None;
    for day in 0..days {
        let (report, rejected) = pax_data::step_logged(&mut world, &log);
        assert!(rejected.is_empty(), "day {day}: logged commands rejected: {rejected:?}");
        if day + window >= days {
            final_demand += report.household_spending + report.government_spending;
            taxes += report.payouts.taxes;
            gross_income += report.payouts.wages + report.payouts.dividends;
        }
        last = Some(report);
    }
    let last = last.expect("ran");

    // GDP per day over the final year (C + G).
    let gdp = final_demand.div_int(window as i64);
    assert!(gdp >= Fixed::from_int(4500) && gdp <= Fixed::from_int(5500), "GDP/day {gdp} outside [4500, 5500]");

    // Unemployment among worker professions on the last day: 5%..15% (structural
    // drift from fixed capacity; investment, INVESTMENT.md, will move this band).
    let (mut workforce, mut unemployed) = (0u64, 0u64);
    for pool in last.labour.iter().filter(|p| worker[p.profession as usize]) {
        workforce += pool.workforce;
        unemployed += pool.unemployed();
    }
    assert!(
        unemployed * 100 >= workforce * 5 && unemployed * 100 <= workforce * 15,
        "unemployment {unemployed}/{workforce} outside 5%..15%"
    );

    // Tax take over the final year: 11.5%..12.5% (12% policy after the day-360 command).
    let take = taxes.div(gross_income);
    assert!(
        take >= Fixed::ratio(115, 1000) && take <= Fixed::ratio(125, 1000),
        "tax take {take} outside [0.115, 0.125]"
    );

    // Life needs: population-weighted coverage at the last market ≥ 0.98.
    let life = last.life_needs.mean().expect("people at market");
    assert!(life >= Fixed::ratio(98, 100), "life-needs coverage {life} < 0.98");

    // Militancy: population-weighted mean 0.10..0.15 (tax-driven equilibrium ≈ 0.12).
    let population = world.population();
    let militancy = last.militancy.mean().expect("people at market");
    assert!(
        militancy >= Fixed::ratio(10, 100) && militancy <= Fixed::ratio(15, 100),
        "militancy {militancy} outside [0.10, 0.15]"
    );

    // Population: 250k..275k (≈ 262k when set).
    assert!((250_000..=275_000).contains(&population), "population {population} outside [250k, 275k]");
}
