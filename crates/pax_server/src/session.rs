//! The session table (M4-1, D24): one row per connection, owned by the sim thread,
//! and which of them is the host (M4-3).
//!
//! A row holds what the server keeps per client: its connection handle, its
//! subscription, its flow-control window and, once welcomed, its seat (player id and
//! nation). Rows hold no references into the `World`; the sim thread reads both side
//! by side.
//!
//! The table is the only writer of seats ([`SessionTable::sit`], [`SessionTable::unseat`]),
//! so its invariants hold whoever seats a player: player ids are distinct, a nation
//! has at most one player (D24), and no more than the server's limit play at once. Rows are keyed by the connection's session id in a `BTreeMap`, so
//! iteration (broadcasts, a load's new `Welcome`s) is in session order, never in hash
//! order.

use std::collections::BTreeMap;

use crate::net::{ConnHandle, Outbound};
use crate::view::CheckedSubscription;
use crate::window::UpdateWindow;

pub(crate) struct Session {
    pub conn: ConnHandle,
    pub subscription: CheckedSubscription,
    /// Set by `Welcome`. Until then the session may only say `Hello`: one choke
    /// point in `Sim::handle` drops anything else, so no request handler has to
    /// remember to check (a rejected session's queued requests never act). Private:
    /// only [`SessionTable::sit`] and [`SessionTable::unseat`] write it.
    seat: Option<Seat>,
    /// Which days' updates it is sent (D23 flow control).
    pub window: UpdateWindow,
}

impl Session {
    pub(crate) fn seat(&self) -> Option<Seat> {
        self.seat
    }
}

/// Why [`SessionTable::sit`] refused a seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The server's limit of players are already playing.
    Full,
    /// Another player holds the nation (D24).
    NationTaken { player: u16 },
}

/// Who a welcomed session plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Seat {
    /// Distinct among the welcomed sessions. It stamps the session's commands, so it
    /// orders them within a day (D24) and names the player in the command log.
    pub player: u16,
    /// `None`: sandbox, may command every nation (M3; D24 restricts it to `--sandbox`).
    pub nation: Option<u32>,
}

#[derive(Default)]
pub(crate) struct SessionTable {
    rows: BTreeMap<u64, Session>,
    /// The session that sets the speed, saves, loads and kicks (D24). Always a
    /// welcomed session, or `None`.
    host: Option<u64>,
}

impl SessionTable {
    /// A new connection, not yet welcomed.
    pub(crate) fn connect(&mut self, id: u64, conn: ConnHandle) {
        let row =
            Session { conn, subscription: CheckedSubscription::default(), seat: None, window: UpdateWindow::default() };
        self.rows.insert(id, row);
    }

    /// Removes a row. If it was the host, nobody is host until [`Self::set_host`].
    pub(crate) fn remove(&mut self, id: u64) -> Option<Session> {
        if self.host == Some(id) {
            self.host = None;
        }
        self.rows.remove(&id)
    }

    pub(crate) fn host(&self) -> Option<u64> {
        self.host
    }

    pub(crate) fn is_host(&self, id: u64) -> bool {
        self.host == Some(id)
    }

    /// Makes a welcomed session the host.
    pub(crate) fn set_host(&mut self, id: u64) {
        debug_assert!(self.seat(id).is_some(), "only a welcomed session can be host");
        self.host = Some(id);
    }

    /// The welcomed session with the lowest player id: who becomes host when the
    /// host leaves a player-hosted server (D24).
    pub(crate) fn lowest_player(&self) -> Option<u64> {
        self.rows.iter().filter_map(|(&id, s)| s.seat.map(|seat| (seat.player, id))).min().map(|(_, id)| id)
    }

    /// The session holding player id `player`.
    pub(crate) fn session_of(&self, player: u16) -> Option<u64> {
        self.rows.iter().find(|(_, s)| s.seat.is_some_and(|seat| seat.player == player)).map(|(&id, _)| id)
    }

    /// Takes the session's seat: it no longer plays, and the admission choke point
    /// drops whatever it still sends. Its row stays until its connection closes.
    pub(crate) fn unseat(&mut self, id: u64) -> Option<Seat> {
        if self.host == Some(id) {
            self.host = None;
        }
        self.rows.get_mut(&id).and_then(|s| s.seat.take())
    }

    pub(crate) fn get_mut(&mut self, id: u64) -> Option<&mut Session> {
        self.rows.get_mut(&id)
    }

    /// Seats a connected session as a new player of `nation` (`None`: sandbox, which
    /// the caller has allowed): the lowest free player id, if fewer than `max_players`
    /// play and nobody holds the nation. The only way a session gets a seat.
    pub(crate) fn sit(&mut self, id: u64, nation: Option<u32>, max_players: usize) -> Result<Seat, Refusal> {
        if self.players() >= max_players {
            return Err(Refusal::Full);
        }
        if let Some(player) = nation.and_then(|n| self.holder(n)) {
            return Err(Refusal::NationTaken { player });
        }
        let seat = Seat { player: self.free_player(), nation };
        let row = self.rows.get_mut(&id).expect("a session is connected before it says Hello");
        debug_assert!(row.seat.is_none(), "net lets a session say Hello only once");
        row.seat = Some(seat);
        Ok(seat)
    }

