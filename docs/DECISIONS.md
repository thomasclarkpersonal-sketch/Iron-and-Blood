# Design Decisions

This is the binding record of decisions that other documents and the code depend on. Where any other document disagrees with this one, **this one wins**, and the other document is a bug to fix. Code comments refer to entries as `D1`, `D2`, and so on.

Each entry has a status:
- **Accepted**: implemented or binding now.
- **Accepted (principle)**: the rules are binding, and the details are scheduled for a later milestone.
- **Deferred**: deliberately undecided, with a deadline and the criteria for deciding.
- **Proposed**: drafted for review; not binding until the team accepts it, which changes the status.
- **Superseded by Dn**: replaced. The entry keeps its number and a one-line pointer to its successor, so old citations still resolve; its text moves to git history.

To change a decision, edit its entry in the same pull request as the code. Say what changed and why, and update every document that cites it.

**What an entry holds.** An entry is a contract: the critic treats any code that contradicts it as CRITICAL. So it holds only what should be that hard to change:
- the **problem**, when it isn't obvious;
- the **rule**: what code must and must not do, and the invariants tests check;
- the **rationale**, including rejected options and the failure that taught us the rule;
- the **revisit criteria**, when the rule is expected to change.

Mechanism (module layouts, file formats, flag names, step-by-step algorithms) belongs in the system documents, and measurements in [PERFORMANCE.md](PERFORMANCE.md); an entry links to them. A temporary measure gets an entry only if it changes simulation results; its status says *temporary* and the entry names the condition for removing it. Any other temporary measure (a server limit, a workaround) lives in its milestone's follow-ups instead. If an entry grows past about a screen, its mechanism has crept in. See [docs/README.md](README.md) for where each kind of fact lives.

