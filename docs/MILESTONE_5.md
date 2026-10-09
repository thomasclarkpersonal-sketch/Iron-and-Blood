# Milestone 5: Trade, Investment and Economic Unrest

> [!NOTE]
> **Status: planned, not started.** Its decisions D17, D27 and D28 are *Proposed* in [DECISIONS.md](DECISIONS.md); the designs are [TRADE.md](TRADE.md), [INVESTMENT.md](INVESTMENT.md) and [REBELLIONS.md](REBELLIONS.md), and [DATA_MODEL_M5_M6.md](DATA_MODEL_M5_M6.md) holds the data model.

**Goal:** Turn isolated local economies into an interconnected world trading system, unleash industrial growth through capital investment so output can grow beyond the capacity a scenario starts with, introduce economic consequences for worker militancy (strikes and riots), and equip the Godot client with full trade, tariff, and construction controls.

Milestone 5 builds on the single-player and multiplayer foundation established in [Milestone 3](MILESTONE_3.md) and [Milestone 4](MILESTONE_4.md). Binding architectural rules follow [DECISIONS.md](DECISIONS.md) (in particular D1, D3, D5, D6, D13, D14, and proposed decisions D17, D27, D28).

---

## 🧭 Scope

### In Scope
1. **Inter-Market Trade and Logistics (D14, D17):**
   * Directed routes with Samuelson iceberg transport costs ($\tau$, destroying goods, never money).
   * A precomputed **trade horizon** using per-source Dijkstra on retention $\prod(1 - \tau) \ge \text{min\_retention}$, avoiding $O(N^3)$ matrix blowup at scale.
   * `Merchants` table buying in origin markets and selling in destination markets with price-gap arbitrage.
   * 1-day transit delay preserving independent parallel market clearing across markets without cross-market locks.
   * Ad-valorem tariffs on the origin price, paid from merchant cash directly into the importing nation's treasury.
   * Per-route tuning: each route sets its own profit margin and flow speed.
   * Dynamic merchant entry and exit, with starting merchants a scenario may seed per route.
   * Institutional diversity in merchants (`MerchantKind`: private exporter-owned, commercial merchant class, state-chartered monopoly).
2. **Capital Investment and Capacity Growth (D27):**
   * Existing producers expanding capacity from retained earnings when profitable, liquid, and labour is available.
   * Construction recipes demanding physical intermediate goods (tools, steel, timber) through standard D1 market buy orders.
   * Founding brand-new factory/RGO types in provinces, by a one-time transfer from the market's capitalists or the state treasury (D27; no pool holds cash).
   * Depreciation of idle plant capacity after prolonged downturns.
   * Growth in `two_states`: capacity and real GDP rise over 20 years, with unemployment within its band (M5-10 has the baselines).
3. **Economic Feedback on Unrest (D28):**
   * **Strikes:** Deterministic, continuous reduction in effective labour supply when POP militancy exceeds `strike_threshold`.
   * **Riots:** Monthly deterministic inventory destruction and treasury security transfers when province militancy exceeds `riot_threshold`.
4. **Wire Protocol & Godot Client:**
   * Wire protocol additions: `TradeRouteView`, `InvestmentLedgerView`, `SetTariffCommand`, `FoundProducerCommand`.
   * Godot UI: Trade Route overview, tariff policy sliders, construction project queues, and trade flow map mode.
5. **Determinism and Performance Gates:**
   * Golden replays verified in CI, outside-money conservation invariant asserted daily, and tick execution within the 100 ms/day budget at 1M POP rows (D13).

### Out of Scope (Milestone 6 and Milestone 7)
* Political parties, interest groups, consciousness, and voting reforms ([Milestone 6](MILESTONE_6.md)).
* Banking, endogenous inside money, commercial loans, and sovereign debt ([Milestone 6](MILESTONE_6.md)).
* Armed rebellions, civil wars, and market secessions ([Milestone 6](MILESTONE_6.md)).
* Military regiments, standing armies, mobilization, frontlines, and combat ([Milestone 7](MILITARY_SYSTEM.md)).
* Colonization, foreign concessions, and imperialism ([Milestone 7](COLONIZATION_SYSTEM.md)).

---

## 🏛️ System Design & Decisions

This milestone's rules live in their decisions and designs; this section only says where. Each is *Proposed* until accepted.

