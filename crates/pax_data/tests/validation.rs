//! Loader validation: bad data must be rejected with every problem reported.

use std::sync::Arc;

use pax_data::{DefSources, parse_defs, parse_scenario};
use pax_engine::Fixed;

const GOODS: &str = r#"
[[good]]
key = "grain"
base_price = 1.0
[[good]]
key = "cloth"
base_price = "2.5"
"#;

const PROFESSIONS: &str = r#"
[[profession]]
key = "farmer"
spend_rate = 0.2
subsistence = { grain = 0.01 }
preference = { grain = 1, cloth = 2 }
"#;

const PRODUCTION: &str = r#"
[[producer_type]]
key = "farm"
output = "grain"
output_per_worker = 0.02
worker = "farmer"
owner = "farmer"
labor_share = 0.7
"#;

const RULES: &str = include_str!("../../../data/rules.toml");

fn defs(goods: &str, professions: &str, production: &str) -> Result<pax_engine::defs::Defs, pax_data::LoadError> {
    parse_defs(&DefSources { goods, professions, production, rules: RULES })
}

#[test]
fn valid_definitions_load_and_normalise_preferences() {
    let d = defs(GOODS, PROFESSIONS, PRODUCTION).unwrap();
    assert_eq!(d.goods[1].base_price, Fixed::parse_decimal("2.5").unwrap());
    // Weights 1 : 2 normalise to exactly 1; the rounding unit goes to the
    // larger remainder (2/3).
    let beta = &d.professions[0].preference;
    assert_eq!(beta[0] + beta[1], Fixed::ONE);
    assert_eq!(beta[0], Fixed::parse_decimal("0.333333").unwrap());
}

#[test]
fn reports_all_errors_at_once() {
    let goods = format!("{GOODS}\n[[good]]\nkey = \"grain\"\nbase_price = 0\n");
    let production = PRODUCTION
        .replace("output = \"grain\"", "output = \"steel\"")
        .replace("labor_share = 0.7", "labor_share = 1.5");
    let err = defs(&goods, PROFESSIONS, &production).unwrap_err();
    let all = err.messages.join("\n");
    assert!(all.contains("duplicate good key 'grain'"), "{all}");
    assert!(all.contains("base_price must be > 0"), "{all}");
    assert!(all.contains("unknown good 'steel'"), "{all}");
    assert!(all.contains("labor_share must be in [0, 1]"), "{all}");
}

#[test]
fn rejects_hidden_precision_loss() {
    let goods = GOODS.replace("base_price = 1.0", "base_price = 1.0000001");
    let err = defs(&goods, PROFESSIONS, PRODUCTION).unwrap_err();
    assert!(err.messages[0].contains("more than 6 decimal places"), "{err}");
}

#[test]
fn rejects_unknown_fields() {
    let goods = GOODS.replace("base_price = 1.0", "base_price = 1.0\nprice = 3");
    assert!(defs(&goods, PROFESSIONS, PRODUCTION).is_err());
}

#[test]
fn scenario_references_are_checked() {
    let d = Arc::new(defs(GOODS, PROFESSIONS, PRODUCTION).unwrap());
    let text = r#"
name = "broken"
seed = 1
data = "."
[[market]]
key = "m"
[[province]]
key = "p"
market = "nowhere"
[[pop]]
province = "p"
profession = "priest"
size = 10
cash = -1
"#;
    let err = parse_scenario(d, text).map(|_| ()).expect_err("must fail");
    let all = err.messages.join("\n");
    assert!(all.contains("unknown market 'nowhere'"), "{all}");
    assert!(all.contains("unknown profession 'priest'"), "{all}");
    assert!(all.contains("cash must be >= 0"), "{all}");
}

#[test]
fn command_logs_are_validated() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
    let world = pax_data::load_scenario(&dir).expect("two_states loads").world;
    let text = r#"
[[command]]
day = 1
type = "set_income_tax"
nation = "atlantis"
rate = 0.1

[[command]]
day = 2
type = "declare_war"
nation = "lowland_kingdom"
rate = 0.1

[[command]]
day = 3
type = "set_transfer_rate"
nation = "lowland_kingdom"
rate = 2
"#;
    let all = pax_data::parse_commands(&world, text).expect_err("must fail").messages.join("\n");
    assert!(all.contains("unknown nation 'atlantis'"), "{all}");
    assert!(all.contains("unknown type 'declare_war'"), "{all}");
    assert!(all.contains("rate must be in [0, 1]"), "{all}");
}

#[test]
fn consumption_commands_need_a_basket_at_load() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
    let mut world = pax_data::load_scenario(&dir).expect("two_states loads").world;
    world.nations.basket.fill(pax_engine::Fixed::ZERO);
    let text = "[[command]]\nday = 1\ntype = \"set_consumption_rate\"\nnation = \"lowland_kingdom\"\nrate = 0.1\n";
    let err = pax_data::parse_commands(&world, text).expect_err("must fail");
    assert!(err.messages[0].contains("no consumption basket"), "{err}");
}
