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
use std::num::NonZeroU64;

use crate::answer_limit::AnswerLimit;
use crate::net::{ConnHandle, Outbound};
use crate::throttle::{Bandwidth, Throttle};
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
    /// How often, and with the map or not (D24 bandwidth, M4-7; remote sessions only).
    pub throttle: Throttle,
    /// Its `Subscribe` answers, under the temporary map-request limit
    /// (`answer_limit`, MILESTONE_4 "Open follow-ups").
    pub answers: AnswerLimit,
    /// The name the client gave in `Hello`, for the lobby. Display only.
    name: String,
    /// Marked ready in the lobby (M4-2). Cleared whenever the claim changes.
    ready: bool,
    /// The resume token its `Welcome` carries (D24); `None` until seated.
    token: Option<NonZeroU64>,
    /// Silent past the pause threshold (D24's fairness pause); cleared when it
    /// speaks again, and gone with the row.
    stalled: bool,
    /// When its commands of the last second arrived (D24's rate limit).
    recent_commands: std::collections::VecDeque<std::time::Instant>,
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
    /// No seat is kept for this resume token: it was never issued, the player was
    /// kicked, or a load replaced the game.
    UnknownToken,
}

/// Who is host (D24). The table applies the whole rule, electing and succeeding.
#[derive(Clone, Debug)]
pub(crate) enum HostRule {
    /// A player-hosted server: the first player is host, and when the host's seat
    /// ends, the remaining player with the lowest id.
    FirstPlayer,
    /// A dedicated server (`--admin NAME`): the client of that name, proving it with
    /// the admin password (M4-6), is host whenever it plays, and nobody else ever is.
    /// The one copy of the admin: the sim checks the password against it.
    Admin(crate::Admin),
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
    /// The seat's player was holding up a fairness pause (D24).
    pub was_stalled: bool,
}

/// What happens to a seat when its connection closes (D24).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OnLeave {
    /// Kept for the player's resume token: a started multiplayer game.
    KeepSeat,
    /// Ended: the lobby, or single player.
    EndSeat,
}

/// Who a welcomed session plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Seat {
    /// Distinct among the welcomed sessions. It stamps the session's commands, so it
    /// orders them within a day (D24) and names the player in the command log.
    pub player: u16,
    pub claim: Claim,
}

/// What a seat commands (D24): one state, so a seat can't be both a nation's and
/// sandbox. The wire's `nation` and `sandbox` fields come from it (`encode.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Claim {
    /// In the lobby, before claiming a nation (M4-2): commands nothing.
    Unclaimed,
    /// One nation.
    Nation(u32),
    /// Every nation (only on a server run with `--sandbox`).
    Sandbox,
}

impl Claim {
    pub(crate) fn nation(self) -> Option<u32> {
        match self {
            Claim::Nation(n) => Some(n),
            Claim::Unclaimed | Claim::Sandbox => None,
        }
    }
}

impl Seat {
    /// The nation this seat holds, if it holds one.
    pub(crate) fn nation(&self) -> Option<u32> {
        self.claim.nation()
    }

    /// Whether this seat may command `nation` (D24): its own, or any if sandbox. A
    /// player in the lobby without a claim commands nothing.
    pub(crate) fn commands(&self, nation: usize) -> bool {
        match self.claim {
            Claim::Sandbox => true,
            Claim::Nation(n) => n as usize == nation,
            Claim::Unclaimed => false,
        }
    }
}

/// A started game's seat whose player left: kept for their resume token, so they can
/// reclaim it (D24). It still holds its player id and nation, and counts toward the
/// player limit.
#[derive(Clone, Debug)]
struct Reservation {
    seat: Seat,
    name: String,
}

pub(crate) struct SessionTable {
    rows: BTreeMap<u64, Session>,
    /// Seats kept for players who left a started game, by resume token.
    reserved: BTreeMap<NonZeroU64, Reservation>,
    /// The session that sets the speed, saves, loads and kicks (D24). Always a
    /// welcomed session, or `None`.
    host: Option<u64>,
    /// How the host is elected and succeeded (D24).
    host_rule: HostRule,
}

/// One player as the lobby shows them (`LobbyPlayer`, D24): what the table says, for
/// `encode::lobby_state` to put on the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbyEntry {
    pub player: u16,
    pub name: String,
    pub nation: Option<u32>,
    pub sandbox: bool,
    pub ready: bool,
    pub host: bool,
    /// Left the started game; the seat waits for their resume token (D24).
    pub away: bool,
}

