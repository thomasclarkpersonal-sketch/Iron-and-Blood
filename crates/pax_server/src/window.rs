//! Flow control (D23): which `DayUpdate`s a session is sent.
//!
//! A session may have [`UPDATE_WINDOW`] updates unacknowledged. While its window is
//! full the server keeps simulating but sends that session nothing. When the session
//! acknowledges, it gets only the latest day, with `skipped` counting the days it
//! missed. The sim thread decides *when* days happen; this decides who sees them.

use std::collections::VecDeque;

/// How many `DayUpdate`s a session may have unacknowledged (D23).
pub(crate) const UPDATE_WINDOW: usize = 3;

/// One session's flow-control state.
#[derive(Debug, Default)]
pub(crate) struct UpdateWindow {
    /// Days of the updates sent and not yet acknowledged, oldest first.
    in_flight: VecDeque<u64>,
    /// Days that ran since the last update, the current day included. The next
    /// update reports all but the current day as `skipped`.
    days_since_update: u32,
}

impl UpdateWindow {
    /// A day ran. Returns whether the session should be sent it now.
    pub(crate) fn day_ran(&mut self) -> bool {
        self.days_since_update += 1;
        self.has_room()
    }

    /// The session processed the update for `day`: it and everything older leave the
    /// window. Returns whether an update is now owed: days ran while the window was full.
    pub(crate) fn ack(&mut self, day: u64) -> bool {
        while self.in_flight.front().is_some_and(|&d| d <= day) {
            self.in_flight.pop_front();
        }
        self.days_since_update > 0 && self.has_room()
    }

    /// An update for `day` is going out. Returns its `skipped`: the days that ran
    /// since the last update, apart from `day` itself. Only a day that ran is sent
    /// through the window ([`Self::day_ran`] or [`Self::ack`] said so); a
    /// `Subscribe` refresh bypasses it.
    pub(crate) fn sent(&mut self, day: u64) -> u32 {
        let skipped =
            self.days_since_update.checked_sub(1).expect("an update through the window is for a day that ran");
        self.in_flight.push_back(day);
        self.days_since_update = 0;
        skipped
    }

    /// Days ran that the session hasn't been sent, and the window has room: an update
    /// is owed (and waits only for the throttle, M4-7).
    pub(crate) fn owed(&self) -> bool {
        self.days_since_update > 0 && self.has_room()
    }

    fn has_room(&self) -> bool {
        self.in_flight.len() < UPDATE_WINDOW
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `days` days from `first`, sending each one the window allows. Returns
    /// the `(day, skipped)` updates sent.
    fn run(w: &mut UpdateWindow, first: u64, days: u64) -> Vec<(u64, u32)> {
        let mut sent = Vec::new();
        for d in first..first + days {
            if w.day_ran() {
                sent.push((d, w.sent(d)));
            }
        }
        sent
    }

    #[test]
    fn a_full_window_holds_updates_and_the_ack_sends_only_the_latest() {
        let mut w = UpdateWindow::default();
        assert_eq!(run(&mut w, 1, 5), [(1, 0), (2, 0), (3, 0)], "the window holds three");
        // Days 4 and 5 ran unseen. Acknowledging day 1 makes room for day 5 alone.
        assert!(w.ack(1));
        assert_eq!(w.sent(5), 1, "day 4 was skipped");
        assert!(!w.ack(1), "a repeated ack owes nothing");
    }

    #[test]
    fn an_ack_covers_every_older_update() {
        let mut w = UpdateWindow::default();
        run(&mut w, 1, 3);
        assert!(!w.ack(3), "no day ran since, so nothing is owed");
        assert_eq!(run(&mut w, 4, 3), [(4, 0), (5, 0), (6, 0)], "the whole window is free again");
    }
}
