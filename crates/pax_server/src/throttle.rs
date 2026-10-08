//! Bandwidth controls for remote sessions (D24, M4-7).
//!
//! A remote session (its peer is not on this machine) gets at most
//! [`Bandwidth::updates_per_second`] `DayUpdate`s per second. Days that run in
//! between are not queued: the next update carries the latest day and counts the
//! others as `skipped`, as D23's flow control already does. Its `MapView`, the
//! largest part of an update (one value per province), goes out when the
//! subscription changes and then with every [`Bandwidth::map_every`]th update,
//! because map colours don't need a daily refresh. A local session (single player,
//! or a player on the server's machine) is never throttled.
//!
//! The numbers are server settings (`--updates-per-second`, `--map-every`) with
//! D24's defaults, [`crate::UPDATES_PER_SECOND`] and [`crate::MAP_EVERY`], to be
//! confirmed in a playtest.
//!
//! The throttle decides only *whether* and *what*; the sim thread decides when days
//! happen, and [`crate::window`] how many updates may be unacknowledged.

use std::time::{Duration, Instant};

use crate::view::MapPart;

/// A server's bandwidth settings for remote sessions (D24, M4-7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bandwidth {
    /// At most this many updates a second (at least 1).
    pub updates_per_second: u32,
    /// The `MapView` goes out with every this-many-th update (at least 1: every one).
    pub map_every: u32,
}

impl Default for Bandwidth {
    /// D24's defaults.
    fn default() -> Self {
        Bandwidth { updates_per_second: crate::UPDATES_PER_SECOND, map_every: crate::MAP_EVERY }
    }
}

/// One session's throttle.
#[derive(Debug)]
pub(crate) struct Throttle {
    remote: bool,
    /// The shortest gap between two updates to a remote session.
    gap: Duration,
    map_every: u32,
    /// When the last update went out.
    last_sent: Option<Instant>,
    /// A day is waiting for the gap to pass.
    held: bool,
    /// Updates since the last one with a map; `None`: the next one carries it.
    since_map: Option<u32>,
}

impl Throttle {
    pub(crate) fn new(remote: bool, bandwidth: Bandwidth) -> Self {
        // Config::validate refuses zero (ConfigError::NoUpdates).
        assert!(bandwidth.updates_per_second > 0 && bandwidth.map_every > 0, "a validated Bandwidth");
        Throttle {
            remote,
            gap: Duration::from_secs(1) / bandwidth.updates_per_second,
            map_every: bandwidth.map_every,
            last_sent: None,
            held: false,
            since_map: None,
        }
    }

    /// Starts again, as for a new connection: nothing held, and the next update
    /// carries the map. For a new game (a load) or a seat that ended.
    pub(crate) fn restart(&mut self) {
        self.last_sent = None;
        self.held = false;
        self.since_map = None;
    }

    /// Admits an update at `now`, or holds it: then the day waits until
    /// [`Self::ready_at`], when the sim thread sends the latest day. Not a query:
    /// each call sets or clears the hold, so call it only to send.
    pub(crate) fn admit(&mut self, now: Instant) -> bool {
        let early = self.remote && self.last_sent.is_some_and(|t| now < t + self.gap);
        self.held = early;
        !early
    }

    /// When a held update may go out; `None` if nothing is held.
    pub(crate) fn ready_at(&self) -> Option<Instant> {
        self.last_sent.filter(|_| self.held).map(|t| t + self.gap)
    }

    /// An update goes out at `now`. Returns whether it carries the `MapView`.
    pub(crate) fn sent(&mut self, now: Instant) -> MapPart {
        self.held = false;
        self.last_sent = Some(now);
        if !self.remote {
            return MapPart::Include;
        }
        let with_map = self.since_map.is_none_or(|n| n + 1 >= self.map_every);
        self.since_map = Some(if with_map { 0 } else { self.since_map.map_or(0, |n| n + 1) });
        if with_map { MapPart::Include } else { MapPart::Omit }
    }

    /// The session's subscription changed, and its `Subscribe` was answered at `now`
    /// with a full update, map included. It counts as an update: the map count
    /// starts again from there, and the next day keeps the gap from it, so clicking
    /// through provinces can't add days beyond the cap. A held day stays held.
    pub(crate) fn resubscribed(&mut self, now: Instant) {
        self.since_map = Some(0);
        self.last_sent = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_session_is_never_throttled() {
        let mut t = Throttle::new(false, Bandwidth::default());
        let now = Instant::now();
        for _ in 0..20 {
            assert!(t.admit(now));
            assert_eq!(t.sent(now), MapPart::Include, "every update carries the map");
        }
        assert_eq!(t.ready_at(), None);
    }

    #[test]
    fn a_remote_session_gets_four_updates_a_second_and_a_map_every_fifth() {
        let mut t = Throttle::new(true, Bandwidth::default());
        let start = Instant::now();
        let mut sent = Vec::new();
        // A day every 50 ms (Fastest-like) for two seconds.
        for i in 0..40u64 {
            let now = start + Duration::from_millis(50 * i);
            if t.admit(now) {
                sent.push((i, t.sent(now) == MapPart::Include));
            }
        }
        assert_eq!(sent.len(), 8, "four a second: {sent:?}");
        let maps: Vec<u64> = sent.iter().filter(|s| s.1).map(|s| s.0).collect();
        assert_eq!(maps, [0, 25], "the first update, then every fifth");
        // The last day was held: it may go out once the gap has passed.
        assert!(t.ready_at().is_some_and(|at| at <= start + Duration::from_millis(2_000)));
        let now = start + Duration::from_secs(3);
        t.resubscribed(now);
        assert!(!t.admit(now + Duration::from_millis(100)), "the answer counts toward the cap");
        let now = now + Duration::from_millis(250);
        assert!(t.admit(now));
        assert_eq!(t.sent(now), MapPart::Omit, "a resubscription's answer carried the map");
    }

    #[test]
    fn the_settings_set_the_rate_and_the_map_interval() {
        let mut t = Throttle::new(true, Bandwidth { updates_per_second: 10, map_every: 1 });
        let start = Instant::now();
        let mut sent = 0;
        for i in 0..20u64 {
            let now = start + Duration::from_millis(50 * i);
            if t.admit(now) {
                assert_eq!(t.sent(now), MapPart::Include, "every update carries the map");
                sent += 1;
            }
        }
        assert_eq!(sent, 10, "ten a second");
    }

    #[test]
    fn a_restart_drops_what_was_held() {
        let mut t = Throttle::new(true, Bandwidth::default());
        let now = Instant::now();
        assert!(t.admit(now));
        t.sent(now);
        assert!(!t.admit(now + Duration::from_millis(10)));
        assert!(t.ready_at().is_some());
        t.restart();
        assert_eq!(t.ready_at(), None);
        assert!(t.admit(now + Duration::from_millis(20)));
        assert_eq!(
            t.sent(now + Duration::from_millis(20)),
            MapPart::Include,
            "the first update after a restart carries the map"
        );
    }
}
