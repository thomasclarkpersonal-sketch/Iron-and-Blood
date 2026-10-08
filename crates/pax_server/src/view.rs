//! `DayUpdate` views (M3-3, D22): what a session sees of the world each day.
//!
//! Every session gets the world summary and the nation table. The map, the market
//! panel and the province panel are sent only to sessions that subscribed. The full
//! POP and producer tables are never sent.
//!
//! This module only serialises numbers the engine defines:
//! * `pax_engine::views::ProvinceStats` and `province_pops` for per-province and
//!   per-profession aggregates;
//! * the tick's `DayReport` for spending and markets;
//! * `World`'s own accessors for prices and geography.
//!
//! So the client and `pax_cli report` can't disagree about what a number means.

use flatbuffers::{FlatBufferBuilder, WIPOffset};
use pax_engine::systems::market::GoodReport;
use pax_engine::views::{self, ProvinceStats};
use pax_engine::{DayReport, Fixed, World};
use pax_protocol::wire;

/// What a session asked to see (`Subscribe`), as it arrived: not yet checked.
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

/// What the map shows. The one place the server's supported map modes are listed:
/// [`MapLayer::from_wire`] accepts exactly these, and [`map_view`] matches them
/// exhaustively, so the two can't drift apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MapLayer {
    Hidden,
    Nation,
    Population,
    Unemployment,
    LifeNeeds,
    Militancy,
    Price { good: usize },
}

impl MapLayer {
    fn from_wire(mode: wire::MapMode, good: u16, goods: usize) -> Result<MapLayer, String> {
        use wire::MapMode as M;
        Ok(match mode {
            M::None => MapLayer::Hidden,
            M::Nation => MapLayer::Nation,
            M::Population => MapLayer::Population,
            M::Unemployment => MapLayer::Unemployment,
            M::LifeNeeds => MapLayer::LifeNeeds,
            M::Militancy => MapLayer::Militancy,
            M::Price if usize::from(good) < goods => MapLayer::Price { good: usize::from(good) },
            M::Price => return Err(format!("Subscribe names good {good}, but there are {goods}")),
            // A mode newer than this server: D22 says receivers ignore unknown enum
            // values, so a newer client gets no map rather than a disconnect.
            other => {
                tracing::debug!(mode = other.0, "unknown map mode; showing no map");
                MapLayer::Hidden
            }
        })
    }

    fn to_wire(self) -> (wire::MapMode, u16) {
        use wire::MapMode as M;
        match self {
            MapLayer::Hidden => (M::None, 0),
            MapLayer::Nation => (M::Nation, 0),
            MapLayer::Population => (M::Population, 0),
            MapLayer::Unemployment => (M::Unemployment, 0),
            MapLayer::LifeNeeds => (M::LifeNeeds, 0),
            MapLayer::Militancy => (M::Militancy, 0),
            MapLayer::Price { good } => (M::Price, good as u16),
        }
    }
}

/// A subscription known to be valid for the current world. It can only be made by
/// [`Subscription::checked`], so [`day_update`] never sees an unknown mode or an
/// out-of-range id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckedSubscription {
    layer: MapLayer,
    market: Option<usize>,
    province: Option<usize>,
}

impl Default for CheckedSubscription {
    /// Nothing subscribed: the summary and nation table only.
    fn default() -> Self {
        CheckedSubscription { layer: MapLayer::Hidden, market: None, province: None }
    }
}

impl Subscription {
    /// Valid only if every id it names exists and its map mode is one this server
    /// knows. Anything else is a protocol error (D22).
    pub fn checked(self, world: &World) -> Result<CheckedSubscription, String> {
        let layer = MapLayer::from_wire(self.map_mode, self.map_good, world.defs.good_count())?;
        let markets = world.geography.market_count();
        if let Some(m) = self.market.filter(|&m| m as usize >= markets) {
            return Err(format!("Subscribe names market {m}, but there are {markets}"));
        }
        let provinces = world.geography.province_count();
        if let Some(p) = self.province.filter(|&p| p as usize >= provinces) {
            return Err(format!("Subscribe names province {p}, but there are {provinces}"));
        }
        Ok(CheckedSubscription {
            layer,
            market: self.market.map(|m| m as usize),
            province: self.province.map(|p| p as usize),
        })
    }
}

