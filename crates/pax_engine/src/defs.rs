//! Static definitions: goods, professions, producer types and tunable rules.
//!
//! Definitions are loaded once (by `pax_data`) and never change during a game,
//! so they live behind an `Arc` and are excluded from the state hash. All
//! per-good vectors are dense and indexed by good id, so lookups are array
//! reads rather than hash-map probes.

use crate::fixed::Fixed;

/// Dense index of a good (`0..defs.goods.len()`).
pub type GoodId = usize;
/// Dense index of a profession.
pub type ProfessionId = usize;
/// Dense index of a producer type.
pub type ProducerTypeId = usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoodDef {
    /// Stable string key used in data files (e.g. `"grain"`).
    pub key: String,
    /// Opening price in every market. Only a starting point: there is no
    /// anchoring to it afterwards (DECISIONS.md D1).
    pub base_price: Fixed,
}

/// Consumption behaviour of a profession: a Stone-Geary / Linear Expenditure
/// System (DECISIONS.md D2).
///
/// For a POP of `N` people with daily budget `Y` facing prices `p`:
///
/// * subsistence cost `C = Σₖ pₖ γₖ` (per capita)
/// * if `Y ≥ N·C`: `xᵢ = N γᵢ + βᵢ (Y − N C) / pᵢ`  (*comfortable* regime)
/// * else:         `xᵢ = γᵢ Y / C`                 (*deprived* regime: the whole
///   budget buys a scaled-down subsistence basket)
///
/// `γ` is what Victoria 2 called *life needs*; `β` spreads discretionary income
/// over everyday and luxury goods, so demand elasticity emerges from income
/// instead of being hard-coded per tier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfessionDef {
    pub key: String,
    /// Fraction of cash a POP budgets for consumption each day, in `(0, 1]`.
    /// The rest is held as a precautionary buffer.
    pub spend_rate: Fixed,
    /// `γ`: subsistence quantity per person per day, indexed by good.
    pub subsistence: Vec<Fixed>,
    /// `β`: share of discretionary budget per good, indexed by good.
    /// Invariant (enforced by the loader): sums to exactly `Fixed::ONE`.
    pub preference: Vec<Fixed>,
}

impl ProfessionDef {
    /// Per-capita subsistence cost `C = Σₖ pₖ γₖ` at `prices` (indexed by good),
    /// rounded up so that spending the LES demand never exceeds the budget (D2).
    /// The single definition, used by consumer demand and the wage floor (D6).
    pub fn subsistence_cost(&self, prices: &[Fixed]) -> Fixed {
        self.subsistence.iter().zip(prices).filter(|(g, _)| g.is_positive()).map(|(g, p)| g.mul_ceil(*p)).sum()
    }
}

/// A production technology: fixed-coefficient (Leontief) recipe operated by a
/// single worker profession (DECISIONS.md D6).
///
/// Daily output `= min(employed × output_per_worker, minⱼ inventoryⱼ / aⱼ)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProducerTypeDef {
    pub key: String,
    pub output: GoodId,
    pub output_per_worker: Fixed,
    /// `aⱼ`: units of input good `j` per unit of output.
    pub inputs: Vec<(GoodId, Fixed)>,
    /// Profession that supplies labour.
    pub worker: ProfessionId,
    /// Profession that receives dividends (owners).
    pub owner: ProfessionId,
    /// Target share of revenue paid as wages; wages drift toward it slowly.
    pub labor_share: Fixed,
    /// Fraction of cash the producer may spend on inputs per day.
    pub input_spend_rate: Fixed,
}

/// Price-discovery parameters (DECISIONS.md D1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketRules {
    /// Maximum tâtonnement iterations per market per day.
    pub max_iterations: u32,
    /// Initial step size λ in `p ← p (1 + λ z)`.
    pub step: Fixed,
    /// λ is divided by `1 + k / step_decay_iterations` at iteration `k`, which
    /// damps oscillation around the clearing price.
    pub step_decay_iterations: u32,
    /// Stop early once every |z| is at most this.
    pub tolerance: Fixed,
    /// Opt-in per-good adaptive step (D1, accepted as opt-in, off by default). Each good's
    /// step grows ×1.25 (capped at 1) while its excess demand keeps its sign,
    /// and halves when the sign flips. `step` is the starting value, and
    /// `step_decay_iterations` is ignored. Off by default: existing results
    /// are unchanged.
    pub adaptive_step: bool,
    /// The executed price may differ from yesterday's by at most this fraction.
    /// Residual imbalance is rationed pro rata.
    pub max_daily_change: Fixed,
    /// Stock below this many units is not offered and does not count as stock
    /// for price discovery. Without it, rounding dust (10⁻⁶ units) made a market
    /// look supplied: discovery kept raising the price against almost nothing
    /// until it hit the ceiling, which killed whole supply chains.
    pub min_stock: Fixed,
    /// Technical price bounds that keep fixed-point products in range.
    /// They are not economic anchors and should never bind in normal play.
    pub price_floor: Fixed,
    pub price_ceiling: Fixed,
}

