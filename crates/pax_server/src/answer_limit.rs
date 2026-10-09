//! **Temporary** limit on map requests (MILESTONE_4, "Open follow-ups"): at most
//! [`SUBSCRIBE_ANSWERS_PER_SECOND`] `Subscribe` answers a second per session, until
//! D24 decides which requests a per-session limit covers. Then this module and the
//! session's `answers` field go, replaced by that rule.
//!
//! Each answer is a full `DayUpdate` with the `MapView`, built on the sim thread, so
//! a client re-subscribing in a tight loop would cost the thread and the bandwidth
//! cap far more than its commands could. A `Subscribe` that comes sooner still
//! changes the subscription at once; its answer, for the latest subscription, goes
//! out when the gap has passed, so nothing is lost, only coalesced. Every session,
//! local ones too: the cost is the sim thread's, which a player on the server's
//! machine shares with everyone. A server constant, not a setting, until then.

use std::time::{Duration, Instant};

/// At most this many `Subscribe` answers a second per session (temporary).
pub(crate) const SUBSCRIBE_ANSWERS_PER_SECOND: u32 = 4;

/// The shortest gap between two `Subscribe` answers to a session.
const ANSWER_GAP: Duration = Duration::from_millis(1000 / SUBSCRIBE_ANSWERS_PER_SECOND as u64);

/// One session's `Subscribe` answers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AnswerLimit {
    /// No answer yet (or since a reset): the next goes out at once.
    #[default]
    Idle,
    /// The last answer went out then.
    AnsweredAt(Instant),
    /// An answer waits: the last went out then, and the next may go out a gap later.
    Pending(Instant),
}

impl AnswerLimit {
    /// Admits an answer at `now`, or defers it until [`Self::due_at`]. Not a query:
    /// call it only to answer, and call [`Self::answered`] when the answer goes out.
    pub(crate) fn admit(&mut self, now: Instant) -> bool {
        match *self {
            AnswerLimit::Idle => true,
            AnswerLimit::AnsweredAt(last) if now >= last + ANSWER_GAP => true,
            AnswerLimit::AnsweredAt(last) | AnswerLimit::Pending(last) => {
                *self = AnswerLimit::Pending(last);
                false
            }
        }
    }

    /// When a deferred answer may go out; `None` if none waits.
    pub(crate) fn due_at(&self) -> Option<Instant> {
        match *self {
            AnswerLimit::Pending(last) => Some(last + ANSWER_GAP),
            AnswerLimit::Idle | AnswerLimit::AnsweredAt(_) => None,
        }
    }

    /// An answer went out at `now`.
    pub(crate) fn answered(&mut self, now: Instant) {
        *self = AnswerLimit::AnsweredAt(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_limited_and_coalesced() {
        let mut a = AnswerLimit::default();
        let start = Instant::now();
        assert!(a.admit(start), "the first goes out");
        a.answered(start);
        assert_eq!(a.due_at(), None);
        for ms in [10, 50, 200] {
            assert!(!a.admit(start + Duration::from_millis(ms)), "too soon: deferred");
        }
        assert_eq!(a.due_at(), Some(start + ANSWER_GAP), "one answer, when the gap has passed");
        assert!(!a.admit(start + ANSWER_GAP), "a pending answer goes out through the flush");
        a.answered(start + ANSWER_GAP);
        assert_eq!(a.due_at(), None);
        assert!(a.admit(start + ANSWER_GAP * 2));
        a = AnswerLimit::default();
        assert!(a.admit(start + ANSWER_GAP * 2), "a reset forgets the limit");
    }
}
