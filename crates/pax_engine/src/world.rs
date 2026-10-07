//! Simulation state as Struct-of-Arrays tables.
//!
//! The engine uses a hand-rolled ECS-style layout instead of an ECS framework
//! (DECISIONS.md D8): an entity is a dense row index, each component is a
//! column `Vec`, and systems are plain functions over columns. This gives
//!
//! * contiguous, cache-friendly columns (a wage pass touches only `cash`),
//! * a guaranteed iteration order (row order), which is the cornerstone of
//!   determinism,
//! * trivially cheap snapshots (`Clone`) for replay tests.
//!
//! Invariant: every column of a table has the same length. Rows are only added
//! through the `push_*` methods, which maintain it.

use std::sync::Arc;

use crate::defs::{Defs, GoodId, ProducerTypeId, ProfessionId};
use crate::fixed::Fixed;
use crate::hash::StateHasher;
use crate::layout::{LayoutCache, PopLayout};

/// Static map topology. A province belongs to exactly one market (a *state*
/// market in the design docs); M1 has a single nation and no market hierarchy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Geography {
    pub province_keys: Vec<String>,
    /// Market row of each province.
    pub province_market: Vec<u32>,
    pub market_keys: Vec<String>,
    /// Nation that owns each market (`None`: stateless, untaxed), D15.
    /// Either empty (no market belongs to any nation) or exactly one entry per
    /// market, each `Some(n)` naming an existing nation row. The engine checks
    /// this whenever it builds the POP layout ([`World::check_market_nations`]).
    pub market_nation: Vec<Option<u32>>,
}

impl Geography {
    /// Nation of a market, if any.
    pub fn nation_of_market(&self, market: usize) -> Option<usize> {
        self.market_nation.get(market).copied().flatten().map(|n| n as usize)
    }

    pub fn province_count(&self) -> usize {
        self.province_keys.len()
    }

    pub fn market_count(&self) -> usize {
        self.market_keys.len()
    }
}

/// POP table. A POP is a group of people sharing province and profession
/// (culture and religion columns arrive with M2).
///
/// `cash` is the POP's *total* holdings, not per capita: when the POP's size
/// changes the money stays with the survivors (DECISIONS.md D7).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pops {
    pub size: Vec<u32>,
    pub cash: Vec<Fixed>,
    pub profession: Vec<u16>,
    pub province: Vec<u32>,
    /// Life-needs satisfaction from the last consumption pass, in `[0, 1]`:
    /// `minᵢ (bought subsistence goodᵢ / (N γᵢ))`. Drives demographics.
    pub life_needs: Vec<Fixed>,
}

impl Pops {
    pub fn len(&self) -> usize {
        self.size.len()
    }

    pub fn is_empty(&self) -> bool {
        self.size.is_empty()
    }
}

/// Producer table: RGOs and factories (they differ only by recipe).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Producers {
    pub kind: Vec<u16>,
    pub province: Vec<u32>,
    /// Maximum number of workers.
    pub capacity: Vec<u32>,
    /// Workers employed today (set by the labour system).
    pub employed: Vec<u32>,
    pub cash: Vec<Fixed>,
    /// Daily wage per worker.
    pub wage: Vec<Fixed>,
    /// Exponential moving average of daily value added
    /// (sales revenue − input purchases). Wages are a share of it.
    pub value_added_avg: Vec<Fixed>,
    /// Unsold output carried over between days (goods persist; DECISIONS.md D4).
    pub output_stock: Vec<Fixed>,
    /// Input inventory, row-major `[producer * goods + good]`.
    pub input_stock: Vec<Fixed>,
}

impl Producers {
    pub fn len(&self) -> usize {
        self.kind.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kind.is_empty()
    }
}

/// Market table. Prices are row-major `[market * goods + good]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Markets {
    pub price: Vec<Fixed>,
}

/// Nation table (D15). A nation owns markets, levies a flat income tax on wages
/// and dividends paid in them, and pays a daily per-capita transfer from its
/// treasury to its people. Tax and transfer rates are policy *state* (players
/// will change them), so they live here rather than in `Defs`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Nations {
    pub key: Vec<String>,
    /// Outside money held by the state (part of the D5 invariant).
    pub treasury: Vec<Fixed>,
    /// Share of every wage and dividend payment withheld as tax, in `[0, 1]`.
    pub income_tax_rate: Vec<Fixed>,
    /// Share of the treasury paid out each day as transfers to the nation's
    /// POPs, split by size, in `[0, 1]`.
    pub transfer_rate: Vec<Fixed>,
    /// Share of the treasury spent on goods each day (D16), in `[0, 1]`.
    pub consumption_rate: Vec<Fixed>,
    /// Government basket, row-major `[nation * goods + good]`: the share of the
    /// consumption budget spent on each good. Each row sums to exactly 1 when the
    /// nation consumes, and is all zero otherwise.
    pub basket: Vec<Fixed>,
}

impl Nations {
    pub fn len(&self) -> usize {
        self.key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.key.is_empty()
    }
}