impl Session {
    /// Starts the session's update stream again, as for a new connection: the
    /// default subscription, an empty window (D23), a fresh throttle (M4-7) and no
    /// pending `Subscribe` answer (the temporary `answer_limit`). For a new game (a
    /// load) and an ended seat; they always reset together.
    pub(crate) fn restart_updates(&mut self) {
        self.subscription = CheckedSubscription::default();
        self.window = UpdateWindow::default();
        self.throttle.restart();
        self.answers.reset();
    }
}

impl SessionTable {
    /// The dedicated server's admin, if it has one (`HostRule::Admin`).
    pub(crate) fn admin(&self) -> Option<&crate::Admin> {
        match &self.host_rule {
            HostRule::Admin(admin) => Some(admin),
            HostRule::FirstPlayer => None,
        }
    }

    /// An empty table whose host follows `host_rule`.
    pub(crate) fn new(host_rule: HostRule) -> Self {
        SessionTable { rows: BTreeMap::new(), reserved: BTreeMap::new(), host: None, host_rule }
    }

    /// A new connection, not yet welcomed.
    /// `bandwidth` throttles it if it is remote (D24, M4-7).
    /// `answers` limits its `Subscribe` answers (temporary, `answer_limit`).
    pub(crate) fn connect(&mut self, id: u64, conn: ConnHandle, bandwidth: Bandwidth, answers: AnswerLimit) {
        let throttle = Throttle::new(conn.remote, bandwidth);
        let row = Session {
            throttle,
            answers,
            conn,
            subscription: CheckedSubscription::default(),
            seat: None,
            window: UpdateWindow::default(),
            name: String::new(),
            ready: false,
            token: None,
            stalled: false,
            recent_commands: std::collections::VecDeque::new(),
        };
        self.rows.insert(id, row);
    }

    /// Removes a closed connection's row. If it was playing, its seat ends, or is
    /// kept for the player's resume token (`on_leave`, D24): see [`Vacated`].
    pub(crate) fn remove(&mut self, id: u64, on_leave: OnLeave) -> Option<Vacated> {
        let row = self.rows.remove(&id)?;
        let seat = row.seat?;
        if on_leave == OnLeave::KeepSeat {
            let token = row.token.expect("a seated session has a token");
            self.reserved.insert(token, Reservation { seat, name: row.name });
        }
        Some(self.vacated(id, seat, row.stalled))
    }

    /// Ends the session's seat while its connection is still open (a kick, a load
    /// without its nation): it no longer plays, and the admission choke point drops
    /// whatever it still sends. Its row stays until its connection closes. The seat
    /// is not kept.
    pub(crate) fn unseat(&mut self, id: u64) -> Option<Vacated> {
        let row = self.rows.get_mut(&id)?;
        let seat = row.seat.take()?;
        let was_stalled = std::mem::take(&mut row.stalled);
        // It gets no more updates: nothing it was owed stays held (M4-7).
        row.restart_updates();
        Some(self.vacated(id, seat, was_stalled))
    }

    /// Seats a connected session in the seat kept for `token` (D24): the same player
    /// id and nation. The token stays valid, for a later drop.
    /// `admin_proved`: the `Hello` carried the admin password (D24, M4-6).
    pub(crate) fn resume(&mut self, id: u64, token: NonZeroU64, admin_proved: bool) -> Result<Seat, Refusal> {
        let Reservation { seat, name } = self.reserved.remove(&token).ok_or(Refusal::UnknownToken)?;
        let row = self.rows.get_mut(&id).expect("a session is connected before it says Hello");
        debug_assert!(row.seat.is_none(), "net lets a session say Hello only once");
        row.seat = Some(seat);
        row.ready = true;
        row.token = Some(token);
        row.name = name;
        self.elect(id, admin_proved);
        Ok(seat)
    }

    /// Drops the seat kept for an away `player`, if any (a kick, D24). Returns
    /// whether there was one.
    pub(crate) fn forget(&mut self, player: u16) -> bool {
        let before = self.reserved.len();
        self.reserved.retain(|_, r| r.seat.player != player);
        self.reserved.len() < before
    }

