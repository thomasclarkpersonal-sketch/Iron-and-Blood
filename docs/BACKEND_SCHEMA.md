# Backend Architecture & Schema

This document describes the concrete Rust implementation. The code is authoritative for exact field names; this page explains structure and intent. Decisions are cited as D1, D2, … ([DECISIONS.md](DECISIONS.md)).

## 🦀 Workspace Structure

The repository root is a Cargo workspace:

```text
Cargo.toml                 workspace; release profile keeps overflow checks ON (D3)
crates/
├── pax_engine/            no IO
│   └── src/
│       ├── fixed.rs       Fixed: decimal fixed point, floor rounding, panics on overflow
│       ├── alloc.rs       largest-remainder splitting (exact sums)
│       ├── rng.rs         counter-based deterministic RNG
│       ├── hash.rs        FNV-1a state hashing (stable across Rust versions)
│       ├── groups.rs      counting-sort row grouping (replaces HashMap lookups)
│       ├── defs.rs        static definitions: goods, professions, producer types, rules
│       ├── world.rs       SoA tables: Geography, Pops, Producers, Markets; World
│       ├── tick.rs        step(): fixed system order + money conservation assert
│       └── systems/       labor, production, market, firms, demographics
├── pax_data/              TOML schema (schema.rs), validation, World builder, golden files
└── pax_cli/               run | record | verify | bench
data/                      base definitions
scenarios/<name>/          scenario.toml + golden.hashes
```

`pax_server` will be added in M3 (D10).

## 🧩 State Schema

There is no ECS framework (D8). Each table below is a set of equally long column `Vec`s, and an entity is a row index. Static definitions (`Defs`) are shared through `Arc` and excluded from the state hash.

### `Geography` (static topology)
| Column | Type | Notes |
|---|---|---|
| `province_keys` | `Vec<String>` | |
| `province_market` | `Vec<u32>` | Market row of each province. Nation and state are derived, never stored on POPs (D7). |
| `market_keys` | `Vec<String>` | |

### `Pops`
Rows are stored grouped by market: the loader sorts them stably. The market's parallel passes rely on this for small per-job accumulators. Correctness does not depend on it, only speed.

| Column | Type | Notes |
|---|---|---|
| `size` | `u32` | People |
| `cash` | `Fixed` | **Total** holdings of the POP, not per capita (D7) |
| `profession` | `u16` | Index into `Defs::professions` |
| `province` | `u32` | |
| `life_needs` | `Fixed` | `[0, 1]`, subsistence satisfaction from the last market day (D2) |
| *`culture`, `religion`* | *`u16`* | *M2* |
| `militancy` | `Fixed` | `[0, 1]`, updated monthly (D19); no effects yet |
| *`literacy`, `consciousness`* | *`Fixed`* | *M2. Fixed-point, never `f32` (D3)* |

### `Producers` (RGOs and factories; they differ only by recipe)
| Column | Type | Notes |
|---|---|---|
| `kind` | `u16` | Index into `Defs::producer_types` |
| `province` | `u32` | |
| `capacity`, `employed` | `u32` | Workers |
| `cash` | `Fixed` | |
| `wage` | `Fixed` | Daily wage per worker (sticky, D6) |
| `value_added_avg` | `Fixed` | Smoothed revenue − input cost |
| `output_stock` | `Fixed` | Unsold output; goods persist (D4) |
| `input_stock` | `Fixed` | Row-major `[producer × good]` |

### `Nations` (D15)
| Column | Type | Notes |
|---|---|---|
| `key` | `String` | |
| `treasury` | `Fixed` | Outside money held by the state (D5 invariant) |
| `income_tax_rate` | `Fixed` | Withheld from wages and dividends, in [0, 1] |
| `transfer_rate` | `Fixed` | Share of the treasury paid to the nation's POPs each day, in [0, 1] |
| `consumption_rate` | `Fixed` | Share of the treasury spent on goods each day, in [0, 1] (D16) |
| `basket` | `Fixed` | Row-major `[nation × good]` spending shares; each consuming row sums to exactly 1 |

`Geography::market_nation` maps each market to its nation (`None` means stateless).

### `Markets`
| Column | Type | Notes |
|---|---|---|
| `price` | `Fixed` | Row-major `[market × good]` |

**Derived cache:** `World::layout` holds `PopLayout`: labour pools, owner pools and each POP's market. It is derived from `pops.province`/`pops.profession`, fingerprinted against its inputs on every use and rebuilt on any change (D7), and excluded from equality and the state hash. The key encodings `pool_key`/`owner_key` live in `layout.rs`.

Orders and offers are **not** stored in state: they exist only during the market phase. The old `MarketNode { buy_orders: HashMap, … }` design is retired, because HashMap iteration order is non-deterministic (D3).

