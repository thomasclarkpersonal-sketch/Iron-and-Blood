# Iron and Blood: Godot client

The client is a Godot 4 project with a GDScript UI. It loads `crates/pax_godot`, a Rust GDExtension that owns everything about the wire protocol (D12, D22). The client never simulates: it shows what `pax_server` sends.

## Requirements
- **Godot 4.7** or later, the standard build (not .NET).
- **The Rust toolchain** from `rust-toolchain.toml`.

## Build and run

```bash
cargo build -p pax_godot                 # the bridge, loaded by client/pax_godot.gdextension
godot --path client                      # run (or open client/ in the Godot editor)
godot --path client --rendering-driver opengl3 -- --screenshot=/tmp/map.png   # run, save a screenshot, quit
```

- **Release export:** `cargo build -p pax_godot --release` builds the library a release export uses.
- **After changing Rust code:** rebuild the bridge, then reload the project.

## Layout
| Path | What |
|---|---|
| `pax_godot.gdextension` | Where Godot finds the bridge library for each platform (`target/{debug,release}`) |
| `spike/` | The M3-0 spike: decodes demo frames and draws a province map with the province-ID shader. Replaced by the real client in M3-8 |

Anything about the protocol (framing, decoding, conversions) belongs in `crates/pax_godot`, with Rust tests, not in GDScript.