/// Everything views are built from, for one day: the world, its report, and the
/// day's derived numbers. The sim thread computes the stats and the checkpoint
/// hash once per day and lends them to every session's update.
pub struct DayViews<'w> {
    pub world: &'w World,
    /// The report of the day that just ran; `None` before the first tick.
    pub report: Option<&'w DayReport>,
    pub stats: &'w ProvinceStats,
    /// `World::state_hash` after the day (D10, D22). About 30 ms at 1M POP rows, so
    /// the sim thread computes it once per day and every session's update shares it.
    pub state_hash: u64,
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
    let totals = v.stats.totals();
    // The report's life needs and militancy are the tick's own; before the first tick
    // there is none, and the POP table's current values (the same rules) stand in.
    let (life, militancy) = v.report.map_or((totals.life_needs, totals.militancy), |r| (r.life_needs, r.militancy));
    let spending = |f: fn(&DayReport) -> Fixed| fixed(v.report.map_or(Fixed::ZERO, f));
    wire::WorldSummary::create(
        b,
        &wire::WorldSummaryArgs {
            population: totals.population,
            workforce: totals.workforce,
            unemployed: totals.unemployed,
            household_spending: Some(&spending(|r| r.household_spending)),
            government_spending: Some(&spending(|r| r.government_spending)),
            input_spending: Some(&spending(|r| r.input_spending)),
            wages: Some(&spending(|r| r.payouts.wages)),
            dividends: Some(&spending(|r| r.payouts.dividends)),
            taxes: Some(&spending(|r| r.payouts.taxes)),
            transfers: Some(&spending(|r| r.transfers)),
            deprived: life.deprived,
            life_needs: Some(&fixed(life.mean().unwrap_or(Fixed::ZERO))),
            militancy: Some(&fixed(militancy.mean().unwrap_or(Fixed::ZERO))),
        },
    )
}

