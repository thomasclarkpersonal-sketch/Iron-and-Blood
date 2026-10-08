# Iron and Blood: Godot client

The client is a Godot 4 project with a GDScript UI. It loads `crates/pax_godot`, a Rust GDExtension that owns everything about the wire protocol (D12, D22): the connection, the local server it launches, decoding, acknowledgements and keep-alive. The client never simulates: it shows what `pax_server` sends.

## Requirements
- **Godot 4.7** or later, the standard build (not .NET).
- **The Rust toolchain** from `rust-toolchain.toml`.

## Build and run

```bash
cargo build -p pax_godot -p pax_server    # the bridge (loaded by pax_godot.gdextension) and the server it launches
godot --path client                       # run (or open client/ in the Godot editor)
```

Command-line options go after `--`:

| Option | What |
|---|---|
| `--autostart` | Skip the start screen |
| `--scenario=DIR` | Scenario directory (default `../scenarios/two_states`) |
| `--server=PATH` | `pax_server` binary (default `../target/debug/pax_server`, or `$PAX_SERVER`) |
| `--nation=N` | Play nation `N` (default: sandbox) |
| `--map-mode=N` | Start in map mode `N` (a `PaxKeys.MAP_MODE_*` value) |
| `--tab=NAME` | Start on a side-panel tab: `World`, `Nation`, `Market` or `Province` |
| `--select=N` | Select province `N` at start |
| `--open-saves` | Open the save/load menu at start |
| `--screenshot=PATH` | Run to day 30, save a screenshot with the debug overlay, quit (or, if the connection is lost, screenshot the connection-lost screen and fail) |
| `--host=N` | Host a multiplayer game for `N` players at start; with `--screenshot`, the screenshot is of the lobby |
| `--smoke-host` | Headless check (`smoke_host.gd`): host a game over TLS, claim a nation, get ready, start, play, print `SMOKE OK (host) …` (CI runs this too) |
| `--smoke` | Headless check (`smoke.gd`): select a province, switch to the Price map, set a policy, save, list and load at day 20, run to day 40, check the views, the policy and the reload, print `SMOKE OK …`, quit (CI runs this). A stalled run names the stage it stalled at |

Examples:

```bash
godot --path client --rendering-driver opengl3 -- --screenshot=/tmp/client.png
godot --headless --path client --import && godot --headless --path client -- --smoke
```

- **The map:** wheel zooms, right- or middle-drag pans, a left click selects a province. The map files are read from the scenario directory and must hash to the server's `map_hash`.
- **Policies:** the Nation tab's sliders set income tax, transfers and government spending, in tenths of a percent, sent when a slider is released. The bridge converts them to the command's exact `Fixed` rate (D3). In sandbox, any nation can be chosen.
- **F3** toggles the debug overlay: day, `state_hash` and skipped days. A bug report quotes the hash (D23).
- **Saves:** the **Saves** button opens the save/load menu (D23): the server's saves, save under a name, load one (double-click or **Load**). A load replaces the game, so the map, panels and selection start afresh. Saves go to Godot's user data directory, under `saves/`.
- **Connection lost:** a `Goodbye`, a protocol error, a refusal or a closed connection shows the reason, with **Start a new game** (back to the start screen) and **Quit**, and in a multiplayer game **Rejoin**, which reclaims the seat with the resume token (D24).
- **Multiplayer (M4-9):** the start screen hosts or joins a game.
  - **Host a game** starts a server on this machine for 2 to 8 players, over TLS with a fresh certificate, and joins it. The lobby shows the port and the certificate's fingerprint to share with the players.
  - **Join a game** takes the host's address, port and fingerprint (and the server's password, if it has one). The bridge pins the fingerprint, so a different server's certificate is refused.
  - **The lobby:** claim a nation, mark ready; the host starts the game and can kick. In the game, only the host sets the speed (anyone may pause), and the top bar names any player the game is waiting for.
- **After changing Rust code:** rebuild the bridge, then reload the project.
- **First import of a fresh checkout:** with no `.godot/` yet, Godot registers the extension mid-scan, and the editor aborts when it exits (Godot 4.7.2 with godot-rust 0.5.5). The project imports fine; only the exit crashes, and later runs are unaffected. To avoid it, list the extension first: `mkdir -p client/.godot && echo "res://pax_godot.gdextension" > client/.godot/extension_list.cfg` (CI does this).

## Layout

| Path | What |
|---|---|
| `main.gd`, `main.tscn` | The app: start screen, session, routing events to the UI |
| `smoke.gd`, `smoke_host.gd` | The `--smoke` and `--smoke-host` runs, handed every event after main.gd has routed it |
| `ui/` | One script per UI part: `top_bar.gd`, `summary_panel.gd`, `debug_overlay.gd`; the map (`map_view.gd` with `map.gdshader`, `map_modes.gd`, `map_colors.gd`); the side panels (`nation_panel.gd` with the policy sliders, `market_panel.gd`, `province_panel.gd`, sharing `table.gd`); `save_menu.gd` and `lost_screen.gd`; `start_screen.gd` and `lobby_screen.gd` (M4-9); `format.gd` for display formatting |
| `pax_keys.gd` | Every Dictionary key and event tag the bridge uses, and every value GDScript passes back: wire enums (`MAP_MODE_*`, `SPEED_*`, `COMMAND_ERROR_*`, from the schema) and policy names. Generated from `crates/pax_godot/src/keys.rs`; don't edit it. The bridge rejects (and logs) any argument outside them, never coercing it |
| `pax_godot.gdextension` | Where Godot finds the bridge library for each platform (`target/{debug,release}`) |

Anything about the protocol (framing, decoding, conversions, session rules) belongs in `crates/pax_godot`, with Rust tests, not in GDScript.
