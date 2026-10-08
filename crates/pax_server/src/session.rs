//! The session table (M4-1, D24): one row per connection, owned by the sim thread,
//! and which of them is the host (M4-3).
//!
//! A row holds what the server keeps per client: its connection handle, its
//! subscription, its flow-control window and, once welcomed, its seat (player id and
//! nation). Rows hold no references into the `World`; the sim thread reads both side
//! by side.
//!
//! The table is the only writer of seats and of the host role, so its invariants hold
//! whoever seats or unseats a player: player ids are distinct, a nation has at most
//! one player (D24), no more than the server's limit play at once, and the host
//! passes on by D24's rule whenever the host's seat ends. A seat starts only in
//! [`SessionTable::sit`] and ends only in [`SessionTable::remove`] or
//! [`SessionTable::unseat`], which return a [`Vacated`] the caller must handle. Rows are keyed by the connection's session id in a `BTreeMap`, so
//! iteration (broadcasts, a load's new `Welcome`s) is in session order, never in hash
//! order.

use std::collections::BTreeMap;

use crate::encode::LobbyEntry;
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
    /// The name the client gave in `Hello`, for the lobby. Display only.
    name: String,
    /// Marked ready in the lobby (M4-2). Cleared whenever the claim changes.
    ready: bool,
}

/// Why the session table refused a seat ([`SessionTable::sit`]) or a lobby request
/// ([`SessionTable::claim`], [`SessionTable::set_ready`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The server's limit of players are already playing.
    Full,
    /// Another player holds the nation (D24).
    NationTaken { nation: u32, player: u16 },
    /// Only a player holding a nation (or a sandbox seat) can be ready (M4-2).
    NoClaim,
}

impl Refusal {
    /// The reason a client is told (`Rejected`, or a lobby notice). `key` is the
    /// taken nation's key, which the table doesn't know.
    pub(crate) fn reason(self, max_players: usize, key: Option<&str>) -> String {
        match (self, max_players) {
            (Refusal::Full, 1) => "server full: a single-player server accepts one client".to_owned(),
            (Refusal::Full, n) => format!("server full: all {n} players are connected"),
            (Refusal::NationTaken { nation, player }, _) => match key {
                Some(key) => format!("nation {nation} ({key}) is taken by player {player}"),
                None => format!("nation {nation} is taken by player {player}"),
            },
            (Refusal::NoClaim, _) => "claim a nation before you are ready".to_owned(),
        }
    }
}

/// Who is host (D24). The table applies the whole rule, electing and succeeding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HostRule {
    /// A player-hosted server: the first player is host, and when the host's seat
    /// ends, the remaining player with the lowest id.
    FirstPlayer,
    /// A dedicated server (`--admin NAME`): the client of that name is host whenever
    /// it plays, and nobody else ever is. The name is what the client asserts, so
    /// multiplayer binds loopback only until M4-6 authenticates it.
    Admin(String),
}

/// What ending a seat changed. The caller must act on it (D23: when the last player
/// leaves, the clock stops), so it can't be dropped by accident.
#[must_use = "ending a seat may have ended the game for everyone (D23)"]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Vacated {
    pub seat: Seat,
    /// The session that became host because this one was (D24), if any.
    pub new_host: Option<u64>,
    /// Nobody plays any more.
    pub last: bool,
}

/// Who a welcomed session plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Seat {
    /// Distinct among the welcomed sessions. It stamps the session's commands, so it
    /// orders them within a day (D24) and names the player in the command log.
    pub player: u16,
    /// The nation this player commands. `None`: a sandbox seat (below), or in the
    /// lobby a player who hasn't claimed one yet (M4-2).
    pub nation: Option<u32>,
    /// May command every nation (D24: only on a server run with `--sandbox`).
    pub sandbox: bool,
}

impl Seat {
    /// Whether this seat may command `nation` (D24): its own, or any if sandbox. A
    /// player in the lobby without a claim commands nothing.
    pub(crate) fn commands(&self, nation: usize) -> bool {
        self.sandbox || self.nation.is_some_and(|n| n as usize == nation)
    }
}

pub(crate) struct SessionTable {
    rows: BTreeMap<u64, Session>,
    /// The session that sets the speed, saves, loads and kicks (D24). Always a
    /// welcomed session, or `None`.
    host: Option<u64>,
    /// How the host is elected and succeeded (D24).
    host_rule: HostRule,
}

impl SessionTable {
    /// An empty table whose host follows `host_rule`.
    pub(crate) fn new(host_rule: HostRule) -> Self {
        SessionTable { rows: BTreeMap::new(), host: None, host_rule }
    }

    /// A new connection, not yet welcomed.
    pub(crate) fn connect(&mut self, id: u64, conn: ConnHandle) {
        let row = Session {
            conn,
            subscription: CheckedSubscription::default(),
            seat: None,
            window: UpdateWindow::default(),
            name: String::new(),
            ready: false,
        };
        self.rows.insert(id, row);
    }

