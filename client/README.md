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
| `--screenshot=PATH` | Run to day 30, save a screenshot with the debug overlay, quit |
| `--smoke` | Headless check: run to day 40, print `SMOKE OK …`, quit (CI runs this) |

Examples:

```bash
godot --path client --rendering-driver opengl3 -- --screenshot=/tmp/client.png
godot --headless --path client --import && godot --headless --path client -- --smoke
```

- **F3** toggles the debug overlay: day, `state_hash` and skipped days. A bug report quotes the hash (D23).
- **Saves** go to Godot's user data directory, under `saves/`.
- **After changing Rust code:** rebuild the bridge, then reload the project.

## Layout

| Path | What |
|---|---|
| `main.gd`, `main.tscn` | The app: start screen, session, routing events to the UI |
| `ui/` | One script per UI part: `top_bar.gd`, `summary_panel.gd`, `debug_overlay.gd`, and `format.gd` for display formatting |
| `pax_keys.gd` | Every Dictionary key and event tag the bridge uses. Generated from `crates/pax_godot/src/keys.rs`; don't edit it |
| `pax_godot.gdextension` | Where Godot finds the bridge library for each platform (`target/{debug,release}`) |

Anything about the protocol (framing, decoding, conversions, session rules) belongs in `crates/pax_godot`, with Rust tests, not in GDScript.