    /// A load sent a multiplayer game back to the lobby (M4-5): each player keeps a
    /// claim the loaded game has (`nations` of them) and is unclaimed otherwise, and
    /// nobody is ready. A seat is never widened: a lost claim becomes `Unclaimed`,
    /// which commands nothing, never `Sandbox`. (The load drops kept seats itself,
    /// in every phase: `Sim::load_game`.)
    pub(crate) fn load_into_lobby(&mut self, nations: usize) {
        for row in self.rows.values_mut() {
            let Some(seat) = row.seat.as_mut() else { continue };
            if seat.nation().is_some_and(|n| n as usize >= nations) {
                seat.claim = Claim::Unclaimed;
            }
            row.ready = false;
        }
    }

    /// Drops every kept seat: a load replaced the game, whose nations they hold.
    pub(crate) fn forget_all(&mut self) {
        self.reserved.clear();
    }

    /// Counts a command from `id` arriving at `now`, if fewer than `per_second`
    /// arrived in the second before (D24's rate limit). Returns whether it is
    /// admitted; a refused command doesn't count.
    pub(crate) fn admit_command(&mut self, id: u64, now: std::time::Instant, per_second: u32) -> bool {
        let Some(row) = self.rows.get_mut(&id) else { return false };
        let window = std::time::Duration::from_secs(1);
        while row.recent_commands.front().is_some_and(|&t| now.duration_since(t) >= window) {
            row.recent_commands.pop_front();
        }
        if row.recent_commands.len() >= per_second as usize {
            return false;
        }
        row.recent_commands.push_back(now);
        true
    }

    /// Marks a session silent past the pause threshold, as the network reported it
    /// (D24), whatever the game's phase: the row mirrors the report, and the sim
    /// decides whether it pauses anything. Returns whether it is newly stalled.
    pub(crate) fn stall(&mut self, id: u64) -> bool {
        self.rows.get_mut(&id).is_some_and(|r| !std::mem::replace(&mut r.stalled, true))
    }

    /// A stalled session spoke again. Returns whether it was stalled.
    pub(crate) fn unstall(&mut self, id: u64) -> bool {
        self.rows.get_mut(&id).is_some_and(|r| std::mem::take(&mut r.stalled))
    }

    /// The players a fairness pause waits for, in player order.
    pub(crate) fn waiting_for(&self) -> Vec<u16> {
        let mut players: Vec<u16> =
            self.rows.values().filter(|r| r.stalled).filter_map(|r| r.seat).map(|s| s.player).collect();
        players.sort_unstable();
        players
    }

    /// The session's resume token; `None` before it is seated.
    pub(crate) fn token(&self, id: u64) -> Option<NonZeroU64> {
        self.rows.get(&id).and_then(|s| s.token)
    }

    /// A resume token (D24): 64 bits from the OS's secure random source, so a token
    /// can't be guessed, from others or otherwise. Never 0, which `Hello` uses for
    /// "a new session".
    fn new_token() -> NonZeroU64 {
        loop {
            let mut bytes = [0u8; 8];
            getrandom::fill(&mut bytes).expect("the OS provides secure randomness");
            if let Some(token) = NonZeroU64::new(u64::from_le_bytes(bytes)) {
                return token;
            }
        }
    }

    /// Elects `id` host if the rule says so and nobody is host (D24).
    /// An admin must give the admin's name and prove it (`admin_proved`: the admin
    /// password, which the sim checked): a name alone proves nothing.
    fn elect(&mut self, id: u64, admin_proved: bool) {
        let elected = match &self.host_rule {
            HostRule::FirstPlayer => true,
            HostRule::Admin(admin) => admin_proved && self.rows.get(&id).is_some_and(|s| s.name == admin.name),
        };
        if elected && self.host.is_none() {
            self.host = Some(id);
        }
    }

