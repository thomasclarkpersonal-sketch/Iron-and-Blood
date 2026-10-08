//! The game clock (D23): how fast days run, and when the next one is due.
//!
//! Wall-clock time paces the simulation but never reaches its state (D3): a tick is
//! the same whenever it runs.

use std::time::{Duration, Instant};

use pax_protocol::wire;

/// How a speed paces the clock (D23).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pace {
    Paused,
    /// One day per interval; a zero interval for `Fastest`, which ticks as fast as
    /// the engine allows.
    Every(Duration),
    /// A speed newer than this server.
    Unknown,
}

/// Days per real second at each speed (D23).
pub(crate) fn pace(speed: wire::Speed) -> Pace {
    use wire::Speed as S;
    match speed {
        S::Paused => Pace::Paused,
        S::Slowest => Pace::Every(Duration::from_millis(2_000)), // 0.5 days/s
        S::Slow => Pace::Every(Duration::from_millis(1_000)),    // 1
        S::Normal => Pace::Every(Duration::from_millis(500)),    // 2
        S::Fast => Pace::Every(Duration::from_millis(200)),      // 5
        S::Fastest => Pace::Every(Duration::ZERO),
        _ => Pace::Unknown,
    }
}

/// The game clock: one value, so "paused" and "a tick is due" can't disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Clock {
    Paused,
    Running { speed: wire::Speed, interval: Duration, next: Instant },
}

impl Clock {
    /// The clock at `speed`, its first tick one interval after `now`, so unpausing
    /// never fires a burst of catch-up ticks. `None` for an unknown speed.
    pub(crate) fn at(speed: wire::Speed, now: Instant) -> Option<Clock> {
        match pace(speed) {
            Pace::Paused => Some(Clock::Paused),
            Pace::Every(interval) => Some(Clock::Running { speed, interval, next: now + interval }),
            Pace::Unknown => None,
        }
    }

    pub(crate) fn speed(self) -> wire::Speed {
        match self {
            Clock::Paused => wire::Speed::Paused,
            Clock::Running { speed, .. } => speed,
        }
    }

    /// When the next tick is due; `None` while paused.
    pub(crate) fn due(self) -> Option<Instant> {
        match self {
            Clock::Paused => None,
            Clock::Running { next, .. } => Some(next),
        }
    }

    /// A tick ran at `now`: the next is one interval after the one that was due,
    /// keeping the cadence, but never in the past after a slow tick.
    pub(crate) fn ticked(&mut self, now: Instant) {
        if let Clock::Running { interval, next, .. } = self {
            *next = (*next + *interval).max(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_keeps_its_cadence_but_never_schedules_in_the_past() {
        let start = Instant::now();
        let mut clock = Clock::at(wire::Speed::Normal, start).expect("a known speed");
        assert_eq!(clock.due(), Some(start + Duration::from_millis(500)));
        clock.ticked(start + Duration::from_millis(600));
        assert_eq!(clock.due(), Some(start + Duration::from_millis(1_000)), "a late tick keeps the cadence");
        clock.ticked(start + Duration::from_millis(5_000));
        assert_eq!(clock.due(), Some(start + Duration::from_millis(5_000)), "a slow tick never queues catch-up");
        assert_eq!(Clock::at(wire::Speed(9), start), None);
        assert_eq!(Clock::at(wire::Speed::Paused, start), Some(Clock::Paused));
    }
}