    /// The session's seat, if it was welcomed.
    pub(crate) fn seat(&self, id: u64) -> Option<Seat> {
        self.rows.get(&id).and_then(|s| s.seat)
    }

    /// Every connection, welcomed or not, in session order.
    pub(crate) fn all(&self) -> impl Iterator<Item = &Session> {
        self.rows.values()
    }

    /// The welcomed sessions' ids, in session order.
    pub(crate) fn welcomed(&self) -> Vec<u64> {
        self.rows.iter().filter(|(_, s)| s.seat.is_some()).map(|(&id, _)| id).collect()
    }

    pub(crate) fn welcomed_mut(&mut self) -> impl Iterator<Item = (u64, &mut Session)> {
        self.rows.iter_mut().filter(|(_, s)| s.seat.is_some()).map(|(&id, s)| (id, s))
    }

    /// How many sessions are playing.
    pub(crate) fn players(&self) -> usize {
        self.rows.values().filter(|s| s.seat.is_some()).count()
    }

    /// The lowest player id no welcomed session holds. A rejoining player gets the
    /// lowest free id, not necessarily their old one; resume tokens (M4-4) reclaim a seat.
    pub(crate) fn free_player(&self) -> u16 {
        let mut taken: Vec<u16> = self.rows.values().filter_map(|s| s.seat).map(|s| s.player).collect();
        taken.sort_unstable();
        let mut player = 0;
        for t in taken {
            if t != player {
                break;
            }
            player += 1;
        }
        player
    }

    /// The player holding `nation`, if any. Sandbox seats hold none.
    pub(crate) fn holder(&self, nation: u32) -> Option<u16> {
        self.rows.values().filter_map(|s| s.seat).find(|s| s.nation == Some(nation)).map(|s| s.player)
    }

    /// Sends `out` to one session; a closed or unknown session is ignored.
    pub(crate) fn send(&self, id: u64, out: Outbound) {
        if let Some(s) = self.rows.get(&id) {
            s.conn.send(out);
        }
    }

    /// Sends `frame` to every welcomed session.
    pub(crate) fn broadcast(&self, frame: &[u8]) {
        for s in self.rows.values().filter(|s| s.seat.is_some()) {
            s.conn.send(Outbound::Frame(frame.to_vec()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Connects session `id` and seats it, checking it got player id `player`.
    fn seated(table: &mut SessionTable, id: u64, player: u16, nation: Option<u32>) {
        let (conn, _rx) = ConnHandle::for_test();
        table.connect(id, conn);
        assert_eq!(table.sit(id, nation, 8), Ok(Seat { player, nation }));
    }

    #[test]
    fn player_ids_fill_the_lowest_gap() {
        let mut t = SessionTable::default();
        assert_eq!(t.free_player(), 0);
        seated(&mut t, 1, 0, None);
        seated(&mut t, 2, 1, Some(0));
        seated(&mut t, 3, 2, Some(1));
        assert_eq!(t.free_player(), 3);
        t.remove(2);
        assert_eq!(t.free_player(), 1, "player 1 left: the next player takes id 1");
        assert_eq!((t.players(), t.welcomed()), (2, vec![1, 3]));
        assert_eq!((t.session_of(2), t.session_of(1)), (Some(3), None));
    }

    #[test]
    fn the_host_is_cleared_when_it_leaves_or_is_unseated() {
        let mut t = SessionTable::default();
        seated(&mut t, 9, 0, None);
        seated(&mut t, 5, 1, None);
        t.set_host(5);
        assert!(t.is_host(5));
        assert_eq!(t.lowest_player(), Some(9), "player 0 is session 9");
        t.remove(5);
        assert_eq!(t.host(), None);
        t.set_host(9);
        assert_eq!(t.unseat(9).map(|s| s.player), Some(0));
        assert_eq!((t.host(), t.players()), (None, 0));
    }

    #[test]
    fn sit_refuses_a_full_table_and_a_taken_nation() {
        let mut t = SessionTable::default();
        seated(&mut t, 1, 0, Some(0));
        for id in [2, 3] {
            let (conn, _rx) = ConnHandle::for_test();
            t.connect(id, conn);
        }
        assert_eq!(t.sit(2, Some(0), 8), Err(Refusal::NationTaken { player: 0 }));
        assert_eq!(t.sit(2, Some(1), 1), Err(Refusal::Full));
        assert_eq!(t.sit(3, Some(1), 2), Ok(Seat { player: 1, nation: Some(1) }));
        assert_eq!(t.seat(2), None, "a refused session stays unseated");
    }

    #[test]
    fn a_nation_is_held_by_its_seat_and_sandbox_holds_none() {
        let mut t = SessionTable::default();
        seated(&mut t, 1, 0, None);
        seated(&mut t, 2, 1, Some(1));
        let (conn, _rx) = ConnHandle::for_test();
        t.connect(3, conn); // connected, not welcomed
        assert_eq!((t.holder(0), t.holder(1)), (None, Some(1)));
        assert_eq!(t.players(), 2);
    }
}
