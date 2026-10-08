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
    assert!(all.contains("outside [0, 1]"), "{all}");
}

#[test]
fn consumption_commands_need_a_basket_at_load() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
    let mut world = pax_data::load_scenario(&dir).expect("two_states loads").world;
    world.nations.basket.fill(pax_engine::Fixed::ZERO);
    let text = "[[command]]\nday = 1\ntype = \"set_consumption_rate\"\nnation = \"lowland_kingdom\"\nrate = 0.1\n";
    let err = pax_data::parse_commands(&world, text).expect_err("must fail");
    assert!(err.messages[0].contains("has no consumption basket"), "{err}");
}

/// `Welcome.content_hash` (D22) must identify content: stable across loads and
/// paths, and different whenever any one loaded file changes.
#[test]
fn content_hash_is_stable_and_sensitive_to_every_file() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let original = pax_data::load_scenario(&repo.join("scenarios/two_states")).unwrap().content_hash;

    // A copy elsewhere: same layout (scenario.toml says data = "../../data"), new path.
    let tmp = std::env::temp_dir().join(format!("pax-content-hash-{}", std::process::id()));
    let scenario = tmp.join("scenarios/two_states");
    std::fs::create_dir_all(&scenario).unwrap();
    std::fs::create_dir_all(tmp.join("data")).unwrap();
    let files = [
        "scenarios/two_states/scenario.toml",
        "scenarios/two_states/commands.toml",
        "data/goods.toml",
        "data/professions.toml",
        "data/production.toml",
        "data/rules.toml",
    ];
    for f in files {
        std::fs::copy(repo.join(f), tmp.join(f)).unwrap();
    }
    let hash = || pax_data::load_scenario(&scenario).unwrap().content_hash;
    assert_eq!(hash(), original, "the hash must not depend on where the files are");

    for f in files {
        let path = tmp.join(f);
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("{text}\n# a comment changes the content\n")).unwrap();
        assert_ne!(hash(), original, "changing {f} must change the hash");
        std::fs::write(&path, text).unwrap();
    }
    assert_eq!(hash(), original);
    std::fs::remove_dir_all(&tmp).unwrap();
}