/// Initial values for a new nation row.
#[derive(Clone, Debug)]
pub struct NewNation {
    pub key: String,
    pub treasury: Fixed,
    pub income_tax_rate: Fixed,
    pub transfer_rate: Fixed,
    pub consumption_rate: Fixed,
    /// Basket weights by good; normalised to sum to exactly 1 on insertion.
    pub basket: Vec<Fixed>,
}

/// Complete mutable simulation state plus a handle to the static definitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct World {
    pub defs: Arc<Defs>,
    /// Root of all randomness (see [`crate::rng`]).
    pub seed: u64,
    /// Days elapsed since the scenario start.
    pub day: u64,
    pub geography: Geography,
    pub pops: Pops,
    pub producers: Producers,
    pub markets: Markets,
    pub nations: Nations,
    /// Cached POP groupings; derived, not state (see [`crate::layout`]).
    pub layout: LayoutCache,
}

/// Initial values for a new producer row.
#[derive(Clone, Debug)]
pub struct NewProducer {
    pub kind: ProducerTypeId,
    pub province: u32,
    pub capacity: u32,
    pub cash: Fixed,
    pub wage: Fixed,
    pub output_stock: Fixed,
}

impl World {
    /// Creates a world with every market at the goods' base prices.
    pub fn new(defs: Arc<Defs>, geography: Geography, seed: u64) -> World {
        let goods = defs.good_count();
        let mut price = Vec::with_capacity(geography.market_count() * goods);
        for _ in 0..geography.market_count() {
            price.extend(defs.goods.iter().map(|g| g.base_price));
        }
        World {
            defs,
            seed,
            day: 0,
            geography,
            pops: Pops::default(),
            producers: Producers::default(),
            markets: Markets { price },
            nations: Nations::default(),
            layout: LayoutCache::default(),
        }
    }

    pub fn push_pop(&mut self, province: u32, profession: ProfessionId, size: u32, cash: Fixed) -> usize {
        assert!((province as usize) < self.geography.province_count(), "unknown province");
        assert!(profession < self.defs.professions.len(), "unknown profession");
        assert!(!cash.is_negative(), "negative cash");
        let p = &mut self.pops;
        p.size.push(size);
        p.cash.push(cash);
        p.profession.push(u16::try_from(profession).expect("too many professions"));
        p.province.push(province);
        p.life_needs.push(Fixed::ONE);
        self.invalidate_pop_layout();
        self.pops.len() - 1
    }

    /// The cached POP groupings, rebuilt whenever their inputs changed.
    ///
    /// Each call fingerprints the inputs (`layout::fingerprint`, O(N), ~1 ms per
    /// 1M rows) and rebuilds on a mismatch, so callers never see stale data even
    /// if a system forgot [`World::invalidate_pop_layout`] (DECISIONS.md D7).
    /// Debug builds also compare a reused cache with a fresh build. That check
    /// is deliberate, so debug-build timings are not representative.
    pub fn pop_layout(&mut self) -> Arc<PopLayout> {
        if let Some(cached) = &self.layout.0
            && cached.fingerprint() == crate::layout::fingerprint(self)
        {
            debug_assert!(**cached == PopLayout::build(self), "POP layout fingerprint collision");
            return cached.clone();
        }
        let fresh = Arc::new(PopLayout::build(self));
        self.layout.0 = Some(fresh.clone());
        fresh
    }

    /// Drops the cached POP groupings. Optional: [`World::pop_layout`] detects
    /// changed inputs by itself; this just skips one fingerprint comparison.
    pub fn invalidate_pop_layout(&mut self) {
        self.layout.0 = None;
    }

    pub fn push_producer(&mut self, new: NewProducer) -> usize {
        assert!((new.province as usize) < self.geography.province_count(), "unknown province");
        let def = &self.defs.producer_types[new.kind];
        assert!(!new.cash.is_negative() && !new.wage.is_negative(), "negative cash or wage");
        // Seed the value-added average at the level that sustains the opening
        // wage bill, so wages do not collapse on day one.
        let value_added_avg = if def.labor_share.is_positive() {
            new.wage.mul_int(new.capacity as i64).div(def.labor_share)
        } else {
            Fixed::ZERO
        };
        let goods = self.defs.good_count();
        let p = &mut self.producers;
        p.kind.push(u16::try_from(new.kind).expect("too many producer types"));
        p.province.push(new.province);
        p.capacity.push(new.capacity);
        p.employed.push(0);
        p.cash.push(new.cash);
        p.wage.push(new.wage);
        p.value_added_avg.push(value_added_avg);
        p.output_stock.push(new.output_stock);
        p.input_stock.extend(std::iter::repeat_n(Fixed::ZERO, goods));
        p.len() - 1
    }