/// Firm wage and dividend behaviour (DECISIONS.md D6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirmRules {
    /// Wages close `1 / wage_stickiness_days` of the gap to their target per day.
    pub wage_stickiness_days: u32,
    /// Revenue moving average horizon, in days.
    pub revenue_smoothing_days: u32,
    /// Cash kept back before dividends, in days of current wage bill.
    pub reserve_days: u32,
    /// Fraction of cash above the reserve paid to owners each day.
    pub dividend_payout_rate: Fixed,
    /// Producers stop producing once unsold output reaches this many days of
    /// full-capacity production (inventory targeting).
    pub target_stock_days: u32,
    /// Floor on a producer's *target* wage: this multiple of a worker's daily
    /// subsistence cost (`Σ γ p` for the worker profession at market prices).
    /// Without a floor, wages, reservation prices and prices can chase each other
    /// to zero in a chain whose buyer purchases a fixed quantity (MILESTONE_1
    /// "reservation-price drift"). The floor anchors prices to the cost of labour.
    pub subsistence_wage_multiple: Fixed,
}

/// Monthly population change (DECISIONS.md D7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemographicRules {
    /// Monthly growth when life needs are fully met.
    pub growth_rate: Fixed,
    /// Monthly decline at zero life-needs satisfaction (scaled linearly).
    pub starvation_rate: Fixed,
    /// Share of a province's unemployed workers of one profession who move each
    /// month to a profession with vacancies in the same province (D18).
    pub mobility_rate: Fixed,
    /// Share of a province's surplus workers of one profession who migrate each
    /// month to provinces of the same market with vacancies (D20).
    pub migration_rate: Fixed,
    /// Share of a pool's surplus workers who move each month to vacancies of another
    /// profession in another province of the same market (D25).
    pub occupational_migration_rate: Fixed,
    /// **Band-aid** (D26): a worker POP's births scale with its pool's employed
    /// share, a stand-in for "people can't raise children without an income" until
    /// POPs model dependents (POP_SYSTEM.md, "Future: dependents").
    pub births_need_employment: bool,
}

/// Monthly militancy dynamics (D19).
///
/// `m ← clamp(m + rise × (1 − life_needs) + tax_weight × tax_rate − decay × m, 0, 1)`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoliticsRules {
    /// Monthly rise at zero life-needs satisfaction.
    pub militancy_rise: Fixed,
    /// Monthly rise per unit of income tax rate.
    pub militancy_tax_weight: Fixed,
    /// Share of militancy that fades each month.
    pub militancy_decay: Fixed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Fixed 30-day months until a real calendar is needed.
    pub days_per_month: u32,
    pub market: MarketRules,
    pub firms: FirmRules,
    pub demographics: DemographicRules,
    pub politics: PoliticsRules,
}

/// Everything static about a game. Shared immutably by all systems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Defs {
    pub goods: Vec<GoodDef>,
    pub professions: Vec<ProfessionDef>,
    pub producer_types: Vec<ProducerTypeDef>,
    pub rules: Rules,
}

impl Defs {
    pub fn good_count(&self) -> usize {
        self.goods.len()
    }

    /// The *worker professions*: those some producer type employs. Owner-only
    /// professions never take jobs (D18). This is the single definition, used by
    /// mobility and by reports.
    pub fn worker_professions(&self) -> WorkerProfessions {
        WorkerProfessions(
            (0..self.professions.len()).map(|c| self.producer_types.iter().any(|t| t.worker == c)).collect(),
        )
    }
}

/// Which professions are worker professions ([`Defs::worker_professions`]), by
/// profession id. A type of its own, so a rule that needs this set (such as
/// `labor::pool_unemployment`) can't be handed some other per-profession mask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerProfessions(Vec<bool>);

impl WorkerProfessions {
    pub fn contains(&self, profession: usize) -> bool {
        self.0[profession]
    }
}
