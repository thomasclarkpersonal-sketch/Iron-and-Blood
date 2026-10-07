//! Player and AI commands (DECISIONS.md D10, D21).
//!
//! A command is the only way anything outside the engine changes simulation
//! state during a game. Commands are applied at the **start of a tick, in the
//! order given**, before any system runs, so a game is fully determined by its
//! initial state plus its command log (the D10 save model). Every command is
//! validated first; an invalid command changes nothing and is reported.

use std::fmt;

use crate::fixed::Fixed;
use crate::world::World;

/// A change of policy issued by a player (or later an AI). Rates are `Fixed`
/// (D3): a command never carries a float.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Set a nation's flat income tax rate (D15), in `[0, 1]`.
    SetIncomeTax { nation: usize, rate: Fixed },
    /// Set a nation's daily transfer rate (D15), in `[0, 1]`.
    SetTransferRate { nation: usize, rate: Fixed },
    /// Set a nation's daily government consumption rate (D16), in `[0, 1]`.
    /// Fails if the nation has no basket to spend on.
    SetConsumptionRate { nation: usize, rate: Fixed },
}

/// Why a command was rejected. A rejected command changes nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    UnknownNation(usize),
    RateOutOfRange(Fixed),
    NoBasket(usize),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::UnknownNation(n) => write!(f, "unknown nation {n}"),
            CommandError::RateOutOfRange(r) => write!(f, "rate {r} outside [0, 1]"),
            CommandError::NoBasket(n) => write!(f, "nation {n} has no consumption basket"),
        }
    }
}

impl std::error::Error for CommandError {}

impl World {
    /// Validates and applies one command. On error nothing changes.
    pub fn apply(&mut self, command: Command) -> Result<(), CommandError> {
        let (nation, rate) = match command {
            Command::SetIncomeTax { nation, rate }
            | Command::SetTransferRate { nation, rate }
            | Command::SetConsumptionRate { nation, rate } => (nation, rate),
        };
        if nation >= self.nations.len() {
            return Err(CommandError::UnknownNation(nation));
        }
        if rate < Fixed::ZERO || rate > Fixed::ONE {
            return Err(CommandError::RateOutOfRange(rate));
        }
        let n = &mut self.nations;
        match command {
            Command::SetIncomeTax { .. } => n.income_tax_rate[nation] = rate,
            Command::SetTransferRate { .. } => n.transfer_rate[nation] = rate,
            Command::SetConsumptionRate { .. } => {
                let goods = self.defs.good_count();
                if rate.is_positive() && !n.basket[nation * goods..(nation + 1) * goods].iter().any(|w| w.is_positive())
                {
                    return Err(CommandError::NoBasket(nation));
                }
                n.consumption_rate[nation] = rate;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_name_the_problem() {
        assert_eq!(CommandError::UnknownNation(3).to_string(), "unknown nation 3");
        assert!(CommandError::RateOutOfRange(Fixed::from_int(2)).to_string().contains("outside [0, 1]"));
    }
}