### *M2 tables (planned)*
- Laws, and tariffs on `Nations`.
- `Accounts` for inside money: deposits, loans and bonds as asset/liability pairs (D5).
- `Shares` (owner POP/nation → producer).

## ⚙️ Core Systems

All systems are plain functions over `&mut World`, called by `tick::step` in the order given in [ARCHITECTURE.md](ARCHITECTURE.md#-the-game-loop).

1. **`labor::assign_employment`.** Labour pool = `(province, profession)`. If total capacity exceeds the pool, employment is split pro rata to capacity (largest remainder). It returns a `LabourReport` per non-empty pool (workforce, jobs, employed), exposed as `DayReport::labour` for diagnostics. It is not state.
2. **`production::produce`.** `output = min(Eπ, minⱼ stockⱼ/aⱼ, T·Eπ − unsold)`.
3. **`market::clear_markets`.** Runs four phases:
   - **Map:** POPs are summed in parallel into `(market, profession, regime)` aggregates.
   - **Discover:** each market runs bounded tâtonnement in parallel.
   - **Settle:** POPs buy in parallel; producers buy inputs; sellers are paid pro rata to their offers.
   - The concurrency rule (AGENTS.md §3) is satisfied by construction: no POP ever touches shared market state.
4. **`firms::pay_wages_and_dividends`.** Value-added smoothing, sticky wages, then wage and dividend transfers with income tax withheld (D15).
4b. **`government::pay_transfers`.** Each nation pays `treasury × transfer_rate` to its POPs, split by size.
5. **`mobility::reassign_workers`** then **`mobility::migrate_within_markets`** (month end). Unemployed workers move to vacancies in their province (D18), then surplus workers migrate to vacancies in other provinces of the same market (D20), taking their share of cash.
6. **`politics::update_militancy`** (month end). Rises with hunger and taxes, and decays (D19).
7. **`demographics::update_population`** (month end). Growth or starvation from `life_needs`; the estate of an extinct POP passes to an heir. Then **`World::compact_pops`** merges duplicate identities and drops empty rows (D7).

> [!IMPORTANT]
> **Determinism:** money, prices, quantities *and all rates* are `Fixed` (D3). `tax_rate`, `literacy` and `militancy` were `f32` in earlier drafts. That is no longer allowed, because a float tax rate applied to fixed-point wealth makes money itself platform-dependent.

### `DayReport` (diagnostics returned by `tick::step`, never state)
| Field | Meaning |
|---|---|
| `day` | The simulated day (0-based) |
| `goods` | Price, demand, supply and traded quantity per `[market × good]` |
| `iterations` | Tâtonnement iterations per market |
| `labour` | Workforce, jobs and employed per non-empty labour pool (`LabourReport`) |
| `household_spending` | POP consumption spending (final demand; expenditure GDP in a closed economy) |
| `input_spending` | Producer spending on inputs (intermediate consumption) |
| `payouts` | Gross wages and dividends paid that day, and the income tax withheld from them |
| `transfers` | Paid from treasuries to POPs that day |
| `government_spending` | Paid from treasuries for goods that day (D16) |
| `moved` | People who changed profession that day (month end only, D18) |
| `migrated` | People who moved to another province of their market that day (D20) |
| `compacted` | POP rows removed by month-end compaction (D7) |
| `life_needs` | Life-needs coverage at the market: people, deprived, weighted mean (`LifeNeedsSummary`) |
| `militancy` | Population-weighted militancy at the end of the day (`MilitancySummary`, D19) |
| `total_money` | Outside money after the day (asserted unchanged) |

## 🔌 API Boundary (M3)

`pax_server` wraps the engine. It is server-authoritative (D10) and speaks size-prefixed FlatBuffers over TCP (D22). The wire format is in [NETWORK_PROTOCOL.md](NETWORK_PROTOCOL.md), and the pacing and save rules are in D23.

> [!WARNING]
> **Serialization overhead:** never use JSON for the per-tick state sync. Sending aggregated state for thousands of provinces each tick as JSON costs severe CPU time and bandwidth.

**Server → client (views, not state, D22):**
* Every day: world totals and the per-nation table (treasury, policy rates, population).
* On subscription: one map mode (a value per province), one market's goods, one province's POPs, labour pools and producers.
* The full POP and producer tables are never sent. POPs are identified by `(province, profession)`, never by row index (D7).

**Client → server (commands):** applied at the start of the next tick, ordered by `(day, player, sequence)`. The server stamps all three (D22).
* **Implemented** (`pax_engine::Command`, `tick::step_with`, D21): `SetIncomeTax`, `SetTransferRate`, `SetConsumptionRate { nation, rate: Fixed }`. Rates travel as `Fixed`'s raw `i64`, never as floats.
* **Planned:** `SubsidizeFactory { producer, enabled }`, `MoveArmy { army, target_province }`.
