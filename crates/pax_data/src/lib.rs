//! # pax_data
//!
//! Loads TOML definition files and scenarios, validates them, and builds a
//! [`pax_engine::World`]. This is the only crate that touches the filesystem
//! for game data; the engine stays IO-free (`AGENTS.md` §2).
//!
//! File layout and every field are documented in `docs/DATA_FORMAT.md`.
//!
//! Numbers are converted to [`Fixed`] *exactly*: a TOML float such as `0.015`
//! is formatted with Rust's shortest round-trip representation and parsed as a
//! decimal, and more than 6 decimal places is a load error rather than a
//! silent rounding. Load-time conversion therefore cannot introduce
//! platform-dependent values (DECISIONS.md D3).

pub mod golden;
mod schema;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pax_engine::alloc::allocate;
use pax_engine::defs::{
    Defs, DemographicRules, FirmRules, GoodDef, MarketRules, ProducerTypeDef, ProfessionDef, Rules,
};
use pax_engine::world::{Geography, NewNation, NewProducer};
use pax_engine::{Fixed, World};

pub use schema::Dec;
use schema::{GoodsFile, ProductionFile, ProfessionsFile, RulesFile, ScenarioFile};

/// All problems found while loading. Validation keeps going after the first
/// error so modders see every mistake in one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    pub messages: Vec<String>,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} data error(s):", self.messages.len())?;
        for m in &self.messages {
            writeln!(f, "  - {m}")?;
        }
        Ok(())
    }
}

impl std::error::Error for LoadError {}

impl LoadError {
    fn single(message: String) -> LoadError {
        LoadError { messages: vec![message] }
    }
}

/// Raw text of the four definition files.
pub struct DefSources<'a> {
    pub goods: &'a str,
    pub professions: &'a str,
    pub production: &'a str,
    pub rules: &'a str,
}

fn read(path: &Path) -> Result<String, LoadError> {
    std::fs::read_to_string(path).map_err(|e| LoadError::single(format!("{}: {e}", path.display())))
}

fn parse<T: serde::de::DeserializeOwned>(name: &str, text: &str) -> Result<T, LoadError> {
    toml::from_str(text).map_err(|e| LoadError::single(format!("{name}: {e}")))
}

/// Loads `goods.toml`, `professions.toml`, `production.toml` and `rules.toml`
/// from a definitions directory.
pub fn load_defs(dir: &Path) -> Result<Defs, LoadError> {
    let goods = read(&dir.join("goods.toml"))?;
    let professions = read(&dir.join("professions.toml"))?;
    let production = read(&dir.join("production.toml"))?;
    let rules = read(&dir.join("rules.toml"))?;
    parse_defs(&DefSources { goods: &goods, professions: &professions, production: &production, rules: &rules })
}

/// A loaded scenario: its display name and the initial world.
pub struct Scenario {
    pub name: String,
    pub world: World,
}

/// Loads a scenario directory (containing `scenario.toml`) and its definitions.
pub fn load_scenario(dir: &Path) -> Result<Scenario, LoadError> {
    let text = read(&dir.join("scenario.toml"))?;
    let scenario: ScenarioFile = parse("scenario.toml", &text)?;
    let defs_dir: PathBuf = dir.join(&scenario.data);
    let defs = load_defs(&defs_dir)?;
    let world = build_world(Arc::new(defs), &scenario)?;
    Ok(Scenario { name: scenario.name, world })
}

/// Parses a scenario from text against already-loaded definitions.
pub fn parse_scenario(defs: Arc<Defs>, text: &str) -> Result<World, LoadError> {
    let scenario: ScenarioFile = parse("scenario.toml", text)?;
    build_world(defs, &scenario)
}

/// Collects validation errors.
#[derive(Default)]
struct Errors(Vec<String>);

impl Errors {
    fn check(&mut self, ok: bool, msg: impl FnOnce() -> String) {
        if !ok {
            self.0.push(msg());
        }
    }

    fn finish<T>(self, value: T) -> Result<T, LoadError> {
        if self.0.is_empty() { Ok(value) } else { Err(LoadError { messages: self.0 }) }
    }
}

/// Builds a key → index map, reporting duplicates.
fn index<'a>(kind: &str, keys: impl Iterator<Item = &'a str>, errors: &mut Errors) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    for (i, k) in keys.enumerate() {
        if map.insert(k.to_string(), i).is_some() {
            errors.0.push(format!("duplicate {kind} key '{k}'"));
        }
    }
    map
}

