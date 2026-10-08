//! Every `Dictionary` key and event tag the bridge hands to GDScript, and every value
//! GDScript hands back (wire enums, policy names), named once.
//!
//! GDScript reads them through `client/pax_keys.gd` (`PaxKeys.DAY` rather than
//! `"day"`, with `const PaxKeys := preload("res://pax_keys.gd")`), which is
//! generated from [`ALL`]. A test fails when the checked-in file
//! drifts from this list or from the schema. Regenerate it with
//! `UPDATE_PAX_KEYS=1 cargo test -p pax_godot keys`.

/// Declares each key as a `pub const` and lists them all, in order, in [`ALL`].
///
/// A key always holds one type of value. Inside a table Dictionary (`NATION_TABLE`,
/// `POPS`, `LABOUR`, `PRODUCERS`, a market), a key names a column: an array with one
/// entry per row.
macro_rules! keys {
    ($($(#[$doc:meta])* $name:ident = $value:literal,)*) => {
        $($(#[$doc])* pub const $name: &str = $value;)*
        /// `(GDScript constant name, value)` for every key.
        pub const ALL: &[(&str, &str)] = &[$((stringify!($name), $value),)*];
    };
}

keys! {
    // Event tags (the `TYPE` of every event).
    WELCOME = "welcome",
    REJECTED = "rejected",
    DAY_UPDATE = "day_update",
    COMMAND_RESULT = "command_result",
    SERVER_STATE = "server_state",
    PONG = "pong",
    SAVE_RESULT = "save_result",
    SAVE_LIST = "save_list",
    GOODBYE = "goodbye",
    LOBBY_STATE = "lobby_state",
    UNKNOWN = "unknown",
    /// Not a server message: the connection ended (reason in `REASON`).
    CLOSED = "closed",

    // Every event.
    TYPE = "type",

    // Welcome.
    PROTOCOL_MINOR = "protocol_minor",
    PLAYER = "player",
    /// Welcome: present it to `PaxClient.resume` to reclaim the seat after a drop.
    RESUME_TOKEN = "resume_token",
    NATION = "nation",
    DAY = "day",
    SPEED = "speed",
    SCENARIO = "scenario",
    CONTENT_HASH = "content_hash",
    MAP_HASH = "map_hash",
    MAP_DIR = "map_dir",
    GOODS = "goods",
    PROFESSIONS = "professions",
    PRODUCER_TYPES = "producer_types",
    PROVINCES = "provinces",
    PROVINCE_MARKET = "province_market",
    MARKETS = "markets",
    /// The nation keys (Welcome).
    NATIONS = "nations",
    NATION_MARKETS = "nation_markets",

    // Rejected, Goodbye, closed.
    REASON = "reason",

    // DayUpdate.
    SKIPPED = "skipped",
    STATE_HASH = "state_hash",
    WORLD = "world",
    /// The NationTable: one column per field, one entry per nation (DayUpdate).
    NATION_TABLE = "nation_table",
    MAP = "map",
    /// The market panel, a Dictionary, or `null` (DayUpdate).
    MARKET = "market",
    /// The province panel, a Dictionary, or `null` (DayUpdate).
    PROVINCE = "province",
    /// The market a panel shows (inside `MARKET`).
    MARKET_ID = "market_id",
    /// The province a panel shows (inside `PROVINCE`).
    PROVINCE_ID = "province_id",
    // WorldSummary.
    POPULATION = "population",
    WORKFORCE = "workforce",
    UNEMPLOYED = "unemployed",
    HOUSEHOLD_SPENDING = "household_spending",
    GOVERNMENT_SPENDING = "government_spending",
    INPUT_SPENDING = "input_spending",
    WAGES = "wages",
    DIVIDENDS = "dividends",
    TAXES = "taxes",
    TRANSFERS = "transfers",
    DEPRIVED = "deprived",
    LIFE_NEEDS = "life_needs",
    MILITANCY = "militancy",
    // NationTable columns (inside `NATION_TABLE`). Rates are raw Fixed integers (D3).
    TREASURY = "treasury",
    INCOME_TAX_RATE_RAW = "income_tax_rate_raw",
    TRANSFER_RATE_RAW = "transfer_rate_raw",
    CONSUMPTION_RATE_RAW = "consumption_rate_raw",
    // MapView.
    MODE = "mode",
    GOOD = "good",
    VALUES = "values",
    // MarketDetail.
    PRICE = "price",
    SUPPLY = "supply",
    DEMAND = "demand",
    TRADED = "traded",
    // ProvinceDetail.
    POPS = "pops",
    LABOUR = "labour",
    PRODUCERS = "producers",
    PROFESSION = "profession",
    PEOPLE = "people",
    CASH = "cash",
    JOBS = "jobs",
    EMPLOYED = "employed",
    PRODUCER_TYPE = "producer_type",
    CAPACITY = "capacity",
    WAGE = "wage",

    // CommandResult.
    CLIENT_SEQ = "client_seq",
    /// A `CommandError` value (`PaxKeys.COMMAND_ERROR_*`).
    COMMAND_ERROR = "command_error",
    APPLIES_ON_DAY = "applies_on_day",
    /// A message for the player; empty when there is none (SaveResult, load_map).
    ERROR = "error",
    // ServerState.
    CHANGED_BY = "changed_by",
    /// Players a fairness pause waits for (ServerState, D24); empty when none.
    WAITING_FOR = "waiting_for",
    // Pong.
    NONCE = "nonce",
    // SaveResult, SaveList.
    /// The request a SaveResult answers: `REQUEST_SAVE` or `REQUEST_LOAD` (the bridge
    /// pairs answers with requests, so GDScript needn't guess).
    REQUEST = "request",
    NAME = "name",
    NAMES = "names",

    // LobbyState (M4-2). `LOBBY_PLAYERS` is a table: one column per key below (and
    // `PLAYER`, `NAME`, `NATION`), one entry per player; `NATION` entries may be null.
    LOBBY_PLAYERS = "lobby_players",
    SANDBOX = "sandbox",
    READY = "ready",
    HOST = "host",
    /// Left the started game; the seat waits for their resume token.
    AWAY = "away",
    STARTED = "started",
    /// Why this client's last lobby request was refused, or null.
    NOTICE = "notice",

    // A hosted game (PaxClient.hosted): what the host shares with the players.
    PORT = "port",
    FINGERPRINT = "fingerprint",

    // A loaded map (PaxClient.load_map): ERROR, or the ID texture and labels.
    WIDTH = "width",
    HEIGHT = "height",
    IDS = "ids",
    LABELS = "labels",
}

/// `LifeNeeds` → `LIFE_NEEDS`.
fn upper_snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

/// Wire enum values GDScript passes back to the bridge or compares with, from the
/// generated schema code itself (`ENUM_VALUES`, `variant_name`), so a schema change
/// regenerates them and the client never hard-codes a protocol number.
pub fn enum_values() -> Vec<(String, i64)> {
    use pax_protocol::wire::{CommandError, MapMode, Speed};
    let mut out = Vec::new();
    for v in MapMode::ENUM_VALUES {
        out.push((format!("MAP_MODE_{}", upper_snake(v.variant_name().expect("listed"))), i64::from(v.0)));
    }
    for v in Speed::ENUM_VALUES {
        out.push((format!("SPEED_{}", upper_snake(v.variant_name().expect("listed"))), i64::from(v.0)));
    }
    for v in CommandError::ENUM_VALUES {
        out.push((format!("COMMAND_ERROR_{}", upper_snake(v.variant_name().expect("listed"))), i64::from(v.0)));
    }
    out
}

/// `client/pax_keys.gd`, as generated from [`ALL`], [`enum_values`] and the policies.
pub fn gdscript() -> String {
    let mut out = String::from(
        "## Every Dictionary key and event tag the Rust bridge (crates/pax_godot) hands to\n\
         ## GDScript. Generated from crates/pax_godot/src/keys.rs: do not edit. Regenerate with\n\
         ## `UPDATE_PAX_KEYS=1 cargo test -p pax_godot keys`.\n\
         ## Scripts use it as `const PaxKeys := preload(\"res://pax_keys.gd\")`.\n\n",
    );
    for (name, value) in ALL {
        out.push_str(&format!("const {name} := \"{value}\"\n"));
    }
    out.push_str("\n## Wire enum values (schemas/common.fbs).\n");
    for (name, value) in enum_values() {
        out.push_str(&format!("const {name} := {value}\n"));
    }
    let names: Vec<String> = pax_protocol::wire::CommandError::ENUM_VALUES
        .iter()
        .map(|v| format!("\"{}\"", v.variant_name().expect("listed")))
        .collect();
    out.push_str(&format!("## CommandError names, by value.\nconst COMMAND_ERROR_NAMES := [{}]\n", names.join(", ")));
    let intervals: Vec<String> = pax_protocol::wire::Speed::ENUM_VALUES
        .iter()
        .map(|&s| match pax_protocol::pacing(s) {
            pax_protocol::Pacing::Every(d) => d.as_millis().to_string(),
            pax_protocol::Pacing::Paused | pax_protocol::Pacing::Unknown => "null".to_owned(),
        })
        .collect();
    out.push_str(&format!(
        "## Milliseconds per day at each speed, by value (D23); null when paused.\nconst SPEED_DAY_MS := [{}]\n",
        intervals.join(", ")
    ));
    out.push_str("\n## Policy names for PaxClient.submit_policy.\n");
    for p in crate::encode::Policy::ALL {
        out.push_str(&format!("const POLICY_{} := \"{}\"\n", p.name().to_uppercase(), p.name()));
    }
    out.push_str("\n## The requests a SaveResult answers (REQUEST).\n");
    for r in crate::decode::SaveRequest::ALL {
        out.push_str(&format!("const REQUEST_{} := \"{}\"\n", r.name().to_uppercase(), r.name()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `COMMAND_ERROR_NAMES` and `SPEED_DAY_MS` are indexed by value, so the values
    /// must be 0, 1, 2, ….
    #[test]
    fn indexed_enums_are_contiguous() {
        let values: Vec<u8> = pax_protocol::wire::CommandError::ENUM_VALUES.iter().map(|v| v.0).collect();
        assert_eq!(values, (0..values.len() as u8).collect::<Vec<_>>());
        let values: Vec<u8> = pax_protocol::wire::Speed::ENUM_VALUES.iter().map(|v| v.0).collect();
        assert_eq!(values, (0..values.len() as u8).collect::<Vec<_>>());
    }

    #[test]
    fn keys_are_unique() {
        let mut values: Vec<_> = ALL.iter().map(|(_, v)| *v).collect();
        values.sort_unstable();
        let before = values.len();
        values.dedup();
        assert_eq!(values.len(), before, "two constants share a value");
    }

    #[test]
    fn the_gdscript_keys_file_is_up_to_date() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../client/pax_keys.gd");
        let expected = gdscript();
        if std::env::var_os("UPDATE_PAX_KEYS").is_some() {
            std::fs::write(&path, &expected).unwrap();
        }
        let actual = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            actual == expected,
            "client/pax_keys.gd is out of date: run `UPDATE_PAX_KEYS=1 cargo test -p pax_godot keys`"
        );
    }
}
