//! Stability of the game content (MILESTONE_1 task T2, acceptance criterion A11).
//!
//! Runs `scenarios/two_states` (12 goods, 6 professions, 2 markets) for 20
//! years in release builds (5 in debug, which is ~10x slower) and checks that
//! the economy neither collapses nor degenerates:
//!
//! * every good traded in every market during the final 30 days (a buyer with
//!   full input stock legitimately skips single days);
//! * no price is pinned at a technical bound (D1);
//! * population stays within 5% of its start (the scenario opens slightly out of
//!   equilibrium, so it dips ~2% in the first years before growing past its start);
//! * population-weighted life-needs coverage stays high.

use std::path::PathBuf;

use pax_engine::Fixed;

const YEARS: u64 = if cfg!(debug_assertions) { 5 } else { 20 };

#[test]
fn two_states_is_stable() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
    let scenario = pax_data::load_scenario(&dir).expect("two_states loads");
    let (mut world, commands) = (scenario.world, scenario.commands);
    let goods = world.defs.good_count();
    let start_population = world.population();
    let rules = world.defs.rules.market.clone();

    let days = YEARS * 360;
    let mut traded_recently = vec![false; world.geography.market_count() * goods];
    let mut last = None;
    for day in 0..days {
        let (report, results) = pax_engine::tick::step_with(&mut world, commands.for_day(day));
        assert!(results.iter().all(Result::is_ok), "a logged command was rejected on day {day}");
        if day + 30 >= days {
            for (seen, g) in traded_recently.iter_mut().zip(&report.goods) {
                *seen |= g.traded.is_positive();
            }
        }
        last = Some(report);
    }
    let last = last.expect("ran at least one day");

    for (k, g) in last.goods.iter().enumerate() {
        let (market, good) = (&world.geography.market_keys[k / goods], &world.defs.goods[k % goods].key);
        assert!(traded_recently[k], "{market}/{good} did not trade in the last 30 days of {YEARS} years: {g:?}");
        assert!(
            g.price > rules.price_floor && g.price < rules.price_ceiling,
            "{market}/{good} price pinned at a technical bound: {}",
            g.price
        );
    }

    let end_population = world.population();
    assert!(
        end_population * 100 >= start_population * 95,
        "population collapsed: {start_population} -> {end_population} after {YEARS} years"
    );

    // The engine's own definition, weighted by the sizes the market saw.
    let coverage = last.life_needs.mean().expect("people took part in the last market");
    assert!(coverage >= Fixed::ratio(95, 100), "life-needs coverage {coverage} < 0.95 after {YEARS} years");
}
