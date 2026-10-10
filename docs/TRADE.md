# Inter-Market Trade (M5): Design

**Status: design agreed with the owner (2026-10-09), not implemented yet; its decision was accepted on 2026-10-10, by the maintainer for M5.** It turns the binding principles of [DECISIONS.md D14](DECISIONS.md#d14-market-hierarchy-and-inter-market-trade) into a concrete mechanism, and is delivered in Milestone 5. Its rules are [D17](DECISIONS.md#d17-inter-market-trade-routes-merchants-and-tariffs) (binding); this document holds the mechanism. The owner's answers to the review questions are under [Decisions](#decisions-owner-2026-10-09).

## Goals

D14 already fixes the rules:
1. Trade is a **flow of goods driven by price gaps**, not a queue of leftover orders.
2. Every market still clears by itself with D1.
3. Scarce exports are shared **pro rata**: no priority by nation rank or table order.
4. Iceberg losses destroy goods, never money. Tariffs are a transfer to the importing treasury.
5. Friction comes from data precomputed at load or when infrastructure changes. No pathfinding in the tick.

On top of that, implementation constraints:
- Stock-flow consistent (D5) and deterministic (D3).
- Markets stay **independent within a tick**, so they keep clearing in parallel. No global simultaneous clearing.
- No new clearing machinery: reuse D1's `BuyOrder` and `SellOffer`.

## Decisions (owner, 2026-10-09)

| Question | Decision |
|---|---|
| Who owns merchants | **Three kinds** (`MerchantKind`): **Private**, owned by the origin market's capitalists; **Commercial**, owned by a merchant profession; **Chartered**, a state company paying the treasury. See [Ownership](#ownership-merchantkind). |
| Transit time | **One day** on every route. Per-route `transit_days` is a later extension. |
| Margin and flow speed | **Per route**: each `[[route]]` sets its own `margin` and `k`. |
| Tariff base | **Origin price**: yesterday's price in the exporting market. |
| How merchants come to exist | **Dynamic entry and exit**, and a scenario **may seed** starting merchants per route. See [Entry and exit](#entry-and-exit). |
| Friction data | A sparse **trade horizon**, not a dense all-pairs matrix. See [Routes and the trade horizon](#routes-and-the-trade-horizon). |

## Mechanism: merchants

A **merchant** is an agent bound to one directed route `A → B`. It's a new table, `Merchants`, with these columns:

| Column | Type | Meaning |
|---|---|---|
| `route` | `u32` | Index into the route table: `(from, to, τ, capacity, margin, k)` |
| `kind` | `MerchantKind` | Private, Commercial or Chartered: who owns it and receives its dividends |
| `cash` | `Fixed` | Working capital. Outside money, so part of the D5 invariant |
| `transit` | `Fixed` per good | Goods bought in A, arriving in B tomorrow |
| `stock` | `Fixed` per good | Goods already in B, offered for sale there |
| `cost_basis` | `Fixed` per good | Average landed cost per unit in B (reservation price) |
| `profit_avg` | `Fixed` | Smoothed profit, for dividends and the exit test |

### Daily cycle

These steps slot into the existing tick (`tick.rs`):

1. **Arrival** (start of day, before production). `transit` lands in B as `transit × (1 − τ_AB)`, and the iceberg share is destroyed.
   - The **tariff** is paid now, from merchant cash to the treasury of B's nation: `tariff_rate(B, A) × landed value`. Landed value is the units arriving times **yesterday's price in A** (the origin price, decided).
   - The cost basis is updated (purchase cost + tariff, divided by units landed).
2. **Orders**, during market order formation, using **yesterday's** executed prices and the route's own `margin` and `k`:
   - **Export leg (buy in A):** buy good `g` if the netback `p_B·(1 − τ) − t·p_A` exceeds `p_A·(1 + margin)`. The merchant places a `BuyOrder` in A.
     - Demand is `min(q*, budget/p)`.
     - `q* = capacity_share × min(1, gap / (k · p_A))`, so the flow grows with the price gap (D14.2) and is throttled by route capacity.
     - The budget comes from merchant cash; `capacity_share` splits the route's capacity across goods in proportion to their gaps, and across the route's merchants in proportion to their cash.
   - **Import leg (sell in B):** the merchant offers `stock` in B as a `SellOffer`, with reservation price `cost_basis × (1 + margin)`.
3. **Clearing:** markets clear independently with D1. **Merchants are ordinary buyers and sellers, so pro-rata rationing (D14.3) holds automatically.** If exporters want more of A's grain than A's sellers offer, every buyer gets the same fraction, local or merchant.
4. **Settlement:**
   - Goods bought in A go into `transit`.
   - Sales in B reduce `stock`, and the receipts go to merchant cash.
5. **Dividends** (in the firms step): cash above a working-capital reserve is paid at `dividend_payout_rate` to the merchant's owner, by kind (below), with the same income tax (D15) where the owner is a POP.

### Why one day of transit

- **Markets stay independent within a tick:** a merchant buys in A *today* and sells in B *tomorrow*. Without transit, A and B would need joint clearing, losing parallelism and adding a new algorithm.
- **It's realistic:** shipping takes time, and multi-day transit for long routes is a natural extension (`transit_days` per route).
- **It's stable:** prices converge over days through arbitrage instead of jumping, which suits the ±10% daily band (D1).

## Ownership (`MerchantKind`)

| Kind | Owner | Dividends go to | Founded from |
|---|---|---|---|
| **Private** | the origin market's capitalists | the origin market's capitalist owner pool, as producers' dividends | a one-time transfer from those capitalists (as new producers, D27) |
| **Commercial** | a **merchant** profession (new, in `professions.toml`) | the merchant POPs of the origin province | the merchant POPs' savings |
| **Chartered** | the origin market's nation | its treasury (no income tax: it's the state) | the treasury, by a player command |

Who receives trade profits shapes politics later (interest groups, M6), which is why the kinds are explicit.

## Entry and exit

Merchants are not fixed at load. Each month end:
- **Entry:** on a route in the trade horizon whose smoothed price gap for some good exceeds its `margin`, and whose merchants' combined cash can't buy the route's `capacity` at today's prices, an owner with funds founds a new merchant with starting capital. Private and Commercial merchants are founded by their owner pools with the same funding rule as new producers (D27, [INVESTMENT.md](INVESTMENT.md)); Chartered merchants only by command. At most one entry per route and month, so the response is gradual and deterministic (largest gap first, then lowest route).
- **Exit:** a merchant whose smoothed profit has been negative for `exit_months` winds up: it stops buying, sells off its stock, and its remaining cash returns to its owner (pool, POPs or treasury). Money is moved, never destroyed (D5).
- **Seeding:** a scenario may list starting merchants per route (for example historical trading houses), with their kind and capital, so trade works from day 1 and before any route has proved itself:

  ```toml
  [[merchant]]
  route = "lowland-highland"
  kind = "private"
  cash = 500
  ```

**Order of delivery:** entry uses the investment design's funding rules, so dynamic entry lands after them. Until then, seeded merchants trade, and exit already works.

## Routes and the trade horizon

- **Routes** are scenario data, each with its own tuning:

  ```toml
  [[route]]
  from = "lowland"
  to = "highland"
  iceberg = 0.05      # τ: share of goods lost in transit
  capacity = 200      # units per day, all goods together
  margin = 0.05       # minimum netback over the origin price before a merchant buys
  k = 0.2             # flow speed: the gap at which flow reaches full capacity
  both_ways = true    # also creates highland → lowland, with the same values
  ```

- **Trade horizon:** a dense all-pairs matrix costs `markets²` entries (about 72 MB of `Fixed` at 3,000 markets) and O(markets³) to build. Instead, at load (and when infrastructure changes) a **per-source Dijkstra** over the sparse route graph finds the best retention `Π(1 − τ)` to each reachable market, in fixed point with a deterministic tie-break, and keeps only pairs with retention at least `min_retention` (e.g. 0.5), in a sparse per-market list. Beyond that, goods lose too much to be worth shipping. Capacity is the minimum along the path.
  - Merchants trade on *direct* routes first; multi-hop merchants on horizon pairs are a follow-up.
- **Tariffs:**
  - A nation sets `tariff_rate`, an ad-valorem rate on goods arriving from a market of another nation (or a stateless market), charged on the **origin price**.
  - Trade within one nation is untaxed. Customs unions (nations that waive tariffs between them) come later.
  - A tariff is paid in money from merchant cash to the importer's treasury, so it's conserved (D5, D14.4).

## Accounting (D5)

| Flow | From | To |
|---|---|---|
| Founding (entry) | the origin market's capitalists, merchant POPs or treasury, in one transfer | new merchant's cash |
| Purchase in A | merchant cash | A's sellers (normal receipts split) |
| Tariff | merchant cash | treasury of B's nation |
| Sale in B | B's buyers | merchant cash |
| Dividend | merchant cash | the owner, by kind (net of D15 income tax for POPs) |
| Winding up (exit) | merchant cash | the owner, by kind |
| Iceberg loss | goods in transit | destroyed (no money moves) |

- `World::total_money` gains `Σ merchant cash`. The conservation test and the shared random-world generator get random routes, seeded merchants, and entries and exits.
- New goods invariant per tick: `Σ(transit + stock)` changes exactly by purchases − sales − iceberg loss.

## What it should do (acceptance tests)

1. **Price convergence:** with one route and no tariff, a good with `p_B > p_A/(1 − τ)` sees prices converge until `p_B(1 − τ) ≈ p_A(1 + margin)`, with the route's own margin.
2. **No trade without a gap:** equal prices produce zero flow (no churn), and no entry.
3. **Tariffs shrink trade:** raising B's tariff reduces the flow A→B, and the treasury receives exactly the tariffs paid, computed on the origin price.
4. **Capacity binds:** the flow never exceeds route capacity, however many merchants share it.
5. **Pro-rata exports:** when exporters and locals compete for A's scarce supply, both receive the same rationing fraction.
6. **Entry and exit:** a persistent gap with too little merchant cash founds a merchant; a loss-making merchant winds up and returns its cash; money is conserved through both.
7. **Ownership:** each kind's dividends reach its owner (origin capitalists, merchant POPs, treasury).
8. **Determinism and conservation:** the existing harness, with merchants in the hash and the invariant.
9. **two_states:** a route between Lowland (cheap cloth, dear tools) and Highland (cheap tools, dear cloth), seeded with one Private merchant each way. Both should gain real GDP compared with autarky (comparative advantage), and the 20-year economic bands should hold or move for a stated reason.

## Delivery plan (M5)

| Step | Content |
|---|---|
| 1 | Route data (with per-route `margin`, `k`) and loader; the trade horizon (per-source Dijkstra) with tests. No behaviour change. |
| 2 | `Merchants` table (with `kind`), seeded merchants, arrival, transit, money invariant; merchants only *buy* (stock accumulates). Conservation tests. |
| 3 | Selling in B, cost basis, tariffs on the origin price, dividends by kind, exit. Acceptance tests 1–5, 7. |
| 4 | Route and seeded merchants in `two_states`; golden hashes re-recorded; bands and comparative-advantage checks. |
| 5 | Dynamic entry, after the investment design's funding rules. Acceptance test 6. |