fn nation_table<'a>(b: &mut FlatBufferBuilder<'a>, v: &DayViews<'_>) -> WIPOffset<wire::NationTable<'a>> {
    let w = v.world;
    let n = &w.nations;
    let mut population = vec![0u64; n.key.len()];
    for (province, &people) in v.stats.population.iter().enumerate() {
        if let Some(nation) = w.geography.nation_of_market(w.market_of_province(province as u32)) {
            population[nation] += people;
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
    layer: MapLayer,
) -> Option<WIPOffset<wire::MapView<'a>>> {
    let s = v.stats;
    let w = v.world;
    let provinces = 0..s.population.len();
    let values = match layer {
        MapLayer::Hidden => return None,
        MapLayer::Nation => None, // drawn by the client from StaticData
        MapLayer::Population => Some(fixeds(b, provinces.map(|p| Fixed::from_int(s.population[p] as i64)))),
        MapLayer::Unemployment => Some(fixeds(b, provinces.map(|p| s.unemployment_rate(p)))),
        MapLayer::LifeNeeds => Some(fixeds(b, provinces.map(|p| s.life_needs[p].mean().unwrap_or(Fixed::ZERO)))),
        MapLayer::Militancy => Some(fixeds(b, provinces.map(|p| s.militancy[p].mean().unwrap_or(Fixed::ZERO)))),
        MapLayer::Price { good } => Some(fixeds(b, provinces.map(|p| w.price(w.market_of_province(p as u32), good)))),
    };
    let (mode, good) = layer.to_wire();
    Some(wire::MapView::create(b, &wire::MapViewArgs { mode, good, values }))
}

fn market_detail<'a>(
    b: &mut FlatBufferBuilder<'a>,
    v: &DayViews<'_>,
    market: usize,
) -> WIPOffset<wire::MarketDetail<'a>> {
    let goods = v.world.defs.good_count();
    let price = fixeds(b, v.world.prices(market).iter().copied());
    // The report's goods are row-major per market, like prices (DayReport::goods).
    let report = |f: fn(&GoodReport) -> Fixed| -> Vec<Fixed> {
        match v.report {
            Some(r) => r.goods[market * goods..(market + 1) * goods].iter().map(f).collect(),
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
    let groups = views::province_pops(w, province as u32);
    let profession = b.create_vector(&groups.iter().map(|g| g.profession).collect::<Vec<_>>());
    let people: Vec<u32> = groups
        .iter()
        .map(|g| u32::try_from(g.people).expect("a profession's people in one province fit the wire's u32"))
        .collect();
    let people = b.create_vector(&people);
    let cash = fixeds(b, groups.iter().map(|g| g.cash));
    let life_needs = fixeds(b, groups.iter().map(|g| g.life_needs.mean().unwrap_or(Fixed::ZERO)));
    let militancy = fixeds(b, groups.iter().map(|g| g.militancy.mean().unwrap_or(Fixed::ZERO)));
    let pops = wire::PopRows::create(
        b,
        &wire::PopRowsArgs {
            profession: Some(profession),
            people: Some(people),
            cash: Some(cash),
            life_needs: Some(life_needs),
            militancy: Some(militancy),
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
pub fn day_update(v: &DayViews<'_>, sub: &CheckedSubscription, speed: wire::Speed, skipped: u32) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let world = world_summary(&mut b, v);
    let nations = nation_table(&mut b, v);
    let map = map_view(&mut b, v, sub.layer);
    let market = sub.market.map(|m| market_detail(&mut b, v, m));
    let province = sub.province.map(|p| province_detail(&mut b, v, p));
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
        day_update(v, &sub.checked(v.world).expect("a valid subscription"), wire::Speed::Normal, 3)
    }

    fn stats_of(world: &World, report: Option<&DayReport>) -> ProvinceStats {
        ProvinceStats::of(world, report.map(|r| r.labour.as_slice()))
    }

    #[test]
    fn summary_matches_the_engines_own_numbers() {
        let (world, report) = world_after(45);
        let stats = stats_of(&world, Some(&report));
        let v = DayViews { world: &world, report: Some(&report), stats: &stats, state_hash: world.state_hash() };
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
    fn every_update_carries_the_days_state_hash() {
        let (world, report) = world_after(45);
        let stats = stats_of(&world, Some(&report));
        let v = DayViews { world: &world, report: Some(&report), stats: &stats, state_hash: world.state_hash() };
        let frame = update(&v, Subscription::default());
        let u = read_server_message(&frame).unwrap().payload_as_day_update().unwrap();
        assert_eq!(u.state_hash(), world.state_hash());
    }

    #[test]
    fn every_map_mode_has_one_value_per_province() {
        use wire::MapMode as M;
        let (world, report) = world_after(45);
        let stats = stats_of(&world, Some(&report));
        let v = DayViews { world: &world, report: Some(&report), stats: &stats, state_hash: world.state_hash() };
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
        let stats = stats_of(&world, Some(&report));
        let v = DayViews { world: &world, report: Some(&report), stats: &stats, state_hash: world.state_hash() };
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
        let stats = stats_of(&world, None);
        let v = DayViews { world: &world, report: None, stats: &stats, state_hash: world.state_hash() };
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
        ] {
            assert!(bad.checked(&world).is_err(), "{bad:?}");
        }
    }

    /// D22: an enum value newer than this server is ignored, not a protocol error.
    #[test]
    fn an_unknown_map_mode_shows_no_map() {
        let world = two_states().world;
        let sub = Subscription { map_mode: wire::MapMode(200), ..Default::default() }.checked(&world).unwrap();
        let stats = stats_of(&world, None);
        let v = DayViews { world: &world, report: None, stats: &stats, state_hash: 0 };
        let frame = day_update(&v, &sub, wire::Speed::Paused, 0);
        assert!(read_server_message(&frame).unwrap().payload_as_day_update().unwrap().map().is_none());
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
        let sub = Subscription { map_mode: wire::MapMode::LifeNeeds, map_good: 0, market: Some(7), province: Some(11) }
            .checked(&world)
            .unwrap();
        let runs = 20;
        let start = std::time::Instant::now();
        let mut bytes = 0;
        // The state hash is computed once per day and shared by every session (the sim's
        // DayCache), so it is measured separately, not charged to each update.
        let hashing = std::time::Instant::now();
        let state_hash = world.state_hash();
        let hash_ms = hashing.elapsed().as_secs_f64() * 1e3;
        for _ in 0..runs {
            let stats = stats_of(&world, Some(&report));
            let views = DayViews { world: &world, report: Some(&report), stats: &stats, state_hash };
            bytes = day_update(&views, &sub, wire::Speed::Normal, 0).len();
        }
        let ms = start.elapsed().as_secs_f64() * 1e3 / runs as f64;
        println!(
            "{} POP rows, {} provinces, {} markets, {} nations: {ms:.2} ms per full update, {bytes} bytes; state hash {hash_ms:.1} ms once per day",
            world.pops.size.len(),
            world.geography.province_count(),
            world.geography.market_count(),
            world.nations.key.len()
        );
        assert!(ms <= 5.0, "view building took {ms:.2} ms (budget 5 ms)");
        assert!(bytes <= 128 * 1024, "update is {bytes} bytes (budget 128 KiB)");
    }
}
