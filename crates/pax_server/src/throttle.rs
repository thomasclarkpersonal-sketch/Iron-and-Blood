//! Bandwidth controls for remote sessions (D24, M4-7).
//!
//! A remote session (its peer is not on this machine) gets at most
//! [`UPDATES_PER_SECOND`] `DayUpdate`s per second. Days that run in between are not
//! queued: the next update carries the latest day and counts the others as
//! `skipped`, as D23's flow control already does. Its `MapView`, the largest part of
//! an update (one value per province), goes out when the subscription changes and
//! then with every [`MAP_EVERY`]th update, because map colours don't need a daily
//! refresh. A local session (single player, or a player on the server's machine) is
//! never throttled.
//!
//! The throttle decides only *whether* and *what*; the sim thread decides when days
//! happen, and [`crate::window`] how many updates may be unacknowledged.

use std::time::{Duration, Instant};

/// D24: at most this many updates per second to a remote session.
pub(crate) const UPDATES_PER_SECOND: u32 = 4;

/// D24: a remote session's `MapView` goes out with every this-many-th update (and
/// whenever its subscription changes).
pub(crate) const MAP_EVERY: u32 = 5;

/// The shortest gap between two updates to a remote session.
const MIN_GAP: Duration = Duration::from_millis(1000 / UPDATES_PER_SECOND as u64);

/// One session's throttle.
#[derive(Debug, Default)]
pub(crate) struct Throttle {
    remote: bool,
    /// When the last update went out.
    last_sent: Option<Instant>,
    /// A day is waiting for the gap to pass.
    held: bool,
    /// Updates since the last one with a map; `None`: the next one carries it.
    since_map: Option<u32>,
}

impl Throttle {
    pub(crate) fn new(remote: bool) -> Self {
        Throttle { remote, ..Throttle::default() }
    }

    /// Whether an update may go out at `now`. If not, the day is held until
    /// [`Self::ready_at`], when the sim thread sends the latest day.
    pub(crate) fn may_send(&mut self, now: Instant) -> bool {
        let early = self.remote && self.last_sent.is_some_and(|t| now < t + MIN_GAP);
        self.held = early;
        !early
    }

    /// When a held update may go out; `None` if nothing is held.
    pub(crate) fn ready_at(&self) -> Option<Instant> {
        self.last_sent.filter(|_| self.held).map(|t| t + MIN_GAP)
    }

    /// An update goes out at `now`. Returns whether it carries the `MapView`.
    pub(crate) fn sent(&mut self, now: Instant) -> bool {
        self.held = false;
        self.last_sent = Some(now);
        if !self.remote {
            return true;
        }
        let with_map = self.since_map.is_none_or(|n| n + 1 >= MAP_EVERY);
        self.since_map = Some(if with_map { 0 } else { self.since_map.map_or(0, |n| n + 1) });
        with_map
    }

    /// The session's subscription changed, and its `Subscribe` was answered with a
    /// full update, map included: the count starts again from there.
    pub(crate) fn resubscribed(&mut self) {
        self.since_map = Some(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_session_is_never_throttled() {
        let mut t = Throttle::new(false);
        let now = Instant::now();
        for _ in 0..20 {
            assert!(t.may_send(now));
            assert!(t.sent(now), "every update carries the map");
        }
        assert_eq!(t.ready_at(), None);
    }

    #[test]
    fn a_remote_session_gets_four_updates_a_second_and_a_map_every_fifth() {
        let mut t = Throttle::new(true);
        let start = Instant::now();
        let mut sent = Vec::new();
        // A day every 50 ms (Fastest-like) for two seconds.
        for i in 0..40u64 {
            let now = start + Duration::from_millis(50 * i);
            if t.may_send(now) {
                sent.push((i, t.sent(now)));
            }
        }
        assert_eq!(sent.len(), 8, "four a second: {sent:?}");
        let maps: Vec<u64> = sent.iter().filter(|s| s.1).map(|s| s.0).collect();
        assert_eq!(maps, [0, 25], "the first update, then every fifth");
        // The last day was held: it may go out once the gap has passed.
        assert!(t.ready_at().is_some_and(|at| at <= start + Duration::from_millis(2_000)));
        t.resubscribed();
        let now = start + Duration::from_secs(3);
        assert!(t.may_send(now));
        assert!(!t.sent(now), "a resubscription's answer carried the map");
    }
}
