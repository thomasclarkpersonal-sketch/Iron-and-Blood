//! `pax_cli report`: economy health indicators (MILESTONE_1 task T3).
//!
//! Presentation only. Values are converted to `f64` for display, which is
//! allowed outside simulation state (DECISIONS.md D3). Each row summarises one
//! period of `--every` days:
//!
//! * **GDP:** mean daily household consumption spending. In this closed economy
//!   without government or investment, final demand equals expenditure GDP.
//! * **Price index:** Laspeyres index of end-of-period prices, using the basket
//!   traded on day 1 and day-1 prices = 100: `Σ pₜ q₀ / Σ p₀ q₀ × 100`.
//! * **Real GDP:** GDP deflated by the price index, in day-1 prices.
//! * **Unemployment:** unemployed ÷ workforce at period end. It counts only
//!   professions that some producer type employs; owner professions are
//!   outside the labour force.
//! * **Wage share:** wages ÷ (wages + dividends) paid during the period.
//! * **Life needs:** population-weighted mean `life_needs`, and the share of
//!   people whose subsistence was not fully met, at period end.

use std::process::ExitCode;

use pax_engine::{DayReport, Fixed, World, step};

fn f(v: Fixed) -> f64 {
    v.raw() as f64 / pax_engine::fixed::SCALE as f64
}

/// Σ price × base quantity over every market and good.
fn basket_value(prices: &[Fixed], basket: &[f64]) -> f64 {
    prices.iter().zip(basket).map(|(&p, &q)| f(p) * q).sum()
}

pub fn run(mut world: World, days: u64, every: u64) -> Result<ExitCode, String> {
    let worker_professions: Vec<bool> =
        (0..world.defs.professions.len()).map(|c| world.defs.producer_types.iter().any(|t| t.worker == c)).collect();

    println!(
        "{:>5} {:>10} {:>12} {:>9} {:>12} {:>8} {:>8} {:>10} {:>9}",
        "day", "population", "GDP/day", "prices", "real GDP", "unempl%", "wage%", "lifeneeds", "deprived%"
    );

    let mut base: Option<(Vec<f64>, f64)> = None; // (basket q0, Σ p0 q0)
    let (mut spending, mut wages, mut dividends, mut n) = (0.0, 0.0, 0.0, 0u64);
    for _ in 0..days {
        let report: DayReport = step(&mut world);
        if base.is_none() {
            let basket: Vec<f64> = report.goods.iter().map(|g| f(g.traded)).collect();
            let value = basket_value(&world.markets.price, &basket);
            base = Some((basket, value));
        }
        spending += f(report.household_spending);
        wages += f(report.payouts.wages);
        dividends += f(report.payouts.dividends);
        n += 1;

        let day = report.day + 1;
        if !day.is_multiple_of(every) && day != days {
            continue;
        }

        let (basket, base_value) = base.as_ref().expect("set on day 1");
        let index =
            if *base_value > 0.0 { basket_value(&world.markets.price, basket) / base_value * 100.0 } else { 100.0 };
        let gdp = spending / n as f64;
        let real_gdp = if index > 0.0 { gdp / index * 100.0 } else { 0.0 };

        let (mut workforce, mut unemployed) = (0u64, 0u64);
        for pool in report.labour.iter().filter(|p| worker_professions[p.profession as usize]) {
            workforce += pool.workforce;
            unemployed += pool.unemployed();
        }
        let unemployment = if workforce > 0 { unemployed as f64 / workforce as f64 * 100.0 } else { 0.0 };
        let wage_share = if wages + dividends > 0.0 { wages / (wages + dividends) * 100.0 } else { 0.0 };

        let population = world.population();
        let (mut weighted_life, mut deprived) = (0.0, 0u64);
        for (&size, &life) in world.pops.size.iter().zip(&world.pops.life_needs) {
            weighted_life += size as f64 * f(life);
            if life < Fixed::ONE {
                deprived += size as u64;
            }
        }
        let (life_mean, deprived_pct) = if population > 0 {
            (weighted_life / population as f64, deprived as f64 / population as f64 * 100.0)
        } else {
            (0.0, 0.0)
        };

        println!(
            "{day:>5} {population:>10} {gdp:>12.2} {index:>9.1} {real_gdp:>12.2} {unemployment:>8.1} {wage_share:>8.1} {life_mean:>10.3} {deprived_pct:>9.1}"
        );
        (spending, wages, dividends, n) = (0.0, 0.0, 0.0, 0);
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_runs_on_reference_scenario() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mini_valley");
        let world = pax_data::load_scenario(&dir).expect("scenario loads").world;
        assert_eq!(run(world, 40, 20).unwrap(), ExitCode::SUCCESS);
    }

    #[test]
    fn laspeyres_basket_value() {
        let prices = [Fixed::from_int(2), Fixed::from_int(3)];
        assert_eq!(basket_value(&prices, &[10.0, 1.0]), 23.0);
    }
}
