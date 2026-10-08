//! Every `Dictionary` key and event tag the bridge hands to GDScript, named once.
//!
//! GDScript reads them through `client/pax_keys.gd` (`PaxKeys.DAY` rather than
//! `"day"`, with `const PaxKeys := preload("res://pax_keys.gd")`), which is
//! generated from [`ALL`]. A test fails when the checked-in file
//! drifts from this list. Regenerate it with
//! `UPDATE_PAX_KEYS=1 cargo test -p pax_godot keys`.

/// Declares each key as a `pub const` and lists them all, in order, in [`ALL`].
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
    UNKNOWN = "unknown",
    /// Not a server message: the connection ended (reason in `REASON`).
    CLOSED = "closed",

    // Every event.
    TYPE = "type",

    // Welcome.
    PROTOCOL_MINOR = "protocol_minor",
    PLAYER = "player",
    NATION = "nation",
    DAY = "day",
    SPEED = "speed",
    SCENARIO = "scenario",
    CONTENT_HASH = "content_hash",
    MAP_HASH = "map_hash",
    GOODS = "goods",
    PROFESSIONS = "professions",
    PRODUCER_TYPES = "producer_types",
    PROVINCES = "provinces",
    PROVINCE_MARKET = "province_market",
    MARKETS = "markets",
    NATIONS = "nations",
    NATION_MARKETS = "nation_markets",

    // Rejected, Goodbye, closed.
    REASON = "reason",

    // DayUpdate.
    SKIPPED = "skipped",
    STATE_HASH = "state_hash",
    WORLD = "world",
    MAP = "map",
    MARKET = "market",
    PROVINCE = "province",
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
    // NationTable (`NATIONS` in a DayUpdate). Rates are raw Fixed integers (D3).
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
    ERROR = "error",
    APPLIES_ON_DAY = "applies_on_day",
    // ServerState.
    CHANGED_BY = "changed_by",
    // Pong.
    NONCE = "nonce",
    // SaveResult, SaveList.
    NAME = "name",
    NAMES = "names",
}

/// `client/pax_keys.gd`, as generated from [`ALL`].
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
