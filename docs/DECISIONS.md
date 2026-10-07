# Design Decisions

This is the binding record of decisions that other documents and the code depend on. Where any other document disagrees with this one, **this one wins**, and the other document is a bug to fix. Code comments refer to entries as `D1`–`D14`.

Each entry has a status:
- **Accepted**: implemented or binding now.
- **Accepted (principle)**: the rules are binding, and the details are scheduled for a later milestone.
- **Deferred**: deliberately undecided, with a deadline and the criteria for deciding.

To change a decision, edit its entry in the same pull request as the code. Say what changed and why, and update every document that cites it.

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
| [D10](#d10-network-model-server-authoritative-deterministic-core) | Network/multiplayer | Accepted (principle) |
| [D11](#d11-determinism-harness-and-golden-files) | Determinism harness | Accepted |
| [D12](#d12-frontend-headless-first-client-chosen-before-m3) | Frontend | Deferred (before M3) |
| [D13](#d13-performance-budget) | Performance budget | Accepted |
| [D14](#d14-market-hierarchy-and-inter-market-trade) | Market hierarchy | Accepted (principle), M2 |
| [D15](#d15-nations-treasuries-income-tax-and-transfers) | Nations and fiscal policy | Accepted (M2-1) |

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
| 3 | Market: orders → price discovery → settlement | daily |
| 4 | Firms: wages, dividends | daily |
| 5 | *(M2)* Promotion/demotion, migration | weekly (day 7, 14, …) |
| 6 | *(M2)* Politics: militancy, consciousness | month end |
| 7 | Demographics | month end |

- A month is 30 days until a real calendar is needed (`rules.days_per_month`).
- **Goods persist.** Unsold output stays in the producer's stock, and producers stop producing when stock reaches `target_stock_days` of output (D6).
- Goods bought by POPs are consumed immediately; households hold no inventory in M1.
- Perishability, if wanted, will be a per-good decay rate in data, not an implicit loss.

## D5. Money model and stock-flow consistency

**Problem.** "Total cash must equal M0" conflicted with banks that lend deposits (which creates broad money). Currencies were unspecified, and so was what happens to money when POPs die or firms fail.

**Decision.**
- **One world currency** until at least M3. Per-nation currencies would need a foreign-exchange market and are out of scope.
- **Outside money** (`Σ cash` over all agents) is constant. It may change only through an explicit, logged *mint* or *burn* event. Today none exists, and `tick::step` panics if the total moves.
- **Inside money** (deposits, loans, bonds; M2) is always created as a matched asset/liability pair. The invariant becomes:

  `Σ financial assets − Σ financial liabilities = Σ outside money` (every IOU nets to zero).

  A bank loan creates a deposit (asset of the borrower) and a loan (asset of the bank), offset by a liability on each side. This is endogenous money in Godley & Lavoie's sense, and it does not break conservation. A default writes off both sides; nobody's *cash* disappears.
- **Every transfer debits one account and credits another by the same amount.** Splits use largest-remainder allocation.
- **Death/extinction:** money stays with surviving POP members; an extinct POP's cash passes to an heir (D7).
- **Firm failure (M2):** cash goes to creditors first, then owners.
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
- **Ownership is profession-level in M1.** A share registry (who owns which firm, so capitalists in one market can own firms in another) is an M2 item and will be needed for investment and bankruptcy.

## D7. POP accounting

- `cash` is the POP's **total** holdings, not a per-capita amount. When size changes (births, deaths, casualties) the money stays with the survivors.
- **Extinction:** if a POP reaches size 0, its cash passes to the largest living POP in the same province (lowest row on ties). If none exists, the empty row keeps it until someone moves in.
- **Splitting** (promotion, migration, conscription; M2): the moving fraction takes cash in proportion to people moved, using largest remainder. Intensive attributes (literacy, militancy) are copied.
- **Merging:** cash adds up. Intensive attributes become size-weighted averages, computed in `Fixed` with one rounding.
- **Promotion and migration are deterministic fractional flows** (`ΔN = ⌊N × rate⌋`), not dice rolls. The counter-based RNG (D3) is available where genuine randomness is wanted, e.g. rebellions.
- **POP identity** is `(province, profession, culture, religion)`. Culture and religion columns arrive in M2. Lookups by identity use a sorted index, never a `HashMap`.
- **Derived values are never stored as state.** Nation, market and state come from the province; storing `nation_id` on POPs would go stale on conquest.
  - **Exception: self-validating caches.** For performance, derived data may live in a cache *outside* state, under three conditions: it's excluded from equality and `World::state_hash`; it fingerprints all of its inputs on every use and rebuilds on mismatch; and debug builds check it against a fresh build.
  - The only such cache is `World::layout` (`layout.rs`). Its inputs are the POP row count, `pops.province`, `pops.profession`, `geography.province_market` and the number of professions.

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

## D10. Network model: server-authoritative, deterministic core

**Problem.** AGENTS.md assumed lockstep multiplayer, while BACKEND_SCHEMA described a server pushing state to clients.

**Decision.**
- The simulation runs in **one authoritative process** (`pax_server`, M3). Clients send commands and receive aggregated state snapshots plus on-demand detail (e.g. one province's POPs). Commands are applied at the start of the next tick, in order of `(tick, player id, sequence)`.
- Determinism (D3) is kept anyway. It makes save files tiny (initial state + command log), makes desyncs debuggable, and keeps lockstep possible later without a rewrite.
- The protocol is binary (no JSON on the hot path). **FlatBuffers vs Cap'n Proto is deferred to the start of M3.** Criteria: Godot/web client library support (D12), schema evolution, zero-copy reads.

## D11. Determinism harness and golden files

- `tick::run` returns a stable FNV-1a hash of all state after each day (`World::state_hash`). It does not use `DefaultHasher`, which is not stable across Rust versions.
- `scenarios/*/golden.hashes` pins one year of hashes. CI runs `pax_cli verify` at 1 and 4 threads on Linux, and on Windows and macOS.
- Tests also check: same input gives same hashes; results are identical at 1/2/3/8 threads; resuming from a snapshot matches a continuous run.
- **Any change that alters simulation results must re-record the golden file in the same PR**, and say so in the description. Unexpected golden diffs are bugs.

## D12. Frontend: headless-first, client chosen before M3

**Deferred.** M1–M2 need no client: `pax_cli` prints market reports.

Decide Godot or Web (React + Three.js / WebGL) before M3, based on:
1. desktop vs browser distribution;
2. map rendering needs (thousands of provinces);
3. the team's skills;
4. protocol library support (D10).

The engine does not care, by construction.

## D13. Performance budget

| Target | Budget | Measured |
|---|---|---|
| M1: 1M POP rows, 1 market, 4 goods, 8 threads | ≤ 100 ms/day | **≈37 ms/day** after T4 (was 45; `pax_cli bench … --scale 170000 --threads 8`) |
| Long-term: 2M POP rows, ~3,000 markets, ~50 goods, 8-core desktop | ≤ 100 ms/day | 1M rows / 3,000 markets / 4 goods: **≈35 ms/day** after T5 (`bench --scale 56 --regions 3000`) |

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

