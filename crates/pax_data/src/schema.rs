//! Serde mirrors of the TOML files. Field names here *are* the file format;
//! keep `docs/DATA_FORMAT.md` in sync when changing them.

use std::collections::BTreeMap;
use std::fmt;

use pax_engine::Fixed;
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};

/// A decimal number read exactly into [`Fixed`]. Accepts TOML integers,
/// floats (`0.015`) and strings (`"0.015"`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dec(pub Fixed);

impl<'de> Deserialize<'de> for Dec {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Dec, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Dec;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a decimal number with at most 6 decimal places")
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Dec, E> {
                Fixed::parse_decimal(&v.to_string()).map(Dec).map_err(E::custom)
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Dec, E> {
                Fixed::parse_decimal(&v.to_string()).map(Dec).map_err(E::custom)
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Dec, E> {
                // `Display` for f64 prints the shortest decimal that round-trips
                // and never uses exponent notation, so `0.015` becomes "0.015".
                if !v.is_finite() {
                    return Err(E::custom("number must be finite"));
                }
                Fixed::parse_decimal(&v.to_string()).map(Dec).map_err(E::custom)
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Dec, E> {
                Fixed::parse_decimal(v).map(Dec).map_err(E::custom)
            }
        }
        d.deserialize_any(V)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoodsFile {
    #[serde(default)]
    pub good: Vec<GoodEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoodEntry {
    pub key: String,
    pub base_price: Dec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfessionsFile {
    #[serde(default)]
    pub profession: Vec<ProfessionEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfessionEntry {
    pub key: String,
    pub spend_rate: Dec,
    #[serde(default)]
    pub subsistence: BTreeMap<String, Dec>,
    #[serde(default)]
    pub preference: BTreeMap<String, Dec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionFile {
    #[serde(default)]
    pub producer_type: Vec<ProducerTypeEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerTypeEntry {
    pub key: String,
    pub output: String,
    pub output_per_worker: Dec,
    #[serde(default)]
    pub inputs: BTreeMap<String, Dec>,
    pub worker: String,
    pub owner: String,
    pub labor_share: Dec,
    #[serde(default)]
    pub input_spend_rate: Dec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesFile {
    pub days_per_month: u32,
    pub market: MarketRulesEntry,
    pub firms: FirmRulesEntry,
    pub demographics: DemographicRulesEntry,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketRulesEntry {
    pub max_iterations: u32,
    pub step: Dec,
    pub step_decay_iterations: u32,
    pub tolerance: Dec,
    pub max_daily_change: Dec,
    pub min_stock: Dec,
    pub price_floor: Dec,
    pub price_ceiling: Dec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirmRulesEntry {
    pub wage_stickiness_days: u32,
    pub revenue_smoothing_days: u32,
    pub reserve_days: u32,
    pub dividend_payout_rate: Dec,
    pub target_stock_days: u32,
    pub subsistence_wage_multiple: Dec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicRulesEntry {
    pub growth_rate: Dec,
    pub starvation_rate: Dec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFile {
    pub name: String,
    pub seed: u64,
    /// Definitions directory, relative to the scenario directory.
    pub data: String,
    #[serde(default)]
    pub nation: Vec<NationEntry>,
    #[serde(default)]
    pub market: Vec<MarketEntry>,
    #[serde(default)]
    pub province: Vec<ProvinceEntry>,
    #[serde(default)]
    pub pop: Vec<PopEntry>,
    #[serde(default)]
    pub producer: Vec<ProducerEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketEntry {
    pub key: String,
    /// Owning nation (optional: stateless markets are untaxed).
    #[serde(default)]
    pub nation: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NationEntry {
    pub key: String,
    #[serde(default)]
    pub treasury: Dec,
    #[serde(default)]
    pub income_tax_rate: Dec,
    #[serde(default)]
    pub transfer_rate: Dec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvinceEntry {
    pub key: String,
    pub market: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopEntry {
    pub province: String,
    pub profession: String,
    pub size: u32,
    pub cash: Dec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerEntry {
    #[serde(rename = "type")]
    pub kind: String,
    pub province: String,
    pub capacity: u32,
    pub cash: Dec,
    pub wage: Dec,
    #[serde(default)]
    pub stock: Dec,
}
