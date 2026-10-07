# Inter-Market Trade (M2-3) — Design Proposal

**Status: proposal for review.** Nothing here is implemented yet. It turns the binding principles of [DECISIONS.md D14](DECISIONS.md#d14-market-hierarchy-and-inter-market-trade) into a concrete mechanism.

**On approval** it becomes decision **D17** and is implemented in small PRs (see [Delivery plan](#delivery-plan)). Please answer the [open questions](#open-questions-for-review) first.

## Goals

D14 already fixes the rules:
1. Trade is a **flow of goods driven by price gaps**, not a queue of leftover orders.
2. Every market still clears by itself with D1.
3. Scarce exports are shared **pro rata**: no priority by nation rank or table order.
4. Iceberg losses destroy goods, never money. Tariffs are a transfer to the importing treasury.
5. Friction comes from a matrix precomputed at load or when infrastructure changes. No pathfinding in the tick.

On top of that, implementation constraints:
- Stock-flow consistent (D5) and deterministic (D3).
- Markets stay **independent within a tick**, so they keep clearing in parallel. No global simultaneous clearing.
- No new clearing machinery: reuse D1's `BuyOrder` and `SellOffer`.

## Mechanism: merchants

A **merchant** is an agent bound to one directed route `A → B`. It's a new table, `Merchants`, with these columns:

| Column | Type | Meaning |
|---|---|---|
| `route` | `u32` | Index into the route table: `(from, to, τ, capacity)` |
| `cash` | `Fixed` | Working capital. Outside money, so part of the D5 invariant |
| `transit` | `Fixed` per good | Goods bought in A, arriving in B tomorrow |
| `stock` | `Fixed` per good | Goods already in B, offered for sale there |
| `cost_basis` | `Fixed` per good | Average landed cost per unit in B (reservation price) |
| `profit_avg` | `Fixed` | Smoothed profit, for dividends |

### Daily cycle

These steps slot into the existing tick (`tick.rs`):

1. **Arrival** (start of day, before production). `transit` lands in B as `transit × (1 − τ_AB)`, and the iceberg share is destroyed.
   - The **tariff** is paid now, from merchant cash to the treasury of B's nation: `tariff_rate(B, A) × landed value`. Landed value is the units arriving times yesterday's price in A.
   - The cost basis is updated (purchase cost + tariff, divided by units landed).
2. **Orders**, during market order formation, using **yesterday's** executed prices:
   - **Export leg (buy in A):** buy good `g` if the netback `p_B·(1 − τ) − t·p_A` exceeds `p_A·(1 + margin)`. The merchant places a `BuyOrder` in A.
     - Demand is `min(q*, budget/p)`.
     - `q* = capacity_share × min(1, gap / (k · p_A))`, so the flow grows with the price gap (D14.2) and is throttled by route capacity.
     - The budget comes from merchant cash; `capacity_share` splits the route's capacity across goods in proportion to their gaps.
   - **Import leg (sell in B):** the merchant offers `stock` in B as a `SellOffer`, with reservation price `cost_basis × (1 + margin)`.
3. **Clearing:** markets clear independently with D1. **Merchants are ordinary buyers and sellers, so pro-rata rationing (D14.3) holds automatically.** If exporters want more of A's grain than A's sellers offer, every buyer gets the same fraction, local or merchant.
4. **Settlement:**
   - Goods bought in A go into `transit`.
   - Sales in B reduce `stock`, and the receipts go to merchant cash.
5. **Dividends** (in the firms step): cash above a working-capital reserve is paid at `dividend_payout_rate` to the merchant's **owner pool**: the capitalists of market A, as with producers. That's the same mechanism and the same income tax (D15).

### Why one day of transit

- **Markets stay independent within a tick:** a merchant buys in A *today* and sells in B *tomorrow*. Without transit, A and B would need joint clearing, losing parallelism and adding a new algorithm.
- **It's realistic:** shipping takes time, and multi-day transit for long routes is a natural extension (`transit_days` per route).
- **It's stable:** prices converge over days through arbitrage instead of jumping, which suits the ±10% daily band (D1).

## Routes, friction and tariffs

- **Routes** are scenario data:

  ```toml
  [[route]]
  from = "lowland"
  to = "highland"
  iceberg = 0.05      # τ: share of goods lost in transit
  capacity = 200      # units per day, all goods together
  both_ways = true    # also creates highland → lowland
  ```

- **Multi-hop routes:** at load time, the best retention `Π(1 − τ)` between market pairs (losses multiply along a path) is computed in fixed point, with no floats or logarithms. The capacity is the minimum along the path. That gives the precomputed friction data D14.5 requires; see open question 6 for its size.
  - In M2-3 a merchant is created only for each *direct* route. Multi-hop merchants are a follow-up.
- **Tariffs:**
  - A nation gets `tariff_rate`, an ad-valorem rate on goods arriving from a market of another nation (or a stateless market).
  - Trade within one nation is untaxed. Customs unions (nations that waive tariffs between them) come later.
  - A tariff is paid in money from merchant cash to the importer's treasury, so it's conserved (D5, D14.4).

## Accounting (D5)

| Flow | From | To |
|---|---|---|
| Purchase in A | merchant cash | A's sellers (normal receipts split) |
| Tariff | merchant cash | treasury of B's nation |
| Sale in B | B's buyers | merchant cash |
| Dividend | merchant cash | capitalist owner pool (net of D15 income tax) |
| Iceberg loss | goods in transit | destroyed (no money moves) |

- `World::total_money` gains `Σ merchant cash`. The conservation test and the random-world generator get random routes.
- New goods invariant per tick: `Σ(transit + stock)` changes exactly by purchases − sales − iceberg loss.

## What it should do (acceptance tests)

1. **Price convergence:** with one route and no tariff, a good with `p_B > p_A/(1 − τ)` sees prices converge until `p_B(1 − τ) ≈ p_A(1 + margin)`.
2. **No trade without a gap:** equal prices produce zero flow (no churn).
3. **Tariffs shrink trade:** raising B's tariff reduces the flow A→B, and the treasury receives exactly the tariffs paid.
4. **Capacity binds:** the flow never exceeds route capacity.
5. **Pro-rata exports:** when exporters and locals compete for A's scarce supply, both receive the same rationing fraction.
6. **Determinism and conservation:** the existing harness, with merchants in the hash and the invariant.
7. **two_states:** a route between Lowland (cheap cloth, dear tools) and Highland (cheap tools, dear cloth). Both should gain real GDP compared with autarky (comparative advantage), and the 20-year stability test should still pass.

## Delivery plan

| PR | Content |
|---|---|
| 1 | D17 in DECISIONS.md; route data and loader; friction matrix (Floyd–Warshall) with tests. No behaviour change. |
| 2 | `Merchants` table, arrival, transit, money invariant; merchants only *buy* (stock accumulates). Conservation tests. |
| 3 | Selling in B, cost basis, dividends, tariffs. Acceptance tests 1–5. |
| 4 | Route in `two_states`; golden hashes re-recorded; stability and comparative-advantage checks. |

## Open questions for review

1. **Ownership:** should merchants be owned by the capitalists of the *origin* market (proposed), the destination, or a new "merchant" profession? This decides who gets trade profits, and so politics later.
2. **Transit time:** one day for every route (proposed), or `transit_days` per route from the start?
3. **Margin and flow speed (`margin`, `k`):** global tuning in `rules.toml` (proposed), or per route?
4. **Tariff base:** origin price (proposed; simple and deterministic) or destination price?
5. **Merchant creation:** one merchant per direct route, created at load (proposed), or dynamic entry when profitable? Dynamic entry needs investment rules (a later M2 item).
6. **Friction matrix size:** a dense all-pairs matrix (D14.5) costs `markets²` entries: about 72 MB of `Fixed` at 3,000 markets, and Floyd–Warshall is O(markets³). Proposed: a **trade horizon**. Run a per-source Dijkstra over the sparse route graph (max-product of retention, deterministic tie-break) and keep only pairs whose retention `Π(1 − τ)` is at least a `min_retention` threshold (e.g. 0.5) in a per-market sparse list. Beyond that, goods lose too much to be worth shipping. Is a horizon acceptable for gameplay, or do you want every pair?
