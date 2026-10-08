//! Engine ↔ wire conversions for commands (M3-4, D21, D22).
//!
//! The matches here are over the *engine's* enums (`Command`, `CommandError`) and
//! have no `_` arm (NETWORK_PROTOCOL §5). A new engine command or error variant then
//! fails to compile here until the schema has its wire form. The wire unions are open
//! newtypes and can't be matched exhaustively, so unknown wire members are handled in
//! `request.rs` (they become `Malformed`).

use pax_engine::{Command, CommandError, Fixed};
use pax_protocol::wire;

use crate::request::WireCommand;

/// The engine command for a wire command, or `Malformed` if its rate was absent.
pub fn to_engine(c: WireCommand) -> Result<Command, wire::CommandError> {
    let missing = wire::CommandError::Malformed;
    let rate = |raw: Option<i64>| raw.map(Fixed::from_raw).ok_or(missing);
    Ok(match c {
        WireCommand::SetIncomeTax { nation, rate_raw } => {
            Command::SetIncomeTax { nation: nation as usize, rate: rate(rate_raw)? }
        }
        WireCommand::SetTransferRate { nation, rate_raw } => {
            Command::SetTransferRate { nation: nation as usize, rate: rate(rate_raw)? }
        }
        WireCommand::SetConsumptionRate { nation, rate_raw } => {
            Command::SetConsumptionRate { nation: nation as usize, rate: rate(rate_raw)? }
        }
    })
}

/// The nation a command acts on: what the permission check compares (D24).
pub fn nation_of(c: &Command) -> usize {
    match *c {
        Command::SetIncomeTax { nation, .. }
        | Command::SetTransferRate { nation, .. }
        | Command::SetConsumptionRate { nation, .. } => nation,
    }
}

/// The wire form of the engine's single validity rule's verdict (D21).
pub fn error_to_wire(e: &CommandError) -> wire::CommandError {
    match e {
        CommandError::UnknownNation(_) => wire::CommandError::UnknownNation,
        CommandError::RateOutOfRange(_) => wire::CommandError::RateOutOfRange,
        CommandError::NoBasket(_) => wire::CommandError::NoBasket,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_commands_become_engine_commands() {
        let c = to_engine(WireCommand::SetTransferRate { nation: 1, rate_raw: Some(50_000) }).unwrap();
        assert_eq!(c, Command::SetTransferRate { nation: 1, rate: Fixed::from_raw(50_000) });
        assert_eq!(nation_of(&c), 1);
    }

    #[test]
    fn a_missing_rate_is_malformed() {
        assert_eq!(
            to_engine(WireCommand::SetIncomeTax { nation: 0, rate_raw: None }),
            Err(wire::CommandError::Malformed)
        );
    }

    #[test]
    fn every_engine_error_has_a_wire_form() {
        assert_eq!(error_to_wire(&CommandError::NoBasket(0)), wire::CommandError::NoBasket);
        assert_eq!(error_to_wire(&CommandError::RateOutOfRange(Fixed::ONE)), wire::CommandError::RateOutOfRange);
        assert_eq!(error_to_wire(&CommandError::UnknownNation(9)), wire::CommandError::UnknownNation);
    }
}
