# Hosting a multiplayer game

How to play Iron and Blood with other people (Milestone 4). The rules behind it are D24 in [DECISIONS.md](DECISIONS.md); the wire details are in [NETWORK_PROTOCOL.md](NETWORK_PROTOCOL.md).

There are two ways to host:
- **From the client** (a player hosts): quick, for a game among friends. The server runs on the host's machine and stops when the host quits or starts a new game.
- **A dedicated server** (Docker): runs on its own, survives restarts, and has an admin.

Either way, every player needs the same game version: `protocol_major` must match, and each client draws the map from its own copy of the scenario, which must hash to the server's.

## From the client

1. **Host:** on the start screen, choose the number of players (2 to 8) and **Host a game**. The lobby shows a **port** and a **fingerprint**: give both, with your address, to the other players.
2. **Players:** on the start screen, enter the host's address, the port and the fingerprint, then **Join a game**.
3. **Everyone:** pick a nation and mark **Ready**. The host starts the game.

The host's server listens on every network interface over TLS, on a free port that changes each time. Other players must be able to reach that port: on a LAN that usually just works; over the internet, forward the port on the host's router, or use a dedicated server.

## A dedicated server

```bash
docker compose up -d                       # builds the image and starts the server on port 7777
docker compose logs pax-server | grep SHA-256   # the fingerprint to give the players
```

Settings go in `compose.yaml`'s `environment` (see the `Dockerfile` for the full list):

| Setting | What |
|---|---|
| `PAX_PLAYERS` | How many players (default 4) |
| `PAX_SCENARIO` | The scenario directory inside the image (default `scenarios/two_states`) |
| `PAX_PORT` | The port (default 7777; change the `ports` mapping too) |
| `PAX_PASSWORD_FILE` | A file holding the server password, e.g. a compose secret; players enter it when joining |
| `PAX_ADMIN`, `PAX_ADMIN_PASSWORD_FILE` | The admin: the client with this name that gives this password is the host (sets the speed, saves, loads, kicks) |
| `PAX_COMMANDS_PER_SECOND`, `PAX_PAUSE_AFTER`, `PAX_DROP_AFTER` | D24's rate limit (20) and lag thresholds (5 s and 30 s) |
| `PAX_UPDATES_PER_SECOND`, `PAX_MAP_EVERY` | D24's bandwidth for remote players: updates a second (4), and the map with every Nth update (5) |

- **Saves** live in the `saves` volume, and survive restarts and upgrades.
- **The certificate** is made on the first start and kept in the `tls` volume, so the fingerprint players pin stays the same. To use your own, mount `cert.pem` and `key.pem` into `/app/tls`.
- **Without Docker:** `pax_server --scenario scenarios/two_states --bind 0.0.0.0:7777 --players 4 --tls-cert cert.pem --tls-key key.pem` (or `--tls-self-signed`, whose fingerprint changes at every start). `pax_server` with no arguments lists every option.

## During the game

- **The host** sets the speed, saves, loads and kicks; anyone may pause (D24). Without an admin, the first player to join is host, and if the host's connection ends, the remaining player with the lowest id becomes host. A server hosted from the client belongs to the host's client: if the host's connection drops, the game goes on and the host can **Rejoin**; it ends when the host starts a new game or quits (players are told the server is shutting down), and goes on even while nobody is connected.
- **A silent player** (no message for 5 s, for example a frozen client) pauses the game for everyone, and the top bar says who it waits for. When they are back, the game resumes at its speed; after 30 s they are dropped and the others play on.
- **Rejoining:** a player who left or was dropped can rejoin from the connection-lost screen (**Rejoin**) and gets their nation back. Their seat waits for them, and nobody else can take that nation, until the host kicks them or loads a save.
- **Loading a save** sends everyone back to the lobby: players keep the nations the save has, re-claim the others, get ready, and the host starts again.

## Security

- Off localhost, a server with several players speaks only TLS (D24). Players pin the certificate by its fingerprint: a server with any other certificate is refused, so check the fingerprint came from the host.
- Passwords are read from files, so they never show in the process list. A wrong one is refused with the same words as a missing one.
- Each player can send at most 20 commands a second.
- Not covered yet: player accounts, NAT traversal and relays (MILESTONE_4, "Out of scope").
