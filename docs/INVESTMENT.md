# Investment and Capacity Growth (M5): Design

**Status: design accepted by the owner (2026-10-09), not implemented yet.** It is delivered in Milestone 5. Its binding rules are [D27](DECISIONS.md#d27-capital-investment-and-capacity-expansion); this document holds the mechanism. The owner's answers to the review questions are under [Decisions](#decisions-owner-2026-10-09).

## The problem it solves

Producer capacity is fixed at load. Nothing can build a factory, expand a mine or close one down, so:
- **the economy can't grow:** real output is capped by the capacity a scenario starts with. Population grows only with employment (the births band-aid, D26), so with fixed capacity it stalls too;
- **capital has no productive use:** owners' dividends and producers' retained cash can only be consumed;
- **capacity can't follow the economy:** where demand vanishes, the jobs stay; where workers wait (the farm provinces of `two_states`), no one builds.

Labour mobility (D18), migration (D20) and occupational migration (D25) only move workers between the jobs that exist. In `two_states` they hold unemployment at about 0.8% (MILESTONE_2, "Measured state"), but output has nowhere to grow.

## Decisions (owner, 2026-10-09)

| Question | Decision |
|---|---|
| Who invests | **Producers expand from retained earnings.** **New producers are founded** from a capitalist investment pool or by the state. See [Founding](#founding-new-producers). |
| New producers | **Yes**, in provinces with unemployed workers. |
| Profit test | **A margin over the wage bill** (`V̄ > wage bill × (1 + profit_margin)`), not a payback period. |
| Depreciation and maintenance | **Idle capacity shrinks.** Capacity in use does **not** wear out or need maintenance goods in M5. |
| State industry | **Yes**: a nation can found producers from its treasury, by command. |

## Mechanism: producers expand from retained earnings

Investment is decided **per producer**, monthly. It uses *real goods* bought on the market, so it creates demand for tools, steel and timber. That ties heavy industry to growth, which is the core Victoria 2 loop.

### 1. Construction recipe (data)

Each producer type gets a construction recipe per added unit of capacity:

```toml
[[producer_type]]
key = "steel_mill"
# ...
expansion = { inputs = { tools = 0.02, timber = 0.05 }, step = 500 }   # goods per worker slot; slots per project
```

### 2. Decision (month end)

A producer starts an expansion project of `step` worker slots when **all** of these hold:
- **Profitable:** smoothed value added `V̄` exceeds the wage bill by `profit_margin`, so expanding would pay. Returns on new capacity are estimated with today's prices; no foresight (bounded rationality).
- **Workers available:** its labour pool has unemployment of at least `step` (no point building jobs nobody can fill). D20 and D25 widen this to the market, since they move workers to vacancies.
- **Liquid:** cash above the D6 restart reserve and the dividend reserve covers the project at today's prices.
- **No project already running.**

### 3. Building (daily, while a project runs)

- **Buying:** the producer places `BuyOrder`s for the construction goods, through D1 like any buyer and pro-rata rationed. The budget is fixed when the project starts.
- **Completion:** when all construction goods are delivered, `capacity += step` and the project closes. Goods are consumed by the construction, and money went to their sellers (D5).
- **Partial projects** carry over until completed.

### 4. Who pays

The producer pays from **retained cash**. This competes with dividends: the D6 dividend rule pays out cash above the reserve, so investment must reserve its budget *before* dividends.
- **Effect:** owners receive less today and more later. It's the classical trade-off between saving and consumption.
- **Banking** (inside money, M6) later lets producers borrow for projects instead of waiting to accumulate cash.

### 5. Disinvestment

A producer whose workforce has been below `capacity × (1 − slack)` for `idle_months_before_shrink` loses `step` capacity. That is depreciation of unused plant, and it stops capacity piling up where demand vanished. No money moves. Capacity in use doesn't decay in M5 (decided); maintenance goods can come later, once investment is balanced.

## Founding new producers

Each month end, in a province with a labour pool whose unemployment is at least a producer type's `step`, a new producer of a type employing that profession may be founded, when the type is profitable in that market: the same margin test, on the smoothed value added of the market's existing producers of that type, or on today's prices for a type the market doesn't have yet. The new producer starts with zero capacity and an expansion project of `step`, which it builds as above. Who funds it:

| Funder | Where the money comes from | Owner (dividends) |
|---|---|---|
| **Capitalist investment pool** | the market's capitalist owner POPs: the share of their cash above `investor_reserve` days of their consumption, taken pro rata to their cash (largest remainder, D7) | the market's capitalist owner pool, as the producer type's data says |
| **The state** | the nation's treasury, by a `FoundProducer { province, producer_type }` command | the nation's treasury |

- At most one founding per province and month, largest unemployment first, then lowest province and producer type, so the response is gradual and deterministic.
- The funding is a transfer to the new producer's cash: money moves, never appears (D5).
- The capitalist pool is also what founds Private merchants in the trade design (TRADE.md, "Entry and exit"), with the same rule.

## New state

| Table | Column | Type |
|---|---|---|
| `Producers` | `project_remaining` | `Fixed` per good (construction goods still to buy) |
| `Producers` | `project_budget` | `Fixed` |
| `Producers` | `idle_months` | `u32` |
| `Producers` | `state_owned` | `Option<nation>`: set for producers the state founded (dividends to its treasury) |

New `rules.toml` section `[investment]`: `profit_margin`, `idle_months_before_shrink`, `slack`, `investor_reserve`.

## Acceptance tests

1. **The economy grows:** in `two_states` over 20 years, total capacity and real GDP rise where today they are flat (about 4,200 a day in day-1 prices from year 4), while unemployment stays within its band.
2. **Investment creates demand:** tool and steel output and prices respond when projects run.
3. **No investment without profit, without spare workers, or without cash.** Each condition is tested separately.
4. **Founding:** a province with unemployed workers and a profitable producer type gets a new producer, funded by the capitalist pool or the state; the money moved is exactly the funding.
5. **Disinvestment:** a chronically idle producer loses capacity; capacity in use never does.
6. **Conservation:** money via the existing per-tick assert, and a case in `conservation.rs` for each new flow (project purchases, founding transfers); goods via a project ledger invariant (bought = consumed by completed or partial projects).
7. **Determinism:** projects are in the state hash, and golden hashes are re-recorded.

## Delivery plan (M5)

| Step | Content |
|---|---|
| 1 | Construction recipes in data; the project columns on `Producers`. No behaviour change. |
| 2 | Producer expansion: the monthly decision, daily construction orders, completion. Acceptance tests 2, 3. |
| 3 | Founding: the capitalist investment pool and the state's `FoundProducer` command. Acceptance test 4. |
| 4 | Disinvestment of idle capacity. Acceptance test 5. |
| 5 | `two_states` over 20 years: growth, bands re-measured, golden hashes re-recorded. Acceptance test 1. |
