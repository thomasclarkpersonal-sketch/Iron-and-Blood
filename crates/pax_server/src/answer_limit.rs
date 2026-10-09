//! **Temporary** limit on map requests (MILESTONE_4, "Open follow-ups"): at most
//! `--subscribe-answers-per-second` `Subscribe` answers a second per session
//! ([`crate::SUBSCRIBE_ANSWERS_PER_SECOND`] by default), until D24 decides which
//! requests a per-session limit covers. Then this module, the session's `answers`
//! field and the setting go, replaced by that rule.
//!
//! Each answer is a full `DayUpdate` with the `MapView`, built on the sim thread, so
//! a client re-subscribing in a tight loop would cost the thread and the bandwidth
//! cap far more than its commands could. A `Subscribe` that comes sooner still
//! changes the subscription at once; its answer, for the latest subscription, goes
//! out when the gap has passed, so nothing is lost, only coalesced. Every session,
//! local ones too: the cost is the sim thread's, which a player on the server's
//! machine shares with everyone.

use std::time::{Duration, Instant};

/// One session's `Subscribe` answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AnswerLimit {
    /// The shortest gap between two answers.
    gap: Duration,
    state: State,
}

/// Where a session's answers stand: one value, so a deferred answer can't exist
/// without the time it is due from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// No answer yet (or since a reset): the next goes out at once.
    Idle,
    /// The last answer went out then.
    AnsweredAt(Instant),
    /// An answer waits: the last went out then, and the next may go out a gap later.
    Pending(Instant),
}

impl AnswerLimit {
    /// At most `per_second` answers a second (at least 1, `Config::validate`).
    pub(crate) fn new(per_second: u32) -> Self {
        assert!(per_second > 0, "a validated Config");
        AnswerLimit { gap: Duration::from_secs(1) / per_second, state: State::Idle }
    }

    /// Forgets past answers, as for a new connection.
    pub(crate) fn reset(&mut self) {
        self.state = State::Idle;
    }

    /// Admits an answer at `now`, or defers it until [`Self::due_at`]. Not a query:
    /// call it only to answer, and call [`Self::answered`] when the answer goes out.
    pub(crate) fn admit(&mut self, now: Instant) -> bool {
        match self.state {
            State::Idle => true,
            State::AnsweredAt(last) if now >= last + self.gap => true,
            State::AnsweredAt(last) | State::Pending(last) => {
                self.state = State::Pending(last);
                false
            }
        }
    }

    /// When a deferred answer may go out; `None` if none waits.
    pub(crate) fn due_at(&self) -> Option<Instant> {
        match self.state {
            State::Pending(last) => Some(last + self.gap),
            State::Idle | State::AnsweredAt(_) => None,
        }
    }

    /// An answer went out at `now`.
    pub(crate) fn answered(&mut self, now: Instant) {
        self.state = State::AnsweredAt(now);
    }
}

impl Default for AnswerLimit {
    /// The default setting (D24, temporary).
    fn default() -> Self {
        AnswerLimit::new(crate::SUBSCRIBE_ANSWERS_PER_SECOND)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_limited_and_coalesced() {
        let mut a = AnswerLimit::new(4);
        let gap = Duration::from_millis(250);
        let start = Instant::now();
        assert!(a.admit(start), "the first goes out");
        a.answered(start);
        assert_eq!(a.due_at(), None);
        for ms in [10, 50, 200] {
            assert!(!a.admit(start + Duration::from_millis(ms)), "too soon: deferred");
        }
        assert_eq!(a.due_at(), Some(start + gap), "one answer, when the gap has passed");
        assert!(!a.admit(start + gap), "a pending answer goes out through the flush");
        a.answered(start + gap);
        assert_eq!(a.due_at(), None);
        assert!(a.admit(start + gap * 2));
        a.reset();
        assert!(a.admit(start + gap * 2), "a reset forgets past answers");
        let mut fast = AnswerLimit::new(20);
        fast.answered(start);
        assert!(fast.admit(start + Duration::from_millis(50)), "the setting sets the gap");
    }
}