    /// Removes a closed connection's row. If it was playing, its seat ends: see
    /// [`Vacated`].
    pub(crate) fn remove(&mut self, id: u64) -> Option<Vacated> {
        let seat = self.rows.remove(&id)?.seat?;
        Some(self.vacated(id, seat))
    }

    /// Ends the session's seat while its connection is still open (a kick, a load
    /// without its nation): it no longer plays, and the admission choke point drops
    /// whatever it still sends. Its row stays until its connection closes.
    pub(crate) fn unseat(&mut self, id: u64) -> Option<Vacated> {
        let seat = self.rows.get_mut(&id)?.seat.take()?;
        Some(self.vacated(id, seat))
    }

    /// After a seat ended: the host role passes on by D24's rule.
    fn vacated(&mut self, id: u64, seat: Seat) -> Vacated {
        let mut new_host = None;
        if self.host == Some(id) {
            self.host = None;
            if self.host_rule == HostRule::FirstPlayer {
                new_host = self.lowest_player();
                self.host = new_host;
            }
        }
        Vacated { seat, new_host, last: self.players() == 0 }
    }

    #[cfg(test)]
    pub(crate) fn host(&self) -> Option<u64> {
        self.host
    }

    pub(crate) fn is_host(&self, id: u64) -> bool {
        self.host == Some(id)
    }

    /// The welcomed session with the lowest player id: who becomes host when the
    /// host leaves a player-hosted server (D24).
    fn lowest_player(&self) -> Option<u64> {
        self.rows.iter().filter_map(|(&id, s)| s.seat.map(|seat| (seat.player, id))).min().map(|(_, id)| id)
    }

    /// The session holding player id `player`.
    pub(crate) fn session_of(&self, player: u16) -> Option<u64> {
        self.rows.iter().find(|(_, s)| s.seat.is_some_and(|seat| seat.player == player)).map(|(&id, _)| id)
    }

    pub(crate) fn get_mut(&mut self, id: u64) -> Option<&mut Session> {
        self.rows.get_mut(&id)
    }

    /// Seats a connected session as a new player named `name`, holding `nation`, or
    /// none: a sandbox seat if `sandbox` (which the caller has allowed), else an
    /// unclaimed lobby seat. It gets the lowest free player id, if fewer than
    /// `max_players` play and nobody holds the nation. The only way a session gets a
    /// seat.
    pub(crate) fn sit(
        &mut self,
        id: u64,
        name: &str,
        nation: Option<u32>,
        sandbox: bool,
        max_players: usize,
    ) -> Result<Seat, Refusal> {
        debug_assert!(!(sandbox && nation.is_some()), "a sandbox seat holds no nation");
        if self.players() >= max_players {
            return Err(Refusal::Full);
        }
        if let Some((nation, player)) = nation.and_then(|n| self.holder(n).map(|p| (n, p))) {
            return Err(Refusal::NationTaken { nation, player });
        }
        let seat = Seat { player: self.free_player(), nation, sandbox };
        let row = self.rows.get_mut(&id).expect("a session is connected before it says Hello");
        debug_assert!(row.seat.is_none(), "net lets a session say Hello only once");
        row.seat = Some(seat);
        row.name = name.to_owned();
        row.ready = false;
        // Election (D24): the first player, or the admin by name, when nobody is host.
        let elected = match &self.host_rule {
            HostRule::FirstPlayer => true,
            HostRule::Admin(admin) => name == admin,
        };
        if elected && self.host.is_none() {
            self.host = Some(id);
        }
        Ok(seat)
    }

    /// In the lobby: the player claims `nation`, or gives up their claim with `None`
    /// (M4-2). A nation another player holds is refused. Any change clears the
    /// player's ready mark, so a start never catches a claim mid-change.
    pub(crate) fn claim(&mut self, id: u64, nation: Option<u32>) -> Result<(), Refusal> {
        let own = self.seat(id).map(|s| s.player);
        if let Some((nation, player)) =
            nation.and_then(|n| self.holder(n).map(|p| (n, p))).filter(|&(_, p)| Some(p) != own)
        {
            return Err(Refusal::NationTaken { nation, player });
        }
        let row = self.rows.get_mut(&id).expect("only a seated session claims");
        let seat = row.seat.as_mut().expect("only a seated session claims");
        if seat.nation != nation || seat.sandbox {
            seat.nation = nation;
            seat.sandbox = false;
            row.ready = false;
        }
        Ok(())
    }

    /// In the lobby: marks the player ready, or not. Only a player who holds a
    /// nation, or a sandbox seat, can be ready.
    pub(crate) fn set_ready(&mut self, id: u64, ready: bool) -> Result<(), Refusal> {
        let row = self.rows.get_mut(&id).expect("only a seated session readies");
        let seat = row.seat.expect("only a seated session readies");
        if ready && seat.nation.is_none() && !seat.sandbox {
            return Err(Refusal::NoClaim);
        }
        row.ready = ready;
        Ok(())
    }

    /// Whether the host may start the game: someone plays, and every player is ready.
    pub(crate) fn all_ready(&self) -> bool {
        self.players() > 0 && self.rows.values().filter(|s| s.seat.is_some()).all(|s| s.ready)
    }