fn lookup(map: &BTreeMap<String, usize>, kind: &str, key: &str, context: &str, errors: &mut Errors) -> usize {
    match map.get(key) {
        Some(&i) => i,
        None => {
            errors.0.push(format!("{context}: unknown {kind} '{key}'"));
            0
        }
    }
}

fn in_range(v: Fixed, lo: Fixed, hi: Fixed) -> bool {
    v >= lo && v <= hi
}

/// Parses and validates definitions from text.
pub fn parse_defs(src: &DefSources<'_>) -> Result<Defs, LoadError> {
    let goods_file: GoodsFile = parse("goods.toml", src.goods)?;
    let profs_file: ProfessionsFile = parse("professions.toml", src.professions)?;
    let prod_file: ProductionFile = parse("production.toml", src.production)?;
    let rules_file: RulesFile = parse("rules.toml", src.rules)?;
    let mut errors = Errors::default();

    let good_ix = index("good", goods_file.good.iter().map(|g| g.key.as_str()), &mut errors);
    let prof_ix = index("profession", profs_file.profession.iter().map(|p| p.key.as_str()), &mut errors);
    index("producer type", prod_file.producer_type.iter().map(|p| p.key.as_str()), &mut errors);
    errors.check(!goods_file.good.is_empty(), || "goods.toml: at least one good is required".into());
    errors.check(!profs_file.profession.is_empty(), || "professions.toml: at least one profession is required".into());

    let goods: Vec<GoodDef> = goods_file
        .good
        .iter()
        .map(|g| {
            errors.check(g.base_price.0.is_positive(), || format!("good '{}': base_price must be > 0", g.key));
            GoodDef { key: g.key.clone(), base_price: g.base_price.0 }
        })
        .collect();
    let n_goods = goods.len();

    let per_good = |map: &BTreeMap<String, Dec>, what: &str, owner: &str, errors: &mut Errors| -> Vec<Fixed> {
        let mut v = vec![Fixed::ZERO; n_goods];
        for (key, value) in map {
            let g = lookup(&good_ix, "good", key, &format!("profession '{owner}' {what}"), errors);
            errors.check(!value.0.is_negative(), || format!("profession '{owner}' {what}.{key}: must be >= 0"));
            v[g] = value.0;
        }
        v
    };

    let professions: Vec<ProfessionDef> = profs_file
        .profession
        .iter()
        .map(|p| {
            errors.check(in_range(p.spend_rate.0, Fixed::EPSILON, Fixed::ONE), || {
                format!("profession '{}': spend_rate must be in (0, 1]", p.key)
            });
            let subsistence = per_good(&p.subsistence, "subsistence", &p.key, &mut errors);
            let weights = per_good(&p.preference, "preference", &p.key, &mut errors);
            // Normalise β to sum to exactly 1 so LES spending equals the budget.
            let preference = match allocate(Fixed::ONE, &weights) {
                Some(b) => b,
                None => {
                    errors.0.push(format!("profession '{}': preference weights must not all be zero", p.key));
                    vec![Fixed::ZERO; n_goods]
                }
            };
            ProfessionDef { key: p.key.clone(), spend_rate: p.spend_rate.0, subsistence, preference }
        })
        .collect();

    let producer_types: Vec<ProducerTypeDef> = prod_file
        .producer_type
        .iter()
        .map(|t| {
            let ctx = format!("producer type '{}'", t.key);
            let output = lookup(&good_ix, "good", &t.output, &ctx, &mut errors);
            let worker = lookup(&prof_ix, "profession", &t.worker, &ctx, &mut errors);
            let owner = lookup(&prof_ix, "profession", &t.owner, &ctx, &mut errors);
            errors.check(t.output_per_worker.0.is_positive(), || format!("{ctx}: output_per_worker must be > 0"));
            errors.check(in_range(t.labor_share.0, Fixed::ZERO, Fixed::ONE), || {
                format!("{ctx}: labor_share must be in [0, 1]")
            });
            errors.check(in_range(t.input_spend_rate.0, Fixed::ZERO, Fixed::ONE), || {
                format!("{ctx}: input_spend_rate must be in [0, 1]")
            });
            let inputs = t
                .inputs
                .iter()
                .map(|(key, a)| {
                    errors.check(a.0.is_positive(), || format!("{ctx}: input '{key}' coefficient must be > 0"));
                    (lookup(&good_ix, "good", key, &ctx, &mut errors), a.0)
                })
                .collect();
            ProducerTypeDef {
                key: t.key.clone(),
                output,
                output_per_worker: t.output_per_worker.0,
                inputs,
                worker,
                owner,
                labor_share: t.labor_share.0,
                input_spend_rate: t.input_spend_rate.0,
            }
        })
        .collect();

    let r = &rules_file;
    let m = &r.market;
    errors.check(r.days_per_month >= 1, || "rules: days_per_month must be >= 1".into());
    errors.check(m.max_iterations >= 1, || "rules.market: max_iterations must be >= 1".into());
    errors.check(m.step_decay_iterations >= 1, || "rules.market: step_decay_iterations must be >= 1".into());
    errors.check(in_range(m.step.0, Fixed::EPSILON, Fixed::ONE), || "rules.market: step must be in (0, 1]".into());
    errors.check(!m.tolerance.0.is_negative(), || "rules.market: tolerance must be >= 0".into());
    errors.check(!m.min_stock.0.is_negative(), || "rules.market: min_stock must be >= 0".into());
    errors.check(in_range(m.max_daily_change.0, Fixed::EPSILON, Fixed::ONE - Fixed::EPSILON), || {
        "rules.market: max_daily_change must be in (0, 1)".into()
    });
    errors.check(m.price_floor.0.is_positive() && m.price_ceiling.0 > m.price_floor.0, || {
        "rules.market: need 0 < price_floor < price_ceiling".into()
    });
    errors.check(
        r.firms.wage_stickiness_days >= 1 && r.firms.revenue_smoothing_days >= 1 && r.firms.target_stock_days >= 1,
        || "rules.firms: wage_stickiness_days, revenue_smoothing_days and target_stock_days must be >= 1".into(),
    );
    errors.check(in_range(r.firms.dividend_payout_rate.0, Fixed::ZERO, Fixed::ONE), || {
        "rules.firms: dividend_payout_rate must be in [0, 1]".into()
    });
    errors.check(!r.firms.subsistence_wage_multiple.0.is_negative(), || {
        "rules.firms: subsistence_wage_multiple must be >= 0".into()
    });
    errors.check(in_range(r.demographics.growth_rate.0, Fixed::ZERO, Fixed::ONE), || {
        "rules.demographics: growth_rate must be in [0, 1]".into()
    });
    errors.check(in_range(r.demographics.starvation_rate.0, Fixed::ZERO, Fixed::ONE), || {
        "rules.demographics: starvation_rate must be in [0, 1]".into()
    });
    errors.check(in_range(r.demographics.mobility_rate.0, Fixed::ZERO, Fixed::ONE), || {
        "rules.demographics: mobility_rate must be in [0, 1]".into()
    });
    for g in &goods {
        errors.check(in_range(g.base_price, m.price_floor.0, m.price_ceiling.0), || {
            format!("good '{}': base_price outside price_floor..price_ceiling", g.key)
        });
    }
    let rules = Rules {
        days_per_month: r.days_per_month,
        market: MarketRules {
            max_iterations: m.max_iterations,
            step: m.step.0,
            step_decay_iterations: m.step_decay_iterations,
            tolerance: m.tolerance.0,
            max_daily_change: m.max_daily_change.0,
            min_stock: m.min_stock.0,
            price_floor: m.price_floor.0,
            price_ceiling: m.price_ceiling.0,
        },
        firms: FirmRules {
            wage_stickiness_days: r.firms.wage_stickiness_days,
            revenue_smoothing_days: r.firms.revenue_smoothing_days,
            reserve_days: r.firms.reserve_days,
            dividend_payout_rate: r.firms.dividend_payout_rate.0,
            target_stock_days: r.firms.target_stock_days,
            subsistence_wage_multiple: r.firms.subsistence_wage_multiple.0,
        },
        demographics: DemographicRules {
            growth_rate: r.demographics.growth_rate.0,
            starvation_rate: r.demographics.starvation_rate.0,
            mobility_rate: r.demographics.mobility_rate.0,
        },
    };

    errors.finish(Defs { goods, professions, producer_types, rules })
}

