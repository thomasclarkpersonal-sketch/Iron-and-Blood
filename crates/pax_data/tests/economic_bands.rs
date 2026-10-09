//! Economic regression bands for `two_states` (MILESTONE_2 task N5).
//!
//! Golden hashes catch *any* change; this test says whether the economy still
//! *behaves* the same: year-20 aggregates must stay within bands around the
//! values measured when the bands were set (2026-10-08, after D21; unemployment,
//! employment and population re-set 2026-10-09 after D25 and D26). When a PR
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

    let (days, window) = (7200u64, 360u64);
    let (mut final_demand, mut taxes, mut gross_income) = (Fixed::ZERO, Fixed::ZERO, Fixed::ZERO);
    let mut last = None;
    let mut employed_year_5 = None;
    for day in 0..days {
        let (report, rejected) = pax_data::step_logged(&mut world, &log);
        assert!(rejected.is_empty(), "day {day}: logged commands rejected: {rejected:?}");
        if day + window >= days {
            final_demand += report.household_spending + report.government_spending;
            taxes += report.payouts.taxes;
            gross_income += report.payouts.wages + report.payouts.dividends;
        }
        if day + 1 == 1800 {
            employed_year_5 = Some(employed(&world.defs, &report.labour));
        }
        last = Some(report);
    }
    let last = last.expect("ran");

    // GDP per day over the final year (C + G).
    let gdp = final_demand.div_int(window as i64);
    assert!(gdp >= Fixed::from_int(4500) && gdp <= Fixed::from_int(5500), "GDP/day {gdp} outside [4500, 5500]");

    // Unemployment among worker professions on the last day: 0.3%..2% (0.8% when
    // set, 2026-10-09). Before D25 it drifted to 9.6%: workers could not change
    // profession and province together, so farm unemployment and mine vacancies
    // coexisted (MILESTONE_2, "Measured state").
    let (unemployed, workforce) = pax_engine::systems::labor::unemployment(&world.defs, &last.labour);
    assert!(
        unemployed * 1000 >= workforce * 3 && unemployed * 100 <= workforce * 2,
        "unemployment {unemployed}/{workforce} outside 0.3%..2%"
    );

    // Jobs are not lost over time: employment at year 20 is within 0.5% of year 5
    // (−0.1% when set). The pre-D25 economy lost about 0.5% of its employment a year,
    // ~7% over these 15 years, which showed up as real GDP falling ~9% over 20 years
    // (the "GDP bug"). Years 1-5 still settle after the miners' year-1 famine
    // (MILESTONE_2, "Known issues"), so the band starts at year 5.
    let (then, now) = (employed_year_5.expect("ran past year 5"), employed(&world.defs, &last.labour));
    assert!(now * 1000 >= then * 995, "employment fell from {then} (year 5) to {now} (year 20)");

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

    // Population: 245k..256k (≈ 250.5k when set, 2026-10-09; the births band-aid,
    // D26, stops growth into unemployment, so it no longer drifts up to ≈ 262k).
    assert!((245_000..=256_000).contains(&population), "population {population} outside [245k, 256k]");
}

/// People employed in worker professions, from a day's labour report.
fn employed(defs: &pax_engine::defs::Defs, labour: &[pax_engine::systems::labor::LabourReport]) -> u64 {
    let (unemployed, workforce) = pax_engine::systems::labor::unemployment(defs, labour);
    workforce - unemployed
}
