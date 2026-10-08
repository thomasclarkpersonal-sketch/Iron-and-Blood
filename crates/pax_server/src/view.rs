//! `DayUpdate` views (M3-3, D22): what a session sees of the world each day.
//!
//! Every session gets the world summary and the nation table. The map, the market
//! panel and the province panel are sent only to sessions that subscribed. The full
//! POP and producer tables are never sent.
//!
//! The numbers come from the engine's own definitions: `pax_engine::views::ProvinceStats`
//! for per-province aggregates, and the tick's `DayReport` for spending and markets.
//! So the client and `pax_cli report` can't disagree about what a number means.

use flatbuffers::{FlatBufferBuilder, WIPOffset};
use pax_engine::views::ProvinceStats;
use pax_engine::{DayReport, Fixed, World};
use pax_protocol::wire;

/// What a session asked to see (`Subscribe`), already checked against the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subscription {
    pub map_mode: wire::MapMode,
    /// Good for `MapMode::Price`.
    pub map_good: u16,
    pub market: Option<u32>,
    pub province: Option<u32>,
}

impl Default for Subscription {
    fn default() -> Self {
        Subscription { map_mode: wire::MapMode::None, map_good: 0, market: None, province: None }
    }
}

impl Subscription {
    /// A subscription is valid only if every id it names exists and its map mode is
    /// one this server knows. Anything else is a protocol error (D22).
    pub fn checked(self, world: &World) -> Result<Subscription, String> {
        use wire::MapMode as M;
        let known = [M::None, M::Nation, M::Population, M::Unemployment, M::LifeNeeds, M::Militancy, M::Price];
        if !known.contains(&self.map_mode) {
            return Err(format!("unknown map mode {}", self.map_mode.0));
        }
        let goods = world.defs.good_count();
        if self.map_mode == M::Price && usize::from(self.map_good) >= goods {
            return Err(format!("Subscribe names good {}, but there are {goods}", self.map_good));
        }
        let markets = world.geography.market_count();
        if let Some(m) = self.market.filter(|&m| m as usize >= markets) {
            return Err(format!("Subscribe names market {m}, but there are {markets}"));
        }
        let provinces = world.geography.province_count();
        if let Some(p) = self.province.filter(|&p| p as usize >= provinces) {
            return Err(format!("Subscribe names province {p}, but there are {provinces}"));
        }
        Ok(self)
    }
}

/// Days between state-hash checkpoints (D23): saves verify against them, and
/// `DayUpdate.state_hash` is sent only on these days.
pub const CHECKPOINT_DAYS: u64 = 30;

/// Everything views are built from, for one day. Computed once and shared by every
/// session's update.
pub struct DayViews<'w> {
    pub world: &'w World,
    /// The report of the day that just ran; `None` before the first tick.
    pub report: Option<&'w DayReport>,
    pub stats: ProvinceStats,
    /// `World::state_hash` on checkpoint days only: about 30 ms at 1M POP rows, far
    /// over the 5 ms view budget, so it isn't computed every day (D22, D23).
    pub state_hash: Option<u64>,
}

impl<'w> DayViews<'w> {
    pub fn new(world: &'w World, report: Option<&'w DayReport>) -> Self {
        let stats = ProvinceStats::of(world, report.map(|r| r.labour.as_slice()));
        let state_hash = world.day.is_multiple_of(CHECKPOINT_DAYS).then(|| world.state_hash());
        DayViews { world, report, stats, state_hash }
    }

    fn market_of(&self, province: usize) -> usize {
        self.world.geography.province_market[province] as usize
    }
}

fn fixed(v: Fixed) -> wire::Fixed {
    wire::Fixed::new(v.raw())
}

fn fixeds<'a>(
    b: &mut FlatBufferBuilder<'a>,
    values: impl Iterator<Item = Fixed>,
) -> WIPOffset<flatbuffers::Vector<'a, wire::Fixed>> {
    let v: Vec<wire::Fixed> = values.map(fixed).collect();
    b.create_vector(&v)
}