    /// After a seat ended: the host role passes on by D24's rule.
    fn vacated(&mut self, id: u64, seat: Seat, was_stalled: bool) -> Vacated {
        let mut new_host = None;
        if self.host == Some(id) {
            self.host = None;
            if matches!(self.host_rule, HostRule::FirstPlayer) {
                new_host = self.lowest_player();
                self.host = new_host;
            }
        }
        Vacated { seat, new_host, last: self.players() == 0, was_stalled }
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

    /// Seats a connected session as a new player named `name`, with `claim` (a
    /// sandbox claim only if the caller has allowed it). It gets the lowest free player id, if fewer than
    /// `max_players` play and nobody holds the nation. The only way a session gets a
    /// seat.
    pub(crate) fn sit(
        &mut self,
        id: u64,
        name: &str,
        claim: Claim,
        max_players: usize,
        admin_proved: bool,
    ) -> Result<Seat, Refusal> {
        // Kept seats count: their players may come back (D24).
        if self.all_seats().count() >= max_players {
            return Err(Refusal::Full);
        }
        if let Some((nation, player)) = claim.nation().and_then(|n| self.holder(n).map(|p| (n, p))) {
            return Err(Refusal::NationTaken { nation, player });
        }
        let seat = Seat { player: self.free_player(), claim };
        let row = self.rows.get_mut(&id).expect("a session is connected before it says Hello");
        debug_assert!(row.seat.is_none(), "net lets a session say Hello only once");
        row.seat = Some(seat);
        row.name = name.to_owned();
        row.ready = false;
        self.rows.get_mut(&id).expect("just seated").token = Some(Self::new_token());
        self.elect(id, admin_proved);
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
        let claim = nation.map_or(Claim::Unclaimed, Claim::Nation);
        if seat.claim != claim {
            seat.claim = claim;
            row.ready = false;
        }
        Ok(())
    }

    /// In the lobby: marks the player ready, or not. Only a player who holds a
    /// nation, or a sandbox seat, can be ready.
    pub(crate) fn set_ready(&mut self, id: u64, ready: bool) -> Result<(), Refusal> {
        let row = self.rows.get_mut(&id).expect("only a seated session readies");
        let seat = row.seat.expect("only a seated session readies");
        if ready && seat.claim == Claim::Unclaimed {
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
                    nation: seat.nation(),
                    sandbox: seat.claim == Claim::Sandbox,
                    ready: s.ready,
                    host: self.host == Some(id),
                    away: false,
                })
            })
            .chain(self.reserved.values().map(|r| LobbyEntry {
                player: r.seat.player,
                name: r.name.clone(),
                nation: r.seat.nation(),
                sandbox: r.seat.claim == Claim::Sandbox,
                ready: true,
                host: false,
                away: true,
            }))
            .collect();
        entries.sort_by_key(|e| e.player);
        entries
    }

    /// The session's seat, if it was welcomed.
    pub(crate) fn seat(&self, id: u64) -> Option<Seat> {
        self.rows.get(&id).and_then(|s| s.seat)
    }

    /// The seated sessions whose throttle holds an update they are owed, with when
    /// it may go out (D24, M4-7). The one definition `Sim::next_flush` and
    /// `Sim::flush` share: a row that isn't seated is never sent an update, so it
    /// never counts.
    pub(crate) fn held_updates(&self) -> impl Iterator<Item = (u64, std::time::Instant)> + '_ {
        self.rows
            .iter()
            .filter(|(_, s)| s.seat.is_some() && s.window.owed())
            .filter_map(|(&id, s)| s.throttle.ready_at().map(|at| (id, at)))
    }

    /// The seated sessions whose `Subscribe` answer was deferred by the temporary
    /// map-request limit (`answer_limit`), with when it
    /// may go out. `Sim::next_flush` and `Sim::flush` use it beside
    /// [`Self::held_updates`].
    pub(crate) fn pending_answers(&self) -> impl Iterator<Item = (u64, std::time::Instant)> + '_ {
        self.rows.iter().filter(|(_, s)| s.seat.is_some()).filter_map(|(&id, s)| s.answers.due_at().map(|at| (id, at)))
    }

    /// Every connection, welcomed or not, in session order.
    pub(crate) fn all(&self) -> impl Iterator<Item = &Session> {
        self.rows.values()
    }

    /// Every occupied seat: playing, or kept for a player who left (D24). The one
    /// definition of occupancy, for player ids, nations and the player limit;
    /// [`Self::players`] counts only those playing.
    fn all_seats(&self) -> impl Iterator<Item = Seat> + '_ {
        self.rows.values().filter_map(|s| s.seat).chain(self.reserved.values().map(|r| r.seat))
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

    /// The lowest player id that neither a welcomed session nor a kept seat holds. A
    /// player who comes back without their resume token gets it; with the token, they
    /// get their old seat back ([`Self::resume`]).
    pub(crate) fn free_player(&self) -> u16 {
        let mut taken: Vec<u16> = self.all_seats().map(|s| s.player).collect();
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

    /// The player holding `nation`, playing or away with a kept seat. Sandbox seats
    /// hold none.
    pub(crate) fn holder(&self, nation: u32) -> Option<u16> {
        self.all_seats().find(|s| s.claim == Claim::Nation(nation)).map(|s| s.player)
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
        table.connect(id, conn, Bandwidth::default(), AnswerLimit::default());
        let claim = nation.map_or(Claim::Unclaimed, Claim::Nation);
        assert_eq!(table.sit(id, "p", claim, 8, false), Ok(Seat { player, claim }));
    }

    #[test]
    fn player_ids_fill_the_lowest_gap() {
        let mut t = SessionTable::new(HostRule::FirstPlayer);
        assert_eq!(t.free_player(), 0);
        seated(&mut t, 1, 0, None);
        seated(&mut t, 2, 1, Some(0));
        seated(&mut t, 3, 2, Some(1));
        assert_eq!(t.free_player(), 3);
        let _ = t.remove(2, OnLeave::EndSeat);
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
        let v = t.remove(7, OnLeave::EndSeat).unwrap();
        assert_eq!((v.new_host, v.last, t.host()), (Some(9), false, Some(9)), "player 1 is session 9");
        assert_eq!(t.unseat(5).map(|v| (v.new_host, v.last)), Some((None, false)), "not the host: no change");
        assert_eq!(t.unseat(9).map(|v| (v.new_host, v.last)), Some((None, true)), "nobody left to pass it to");
        assert_eq!((t.host(), t.unseat(9)), (None, None), "a seat ends once");
    }

    #[test]
    fn an_admin_is_host_by_name_and_the_role_never_passes() {
        let mut t = SessionTable::new(HostRule::Admin(crate::Admin {
            name: "ada".into(),
            password: crate::Secret::new("s3cret"),
        }));
        seated(&mut t, 1, 0, None);
        assert_eq!(t.host(), None, "the first player isn't host on a dedicated server");
        let (conn, _rx) = ConnHandle::for_test();
        t.connect(2, conn, Bandwidth::default(), AnswerLimit::default());
        assert!(t.sit(2, "ada", Claim::Unclaimed, 8, false).is_ok());
        assert_eq!(t.host(), None, "the admin's name without its password is nobody");
        let (conn, _rx) = ConnHandle::for_test();
        t.connect(3, conn, Bandwidth::default(), AnswerLimit::default());
        assert!(t.sit(3, "ada", Claim::Unclaimed, 8, true).is_ok());
        assert_eq!(t.host(), Some(3));
        assert_eq!(t.remove(3, OnLeave::EndSeat).map(|v| v.new_host), Some(None));
        assert_eq!(t.host(), None);
        let _ = t.remove(2, OnLeave::EndSeat);
        let (conn, _rx) = ConnHandle::for_test();
        t.connect(2, conn, Bandwidth::default(), AnswerLimit::default());
        assert!(t.sit(2, "ada", Claim::Unclaimed, 8, true).is_ok());
        assert_eq!(t.host(), Some(2));
        assert_eq!(t.remove(2, OnLeave::EndSeat).map(|v| v.new_host), Some(None));
        assert_eq!(t.host(), None);
    }

    #[test]
    fn commands_are_limited_per_session_per_second() {
        let mut t = SessionTable::new(HostRule::FirstPlayer);
        seated(&mut t, 1, 0, None);
        let start = std::time::Instant::now();
        let admitted = (0..30).filter(|_| t.admit_command(1, start, 20)).count();
        assert_eq!(admitted, 20, "twenty in one instant");
        let later = start + std::time::Duration::from_millis(1_000);
        assert!(t.admit_command(1, later, 20), "a second later the window has room again");
    }

    #[test]
    fn sit_refuses_a_full_table_and_a_taken_nation() {
        let mut t = SessionTable::new(HostRule::FirstPlayer);
        seated(&mut t, 1, 0, Some(0));
        for id in [2, 3] {
            let (conn, _rx) = ConnHandle::for_test();
            t.connect(id, conn, Bandwidth::default(), AnswerLimit::default());
        }
        assert_eq!(t.sit(2, "p", Claim::Nation(0), 8, false), Err(Refusal::NationTaken { nation: 0, player: 0 }));
        assert_eq!(t.sit(2, "p", Claim::Nation(1), 1, false), Err(Refusal::Full));
        assert_eq!(t.sit(3, "p", Claim::Nation(1), 2, false), Ok(Seat { player: 1, claim: Claim::Nation(1) }));
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
        t.connect(3, conn, Bandwidth::default(), AnswerLimit::default()); // connected, not welcomed
        assert_eq!((t.holder(0), t.holder(1)), (None, Some(1)));
        assert_eq!(t.players(), 2);
    }
}
