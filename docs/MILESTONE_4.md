# Milestone 4: Multiplayer

**Goal:** several players, each running a nation, play one game on one authoritative server. The server is either hosted by a player or dedicated, over a LAN or the internet. Games can be saved, a dropped player can rejoin, and a slow or hostile client can't stall or corrupt the game for everyone else.

M4 builds on [Milestone 3](MILESTONE_3.md) without replacing it: the same server, protocol and client. Binding rules: D10, D22, D23 and **D24 (multiplayer authority)** in [DECISIONS.md](DECISIONS.md). The wire format is in [NETWORK_PROTOCOL.md](NETWORK_PROTOCOL.md).

**Prerequisite:** M3's definition of done, in particular the session replay test and the measured `DayUpdate` sizes.

> [!IMPORTANT]
> **Reminder: set up the Dockerfile in M4.** The early draft is kept at [`docs/drafts/Dockerfile.m4-draft`](drafts/Dockerfile.m4-draft), out of the repository root so tools don't pick it up. It **does not build**:
> - it compiles `schemas/protocol.fbs`, which has been split into `common.fbs`, `client.fbs` and `server.fbs`;
> - it installs Debian's flatc 2.0.8, which doesn't match the pinned 24.3.25 runtime crate.
>
> Don't use it, and don't wire it into CI or releases. Task **M4-8** turns it into a working root `Dockerfile` (see "Docker (M4-8)" below).

## What changes from M3

| Area | M3 | M4 |
|---|---|---|
| Sessions | one, localhost | many; lobby; resume tokens |
| Nations | sandbox (any nation) | one nation per player, assigned in the lobby; the server enforces it (D24) |
| Speed, pause, saves | the player | the host; any player may pause (D24) |
| A slow client | sees coalesced updates | the same, plus fairness pauses and drop rules in wall-clock time (D24) |
| Transport | plain TCP on 127.0.0.1 | TLS when not on localhost; server password |
| Hosting | launched by the client | player-hosted (`--bind 0.0.0.0`) or dedicated (Docker) |

### Carried over from M3