fn build_world(defs: Arc<Defs>, s: &ScenarioFile) -> Result<World, LoadError> {
    let mut errors = Errors::default();
    let good_ix: BTreeMap<String, usize> = defs.goods.iter().enumerate().map(|(i, g)| (g.key.clone(), i)).collect();
    let nation_ix = index("nation", s.nation.iter().map(|n| n.key.as_str()), &mut errors);
    let market_ix = index("market", s.market.iter().map(|m| m.key.as_str()), &mut errors);
    let province_ix = index("province", s.province.iter().map(|p| p.key.as_str()), &mut errors);
    let prof_ix: BTreeMap<String, usize> =
        defs.professions.iter().enumerate().map(|(i, p)| (p.key.clone(), i)).collect();
    let type_ix: BTreeMap<String, usize> =
        defs.producer_types.iter().enumerate().map(|(i, t)| (t.key.clone(), i)).collect();
    errors.check(!s.market.is_empty(), || "scenario: at least one market is required".into());

    let geography = Geography {
        province_keys: s.province.iter().map(|p| p.key.clone()).collect(),
        province_market: s
            .province
            .iter()
            .map(|p| lookup(&market_ix, "market", &p.market, &format!("province '{}'", p.key), &mut errors) as u32)
            .collect(),
        market_keys: s.market.iter().map(|m| m.key.clone()).collect(),
        market_nation: s
            .market
            .iter()
            .map(|m| {
                m.nation
                    .as_ref()
                    .map(|n| lookup(&nation_ix, "nation", n, &format!("market '{}'", m.key), &mut errors) as u32)
            })
            .collect(),
    };
    let nations: Vec<NewNation> = s
        .nation
        .iter()
        .map(|n| {
            let ctx = format!("nation '{}'", n.key);
            errors.check(!n.treasury.0.is_negative(), || format!("{ctx}: treasury must be >= 0"));
            for (name, v) in [
                ("income_tax_rate", n.income_tax_rate.0),
                ("transfer_rate", n.transfer_rate.0),
                ("consumption_rate", n.consumption_rate.0),
            ] {
                errors.check(in_range(v, Fixed::ZERO, Fixed::ONE), || format!("{ctx}: {name} must be in [0, 1]"));
            }
            let mut basket = vec![Fixed::ZERO; defs.good_count()];
            for (good, w) in &n.basket {
                let g = lookup(&good_ix, "good", good, &format!("{ctx} basket"), &mut errors);
                errors.check(!w.0.is_negative(), || format!("{ctx}: basket.{good} must be >= 0"));
                basket[g] = w.0.max(Fixed::ZERO);
            }
            errors.check(n.consumption_rate.0.is_zero() || basket.iter().any(|w| w.is_positive()), || {
                format!("{ctx}: consumption_rate > 0 needs a basket with a positive weight")
            });
            NewNation {
                key: n.key.clone(),
                treasury: n.treasury.0,
                income_tax_rate: n.income_tax_rate.0,
                transfer_rate: n.transfer_rate.0,
                consumption_rate: n.consumption_rate.0,
                basket,
            }
        })
        .collect();

    // Resolve every reference before mutating the world, so a bad file never
    // trips the engine's own assertions.
    let pops: Vec<_> = s
        .pop
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let ctx = format!("pop #{}", i + 1);
            let province = lookup(&province_ix, "province", &p.province, &ctx, &mut errors) as u32;
            let profession = lookup(&prof_ix, "profession", &p.profession, &ctx, &mut errors);
            errors.check(!p.cash.0.is_negative(), || format!("{ctx}: cash must be >= 0"));
            (province, profession, p.size, p.cash.0)
        })
        .collect();
    let producers: Vec<_> = s
        .producer
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let ctx = format!("producer #{} ('{}')", i + 1, p.kind);
            let kind = lookup(&type_ix, "producer type", &p.kind, &ctx, &mut errors);
            let province = lookup(&province_ix, "province", &p.province, &ctx, &mut errors) as u32;
            for (name, v) in [("cash", p.cash.0), ("wage", p.wage.0), ("stock", p.stock.0)] {
                errors.check(!v.is_negative(), || format!("{ctx}: {name} must be >= 0"));
            }
            NewProducer {
                kind,
                province,
                capacity: p.capacity,
                cash: p.cash.0,
                wage: p.wage.0,
                output_stock: p.stock.0,
            }
        })
        .collect();
    let mut world = errors.finish(World::new(defs, geography, s.seed))?;
    for n in nations {
        world.push_nation(n);
    }
    for (province, profession, size, cash) in pops {
        world.push_pop(province, profession, size, cash);
    }
    for p in producers {
        world.push_producer(p);
    }
    // Engine-owned layout step: POP rows grouped by market (stable).
    world.group_pops_by_market();
    Ok(world)
}
