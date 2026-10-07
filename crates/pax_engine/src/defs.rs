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
    /// The executed price may differ from yesterday's by at most this fraction.
    /// Residual imbalance is rationed pro rata.
    pub max_daily_change: Fixed,
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
}

/// Monthly population change (DECISIONS.md D7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemographicRules {
    /// Monthly growth when life needs are fully met.
    pub growth_rate: Fixed,
    /// Monthly decline at zero life-needs satisfaction (scaled linearly).
    pub starvation_rate: Fixed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Fixed 30-day months until a real calendar is needed.
    pub days_per_month: u32,
    pub market: MarketRules,
    pub firms: FirmRules,
    pub demographics: DemographicRules,
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
}
