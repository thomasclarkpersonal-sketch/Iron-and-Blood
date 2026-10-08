//! GDScript's arguments, checked before they become wire messages (D12, D22).
//!
//! GDScript is untyped at this edge, so every value is checked here. An absent id
//! (`null`) and an invalid one (negative, out of range, not an int) are different
//! things: `null` means "none", and an invalid value is an error the caller sees. It is
//! never quietly turned into a valid value. An enum value must be one this client
//! knows (`ENUM_VALUES`).

use pax_protocol::wire;

use crate::encode::Policy;

/// An id, or `None` for GDScript's `null`.
pub fn optional_id(value: Option<i64>, what: &str) -> Result<Option<u32>, String> {
    value.map(|v| id(v, what)).transpose()
}

/// A table index: a non-negative int that fits the wire's `uint`.
pub fn id(value: i64, what: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("{what} {value} is not a valid index"))
}

pub fn good(value: i64) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("good {value} is not a valid index"))
}

pub fn map_mode(value: i64) -> Result<wire::MapMode, String> {
    wire::MapMode::ENUM_VALUES
        .iter()
        .copied()
        .find(|m| i64::from(m.0) == value)
        .ok_or_else(|| format!("{value} is not a map mode (PaxKeys.MAP_MODE_*)"))
}

pub fn speed(value: i64) -> Result<wire::Speed, String> {
    wire::Speed::ENUM_VALUES
        .iter()
        .copied()
        .find(|s| i64::from(s.0) == value)
        .ok_or_else(|| format!("{value} is not a speed (PaxKeys.SPEED_*)"))
}

pub fn policy(name: &str) -> Result<Policy, String> {
    Policy::from_name(name).ok_or_else(|| format!("{name:?} is not a policy (PaxKeys.POLICY_*)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_is_none_and_invalid_is_an_error() {
        assert_eq!(optional_id(None, "nation"), Ok(None));
        assert_eq!(optional_id(Some(2), "nation"), Ok(Some(2)));
        assert!(optional_id(Some(-1), "nation").unwrap_err().contains("nation -1"));
        assert!(id(i64::from(u32::MAX) + 1, "province").is_err());
        assert!(good(-1).is_err());
        assert!(good(70_000).is_err());
    }

    #[test]
    fn enums_must_be_known_values() {
        assert_eq!(map_mode(6), Ok(wire::MapMode::Price));
        assert!(map_mode(7).is_err());
        assert!(map_mode(-1).is_err());
        assert_eq!(speed(0), Ok(wire::Speed::Paused));
        assert!(speed(256).is_err());
        assert!(speed(-1).is_err());
        assert_eq!(policy("transfer"), Ok(Policy::Transfer));
        assert!(policy("income-tax").is_err());
    }
}