fn world_summary<'a>(b: &mut FlatBufferBuilder<'a>, v: &DayViews<'_>) -> WIPOffset<wire::WorldSummary<'a>> {
    let s = &v.stats;
    let (unemployed, workforce) = s.unemployment.iter().fold((0, 0), |(u, w), &(pu, pw)| (u + pu, w + pw));
    // Before the first tick there is no report: spending is zero, and life needs and
    // militancy come from the POP table's current values.
    let (life, militancy) = match v.report {
        Some(r) => (r.life_needs, r.militancy),
        None => s.life_needs.iter().zip(&s.militancy).fold(
            Default::default(),
            |(mut l, mut m): (
                pax_engine::systems::market::LifeNeedsSummary,
                pax_engine::systems::market::MilitancySummary,
            ),
             (pl, pm)| {
                l.people += pl.people;
                l.deprived += pl.deprived;
                l.weighted_raw += pl.weighted_raw;
                m.people += pm.people;
                m.weighted_raw += pm.weighted_raw;
                (l, m)
            },
        ),
    };
    let r = v.report;
    let get = |f: fn(&DayReport) -> Fixed| r.map_or(Fixed::ZERO, f);
    wire::WorldSummary::create(
        b,
        &wire::WorldSummaryArgs {
            population: s.population.iter().sum(),
            workforce,
            unemployed,
            household_spending: Some(&fixed(get(|r| r.household_spending))),
            government_spending: Some(&fixed(get(|r| r.government_spending))),
            input_spending: Some(&fixed(get(|r| r.input_spending))),
            wages: Some(&fixed(get(|r| r.payouts.wages))),
            dividends: Some(&fixed(get(|r| r.payouts.dividends))),
            taxes: Some(&fixed(get(|r| r.payouts.taxes))),
            transfers: Some(&fixed(get(|r| r.transfers))),
            deprived: life.deprived,
            life_needs: Some(&fixed(life.mean().unwrap_or(Fixed::ZERO))),
            militancy: Some(&fixed(militancy.mean().unwrap_or(Fixed::ZERO))),
        },
    )
}

fn nation_table<'a>(b: &mut FlatBufferBuilder<'a>, v: &DayViews<'_>) -> WIPOffset<wire::NationTable<'a>> {
    let n = &v.world.nations;
    let geo = &v.world.geography;
    let mut population = vec![0u64; n.key.len()];
    for (province, &people) in v.stats.population.iter().enumerate() {
        if let Some(nation) = geo.market_nation[v.market_of(province)] {
            population[nation as usize] += people;
        }
    }
    let treasury = fixeds(b, n.treasury.iter().copied());
    let income_tax_rate = fixeds(b, n.income_tax_rate.iter().copied());
    let transfer_rate = fixeds(b, n.transfer_rate.iter().copied());
    let consumption_rate = fixeds(b, n.consumption_rate.iter().copied());
    let population = b.create_vector(&population);
    wire::NationTable::create(
        b,
        &wire::NationTableArgs {
            treasury: Some(treasury),
            income_tax_rate: Some(income_tax_rate),
            transfer_rate: Some(transfer_rate),
            consumption_rate: Some(consumption_rate),
            population: Some(population),
        },
    )
}

fn map_view<'a>(
    b: &mut FlatBufferBuilder<'a>,
    v: &DayViews<'_>,
    sub: &Subscription,
) -> Option<WIPOffset<wire::MapView<'a>>> {
    use wire::MapMode as M;
    let s = &v.stats;
    let provinces = 0..s.population.len();
    let goods = v.world.defs.good_count();
    let values = match sub.map_mode {
        M::None => return None,
        M::Nation => None, // drawn by the client from StaticData
        M::Population => Some(fixeds(b, provinces.map(|p| Fixed::from_int(s.population[p] as i64)))),
        M::Unemployment => Some(fixeds(b, provinces.map(|p| s.unemployment_rate(p)))),
        M::LifeNeeds => Some(fixeds(b, provinces.map(|p| s.life_needs[p].mean().unwrap_or(Fixed::ZERO)))),
        M::Militancy => Some(fixeds(b, provinces.map(|p| s.militancy[p].mean().unwrap_or(Fixed::ZERO)))),
        M::Price => {
            let good = usize::from(sub.map_good);
            Some(fixeds(b, provinces.map(|p| v.world.markets.price[v.market_of(p) * goods + good])))
        }
        // Subscription::checked admits only the modes above.
        other => unreachable!("unchecked map mode {}", other.0),
    };
    Some(wire::MapView::create(b, &wire::MapViewArgs { mode: sub.map_mode, good: sub.map_good, values }))
}