    /// Reorders POP rows so they are grouped by market, keeping the existing order
    /// within each market (stable).
    ///
    /// The market's parallel passes keep per-job sums only for the markets a job
    /// touches (`MarketRuns`), so grouped rows keep those sums tiny. Correctness
    /// never depends on the order, only speed. Row order is state, so this changes
    /// the state hash unless the rows were already grouped. Call it once when
    /// building a world (the loader does), not mid-game.
    pub fn group_pops_by_market(&mut self) {
        let market: Vec<u32> = self.pops.province.iter().map(|&p| self.geography.province_market[p as usize]).collect();
        let mut order: Vec<usize> = (0..self.pops.len()).collect();
        order.sort_by_key(|&i| market[i]); // stable
        if order.iter().enumerate().all(|(k, &i)| k == i) {
            return;
        }
        // Exhaustive destructuring: adding a column to `Pops` is a compile error
        // here until it is permuted too.
        let Pops { size, cash, profession, province, life_needs } = &mut self.pops;
        fn permute<T: Copy>(column: &mut Vec<T>, order: &[usize]) {
            *column = order.iter().map(|&i| column[i]).collect();
        }
        permute(size, &order);
        permute(cash, &order);
        permute(profession, &order);
        permute(province, &order);
        permute(life_needs, &order);
        self.invalidate_pop_layout();
    }

    /// Checks the market→nation invariant documented on
    /// [`Geography::market_nation`].
    ///
    /// # Panics
    /// With a message naming the broken invariant, rather than an
    /// index-out-of-bounds deep inside a system.
    pub fn check_market_nations(&self) {
        let map = &self.geography.market_nation;
        if map.is_empty() {
            return;
        }
        assert_eq!(
            map.len(),
            self.geography.market_count(),
            "geography.market_nation must be empty or have one entry per market"
        );
        for (m, n) in map.iter().enumerate() {
            if let Some(n) = n {
                assert!(
                    (*n as usize) < self.nations.len(),
                    "market {m} names nation {n}, but only {} exist",
                    self.nations.len()
                );
            }
        }
    }

    /// Adds a nation. Assign it markets through `geography.market_nation`.
    pub fn push_nation(&mut self, new: NewNation) -> usize {
        assert!(!new.treasury.is_negative(), "negative treasury");
        for rate in [new.income_tax_rate, new.transfer_rate, new.consumption_rate] {
            assert!(rate >= Fixed::ZERO && rate <= Fixed::ONE, "nation rate outside [0, 1]");
        }
        let goods = self.defs.good_count();
        assert_eq!(new.basket.len(), goods, "basket must have one weight per good");
        let basket = crate::alloc::allocate(Fixed::ONE, &new.basket).unwrap_or_else(|| vec![Fixed::ZERO; goods]);
        assert!(
            new.consumption_rate.is_zero() || basket.iter().any(|w| w.is_positive()),
            "a consuming nation needs a basket"
        );
        self.invalidate_pop_layout();
        let n = &mut self.nations;
        n.key.push(new.key);
        n.treasury.push(new.treasury);
        n.income_tax_rate.push(new.income_tax_rate);
        n.transfer_rate.push(new.transfer_rate);
        n.consumption_rate.push(new.consumption_rate);
        n.basket.extend(basket);
        n.len() - 1
    }

    pub fn market_of_province(&self, province: u32) -> usize {
        self.geography.province_market[province as usize] as usize
    }

    /// All prices of one market, indexed by good.
    pub fn prices(&self, market: usize) -> &[Fixed] {
        let goods = self.defs.good_count();
        &self.markets.price[market * goods..(market + 1) * goods]
    }

    pub fn price(&self, market: usize, good: GoodId) -> Fixed {
        self.markets.price[market * self.defs.good_count() + good]
    }

    /// Total money held by every agent. Must stay constant across ticks until a
    /// minting mechanic exists (DECISIONS.md D5).
    pub fn total_money(&self) -> Fixed {
        let pops: Fixed = self.pops.cash.iter().copied().sum();
        let producers: Fixed = self.producers.cash.iter().copied().sum();
        let treasuries: Fixed = self.nations.treasury.iter().copied().sum();
        pops + producers + treasuries
    }

    /// Total population.
    pub fn population(&self) -> u64 {
        self.pops.size.iter().map(|&s| s as u64).sum()
    }

    /// Stable 64-bit hash of all mutable state, used by the determinism
    /// harness. Covers every column; static definitions are excluded.
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::default();
        h.u64(self.seed);
        h.u64(self.day);
        let p = &self.pops;
        h.u32s(&p.size);
        h.fixeds(&p.cash);
        h.u16s(&p.profession);
        h.u32s(&p.province);
        h.fixeds(&p.life_needs);
        let f = &self.producers;
        h.u16s(&f.kind);
        h.u32s(&f.province);
        h.u32s(&f.capacity);
        h.u32s(&f.employed);
        h.fixeds(&f.cash);
        h.fixeds(&f.wage);
        h.fixeds(&f.value_added_avg);
        h.fixeds(&f.output_stock);
        h.fixeds(&f.input_stock);
        h.fixeds(&self.markets.price);
        // Scenarios without nations hash exactly as before nations existed.
        if !self.nations.is_empty() {
            let n = &self.nations;
            h.fixeds(&n.treasury);
            h.fixeds(&n.income_tax_rate);
            h.fixeds(&n.transfer_rate);
            h.fixeds(&n.consumption_rate);
            h.fixeds(&n.basket);
        }
        h.finish()
    }
}
