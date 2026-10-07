# Investment and Capacity Growth (M2) — Design Proposal

**Status: proposal for review.** Nothing here is implemented yet. On approval it becomes a numbered decision and is delivered in small PRs. Please answer the [open questions](#open-questions-for-review) first.

## The problem it solves

Every scenario report shows the same trend: **unemployment climbs steadily** (`two_states`: 2% → 9.5% over 20 years) while prices and life needs stay healthy. The cause is structural:
- producer capacity is fixed at load, while population grows;
- mobility (D18) and migration (D20) only redistribute workers between existing jobs.

Without investment the economy can never grow its way out, and capital (owners' dividends, producers' retained cash) has no productive use beyond consumption.

## Proposed mechanism: producers expand from retained earnings

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
- **Profitable:** smoothed value added `V̄` exceeds the wage bill by a margin, so expanding would pay. Returns on new capacity are estimated with today's prices; no foresight (bounded rationality).
- **Workers available:** its labour pool has unemployment of at least `step` (no point building jobs nobody can fill). Migration (D20) widens this to the market.
- **Liquid:** cash above the D6 restart reserve and the dividend reserve covers the project at today's prices.
- **No project already running.**

### 3. Building (daily, while a project runs)

- **Buying:** the producer places `BuyOrder`s for the construction goods, through D1 like any buyer and pro-rata rationed. The budget is fixed when the project starts.
- **Completion:** when all construction goods are delivered, `capacity += step` and the project closes. Goods are consumed by the construction, and money went to their sellers (D5).
- **Partial projects** carry over until completed.

### 4. Who pays

The producer pays from **retained cash**. This competes with dividends: the D6 dividend rule pays out cash above the reserve, so investment must reserve its budget *before* dividends.
- **Effect:** owners receive less today and more later. It's the classical trade-off between saving and consumption.
- **M2 banking** (inside money, D5) later lets producers borrow for projects instead of waiting to accumulate cash.

### 5. Disinvestment

A producer whose workforce has been below `capacity × (1 − slack)` for a year loses `step` capacity. That is depreciation of unused plant, and it stops capacity piling up where demand vanished. No money moves.

## New state

| Table | Column | Type |
|---|---|---|
| `Producers` | `project_remaining` | `Fixed` per good (construction goods still to buy) |
| `Producers` | `project_budget` | `Fixed` |
| `Producers` | `idle_months` | `u32` |

New `rules.toml` section `[investment]`: `profit_margin`, `idle_months_before_shrink`, `slack`.

## Acceptance tests

1. **Unemployment no longer drifts:** in `two_states` over 20 years it stays bounded, e.g. below 5%, where today it reaches 9.5%.
2. **Investment creates demand:** tool and steel output and prices respond when projects run.
3. **No investment without profit, without spare workers, or without cash.** Each condition is tested separately.
4. **Conservation:** money via the existing per-tick assert; goods via a project ledger invariant (bought = consumed by completed or partial projects).
5. **Determinism:** projects are in the state hash, and golden hashes are re-recorded.

## Open questions for review

1. **Who invests:** the producer from retained earnings (proposed, simple), or the *owner POPs* deciding with their savings (closer to Victoria 2's capitalists, but needs a share registry, M2 item 5)?
2. **New producers:** only expanding existing producers (proposed for the first PRs), or also founding new ones in provinces with unemployment? Founding needs an ownership model, see question 1.
3. **Profit test:** a simple margin over the wage bill (proposed), or a payback period (`project cost / monthly profit < N months`)?
4. **Depreciation:** shrink unused capacity (proposed), and should *used* capacity also decay and need maintenance goods? That's realistic, but adds steady demand and tuning.
5. **Government investment:** should nations also fund capacity (state industry) from the treasury, alongside D16 consumption?