| # | Topic | Status |
|---|-------|--------|
| [D1](#d1-market-clearing-bounded-tâtonnement-with-pro-rata-rationing) | Market clearing | Accepted |
| [D2](#d2-consumer-demand-linear-expenditure-system-stone-geary) | Consumer demand | Accepted |
| [D3](#d3-determinism-fixed-point-everything) | Determinism | Accepted |
| [D4](#d4-tick-schedule-and-goods-persistence) | Tick schedule | Accepted |
| [D5](#d5-money-model-and-stock-flow-consistency) | Money and SFC | Accepted |
| [D6](#d6-firms-production-wages-ownership) | Firms | Accepted |
| [D7](#d7-pop-accounting) | POP accounting | Accepted |
| [D8](#d8-ecs-hand-rolled-struct-of-arrays) | ECS framework | Accepted |
| [D9](#d9-data-format-toml) | Data format | Accepted |
| [D10](#d10-network-model-server-authoritative-deterministic-core) | Network model | Accepted |
| [D11](#d11-determinism-harness-and-golden-files) | Determinism harness | Accepted |
| [D12](#d12-frontend-godot-with-a-rust-gdextension-bridge) | Frontend | Accepted (M3) |
| [D13](#d13-performance-budget) | Performance budget | Accepted |
| [D14](#d14-market-hierarchy-and-inter-market-trade) | Market hierarchy | Accepted (principle), M2 |
| [D15](#d15-nations-treasuries-income-tax-and-transfers) | Nations and fiscal policy | Accepted (M2-1) |
| [D16](#d16-government-consumption) | Government consumption | Accepted (M2-2) |
| [D18](#d18-labour-mobility) | Labour mobility | Accepted (M2-4) |
| [D19](#d19-militancy) | Militancy | Accepted (M2-5) |
| [D20](#d20-migration-within-a-market) | Migration within a market | Accepted (M2-6) |
| [D21](#d21-commands-and-command-logs) | Commands and command logs | Accepted |
| [D22](#d22-wire-protocol-and-client-sessions) | Wire protocol and sessions | Accepted (M3) |
| [D23](#d23-server-loop-pacing-flow-control-and-saves) | Server loop, pacing, saves | Accepted (M3) |
| [D24](#d24-multiplayer-authority) | Multiplayer authority | Accepted (M4-0) |

---

## D1. Market clearing: bounded tâtonnement with pro-rata rationing

**Problem.** Three different mechanisms were specified: Victoria 2-style price nudging with min/max clamps, a full Walrasian tâtonnement, and order-book matching. The research notes also criticised the clamps.

**Decision.** Each market node runs, every day:

1. **Map:** buyers and sellers state their demand and supply as *functions of price*, not fixed quantities.
2. **Discover:** iterate `pᵢ ← pᵢ(1 + λₖ zᵢ)` with `zᵢ = (Dᵢ − Sᵢ)/(Dᵢ + Sᵢ) ∈ [−1, 1]` and a decaying step `λₖ = λ·d/(d + k)`.
   - Iterates stay inside today's band (step 3).
   - Stop when every good is settled (`|zᵢ| ≤ tolerance`, or pinned at a band edge with excess demand pushing outward), or after `max_iterations`.
   - Before the band confinement, unreachable goods kept every market iterating to the cap.
3. **Limit:** the executed price may move at most `max_daily_change` (default ±10%) from yesterday's. If a market holds **no stock at all** of a good, its price is held, since there is nothing to discover from, and the scarcity is reported as unmet demand.
   - Stock below `market.min_stock` (`rules.toml`) counts as none and is not offered. Before this rule, rounding dust of 10⁻⁶ units let discovery push a price to the ceiling against almost nothing, which killed a whole supply chain in `two_states`.
4. **Settle:** if `D > S`, every buyer receives the same fraction `S/D`. No buyer is favoured by queue position, nation rank or entity order.

**Supply** is price-responsive: `S(p) = stock × min(1, p/r)`, with a cost-plus reservation price `r = Σ aⱼpⱼ + (w/π)/labor_share` (see D6).

**Not used:**
- Min/max price bands anchored to a base price: they produced Victoria 2's post-hoc price artifacts.
- Order books: they cost O(orders · log orders) per good and need agent-level bids that millions of POPs cannot afford to compute.

`price_floor`/`price_ceiling` exist only to keep fixed-point products in range and should never bind in play.

**Why it scales.** LES demand (D2) is linear in POP size and budget, so a market needs only `(Σ size, Σ budget)` per profession and regime to evaluate demand at any trial price. Discovery never touches the POP table.

**Code:** `crates/pax_engine/src/systems/market.rs`. **Docs:** [ECONOMY_SYSTEM.md](ECONOMY_SYSTEM.md).

## D2. Consumer demand: Linear Expenditure System (Stone-Geary)

**Problem.** The docs gave three demand models: strict Life/Everyday/Luxury tiers, Stone-Geary, and a constant-elasticity curve.

**Decision.** Every profession has subsistence quantities `γ` (per person per day) and discretionary shares `β` (summing to exactly 1). A POP of `N` people with daily budget `Y = cash × spend_rate` facing prices `p`, with subsistence cost `C = Σ pₖγₖ`:

| Regime | Condition | Demand |
|---|---|---|
| Comfortable | `Y ≥ N·C` | `xᵢ = Nγᵢ + βᵢ(Y − N·C)/pᵢ` |
| Deprived | `Y < N·C` | `xᵢ = γᵢ·Y/C` (the whole budget buys a scaled-down subsistence basket) |

- Spending equals `Y` exactly in both regimes, up to downward rounding. `C` is rounded *up*, so a POP can never overspend.
- **The Victoria 2 tiers still exist as data and as outputs.** "Life needs" are the `γ` goods. "Everyday" and "luxury" are simply goods with `β > 0`, which richer POPs buy more of. Price elasticity emerges from income rather than being hard-coded per tier.
- `life_needs = minᵢ(boughtᵢ / Nγᵢ)` is stored per POP and drives demographics (D7), and later militancy.

**Known approximation.** During price discovery, POPs are aggregated in the regime they were in at the opening prices. Settlement always uses each POP's exact regime at the final prices, so the approximation affects only how good the price discovery is, never conservation.

## D3. Determinism: fixed-point everything

**Problem.** AGENTS.md required fixed-point, but the schema used `f32` for literacy, militancy, consciousness and *tax rate*, the architecture diagram used `float[] Wealth`, and `MarketNode` iterated a std `HashMap`.

**Decision.**
- **Every value that is simulation state, or feeds into it, is `pax_engine::Fixed`.** That includes rates, ratios, militancy and literacy, not only money. A float may appear only in presentation code (CLI output, the client).
- `Fixed` is an `i64` holding the value × 10⁶. Decimal rather than binary, so data values like `0.015` are exact. Range ±9.2·10¹².
- **Rounding:** multiplication and division round down (`mul_ceil` exists where rounding down could create goods). Any split of an amount among recipients uses the largest-remainder method (`alloc.rs`), so parts always sum to the whole.
- **Overflow panics**, in release builds too (`overflow-checks = true`). Wrapping would silently create money.
- **Iteration order:** simulation state never lives in, and is never iterated from, `std::collections::HashMap`/`HashSet`, whose iteration order is randomised per process. Use dense `Vec`s indexed by id, `BTreeMap`, or the counting-sort `Groups` helper.
- **Randomness** comes from `rng.rs`: a counter-based SplitMix64 keyed by `(seed, stream, day, entity, index)`. No RNG has state, so the result is the same at any thread count.
- **Parallel reductions** only sum integers (`Fixed`), which is associative, so rayon's reduction order cannot change results.
- **Load-time conversion is exact:** decimals are parsed from text, and more than 6 decimal places is a load error.
- Floats in power functions (e.g. `(p₀/p)^E`) are not needed: the D2 demand model is rational in `p`.

## D4. Tick schedule and goods persistence

**Problem.** Markets cleared daily but POPs bought weekly, so six days out of seven had no consumer demand. The order within a tick and the fate of unsold goods were also undefined.

**Decision.** All economic systems run **daily**, in this fixed order (`crates/pax_engine/src/tick.rs`):

| # | System | Cadence |
|---|---|---|
| 1 | Labour: assign employment | daily |
| 2 | Production | daily |
| 3 | Market: orders (households, producers' inputs, governments D16) → price discovery → settlement | daily |
| 4 | Firms: wages, dividends (income tax withheld, D15) | daily |
| 4b | Government: transfers from treasuries to POPs (D15) | daily |
| 5 | Labour mobility within a province (D18), then migration within a market (D20); *(planned)* promotion | month end |
| 6 | Politics: militancy (D19); *(planned)* consciousness | month end |
| 7 | Demographics, then POP row compaction (D7) | month end |

- A month is 30 days until a real calendar is needed (`rules.days_per_month`).
- **Goods persist.** Unsold output stays in the producer's stock, and producers stop producing when stock reaches `target_stock_days` of output (D6).
- Goods bought by POPs are consumed immediately; households hold no inventory in M1.
- Perishability, if wanted, will be a per-good decay rate in data, not an implicit loss.

## D5. Money model and stock-flow consistency

**Problem.** "Total cash must equal M0" conflicted with banks that lend deposits (which creates broad money). Currencies were unspecified, and so was what happens to money when POPs die or firms fail.

**Decision.**
- **One world currency.** Per-nation currencies would need a foreign-exchange market and a decision of their own.
- **Outside money** (`Σ cash` over all agents) is constant. It may change only through an explicit, logged *mint* or *burn* event. Today none exists, and `tick::step` panics if the total moves.
- **Inside money** (deposits, loans, bonds; M2) is always created as a matched asset/liability pair. The invariant becomes:

  `Σ financial assets − Σ financial liabilities = Σ outside money` (every IOU nets to zero).

  A bank loan creates a deposit (asset of the borrower) and a loan (asset of the bank), offset by a liability on each side. This is endogenous money in Godley & Lavoie's sense, and it does not break conservation. A default writes off both sides; nobody's *cash* disappears.
- **Every transfer debits one account and credits another by the same amount.** Splits use largest-remainder allocation.
- **Death/extinction:** money stays with surviving POP members; an extinct POP's cash passes to an heir (D7).
- **Firm failure (planned, with investment and banking):** cash goes to creditors first, then owners.
- **Tests:** every day asserts outside-money conservation. `crates/pax_engine/tests/conservation.rs` runs 200 randomised economies through it.

## D6. Firms: production, wages, ownership

- **Technology:** Leontief recipes (`output = min(Eπ, minⱼ stockⱼ/aⱼ)`) with one worker profession per producer type. Substitution between inputs comes later as **alternative production methods** (several recipes per good, chosen by cost), not CES. CES needs fractional powers, which conflict with D3.
- **Shutdown rule:** if the output price does not exceed the input cost per unit, the producer buys no inputs that day.
- **Inventory targeting:** produce at most up to `target_stock_days` of output in stock.
- **Pricing:** cost-plus reservation price (D1).
- **Wages** are sticky and track value added: target `w* = labor_share × max(V̄, 0)/E`, where `V̄` is the smoothed (revenue − input cost). The wage closes `1/wage_stickiness_days` of the gap per day.
- **Wage floor:** the target wage is never below `firms.subsistence_wage_multiple` (`rules.toml`) × a worker's daily subsistence cost `Σ γ p` (`ProfessionDef::subsistence_cost`).
  - This is a classical subsistence wage. It anchors prices to the cost of labour.
  - Without it, in a chain whose buyer buys a fixed quantity, wages, reservation prices and prices chased each other down to the technical floor (seen in `two_states`).
- **Liquidity rule:** wages are paid only from cash above a **restart reserve**: the cost of the inputs still missing for one day of output at today's prices (`production::input_requirements`). It goes into the labour pool `(province, profession)` and is split by POP size.
  - The reserve is kept **even while the producer is shut down** (the market then orders nothing: `planned_inputs` is empty under the shutdown rule). That way the producer can restart when prices recover.
  - A struggling producer can therefore always buy inputs, produce and sell. Its workers absorb the shortfall in pay instead of the firm dying.
  - Paying out the last cash in wages was a permanent trap: no inputs meant no output, no revenue, and no recovery.
- **Dividends:** cash above `reserve_days × wage bill` is paid out at `dividend_payout_rate` per day to the producer type's **owner profession** in the same market, split by size.
- **Ownership is profession-level in M1.** A share registry (who owns which firm, so capitalists in one market can own firms in another) is planned and will be needed for investment and bankruptcy.

## D7. POP accounting

- `cash` is the POP's **total** holdings, not a per-capita amount. When size changes (births, deaths, casualties) the money stays with the survivors.
- **Extinction:** at each month end, the cash of every empty row passes to the largest living POP in the same province (lowest row on ties). That covers rows that died out this month and earlier. If nobody lives in the province, the row keeps the cash until someone does.
- **Splitting** (promotion, migration, conscription; M2): the moving fraction takes cash in proportion to people moved, using largest remainder. Intensive attributes (literacy, militancy) are copied.
- **Merging:** cash adds up. Intensive attributes become size-weighted averages, computed in `Fixed` with one rounding.
- **Compaction** (implemented, `World::compact_pops`, month end after demographics):
  - rows sharing an identity merge into the first such row, with these merge rules;
  - rows that are empty and cashless are dropped;
  - surviving rows keep their order, so the result is deterministic.
  
  This keeps the POP table bounded as mobility, migration and extinctions create and empty rows.
- **Promotion and migration are deterministic fractional flows** (`ΔN = ⌊N × rate⌋`), not dice rolls. The counter-based RNG (D3) is available where genuine randomness is wanted, e.g. rebellions.
- **POP identity** is `(province, profession, culture, religion)`. Culture and religion columns are planned. Lookups by identity use a sorted index, never a `HashMap`.
- **Derived values are never stored as state.** Nation, market and state come from the province; storing `nation_id` on POPs would go stale on conquest.
  - **Exception: self-validating caches.** For performance, derived data may live in a cache *outside* state, under three conditions: it's excluded from equality and `World::state_hash`; it fingerprints all of its inputs on every use and rebuilds on mismatch; and debug builds check it against a fresh build.
  - The only such cache is `World::layout` (`layout.rs`). Its inputs are the POP row count, `pops.province`, `pops.profession`, `geography.province_market`, `geography.market_nation` (D15), and the number of professions, markets and nations.

## D8. ECS: hand-rolled Struct-of-Arrays

**Problem.** bevy_ecs, hecs and flecs were all listed. flecs is a C library.

**Decision.** No ECS framework. An entity is a dense row index, each component is a `Vec` column (`world.rs`), and a system is a plain function called in a fixed order (`tick.rs`).

Reasons:
- Guaranteed iteration order, which is the basis of determinism.
- Columns are as cache-friendly as possible.
- Snapshots are a `Clone`.
- No dependency, and no scheduler whose parallel ordering we would have to audit.

**Revisit if** we need dynamic component sets per entity, or more than ~20 systems with complex dependency graphs. bevy_ecs used headlessly would then be the candidate.

## D9. Data format: TOML

`serde_yaml` is archived and its forks are thinly maintained. The `toml` crate is actively maintained, has no implicit typing traps (the YAML "Norway problem"), and is easy for modders to read. JSON remains acceptable for machine-generated files.

All parsing lives in `pax_data`, so the format can change without touching the engine. File formats are specified in [DATA_FORMAT.md](DATA_FORMAT.md).

**Exception (M3-8b, approved by the owner on 2026-10-08):** the province-map files (`provinces.toml`, `provinces.png`) are parsed by the side-neutral `pax_map` (D12), which `pax_data` calls. So the server validates a map with exactly the code the client draws it with. `scenario.toml` and every other file stay in `pax_data`: the client learns where the map is from the server (`StaticData.map_dir`), never by reading `scenario.toml`.

## D10. Network model: server-authoritative, deterministic core

**Problem.** AGENTS.md assumed lockstep multiplayer, while BACKEND_SCHEMA described a server pushing state to clients.

**Decision.**
- The simulation runs in **one authoritative process** (`pax_server`). Clients never simulate. They send commands and receive **views** of the state: a daily summary, plus detail on request such as one market or one province's POPs. They never receive the full state (D22).
- Commands are applied at the start of the next tick in `(day, player, sequence)` order. The server assigns all three, never the client (D22). The engine side is implemented (D21).
- **Single player is the same server**, launched by the client on localhost (M3). Multiplayer adds sessions and authority to it (M4, D24): one code path for both.
- Determinism (D3) is kept anyway:
  - saves are the scenario plus a command log (D23);
  - any session, and any bug report carrying a `state_hash`, can be replayed exactly;
  - lockstep stays possible later without a rewrite.
- **No desync detection.** A thin client has no state of its own that could drift. The `state_hash` in each update identifies the state for logs and replays; it is not a check the client runs.
- **Protocol: FlatBuffers over TCP** (D22), chosen at the start of M3 over Cap'n Proto:
  - official, maintained FlatBuffers libraries exist for both serious client-language options (Rust for a GDExtension, and C#, D12);
  - schema evolution is append-only and simple to review;
  - reads are zero-copy.

  GDScript has no maintained FlatBuffers library, and that is one input to the client-language decision (D12).
- **FlatBuffers stays even though the client bridge is Rust (D12).** With Rust at both ends, library support no longer separates FlatBuffers from Cap'n Proto, so the choice was reviewed again:
  - Cap'n Proto's distinctive feature, RPC with promise pipelining, doesn't fit a one-way stream of messages in each direction. Its built-in framing replaces only our 4-byte length prefix.
  - FlatBuffers has an official, mature C# library, which keeps D12's C# fallback cheap if the GDExtension route fails.
  - The FlatBuffers schema was already written, compiled and measured.
- **Binary save format** (decided in M3-6b, as D23 scheduled once replay proved too slow): each save carries a versioned binary snapshot of the `World` (`pax_data::snapshot`; layout in [DATA_FORMAT.md](DATA_FORMAT.md#save-files-savesnametoml-d23)), and loading reads it instead of replaying. Its rules:
  - Restoring it runs the engine's table rules (`World::check_tables`), so a crafted file can't put the engine in an impossible state.
  - It stores no definitions; those come from the scenario, whose content hash must match.
  - A loaded snapshot must reproduce the state hash it recorded, so it can only restore exactly what was written.
  - The writer names every column of every table, so a new column fails to compile until the format carries it.
  - It is a save format only, never sent over the wire, and the engine stays serde-free.

## D11. Determinism harness and golden files

- `tick::run` returns a stable FNV-1a hash of all state after each day (`World::state_hash`). It does not use `DefaultHasher`, which is not stable across Rust versions.
- `scenarios/*/golden.hashes` pins **at least one year** of hashes, and for scenarios with a command log (D21) **at least past the last logged command**. `pax_data::golden::min_days` defines that minimum. `pax_cli record` refuses to write less and `pax_cli verify` rejects less. `two_states` pins 730 days.
- `pax_cli record` keeps an existing golden file's length unless `--days` is given, so re-recording never silently drops coverage. A golden file that exists but can't be read is an error, not a fallback. CI runs `pax_cli verify` at 1 and 4 threads on Linux, and on Windows and macOS.
- Tests also check: same input gives same hashes; results are identical at 1/2/3/8 threads; resuming from a snapshot matches a continuous run.
- **Any change that alters simulation results must re-record the golden file in the same PR**, and say so in the description. Unexpected golden diffs are bugs.

## D12. Frontend: Godot with a Rust GDExtension bridge

**Accepted (M3).** M1–M2 needed no client: `pax_cli` prints market reports.

**Decision.**
- **Godot 4**, chosen against the criteria this entry set before M3:
  1. **Distribution:** desktop first. A browser build is not a goal for M3–M4.
  2. **Map rendering:** thousands of provinces render as one province-ID texture plus a per-province colour lookup texture (a shader). Changing map mode rewrites one small texture, not geometry.
  3. **Team skills:** the team writes Rust (engine, data, CLI, server). Godot's own GDScript is used for the UI, so no third language is needed.
  4. **Protocol library support:** the bridge below reuses the server's Rust protocol code.
- **Client structure:**
  - **The UI** (map, panels, menus) is in GDScript.
  - **A Rust GDExtension bridge** (`pax_godot`, using godot-rust/gdext) owns the connection. It reads and writes frames, verifies messages, checks the protocol version, converts `Fixed` for display, and turns map values into shader arrays.
  - The bridge depends on `pax_protocol` only, **never on `pax_engine`**, so the client cannot simulate (D10).
  - *Side-neutral crates* are the exception to "`pax_protocol` only" (M3-8b). They carry neither engine nor wire types, so both sides may link them: `pax_content` (the hash scheme) and `pax_map` (the province-map reader the server validates with and the client draws with). Neither lets the client simulate, and CI checks that neither depends on `pax_engine` or `pax_protocol`. Anything else the bridge links must still be `pax_protocol`: CI allowlists the bridge's workspace dependencies (`pax_protocol`, `pax_map`, `pax_content`). Approved by the maintainer on 2026-10-08 (#45).
- **Fallback: C# (Godot .NET)** with the official FlatBuffers C# library, if the M3-0 spike shows gdext can't do the job. gdext is pre-1.0 (0.5.x), so the spike is the risk gate.
- **M3-0 spike: passed (2026-10-08), so the fallback isn't needed** ([MILESTONE_3.md](MILESTONE_3.md), M3-0). godot-rust is pinned exactly (0.5.5, `api-4-7`), because it is pre-1.0.
- **`unsafe`:** like `pax_protocol`, the bridge *denies* rather than forbids `unsafe_code`, because godot-rust's entry point must be an `unsafe impl`. It allows `unsafe` in that one module only, and `scripts/check_lints.py` (CI) keeps both crates' copied lint tables equal to the workspace's apart from that exception.

**Why GDExtension over C#.** The options considered:

| Option | For | Against |
|---|---|---|
| **Rust GDExtension bridge + GDScript UI** (chosen) | The protocol logic (framing, limits, verification, handshake, flow control, `Fixed` conversion) is written and tested once, shared with the server. One code-generation target. Rust tests and fuzzing cover the client's network layer headlessly | gdext is pre-1.0, so expect breaking changes. A native library is built per platform (CI already runs on all three). Working on the bridge means rebuilding Rust |
| C# (Godot .NET) | Mature official FlatBuffers library; large Godot C# community | Every piece of protocol logic is written a second time and kept in step with the Rust version; a second code-generation target with its own version pin; separate tests |
| GDScript only | No native build | No maintained FlatBuffers library; a hand-written decoder must track every schema change |

## D13. Performance budget

| Target | Budget |
|---|---|
| M1: 1M POP rows, 1 market, 4 goods, 8 threads | ≤ 100 ms/day |
| Long-term: 2M POP rows, ~3,000 markets, ~50 goods, 8-core desktop | ≤ 100 ms/day |
| M2 content: `two_states` (12 goods, nations, taxes) replicated to about 1M POP rows, 8 threads | ≤ 100 ms/day |

Measurements, the commands that take them and their history are in [PERFORMANCE.md](PERFORMANCE.md).

At 100 ms/day, the fastest game speed runs at about 10 in-game days per second. CI fails a PR that makes the tick more than 20% slower. The `Benchmark regression` job times the PR's base and head on the same runner (`scripts/bench-compare.sh`), so runner speed cancels out.

## D14. Market hierarchy and inter-market trade

**Accepted (principle); designed in detail in M2.**

The state → national → sphere → global roll-up in the old ECONOMY_SYSTEM was Victoria 2's priority queue under another name. These rules are binding for the M2 design:

1. **Every market node clears with D1.** Trade between nodes is a **flow of goods** decided by price gaps, not a queue of leftover orders.
2. A flow from market A to B happens when `p_B·(1 − τ_AB) − tariff_AB > p_A`, where `τ_AB` is the iceberg fraction lost in transit. It is throttled by infrastructure capacity and grows with the price gap.
3. **Allocation of scarce goods across importers is pro rata.** No priority by nation rank, prestige or table order.
4. Iceberg losses destroy *goods*, never money. Tariffs are a transfer to the importing treasury.
5. Distances and friction come from a matrix precomputed at load time and rebuilt only when infrastructure changes (MAP_AND_LOGISTICS.md).

## D15. Nations, treasuries, income tax and transfers

**Accepted (M2-1).** The first fiscal layer of [POLITICS_SYSTEM.md](POLITICS_SYSTEM.md).

- **Nations own markets** through `geography.market_nation`. A market without a nation is *stateless* and untaxed, so scenarios may omit nations entirely (`mini_valley` does). A POP's nation is derived from its market, never stored (D7).
- **Treasury:** a nation's `treasury` is outside money and part of the D5 invariant (`World::total_money`).
- **Income tax:** a flat `income_tax_rate` (`Fixed`, in [0, 1]) is withheld at source from every wage and dividend payment by producers in the nation's markets.
  - The labour or owner pool receives the net; the treasury receives the tax.
  - `DayReport::payouts` reports gross wages and dividends plus the taxes.
- **Transfers:** each day the treasury pays `treasury × transfer_rate` (rounded down) to the nation's POPs, split by size with largest remainder. This is a flat per-capita benefit.
  - Some spending is mandatory. A treasury that only collected would drain money from circulation and push prices and wages down (MACROECONOMICS.md §1).
  - Purchases of goods (military upkeep, infrastructure) arrive with M2-2.
- **Rates are state, not definitions.** They live in the `Nations` table and in the state hash, because players will change them. Commands (D10) will set them at the start of a tick.
- **Tick order:** taxes are withheld inside the firms system; transfers run right after it, before demographics.
- **Out of scope for M2-1:** progressive brackets (by profession or income), tariffs (with D14), bonds and debt (inside money, D5), and laws constraining rates (politics).

## D16. Government consumption

**Accepted (M2-2).** Treasuries buy goods as well as redistributing money.

- **Policy:** each nation has a `consumption_rate` (share of the treasury spent per day) and a `basket` (weights by good, normalised to sum to exactly 1). Both are policy state in the `Nations` table.
- **Orders:** the daily budget `treasury × consumption_rate` is split exactly (largest remainder):
  - across the nation's markets, in proportion to their population;
  - within each market, across goods by the basket.

  Each part becomes a `BuyOrder` with buyer `Nation(n)` and demand `budget / p`. That is unit-elastic: spending is fixed and the quantity adjusts to the price.
- **Same market rules as everyone else:** government orders enter price discovery (D1) and are rationed pro rata with every other buyer. No priority for the state.
- **Settlement:** the treasury pays `q × p` to the sellers through the normal receipts split. The goods are consumed, standing in for administration and public works. Money is conserved; goods leave the economy.
- **Reporting:** `DayReport::government_spending`. `pax_cli report` counts it in GDP (C + G).
- **Later:** state-employed POPs (bureaucrats, soldiers) and military upkeep (MILITARY_SYSTEM.md) will replace the abstract basket with real demand.

## D18. Labour mobility

**Accepted (M2-4).** The first deterministic population flow (D7), within a province.

- **When:** at month end, before demographics (D4 step 5), using the day's labour report.
- **Who:** *worker professions* only, meaning professions that some producer type employs. Owner professions don't take jobs.
- **Rule, per province:**
  - Destinations are worker professions with vacancies (`jobs − workforce > 0`), ordered by vacancy (descending), then profession.
  - Each worker profession with unemployment sends `⌊unemployed × mobility_rate⌋` people to the destinations in that order, never beyond a destination's vacancies.
- **Accounting (D7):**
  - Movers leave their POP rows pro rata to size (largest remainder) and take `cash × movers / size`, split exactly.
  - They join the first POP row of the destination profession in the province, creating one if needed (through `push_pop`, which keeps the layout cache valid).
  - The destination's `life_needs` becomes the size-weighted mean.
  - Population and money are conserved, and tested.
- **Tuning:** `demographics.mobility_rate` in `rules.toml`. `mini_valley`'s frozen definitions use 0, which keeps it a pure regression fixture.
- **What it does not fix:** unemployment caused by *total* job capacity lagging population. That needs investment (new and expanding producers; proposal in [#21](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/21)).
- **Later:** migration between provinces (D20 covers provinces of the same market), promotion to higher strata (literacy, D7), culture and religion.

## D19. Militancy

**Accepted (M2-5).** The first political state (POLITICS_SYSTEM.md). It has no effects yet.

- **State:** `Pops::militancy`, a `Fixed` in `[0, 1]` (D3), starting at 0, part of the state hash. When POPs merge or move, the destination takes the size-weighted mean (D7).
- **Monthly update** (D4 step 6, after mobility, before demographics): `m ← clamp(m + rise × (1 − life_needs) + tax_weight × t − decay × m, 0, 1)`, where `t` is the income tax rate of the POP's nation (0 when stateless).
  - The parameters are `[politics]` in `rules.toml`.
  - The equilibrium is `m* = (rise × (1 − s) + tax_weight × t) / decay`. For example, a fed POP under a 10% tax with the defaults settles at 0.1.
- **Reported** as the population-weighted mean in `pax_cli report`.
- **Later:** rebellions, which will be the one place genuine randomness is used (`rng::Stream::REBELLION`, D3); consciousness and reforms; interest groups.

## D20. Migration within a market

**Accepted (M2-6).** People follow jobs across provinces of the same state market.

- **When:** at month end, right after labour mobility (D18), on the workforce as it stands then.
- **Rule:** for each market and worker profession, a province whose workforce exceeds its jobs sends `⌊surplus × migration_rate⌋` people to provinces of the same market with vacancies for that profession. The largest vacancy goes first, then the lowest province; no destination goes beyond its vacancies.
- **Migrants keep their profession** and move with their cash under the D7 split rules, into the destination's existing row or a new one. People and money are conserved, and tested.
- **Never across markets.** Moving between states, or to colonies, needs friction (`τ`, D14) and is left for after trade (D14; proposal in [#18](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/18)).
- **Tuning:** `demographics.migration_rate` (0.05 in `data/`, 0 in `mini_valley`'s frozen definitions). In `two_states` each profession lives in only one province per state, so results there are unchanged.

## D21. Commands and command logs

**Accepted.** The first concrete step of D10's network and save model.

- **`pax_engine::Command`** is the only way the outside world changes state during a game.
  - Commands so far: `SetIncomeTax`, `SetTransferRate`, `SetConsumptionRate`.
  - Rates are `Fixed` (D3); a command never carries a float.
- **Validation first:** `World::validate` is the **only** definition of command validity (`CommandError`: unknown nation, rate outside [0, 1], consumption without a basket).
  - `World::apply` calls it and then applies, so a rejected command changes nothing.
  - Loaders call it on each logged command against the scenario's initial world.
  - New checks belong in `validate`, never in `apply`, so load-time and replay-time validity can't drift apart.
- **Timing:** `tick::step_with(world, commands)` applies commands at the **start** of a tick, in the given order, before any system (D4).
  - With several players, the order is `(tick, player, sequence)` (D10). The server will build that order; the engine only sees an ordered slice.
- **Command logs:** a game is its initial state plus its command log.
  - A scenario may name a `commands` file (`[[command]]` entries with `day`, `type`, `nation`, `rate`; see DATA_FORMAT.md).
  - `pax_data::run_logged` replays it, and `pax_cli` applies it in `run`, `report`, `record` and `verify`. Golden hashes therefore pin the commands too.
  - `two_states` replays a two-command policy timeline.
- **Save files** become "scenario + command log + day". D23 defines the save file and how it loads, and D10 the binary snapshot that makes loading fast.

## D22. Wire protocol and client sessions

**Accepted (M3).** The full protocol is in [NETWORK_PROTOCOL.md](NETWORK_PROTOCOL.md); the schemas in `schemas/` are its source of truth.

- **Framing:** TCP; each message is a size-prefixed FlatBuffer with file identifier `PAXC` (client→server) or `PAXS` (server→client). Client messages are at most 64 KiB and server messages at most 16 MiB.
  - Every inbound buffer is verified before it is read.
  - Any protocol error closes the session with `Goodbye`.
- **Liveness:** the server closes a session that has sent nothing for **10 s**. A client therefore sends `Ping` whenever it has sent nothing for a fifth of that (2 s), so a live client never trips it. Both sides derive their timing from one constant, `pax_protocol::IDLE_TIMEOUT`. In multiplayer, D24's lag rules replace this.
- **Generated code:** produced by flatc **24.3.25**, matching the `flatbuffers` crate, in the engine-free `pax_protocol` crate, by `scripts/gen-protocol.sh`. That script downloads the pinned flatc and checks its checksum. The code is checked in, and CI fails if regenerating it gives a different result.
  - **`unsafe` exception:** flatc's Rust code uses `unsafe` internally, and the workspace forbids `unsafe_code`. `pax_protocol` therefore *denies* `unsafe_code` and allows it on the generated module only, so its hand-written code (framing, readers) is still held to the rule.
  - **planus** (a pure-Rust FlatBuffers compiler) was evaluated in M3-1. Its generated code also uses `unsafe`, so it would remove neither the exception nor the pinned toolchain's role, and flatc was kept.
  - **Reading untrusted frames:** check the length before the file identifier, because flatbuffers' identifier helper asserts on short input. Every frame is verified before anything reads it.
- **Versioning:** `Hello` and `Welcome` carry `protocol_major`/`protocol_minor`. A major mismatch is refused.
  - Compatible changes only append fields, deprecate instead of deleting, and add union members and enum values at the end.
  - Receivers ignore unknown union members and enum values.
- **Content and map hashes:** both come from `pax_content`, a crate with no dependencies, so the server's loader and the client share one scheme (FNV-1a over role-keyed, length-prefixed files). Neither side's types leak into the other: `pax_data` doesn't link `pax_protocol`, and `pax_server` remains the only crate that sees both engine and wire types.
  - `Welcome.content_hash` covers every file the scenario loader reads, maps included; saves record it too (D23).
  - `StaticData.map_hash` covers only the two map files (M3-7). The client hashes its own copy of them the same way and refuses to draw a map that doesn't match.
- **Ids:** every id is a `uint` index into the `StaticData` tables sent in `Welcome`, fixed for the session (player ids are `ushort`). "None" is an absent optional field, never a sentinel such as `-1`, so a missing id can't be cast into a huge index. POPs are identified by `(province, profession)`, never by row index, because compaction reorders rows (D7).
- **Views, not state:** a `DayUpdate` carries `WorldSummary` and `NationTable` always, plus the subscribed `MapView`, `MarketDetail` and `ProvinceDetail` (a remote multiplayer session gets its `MapView` only with some updates: D24, M4-7). The full POP and producer tables are never sent.
  - The views themselves are built from the engine's read-only `views::ProvinceStats` and the day's report.
  - Budget at the D13 long-term scale: ≤ 16 KB summary-only and ≤ 128 KB with every view subscribed (measured in [PERFORMANCE.md](PERFORMANCE.md)).
- **Values:** simulation values travel as `Fixed { raw: long }` (D3), in both directions. Clients use floats for display only.
- **Commands:** `SubmitCommand { client_seq, command }` mirrors `pax_engine::Command`.
  - The server checks, in order: well-formedness, permission (D24), then `World::validate` (D21).
  - It then stamps `(day, player, sequence)`, queues the command, and replies with one `CommandResult`.
  - Commands that apply successfully are appended to the session's command log.

## D23. Server loop: pacing, flow control and saves

**Accepted (M3).**

- **Threads:** one sim thread owns the `World` exclusively and runs ticks (rayon inside). Network tasks (tokio) exchange messages with it over channels. There are no locks around world state.
  - Inbound: a bounded `flume` queue of 1,024 events. Tasks await room in FIFO order, and the sim thread receives with a timeout for pacing.
  - Outbound: 256 frames per connection. A full outbound queue disconnects the client.
- **Speed:** paused, or speeds 1–5 at 0.5, 1, 2 and 5 days per second, and as fast as the tick allows (about 10 days/s at the D13 budget). Speed and pause are **server controls, not engine commands**: they change no results, so they appear in neither the command log nor the state hash. The pacing table is `pax_protocol::pacing`: the server's clock runs on it, and the client labels its speed buttons from it (generated into `PaxKeys.SPEED_DAY_MS`), so the two can't disagree.
- **Command order within a tick:** the scenario's own scripted commands for the day (`commands.toml`) apply first, then players' commands in stamp order `(day, player, sequence)` (D10). The applied-command log holds both kinds, with scripted commands marked as having no player. Saves are built from this log (below). The order lives in one function, `pax_data::step_day`, which `pax_cli`, the tests and the server all call. A server test pins it to every scenario's `golden.hashes` (D11).
- **Replays apply the saved log alone.** The scenario's scripted commands for logged days are in it, so a replay never applies `commands.toml` again for those days. Scripted commands for later days still come from the scenario. The tick and the save loader therefore apply each command exactly once, and both run the day through `pax_data::step_day`.
- **The clock stops when the last player leaves.** When the last welcomed session closes, the server pauses; a game never runs unobserved. While other players remain, the game goes on (D24).
- **Flow control:** each client may have at most 3 unacknowledged `DayUpdate`s.
  - While its window is full, the server keeps simulating but sends that client nothing.
  - When the client acknowledges, the server sends only the latest day, with `skipped` counting the days skipped.
  - In single player the simulation never waits for the client. Multiplayer fairness rules are in D24.
- **Saves** are the scenario (path and content hash), the day, every applied command with the player who sent it (none for the scenario's scripted commands), and `state_hash` checkpoints every 30 days, plus a binary snapshot of the saved day (D10). The file format is in [DATA_FORMAT.md](DATA_FORMAT.md#save-files-savesnametoml-d23). Rules:
  - **Names** are 1 to 64 characters of `[A-Za-z0-9_-]`, so a name can't escape the saves directory. Anything else gets an error `SaveResult`.
  - **Loading never replays**, because replay runs at tick speed (about 4 minutes for a 20-year game at the D13 scale, over a 30-second limit; [PERFORMANCE.md](PERFORMANCE.md)). It reads the snapshot, and refuses the save if:
    - the scenario's content hash changed;
    - a command is invalid (`World::validate`), out of day order, or not before the saved day;
    - the checkpoints aren't exactly the checkpoint days up to the saved day;
    - on a checkpoint day, the last checkpoint differs from the snapshot's hash;
    - the snapshot isn't the saved day's state.

    Beyond these checks the log is trusted until a replay checks it. A refusal is an error, and the running game is left untouched. DATA_FORMAT says how each check reads the file.
  - **Replaying** (`pax_data::save::load_by_replay`, `pax_cli replay`) is the full check. It makes the same checks, then re-applies every logged command on its day through `step_day`, verifies every checkpoint, and must end exactly at the snapshot. The session replay test (M3-9) runs it in CI.
  - A successful load pauses the game and sends every session a new `Welcome`. Commands queued for the next tick are discarded, because they never applied.
  - The scenario's scripted commands for days already played are in the log; later ones still come from the scenario, so nothing applies twice.
  - **The snapshot can't be redirected or forged into an impossible state.** Its path is always `<name>.world`, derived from the save's own name and never read from the file. Its scenario tables (geography, nation keys, seed) must equal the scenario's, and the restored world must pass `World::check_tables`. The state hash is no defence against a crafted file, because its author can recompute it, so the table rules are what refuse one.
  - Saving over an existing save writes both new files to temporary names first. A failed save (a full disk, say) leaves the old one loadable.
  - A missing or damaged snapshot is an error, never a silent fallback to replay.

## D24. Multiplayer authority

**Accepted (M4-0, owner, 2026-10-08).** Full design: [MILESTONE_4.md](MILESTONE_4.md). The numbers below (5 s, 30 s, 20 per second, and MILESTONE_4's bandwidth defaults) are **server settings with defaults**, to be confirmed in a multiplayer playtest; changing a default is not a change to this decision.

- **Permissions:** each session commands at most one nation, claimed in the lobby. The server checks a command's nation against the session's before `World::validate`; a mismatch gets `NotPermitted`. Permission (here) and rule validity (D21) are separate checks, in that order. Sandbox sessions exist only with `--sandbox`.
- **Lobby (M4-2):** a multiplayer server starts in a lobby. Players claim nations (a nation another player holds is refused) and mark themselves ready, which needs a nation or a sandbox seat. The host starts the game once everyone is ready. Until then commands get `NotStarted` and the clock doesn't run. Single player has no lobby. A load in a multiplayer game goes back to the lobby: players keep the claims the loaded game has and become unclaimed otherwise (a seat is never widened to sandbox), and the host starts again (M4-5).
- **Host:** only the host changes speed, unpauses, saves, loads and kicks. Any player may pause.
  - The host is the first player on a player-hosted server. When the host leaves, the remaining player with the lowest id becomes host, so a game is never left without anyone able to unpause it (M4-3).
  - On a dedicated server, `--admin NAME` names the host's client instead. While the admin is away there is no host, and the role never passes to another player. A name is only what the client says it is, so the admin proves it with the admin password (`--admin-password-file`, required with `--admin`, M4-6); the name without it joins as an ordinary player.
  - A refused speed change is answered with the unchanged `ServerState`, and a refused save or load with a `SaveResult` error. A refused kick is ignored.
- **Order:** commands apply in `(day, player, sequence)` order, all server-stamped. Any future command that can conflict with another player's must define its own conflict rule in its decision. Player order is only a deterministic tie-break, and would otherwise always favour lower ids.
- **Lag, in wall-clock time:**
  - updates coalesce per client (D23);
  - a remote session (its peer is not on the server's machine) gets at most 4 day updates a second (`--updates-per-second`; the answer to `Subscribe` goes out at once, and the next day keeps the gap from it), and its `MapView` with the answer to `Subscribe` and then with every 5th update (`--map-every`). The updates in between carry no `MapView`, the one exception to D22's "Views, not state"; the client keeps its colours. Local sessions get every update with the map (M4-7);
  - 5 s of silence from a client pauses the game ("waiting for player"). `ServerState.waiting_for` names who it waits for. When everyone is back, or the silent player is dropped, the game resumes at the speed it had. A speed the host sets while it waits is the speed it resumes at; the game doesn't run until everyone is back or dropped, also when the silence began in the lobby (M4-4);
  - 30 s drops the session, and its nation keeps its current policies. In multiplayer this replaces D22's 10 s liveness rule, which would otherwise drop a client before the fairness pause could help it; single player keeps D22's rule;
  - a resume token (64 bits from the OS's secure random source) reclaims the nation. In a started game, any player who leaves keeps their seat (player id and nation) for their token: the server can't tell a crash from a quit. A kept seat counts toward the player limit, nobody else can take its nation, and the lobby shows it as away. A kick or a load drops kept seats (M4-4).
- **Transport:**
  - TLS whenever the server is not bound to localhost (rustls, M4-6). A player-hosted server makes a self-signed certificate at start (`--tls-self-signed`); a dedicated one loads PEM files (`--tls-cert`, `--tls-key`). Clients pin the certificate's SHA-256, which the server prints and the host shares. No certificate authority is involved, and clients still check the handshake's signatures against the pinned certificate;
  - an optional server password, sent in `Hello` (`--password-file`; M4-6). A wrong or missing one is `Rejected`, and passwords are compared in constant time. On a dedicated server the admin also proves who they are with an admin password (`--admin-password-file`, required with `--admin`): a name alone proves nothing;
  - a per-session command rate limit (default 20 per second, `--commands-per-second`): more get `RateLimited` (M4-6).