fn market_detail<'a>(
    b: &mut FlatBufferBuilder<'a>,
    v: &DayViews<'_>,
    market: usize,
) -> WIPOffset<wire::MarketDetail<'a>> {
    let goods = v.world.defs.good_count();
    let range = market * goods..(market + 1) * goods;
    let price = fixeds(b, v.world.markets.price[range.clone()].iter().copied());
    let report = |f: fn(&pax_engine::systems::market::GoodReport) -> Fixed| -> Vec<Fixed> {
        match v.report {
            Some(r) => r.goods[range.clone()].iter().map(f).collect(),
            None => vec![Fixed::ZERO; goods],
        }
    };
    let supply = fixeds(b, report(|g| g.supply).into_iter());
    let demand = fixeds(b, report(|g| g.demand).into_iter());
    let traded = fixeds(b, report(|g| g.traded).into_iter());
    wire::MarketDetail::create(
        b,
        &wire::MarketDetailArgs {
            market: market as u32,
            price: Some(price),
            supply: Some(supply),
            demand: Some(demand),
            traded: Some(traded),
        },
    )
}

fn province_detail<'a>(
    b: &mut FlatBufferBuilder<'a>,
    v: &DayViews<'_>,
    province: usize,
) -> WIPOffset<wire::ProvinceDetail<'a>> {
    let w = v.world;
    // POPs by profession: a POP's identity is (province, profession) (D7). Rows that
    // share it between month-end compactions are shown merged, as compaction will merge them.
    let professions = w.defs.professions.len();
    let mut people = vec![0u64; professions];
    let mut cash = vec![0i128; professions];
    let mut life = vec![0i128; professions];
    let mut militancy = vec![0i128; professions];
    let p = &w.pops;
    for i in (0..p.size.len()).filter(|&i| p.province[i] as usize == province && p.size[i] > 0) {
        let (c, n) = (p.profession[i] as usize, p.size[i] as i128);
        people[c] += p.size[i] as u64;
        cash[c] += p.cash[i].raw() as i128;
        life[c] += n * p.life_needs[i].raw() as i128;
        militancy[c] += n * p.militancy[i].raw() as i128;
    }
    let present: Vec<usize> = (0..professions).filter(|&c| people[c] > 0).collect();
    let mean = |sum: i128, c: usize| Fixed::from_raw((sum / people[c] as i128) as i64);
    let profession = b.create_vector(&present.iter().map(|&c| c as u16).collect::<Vec<_>>());
    let pop_people =
        b.create_vector(&present.iter().map(|&c| u32::try_from(people[c]).unwrap_or(u32::MAX)).collect::<Vec<_>>());
    let pop_cash = fixeds(
        b,
        present.iter().map(|&c| Fixed::from_raw(i64::try_from(cash[c]).expect("province cash fits in Fixed"))),
    );
    let pop_life = fixeds(b, present.iter().map(|&c| mean(life[c], c)));
    let pop_militancy = fixeds(b, present.iter().map(|&c| mean(militancy[c], c)));
    let pops = wire::PopRows::create(
        b,
        &wire::PopRowsArgs {
            profession: Some(profession),
            people: Some(pop_people),
            cash: Some(pop_cash),
            life_needs: Some(pop_life),
            militancy: Some(pop_militancy),
        },
    );

    let pools: Vec<_> = v
        .report
        .map(|r| r.labour.iter().filter(|l| l.province as usize == province).copied().collect())
        .unwrap_or_default();
    let pool_profession = b.create_vector(&pools.iter().map(|l| l.profession).collect::<Vec<_>>());
    let workforce = b.create_vector(&pools.iter().map(|l| l.workforce).collect::<Vec<_>>());
    let jobs = b.create_vector(&pools.iter().map(|l| l.jobs).collect::<Vec<_>>());
    let employed = b.create_vector(&pools.iter().map(|l| l.employed).collect::<Vec<_>>());
    let labour = wire::LabourRows::create(
        b,
        &wire::LabourRowsArgs {
            profession: Some(pool_profession),
            workforce: Some(workforce),
            jobs: Some(jobs),
            employed: Some(employed),
        },
    );

    let f = &w.producers;
    let here: Vec<usize> = (0..f.kind.len()).filter(|&i| f.province[i] as usize == province).collect();
    let producer_type = b.create_vector(&here.iter().map(|&i| f.kind[i]).collect::<Vec<_>>());
    let capacity = b.create_vector(&here.iter().map(|&i| f.capacity[i]).collect::<Vec<_>>());
    let producer_employed = b.create_vector(&here.iter().map(|&i| f.employed[i]).collect::<Vec<_>>());
    let wage = fixeds(b, here.iter().map(|&i| f.wage[i]));
    let producer_cash = fixeds(b, here.iter().map(|&i| f.cash[i]));
    let producers = wire::ProducerRows::create(
        b,
        &wire::ProducerRowsArgs {
            producer_type: Some(producer_type),
            capacity: Some(capacity),
            employed: Some(producer_employed),
            wage: Some(wage),
            cash: Some(producer_cash),
        },
    );
    wire::ProvinceDetail::create(
        b,
        &wire::ProvinceDetailArgs {
            province: province as u32,
            pops: Some(pops),
            labour: Some(labour),
            producers: Some(producers),
        },
    )
}