- **Tick order:** D4 is amended to the order in [DATA_MODEL_M5_M6.md](DATA_MODEL_M5_M6.md), "Tick order" (arrival before labour; riots after politics; investment before demographics).
- **1. Inter-market trade:** rules in [D17](DECISIONS.md#d17-inter-market-trade-routes-merchants-and-tariffs); mechanism, the owner's answers and acceptance tests in [TRADE.md](TRADE.md). In brief: merchants arbitrage price gaps with one day of transit, three ownership kinds, per-route tuning, tariffs on the origin price, dynamic entry and exit with seeded merchants, a sparse trade horizon.
- **2. Capital investment and construction:** rules in [D27](DECISIONS.md#d27-capital-investment-and-capacity-expansion); mechanism in [INVESTMENT.md](INVESTMENT.md). In brief: producers expand from retained earnings, buying real construction goods; new producers are founded by capitalists or the state; idle capacity shrinks.
- **3. Economic unrest:** rules in [D28](DECISIONS.md#d28-economic-unrest-strikes-and-riots); mechanism in [REBELLIONS.md](REBELLIONS.md). In brief: strikes cut labour supply above a threshold; riots destroy stock and trigger an automatic security transfer. **Open (D28):** how strikers are paid, and whether the riot transfer relieves militancy directly.
- **Data model:** [DATA_MODEL_M5_M6.md](DATA_MODEL_M5_M6.md).

---

## 📋 Tasks

| ID | Task | Depends on | Notes |
|---|---|---|---|
| **M5-1** | **Route loader & precomputed trade horizon** | — | Scenario TOML `[[route]]` definitions, each with its own `margin` and `k`; per-source Dijkstra over retention $\prod(1 - \tau) \ge \text{min\_retention}$; deterministic tie-breaking. |
| **M5-2** | **`Merchants` table & arrival phase** | M5-1 | SoA columns (`route`, `cash`, `transit`, `stock`, `cost_basis`); arrival step destroys iceberg share $\tau$ and transfers ad-valorem tariff to destination treasury; stock-flow conservation tests. |
| **M5-3** | **Merchant market orders & arbitrage** | M5-2 | Export buy orders in $A$ and import sell offers in $B$ based on netback price gaps; pro-rata rationing naturally verified. |
| **M5-4** | **Merchant institutional types, dividends & exit** | M5-3 | `MerchantKind` (Private, Commercial, Chartered); net dividend distribution to origin capitalists, merchant POPs, or national treasuries; scenario-seeded merchants; winding up of loss-making merchants. |
| **M5-5** | **Trade balance & comparative advantage test** | M5-4 | Implement trade route in `two_states` between Lowland (cheap cloth, dear tools) and Highland (cheap tools, dear cloth); verify mutual real GDP growth and price convergence. |
| **M5-6** | **Construction recipes & project state** | — | Add `expansion` recipes in `producer_types.toml`; add SoA columns `project_remaining`, `project_budget`, `idle_months` on `Producers`. |
| **M5-7** | **Producer expansion & construction clearing** | M5-6 | Monthly evaluation gating on profit, labour, and liquidity; daily D1 buy orders for construction goods; capacity increase on completion. |
| **M5-8** | **Founding new producers** | M5-7 | Mechanism to found new factory types in provinces with unemployed workers, by a one-time transfer from the market's capitalists (largest remainder) or from the treasury by command, into the new producer's cash, with no pool in between (D27). |
| **M5-9** | **Capacity depreciation & disinvestment** | M5-7 | Decommission capacity slots of chronically understaffed producers; verify inventory stability. |
| **M5-10** | **Growth verification** | M5-7, M5-8 | 20-year run of `two_states`: prove capacity and real GDP grow (from a flat ≈4,200/day without investment, MILESTONE_2 "Measured state") while unemployment stays within its band (≈0.8% since D25). |
| **M5-11** | **Deterministic strikes** | — | Continuous reduction of effective labour supply above `strike_threshold`; test output drops, wage floor consistency, and money conservation. |
| **M5-12** | **Monthly riots and security transfers** | M5-11 | Output inventory destruction and security transfer from treasury to POP cash at month end above `riot_threshold`. |
| **M5-13** | **Wire protocol additions (`pax_protocol`)** | M5-5, M5-8 | Update FlatBuffers schemas with `TradeRouteView`, `InvestmentLedgerView`, `SetTariffCommand`, and `FoundProducerCommand`; regenerate protocol crate. |
| **M5-14** | **Godot client panels & map modes** | M5-13 | UI screens for trade routes, tariff sliders, construction queues, and trade flow map mode overlay in Godot. |
| **M5-15** | **Replay gate, benchmarks, and docs** | all | Golden hashes re-recorded for `two_states`; `session_replay.rs` covers trade and construction commands; benchmark within D13 budget (1M POPs $\le 100\text{ ms/day}$). |
| **M5-16** | **Dynamic merchant entry** | M5-4, M5-8 | Founding merchants on routes with a persistent gap and too little merchant cash, funded by a one-time transfer from the merchant's owner, as D17 says: the market's capitalists (Private) or merchant POPs (Commercial), with no pool in between (Chartered by command); lands after investment's funding rules (TRADE.md, "Entry and exit"). |

---

## 🎯 Definition of Done

1. **Trade Works and Arbitrages Prices:**
   * In `two_states`, opening trade between Lowland and Highland causes price gap to converge towards the transport friction margin.
   * Tariffs directly reduce traded volume and credit the importing treasury with 100% of tariff payments.
   * Merchants enter routes with a persistent price gap and exit when loss-making, with money conserved through both.
   * Total money invariant asserts on every tick with random trade routes and high trade volumes.
2. **Investment Grows the Economy:**
   * In `two_states` over 20 years, producer capacity and real GDP rise above the baselines in M5-10, while unemployment stays within its band.
   * Expanding producers generate sustained market demand for construction inputs (tools, timber, steel).
3. **Economic Feedback on Discontent:**
   * High militancy above `strike_threshold` measurably reduces production output.
   * High militancy above `riot_threshold` triggers stock destruction and treasury repression payouts.
4. **Authoritative Client & Multiplayer Controls:**
   * Players can view active trade routes, adjust national tariff rates, inspect factory construction progress, and found new factories from the Godot client.
   * Commands are fully validated by `World::validate`, stamped in sequence, and replayed identically in `session_replay.rs`.
5. **Budgets & Determinism:**
   * Verification test passes across 1, 2, 3, and 8 threads with bit-identical results.
   * 1M POP rows scale ticks within the $\le 100\text{ ms/day}$ performance budget on 8 threads.
