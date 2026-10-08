//! The commands accepted for the next tick (D10, D22, D23).
//!
//! Each command is stamped `(player, sequence)` when it arrives, and a tick applies
//! them in stamp order, never in raw arrival order: arrival order between players
//! is a network race. All the commands in the queue are for the same day, so the
//! stamp is D10's `(day, player, sequence)`.

use pax_engine::Command;

/// A command accepted on arrival, waiting for the start of the next tick.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pending {
    pub session: u64,
    /// The stamp, assigned by the server on arrival.
    pub player: u16,
    pub sequence: u64,
    pub client_seq: u32,
    pub command: Command,
}

#[derive(Debug, Default)]
pub(crate) struct CommandQueue {
    pending: Vec<Pending>,
    /// The next command's `sequence`. It increases for the whole game, so it orders
    /// a player's commands as the server received them.
    next_sequence: u64,
}

impl CommandQueue {
    /// Stamps an accepted command and queues it for the next tick.
    pub(crate) fn stamp(&mut self, session: u64, player: u16, client_seq: u32, command: Command) {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.pending.push(Pending { session, player, sequence, client_seq, command });
    }

    /// Everything queued, in stamp order. The queue is left empty.
    pub(crate) fn take(&mut self) -> Vec<Pending> {
        let mut pending = std::mem::take(&mut self.pending);
        pending.sort_by_key(|p| (p.player, p.sequence));
        pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_engine::Fixed;

    fn rate(raw: i64) -> Command {
        Command::SetIncomeTax { nation: 0, rate: Fixed::from_raw(raw) }
    }

    #[test]
    fn commands_come_out_in_stamp_order_not_arrival_order() {
        let mut q = CommandQueue::default();
        // Player 1's command arrives first; player 0's two follow.
        for (player, raw) in [(1, 100), (0, 110), (0, 120)] {
            q.stamp(7, player, 0, rate(raw));
        }
        let order: Vec<_> = q.take().iter().map(|p| (p.player, p.sequence, p.command)).collect();
        assert_eq!(order, [(0, 1, rate(110)), (0, 2, rate(120)), (1, 0, rate(100))]);
        assert!(q.take().is_empty(), "taking empties the queue");
    }

    #[test]
    fn sequences_keep_increasing_across_ticks() {
        let mut q = CommandQueue::default();
        q.stamp(7, 0, 0, rate(1));
        q.take();
        q.stamp(7, 0, 1, rate(2));
        assert_eq!(q.take()[0].sequence, 1);
    }
}