/// One session's `DayUpdate` frame for the current day.
pub fn day_update(v: &DayViews<'_>, sub: &Subscription, speed: wire::Speed, skipped: u32) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let world = world_summary(&mut b, v);
    let nations = nation_table(&mut b, v);
    let map = map_view(&mut b, v, sub);
    let market = sub.market.map(|m| market_detail(&mut b, v, m as usize));
    let province = sub.province.map(|p| province_detail(&mut b, v, p as usize));
    let update = wire::DayUpdate::create(
        &mut b,
        &wire::DayUpdateArgs {
            day: v.world.day,
            speed,
            skipped,
            state_hash: v.state_hash,
            world: Some(world),
            nations: Some(nations),
            map,
            market,
            province,
        },
    );
    let msg = wire::ServerMessage::create(
        &mut b,
        &wire::ServerMessageArgs {
            payload_type: wire::ServerPayload::DayUpdate,
            payload: Some(update.as_union_value()),
        },
    );
    wire::finish_size_prefixed_server_message_buffer(&mut b, msg);
    b.finished_data().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_engine::systems::labor;
    use pax_protocol::read_server_message;

    fn two_states() -> pax_data::Scenario {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        pax_data::load_scenario(&dir).unwrap()
    }

    /// `two_states` after `days` days of its command log, and the last day's report.
    fn world_after(days: u64) -> (World, DayReport) {
        let mut s = two_states();
        let mut report = None;
        for _ in 0..days {
            report = Some(pax_data::step_logged(&mut s.world, &s.commands).0);
        }
        (s.world, report.expect("at least one day"))
    }

    fn update(v: &DayViews<'_>, sub: Subscription) -> Vec<u8> {
        day_update(v, &sub, wire::Speed::Normal, 3)
    }

    #[test]
    fn summary_matches_the_engines_own_numbers() {
        let (world, report) = world_after(45);
        let v = DayViews::new(&world, Some(&report));
        let frame = update(&v, Subscription::default());
        let u = read_server_message(&frame).unwrap().payload_as_day_update().unwrap();
        assert_eq!((u.day(), u.skipped()), (world.day, 3));
        assert!(u.map().is_none() && u.market().is_none() && u.province().is_none());

        let w = u.world().unwrap();
        let population: u64 = world.pops.size.iter().map(|&n| n as u64).sum();
        let (unemployed, workforce) = labor::unemployment(&world.defs, &report.labour);
        assert_eq!((w.population(), w.unemployed(), w.workforce()), (population, unemployed, workforce));
        assert_eq!(w.household_spending().unwrap().raw(), report.household_spending.raw());
        assert_eq!(w.taxes().unwrap().raw(), report.payouts.taxes.raw());
        assert_eq!(w.life_needs().unwrap().raw(), report.life_needs.mean().unwrap().raw());
        assert_eq!(w.militancy().unwrap().raw(), report.militancy.mean().unwrap().raw());

        let n = u.nations().unwrap();
        let rates: Vec<i64> = n.income_tax_rate().unwrap().iter().map(|r| r.raw()).collect();
        assert_eq!(rates, world.nations.income_tax_rate.iter().map(|r| r.raw()).collect::<Vec<_>>());
        // Every market in two_states belongs to a nation, so nation populations add up to the total.
        assert_eq!(n.population().unwrap().iter().sum::<u64>(), population);
    }

    #[test]
    fn the_state_hash_is_sent_on_checkpoint_days_only() {
        for (days, expected) in [(45, false), (60, true)] {
            let (world, report) = world_after(days);
            let v = DayViews::new(&world, Some(&report));
            let frame = update(&v, Subscription::default());
            let u = read_server_message(&frame).unwrap().payload_as_day_update().unwrap();
            assert_eq!(u.state_hash(), expected.then(|| world.state_hash()), "day {}", world.day);
        }
    }

    #[test]
    fn every_map_mode_has_one_value_per_province() {
        use wire::MapMode as M;
        let (world, report) = world_after(45);
        let v = DayViews::new(&world, Some(&report));
        let provinces = world.geography.province_count();
        for mode in [M::Population, M::Unemployment, M::LifeNeeds, M::Militancy, M::Price] {
            let frame = update(&v, Subscription { map_mode: mode, map_good: 2, ..Default::default() });
            let msg = read_server_message(&frame).unwrap();
            let map = msg.payload_as_day_update().unwrap().map().unwrap();
            let values: Vec<i64> = map.values().unwrap().iter().map(|f| f.raw()).collect();
            assert_eq!((map.mode(), values.len()), (mode, provinces), "{mode:?}");
            match mode {
                M::Population => {
                    assert_eq!(values.iter().sum::<i64>(), v.stats.population.iter().sum::<u64>() as i64 * 1_000_000)
                }
                M::Price => {
                    let goods = world.defs.good_count();
                    for (p, &value) in values.iter().enumerate() {
                        let market = world.geography.province_market[p] as usize;
                        assert_eq!(value, world.markets.price[market * goods + 2].raw());
                    }
                }
                _ => assert!(values.iter().all(|&x| (0..=1_000_000).contains(&x)), "{mode:?} is a fraction"),
            }
        }
        // Nation mode is drawn from StaticData: a MapView without values.
        let frame = update(&v, Subscription { map_mode: M::Nation, ..Default::default() });
        let msg = read_server_message(&frame).unwrap();
        assert!(msg.payload_as_day_update().unwrap().map().unwrap().values().is_none());
    }

    #[test]
    fn market_and_province_panels_match_the_world() {
        let (world, report) = world_after(45);
        let v = DayViews::new(&world, Some(&report));
        let frame = update(&v, Subscription { market: Some(1), province: Some(0), ..Default::default() });
        let msg = read_server_message(&frame).unwrap();
        let u = msg.payload_as_day_update().unwrap();

        let m = u.market().unwrap();
        let goods = world.defs.good_count();
        assert_eq!(m.market(), 1);
        for g in 0..goods {
            let r = &report.goods[goods + g];
            assert_eq!(m.price().unwrap().get(g).raw(), world.markets.price[goods + g].raw());
            assert_eq!(
                (m.supply().unwrap().get(g).raw(), m.traded().unwrap().get(g).raw()),
                (r.supply.raw(), r.traded.raw())
            );
        }

        let p = u.province().unwrap();
        let pops = p.pops().unwrap();
        let people: u64 = pops.people().unwrap().iter().map(u64::from).sum();
        assert_eq!(people, v.stats.population[0]);
        // One row per profession present: POP identity is (province, profession) (D7).
        let professions: Vec<u16> = pops.profession().unwrap().iter().collect();
        assert!(professions.windows(2).all(|w| w[0] < w[1]));
        let producers = (0..world.producers.kind.len()).filter(|&i| world.producers.province[i] == 0).count();
        assert_eq!(p.producers().unwrap().capacity().unwrap().len(), producers);
        assert!(!p.labour().unwrap().workforce().unwrap().is_empty());
    }

    #[test]
    fn before_the_first_tick_spending_is_zero_but_the_map_works() {
        let world = two_states().world;
        let v = DayViews::new(&world, None);
        let sub = Subscription { map_mode: wire::MapMode::Population, market: Some(0), ..Default::default() };
        let frame = update(&v, sub);
        let msg = read_server_message(&frame).unwrap();
        let u = msg.payload_as_day_update().unwrap();
        assert_eq!(u.world().unwrap().household_spending().unwrap().raw(), 0);
        assert!(u.world().unwrap().population() > 0);
        assert_eq!(u.map().unwrap().values().unwrap().len(), world.geography.province_count());
        assert_eq!(u.market().unwrap().traded().unwrap().iter().map(|t| t.raw()).sum::<i64>(), 0);
    }

    #[test]
    fn subscriptions_naming_missing_ids_are_refused() {
        let world = two_states().world;
        let ok = Subscription { map_mode: wire::MapMode::Price, map_good: 11, market: Some(1), province: Some(3) };
        assert!(ok.checked(&world).is_ok());
        for bad in [
            Subscription { map_mode: wire::MapMode::Price, map_good: 12, ..Default::default() },
            Subscription { market: Some(2), ..Default::default() },
            Subscription { province: Some(4), ..Default::default() },
            Subscription { map_mode: wire::MapMode(200), ..Default::default() },
        ] {
            assert!(bad.checked(&world).is_err(), "{bad:?}");
        }
    }

    /// The M3 budget at D13's long-term scale (about 1M POP rows, 3,000 markets, 200
    /// nations): one session's full update (summary, nations, a map mode, a market and
    /// a province) is built in ≤ 5 ms and is ≤ 128 KiB.
    /// Timing-sensitive, so it's run by hand:
    /// `cargo test -p pax_server --release -- --ignored view_building_budget --nocapture`
    #[test]
    #[ignore]
    fn view_building_budget() {
        let base = two_states().world;
        // 1,500 copies of the 2-market map = 3,000 markets; POP rows scaled to about 1M.
        let regions = 1_500;
        let scale = (1_000_000 / (base.pops.size.len() as u32 * regions)).max(1);
        let mut world = pax_data::bench::replicate_with_nations(&base, scale, regions, Some(200));
        let report = pax_engine::step(&mut world);
        assert!(!world.day.is_multiple_of(CHECKPOINT_DAYS), "measure an ordinary day, not a checkpoint");
        let sub = Subscription { map_mode: wire::MapMode::LifeNeeds, map_good: 0, market: Some(7), province: Some(11) };
        let runs = 20;
        let start = std::time::Instant::now();
        let mut bytes = 0;
        for _ in 0..runs {
            let views = DayViews::new(&world, Some(&report));
            bytes = day_update(&views, &sub, wire::Speed::Normal, 0).len();
        }
        let ms = start.elapsed().as_secs_f64() * 1e3 / runs as f64;
        println!(
            "{} POP rows, {} provinces, {} markets, {} nations: {ms:.2} ms per full update, {bytes} bytes",
            world.pops.size.len(),
            world.geography.province_count(),
            world.geography.market_count(),
            world.nations.key.len()
        );
        assert!(ms <= 5.0, "view building took {ms:.2} ms (budget 5 ms)");
        assert!(bytes <= 128 * 1024, "update is {bytes} bytes (budget 128 KiB)");
    }
}
