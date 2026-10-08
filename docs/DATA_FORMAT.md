# Data Format

Game data is TOML (see [DECISIONS.md D9](DECISIONS.md#d9-data-format-toml)), loaded and validated by `crates/pax_data`. The Rust structs in `crates/pax_data/src/schema.rs` are the authoritative schema; keep this page in sync with them.

## General rules

- **Numbers** may be written as integers (`2`), decimals (`0.015`) or strings (`"0.015"`). They are converted to `Fixed` exactly. **More than 6 decimal places is an error**, so the engine never silently rounds data.
- **Keys** (`key = "grain"`) are stable identifiers used for cross-references. Duplicates are errors.
- **Unknown fields are errors.** This catches typos such as `base_prise`.
- The loader reports **every** problem in one run rather than stopping at the first.
- Order in the file determines internal ids. Reordering entries changes state hashes, so re-record golden files afterwards.

## Definitions directory (`data/`)

`data/` is the game's content and grows over time. `scenarios/mini_valley` has its own frozen copy in `scenarios/mini_valley/defs/`, because it is the golden regression fixture: content changes must not move its hashes.

### `goods.toml`

```toml
[[good]]
key = "grain"
base_price = 1.0      # opening price in every market; > 0
```

### `professions.toml`

```toml
[[profession]]
key = "farmer"
spend_rate = 0.2                       # fraction of cash budgeted per day, in (0, 1]
subsistence = { grain = 0.01 }         # γ: units per person per day ("life needs")
preference = { grain = 0.3, furniture = 0.35, wine = 0.35 }  # β weights
```

- `preference` weights are relative. The loader normalises them to sum to exactly 1, and at least one must be positive.
- See [DECISIONS.md D2](DECISIONS.md#d2-consumer-demand-linear-expenditure-system-stone-geary) for the demand formula.

### `production.toml`

```toml
[[producer_type]]
key = "furniture_workshop"
output = "furniture"
output_per_worker = 0.006   # units per employed person per day; > 0
inputs = { timber = 2.0 }   # units of input per unit of output; each > 0 (optional)
worker = "craftsman"        # profession supplying labour
owner = "capitalist"        # profession receiving dividends
labor_share = 0.5           # target wage share of value added, in [0, 1]
input_spend_rate = 0.5      # fraction of cash spendable on inputs per day, in [0, 1] (default 0)
```

### `rules.toml`

| Field | Meaning | Constraint |
|---|---|---|
| `days_per_month` | Month length for monthly systems | ≥ 1 |
| `market.max_iterations` | Tâtonnement iteration cap | ≥ 1 |
| `market.step` | Initial step λ | (0, 1] |
| `market.step_decay_iterations` | Step decay `d` in `λ·d/(d+k)` | ≥ 1 |
| `market.tolerance` | Stop when all \|z\| ≤ this | ≥ 0 |
| `market.max_daily_change` | Max executed price move per day | (0, 1) |
| `market.min_stock` | Stocks below this are dust: not offered, and the price is held | ≥ 0 |
| `market.price_floor`, `price_ceiling` | Technical bounds; every `base_price` must lie inside | 0 < floor < ceiling |
| `firms.wage_stickiness_days` | Days to close the gap to the target wage | ≥ 1 |
| `firms.revenue_smoothing_days` | Horizon of the value-added average | ≥ 1 |
| `firms.reserve_days` | Cash reserve before dividends, in days of wage bill | ≥ 0 |
| `firms.dividend_payout_rate` | Daily payout of cash above reserve | [0, 1] |
| `firms.target_stock_days` | Stop producing at this many days of unsold output | ≥ 1 |
| `firms.subsistence_wage_multiple` | Target wage floor, as a multiple of a worker's daily subsistence cost | ≥ 0 |
| `demographics.growth_rate` | Monthly growth at full life needs | [0, 1] |
| `demographics.starvation_rate` | Monthly decline at zero life needs | [0, 1] |
| `demographics.mobility_rate` | Share of a pool's unemployed who move to vacancies each month (D18) | [0, 1] |
| `demographics.migration_rate` | Share of a province's surplus workers who migrate within their market each month (D20) | [0, 1] |
| `politics.militancy_rise` | Monthly militancy rise at zero life needs (D19) | [0, 1] |
| `politics.militancy_tax_weight` | Monthly rise per unit of income tax rate | [0, 1] |
| `politics.militancy_decay` | Share of militancy that fades each month | [0, 1] |

## Scenario directory (`scenarios/<name>/`)

`scenario.toml`:

```toml
name = "Mini Valley"
seed = 1836                 # root of all randomness
data = "../../data"         # definitions directory, relative to this file
map = "map"                 # optional: province map directory, relative to this file (M3-7)

[[nation]]                  # optional (D15); omit for a stateless scenario
key = "lowland_kingdom"
treasury = 2000             # opening treasury cash (default 0)
income_tax_rate = 0.08      # share of wages and dividends withheld (default 0)
transfer_rate = 0.05        # share of the treasury paid to its POPs per day (default 0)
consumption_rate = 0.05     # share of the treasury spent on goods per day (default 0, D16)
basket = { grain = 0.2, tools = 0.8 }   # relative weights; required if consumption_rate > 0

[[market]]
key = "valley"
nation = "lowland_kingdom"  # optional: markets without a nation are untaxed

[[province]]
key = "riverside"
market = "valley"

[[pop]]
province = "riverside"
profession = "farmer"
size = 60000                # people
cash = 6000                 # total holdings of the POP, not per capita (D7)
# POP rows are stored grouped by market (stable sort at load); file order is kept within a market.

[[producer]]
type = "farm"               # producer_type key
province = "riverside"
capacity = 60000            # maximum workers
cash = 3000
wage = 0.014                # opening daily wage per worker
stock = 0                   # opening unsold output (optional)
```

### Command log (optional, D21)

`scenario.toml` may name a command log, `commands = "commands.toml"`, which is replayed during the run:

```toml
[[command]]
day = 360                   # applied at the start of this (0-based) day
type = "set_income_tax"     # set_income_tax | set_transfer_rate | set_consumption_rate
nation = "lowland_kingdom"
rate = 0.12                 # in [0, 1]
```

Commands of the same day apply in file order. Every command is checked at load against the scenario's initial world with the engine's own rule (`World::validate`, D21). Load errors are:
- unknown nations or types;
- rates outside [0, 1];
- a positive `set_consumption_rate` for a nation without a basket.

`golden.hashes` sits next to `scenario.toml` and is generated by `pax_cli record` (see [CONTRIBUTING.md](../CONTRIBUTING.md)).

## Province map (`map/`, M3-7)

The picture the client draws, and which province each colour is. The engine never reads it. A scenario names its map directory with `map = "map"`: a relative path inside the scenario directory, with `/` separators and no `.` or `..` (`pax_map::check_map_dir`; the server refuses anything else at load, and the client refuses it in `StaticData.map_dir`). The directory holds two files:

- **`provinces.png`:** every province painted in one unique colour, 8-bit RGB or RGBA (alpha is ignored). Province edges must be hard, because anti-aliasing creates colours that belong to no province. At most 16384 pixels per side.
- **`provinces.toml`:**

```toml
background = [40, 70, 110]   # optional: pixels of this colour belong to no province (sea)

[[province]]
key = "riverlands"           # a province key from scenario.toml
color = [120, 170, 90]       # its colour in provinces.png
label = [170, 150]           # pixel [x, y] where its name is drawn
```

Load errors:
- a province of the scenario missing, listed twice, or unknown to it;
- two provinces sharing a colour, or one using the background colour;
- a pixel whose colour is neither a province's nor the background (the first five such colours are listed, then a count of the rest);
- a province with no pixels, or a label outside its own pixels;
- an image that isn't 8-bit RGB or RGBA PNG;
- a scenario with more than 65,535 provinces (`pax_map::MAX_PROVINCES`): the client's ID texture can't draw more, so the server refuses such a map at load.

The two files are part of the scenario's content hash, and together they are `StaticData.map_hash` (D22). One reader, `pax_map`, validates them for the server and loads them for the client (D9's one exception). The client finds its copy where the server says (`StaticData.map_dir`) and refuses one whose hash differs from the server's. `scripts/draw_two_states_map.py` regenerates the `two_states` map.

## Save files (`saves/<name>.toml`, D23)

Written by the server's `SaveGame` and read by `LoadGame` (`pax_data::save`). A save is the scenario plus every command that applied, plus checkpoints proving a replay reaches the same state:

```toml
format = 1
scenario = "scenarios/two_states"   # as the server was started
content_hash = "0x9a1c0e5b7d2f3a11" # the scenario's content hash, hex (TOML integers are signed 64-bit)
day = 360                           # the day the game was saved on
snapshot_hash = "0x2c4e8a01f9b3d775" # World::state_hash of the snapshot <name>.world (D10)

[[checkpoint]]                      # World::state_hash when world.day reached `day` (every 30 days)
day = 30
state_hash = "0x51f04c2a9be17d3e"

[[command]]                         # the command-log format above, plus `player`
day = 12
player = 0                          # absent: one of the scenario's scripted commands
type = "set_income_tax"
nation = "lowland_kingdom"
rate = 0.150000
```

- Commands are listed in the order they applied, so days never decrease. Every command's day is before `day`.
- There is one checkpoint for each multiple of 30 up to `day`, in order.
- Every load (`LoadGame`) refuses the save if:
  - the format is unknown;
  - the scenario's files changed (`content_hash`);
  - any command is invalid by the engine's rule (`World::validate`, D21), out of day order, or not before `day`;
  - the checkpoints aren't exactly the checkpoint days up to `day`;
  - `day` is a checkpoint day and its checkpoint differs from `snapshot_hash`;
  - the snapshot is missing or damaged, isn't of `day` with `snapshot_hash`, its scenario tables differ from the scenario's, or it breaks the engine's table rules (`World::check_tables`).

  `LoadGame` doesn't replay, so beyond these checks it trusts the log.
- **Replaying** (`pax_cli replay <saves/name.toml>`, `pax_data::save::load_by_replay`, D23) makes the same checks, then re-applies the log, and refuses the save if any checkpoint, or the end state, differs. It prints the final `state_hash`, and it is how a bug report's save is reproduced (D10).
- Save names are 1 to 64 characters of `[A-Za-z0-9_-]`.
- **Snapshots.** `<name>.world`, next to `<name>.toml`, is the binary world snapshot (format in `pax_data::snapshot`'s documentation). Its name always comes from the save's, never from the file. A save without `snapshot_hash` has no snapshot, and loads by replay.