    /// The lobby as clients see it, in player order.
    pub(crate) fn lobby(&self) -> Vec<LobbyEntry> {
        let mut entries: Vec<LobbyEntry> = self
            .rows
            .iter()
            .filter_map(|(&id, s)| {
                s.seat.map(|seat| LobbyEntry {
                    player: seat.player,
                    name: s.name.clone(),
                    nation: seat.nation,
                    sandbox: seat.sandbox,
                    ready: s.ready,
                    host: self.host == Some(id),
                })
            })
            .collect();
        entries.sort_by_key(|e| e.player);
        entries
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
        assert_eq!(table.sit(id, "p", nation, false, 8), Ok(Seat { player, nation, sandbox: false }));
    }

    #[test]
    fn player_ids_fill_the_lowest_gap() {
        let mut t = SessionTable::new(HostRule::FirstPlayer);
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
    fn the_first_player_is_host_and_the_role_passes_to_the_lowest_id() {
        let mut t = SessionTable::new(HostRule::FirstPlayer);
        seated(&mut t, 7, 0, None);
        seated(&mut t, 9, 1, None);
        seated(&mut t, 5, 2, None);
        assert_eq!(t.host(), Some(7), "the first player");
        let v = t.remove(7).unwrap();
        assert_eq!((v.new_host, v.last, t.host()), (Some(9), false, Some(9)), "player 1 is session 9");
        assert_eq!(t.unseat(5).map(|v| (v.new_host, v.last)), Some((None, false)), "not the host: no change");
        assert_eq!(t.unseat(9).map(|v| (v.new_host, v.last)), Some((None, true)), "nobody left to pass it to");
        assert_eq!((t.host(), t.unseat(9)), (None, None), "a seat ends once");
    }

    #[test]
    fn an_admin_is_host_by_name_and_the_role_never_passes() {
        let mut t = SessionTable::new(HostRule::Admin("ada".into()));
        seated(&mut t, 1, 0, None);
        assert_eq!(t.host(), None, "the first player isn't host on a dedicated server");
        let (conn, _rx) = ConnHandle::for_test();
        t.connect(2, conn);
        assert!(t.sit(2, "ada", None, false, 8).is_ok());
        assert_eq!(t.host(), Some(2));
        assert_eq!(t.remove(2).map(|v| v.new_host), Some(None));
        assert_eq!(t.host(), None);
    }

    #[test]
    fn sit_refuses_a_full_table_and_a_taken_nation() {
        let mut t = SessionTable::new(HostRule::FirstPlayer);
        seated(&mut t, 1, 0, Some(0));
        for id in [2, 3] {
            let (conn, _rx) = ConnHandle::for_test();
            t.connect(id, conn);
        }
        assert_eq!(t.sit(2, "p", Some(0), false, 8), Err(Refusal::NationTaken { nation: 0, player: 0 }));
        assert_eq!(t.sit(2, "p", Some(1), false, 1), Err(Refusal::Full));
        assert_eq!(t.sit(3, "p", Some(1), false, 2), Ok(Seat { player: 1, nation: Some(1), sandbox: false }));
        assert_eq!(t.seat(2), None, "a refused session stays unseated");
    }

    /// M4-2: claims, ready marks and the start condition.
    #[test]
    fn lobby_claims_and_ready_marks() {
        let mut t = SessionTable::new(HostRule::FirstPlayer);
        seated(&mut t, 1, 0, None);
        seated(&mut t, 2, 1, None);
        assert_eq!(t.set_ready(1, true), Err(Refusal::NoClaim));
        assert_eq!(t.claim(1, Some(0)), Ok(()));
        assert_eq!(t.claim(2, Some(0)), Err(Refusal::NationTaken { nation: 0, player: 0 }));
        assert_eq!(t.claim(1, Some(0)), Ok(()), "claiming your own nation again is fine");
        assert_eq!(t.claim(2, Some(1)), Ok(()));
        assert_eq!((t.set_ready(1, true), t.set_ready(2, true)), (Ok(()), Ok(())));
        assert!(t.all_ready());
        assert_eq!(t.claim(2, None), Ok(()));
        assert!(!t.all_ready(), "a changed claim clears the ready mark");
        let lobby = t.lobby();
        assert_eq!((lobby[0].nation, lobby[0].ready, lobby[1].nation, lobby[1].ready), (Some(0), true, None, false));
        assert_eq!(t.holder(1), None, "the given-up nation is free");
    }

    #[test]
    fn a_nation_is_held_by_its_seat_and_sandbox_holds_none() {
        let mut t = SessionTable::new(HostRule::FirstPlayer);
        seated(&mut t, 1, 0, None);
        seated(&mut t, 2, 1, Some(1));
        let (conn, _rx) = ConnHandle::for_test();
        t.connect(3, conn); // connected, not welcomed
        assert_eq!((t.holder(0), t.holder(1)), (None, Some(1)));
        assert_eq!(t.players(), 2);
    }
}