Found during M3 and left for M4, with the reason:
- **Broadcast `Welcome` after another player's load.** The client's decoder accepts a second `Welcome` only as the answer to its own `LoadGame`. D23 sends every session a new `Welcome` after a load, so with several players (D24) the other clients must accept it too.
- **Non-blocking server start in the bridge.** `PaxClient.launch` waits for the local server on the main thread (two_states starts in well under a second). A pollable start-up state keeps the UI responsive for large scenarios.
- **The tick is over D13 at M2 content: task M4-11** (the owner closed M3 with it moved here, 2026-10-08). `two_states` (12 goods, nations) replicated to about 1M POP rows ticks in about 126 ms on 8 threads at 1,500 markets, over the 100 ms budget, while D13's 4-good reference world takes 44 ms on the same machine (D13, "Measured").
- **A faster state hash.** The per-day `state_hash` costs about 31 ms at 1M POP rows (M3 kept it, by the owner's decision). FNV over 8-byte words would cut it to a few ms, but changes every golden file (D11).
- **`pax_cli bench --scale` overflows at month end.** The copies `--scale` makes share their identities, so the month-end compaction merges them past `u32`. The default 30 days reaches month end, so the AGENTS.md D13 command panics as written; `--days 29` works (38 ms/day at 1M rows on 8 threads). The benchmark should give the copies distinct identities (task M4-11). Then `pax_server`'s `server_day_budget` test should build its world through the same `pax_data::bench` helper (one that picks the scale for a target row count), so D13's two "Measured" rows describe the same kind of world.
- **Saves record the scenario by path.** A save stops loading if the game or scenario directory moves. Record the scenario by name, resolved against the server's scenarios root, before saves are shared.

## Design (D24, accepted)

**Authority.** A session commands at most one nation.
- The server checks every command's `nation` against the session's nation before `World::validate`, and a mismatch gets `NotPermitted`. Permission and rule validity (D21) are separate checks, in that order.
- Sandbox mode (`Hello` without `requested_nation`) is refused unless the server runs with `--sandbox`.

**Command order.** Commands apply at the start of the next tick in `(day, player, sequence)` order, all three stamped by the server (D10, D22).
- Today no two players' commands interact, since each changes only its own nation, so the order within a day doesn't affect fairness.
- **Any future command that can conflict** (diplomacy, trade deals, bids on the same thing) must define its own conflict rule in its decision. Player order is only a deterministic tie-break, and on its own it would always favour the lower player id.

**Lobby.**
1. Players connect, and the server assigns each a `player` id and a resume token.
2. The host picks a scenario or save. Players claim nations; the server refuses a nation that's already taken. Players mark themselves ready.
3. The host starts the game.

Loading a save goes through the lobby, and players reclaim their nations.

**Host.** The host is the first session on a player-hosted server, or a configured admin name on a dedicated server.
- Only the host sets speed, unpauses, saves, loads and kicks.
- Any player may pause; the host unpauses.

**Lag and fairness, in wall-clock time** (game speed changes the meaning of "N ticks"):
- Updates always coalesce per client (D23). A slow client sees fewer days, never older ones.
- **Fairness pause:** if a client has sent nothing (no `Ack`, `Ping` or command) for **5 s**, the game pauses with "waiting for *player*". Any traffic resumes it.
- **Drop:** after **30 s** of silence the session is dropped and the game continues. The nation keeps its current policies; nothing plays it.
- **Rejoin:** `Hello { resume_token }` within the save's lifetime reclaims the nation.
- The thresholds are server settings. The defaults above are for playtesting to confirm.
- In multiplayer the 30 s drop replaces M3's 10 s idle timeout, so a silent client gets the fairness pause first. Single player keeps the 10 s rule.

**Desync.** Not applicable: clients don't simulate (D10). `state_hash` in each `DayUpdate` identifies the state in bug reports. The server's command log replays any session exactly, which gives the same guarantee as lockstep desync detection.

**Security.**
- TLS (rustls) whenever the server is not bound to localhost, plus an optional server password in `Hello`. Player accounts are out of scope.
- Every client message is verified and size-limited (D22).
- Commands are rate-limited (default 20 per second per session; `RateLimited`).
- Saves are written only to the server's save directory, with file names restricted to `[A-Za-z0-9_-]`.

**Bandwidth.** The target is **≤ 100 KB/s per remote client** at speed 3.
- Remote sessions get at most 4 updates per second, coalesced.
- `MapView` is sent when the subscription changes and then every 5th update, because map colours don't need daily refresh.
- The summary and panels are sent every update. M3's measured sizes (about 8 KB summary, about 80 KB map at 10,000 provinces) put this well inside the target.

## Tasks

| ID | Task | Depends on | Notes |
|---|---|---|---|
| M4-0 ✅ | Accept D24 (this design) after review; confirm the lag thresholds | M3 done | Accepted by the owner on 2026-10-08, with the thresholds as server settings whose defaults a playtest confirms |
| M4-1 | **Several sessions:** session table on the sim thread; per-session subscriptions, flow-control windows and nations; broadcast of `ServerState` | M4-0 | Sessions are rows in a table, not objects holding references into `World` |
| M4-2 | **Lobby:** protocol additions (`LobbyState`, `ClaimNation`, `Ready`, `StartGame`, new union members at the end), nation claims, host start | M4-1 | Minor protocol version bump |
| M4-3 | **Authority:** permission check before `World::validate`; host-only controls; `NotPermitted`; `--sandbox` flag | M4-1 | Tests: a player can't change another nation's taxes; a non-host can't unpause or save |
| M4-4 | **Lag and drop rules:** fairness pause, drop, `Goodbye`, resume tokens and rejoin | M4-1 | Tests with a scripted client that stalls, recovers, disconnects and resumes |
| M4-5 | **Multiplayer saves:** the command log records the player for each command; loading goes through the lobby | M4-2 | The replay test covers a two-player session |
| M4-6 | **Transport security:** TLS for non-local binds, server password, per-session rate limit | M4-1 | Self-signed certificate for player-hosted games, with its fingerprint shown to join |
| M4-7 | **Bandwidth controls:** per-session update-rate cap; `MapView` refresh policy | M4-1 | Measured at long-term scale over a simulated 50 ms/1% loss link |
| M4-8 | **Dedicated server:** Dockerfile and compose file | M4-6 | See "Docker" below |
| M4-9 | **Client:** lobby screen, player list, "waiting for player" overlay, reconnect, host controls | M4-2 to M4-4 | |
| M4-10 | **Docs:** D24 accepted; NETWORK_PROTOCOL (lobby, security); REPO_SETUP or a hosting guide; this file's status | all | |
| M4-11 | **Tick budget at M2 content (D13):** profile the per-good and per-market loops and bring `two_states` at about 1M POP rows under 100 ms/day on 8 threads | M3 done | Carried over from M3's definition of done (item 4). First fix the `bench --scale` month-end overflow and share one `pax_data::bench` world-building helper with `server_day_budget`, so the two measurements describe the same world. The cost grows with goods and markets, so it must hold before content grows towards D13's 50 goods |

### Docker (M4-8)

The draft Dockerfile needs these fixes before it is useful:
- **Generated code:** don't run flatc in the image. Use `pax_protocol`'s checked-in generated code (M3-1). Debian bookworm's `flatbuffers-compiler` is 2.0.8, which doesn't match the 24.3.25 runtime crate.
- **`.dockerignore`:** add one excluding `target/` and `.git/`. Without it, `COPY . .` sends gigabytes of build output into the build.
- **Dependency caching:** cache dependencies in their own layer (e.g. `cargo-chef`), so a code change doesn't rebuild every dependency.
- **Non-root user:** run as a non-root user.
- **Saves volume:** put saves on a volume (`/app/saves`).
- **Configuration:** take settings through arguments or environment: scenario, bind address, password, admin name, `--sandbox`.
- **Health check:** use a TCP probe on the port.

## Definition of done

1. **Multiplayer works:** three players on two machines play `two_states` with different nations. Each sets policy only for their own nation; the server rejects attempts on other nations with `NotPermitted`.
2. **Host controls:** the host pauses, changes speed, saves and kicks; a non-host can only pause.
3. **Slow and lost players:** a player whose connection stalls triggers a fairness pause within 5 s, is dropped after 30 s while the others continue, and can rejoin with the resume token to reclaim their nation.
4. **Saves:** a two-player game saves, reloads through the lobby, and its replayed command log reproduces the original `state_hash` (CI).
5. **Bandwidth:** ≤ 100 KB/s per remote client at speed 3 at long-term scale, measured and recorded.
6. **Security:** TLS on non-local binds; fuzzing covers the lobby messages; command rate limits are enforced.
7. **Dedicated server:** `docker compose up` starts it with a persistent saves volume.
8. **Tick budget (D13, from M3):** `two_states` replicated to about 1M POP rows ticks within 100 ms/day on 8 threads, measured and recorded in D13's "Measured" column (M4-11).

## Out of scope for M4

- Player accounts, matchmaking, NAT traversal and relays (use port forwarding, a dedicated server or a LAN).
- Client-side prediction and lockstep.
- AI for unclaimed or dropped nations.
- Spectators, chat (beyond a possible stretch goal) and host migration.
- Cross-version play (`protocol_major` must match).
