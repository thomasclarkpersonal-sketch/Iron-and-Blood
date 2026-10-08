//! Loader validation: bad data must be rejected with every problem reported.

mod common;

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
    let original = pax_data::load_scenario(&common::repo().join("scenarios/two_states")).unwrap().content_hash;
    let copy = common::TempScenario::copy("two_states", "content-hash");
    let hash = || pax_data::load_scenario(&copy.dir).unwrap().content_hash;
    assert_eq!(hash(), original, "the hash must not depend on where the files are");

    // Every file the loader reads for two_states (golden.hashes isn't one of them).
    for f in [
        "scenarios/two_states/scenario.toml",
        "scenarios/two_states/commands.toml",
        "scenarios/two_states/map/provinces.toml",
        "scenarios/two_states/map/provinces.png",
        "data/goods.toml",
        "data/professions.toml",
        "data/production.toml",
        "data/rules.toml",
    ] {
        let path = copy.path(f);
        let bytes = std::fs::read(&path).unwrap();
        // A TOML comment, or bytes after a PNG's end chunk: valid files, new content.
        let mut changed = bytes.clone();
        changed.extend_from_slice(b"\n# a comment changes the content\n");
        std::fs::write(&path, changed).unwrap();
        assert_ne!(hash(), original, "changing {f} must change the hash");
        std::fs::write(&path, bytes).unwrap();
    }
    assert_eq!(hash(), original);
}

/// A save names its scenario and that scenario's content hash; a save whose scenario
/// files changed since is refused rather than replayed into a different game (D23).
#[test]
fn a_save_for_changed_content_is_refused() {
    use pax_data::save::{SaveData, load};
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scenario = repo.join("scenarios/two_states");
    let world = pax_data::load_scenario(&scenario).unwrap().world;
    let dir = std::env::temp_dir().join(format!("pax-save-test-{}", std::process::id()));
    let save = |content_hash: u64, name: &str| {
        let data = SaveData { scenario: scenario.clone(), content_hash, day: 0, checkpoints: vec![], commands: vec![] };
        let path = dir.join(name);
        data.write(&path, &world).unwrap();
        path
    };
    let right = pax_data::load_scenario(&scenario).unwrap().content_hash;
    assert_eq!(load(&save(right, "ok.toml")).unwrap().scenario.world.day, 0);
    let error = load(&save(right ^ 1, "changed.toml")).err().expect("changed content must be refused");
    assert!(error.to_string().contains("files changed since this game was saved"), "{error}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Snapshot loading (M3-6b) and full replay reach the same game, and replay still
/// catches a tampered checkpoint or a snapshot from a different game.
#[test]
fn snapshot_loads_match_replays_and_tampering_is_caught() {
    use pax_data::save::{SaveData, SavedCommand, load, load_by_replay};
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scenario_dir = repo.join("scenarios/two_states");
    let mut s = pax_data::load_scenario(&scenario_dir).unwrap();
    let mut commands = Vec::new();
    let mut checkpoints = Vec::new();
    for _ in 0..95 {
        let day = s.world.day;
        let scripted = s.commands.for_day(day).to_vec();
        let player = if day == 10 {
            vec![pax_engine::Command::SetIncomeTax { nation: 1, rate: pax_engine::Fixed::from_raw(160_000) }]
        } else {
            vec![]
        };
        let all: Vec<_> = scripted.iter().chain(&player).copied().collect();
        pax_engine::tick::step_with(&mut s.world, &all);
        commands.extend(scripted.into_iter().map(|command| SavedCommand { day, player: None, command }));
        commands.extend(player.into_iter().map(|command| SavedCommand { day, player: Some(0), command }));
        if s.world.day.is_multiple_of(pax_data::save::CHECKPOINT_DAYS) {
            checkpoints.push((s.world.day, s.world.state_hash()));
        }
    }
    let dir = std::env::temp_dir().join(format!("pax-snapshot-save-{}", std::process::id()));
    let save = SaveData {
        scenario: scenario_dir.clone(),
        content_hash: s.content_hash,
        day: s.world.day,
        checkpoints,
        commands,
    };
    let path = dir.join("g.toml");
    save.write(&path, &s.world).unwrap();

    let fast = load(&path).unwrap();
    let slow = load_by_replay(&path).unwrap();
    assert_eq!(fast.scenario.world.state_hash(), s.world.state_hash());
    assert_eq!(slow.scenario.world.state_hash(), s.world.state_hash());
    assert_eq!(fast.save, slow.save);

    // A snapshot from another game (a different day) can't be swapped in.
    let other = SaveData { day: 0, checkpoints: vec![], commands: vec![], ..save.clone() };
    other.write(&dir.join("other.toml"), &pax_data::load_scenario(&scenario_dir).unwrap().world).unwrap();
    std::fs::copy(dir.join("other.world"), dir.join("g.world")).unwrap();
    assert!(load(&path).err().unwrap().to_string().contains("not this save's snapshot"));

    // Replay catches a tampered checkpoint even though the snapshot path doesn't look at it.
    save.write(&path, &s.world).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let at = text.find("state_hash = \"0x").unwrap() + "state_hash = \"0x".len();
    let flipped = if &text[at..=at] == "0" { "1" } else { "0" };
    std::fs::write(&path, format!("{}{flipped}{}", &text[..at], &text[at + 1..])).unwrap();
    assert!(load_by_replay(&path).err().unwrap().to_string().contains("different state on day 30"));
    std::fs::remove_dir_all(&dir).unwrap();
}
