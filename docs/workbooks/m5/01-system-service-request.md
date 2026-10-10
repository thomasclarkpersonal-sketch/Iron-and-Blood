# System Service Request: Milestone 5

> [!NOTE]
> **Status: planning record** for the "m5" run, kept by the SDLC workflow (docs/SDLC_WORKFLOW.md). Not binding: the contract wins, and each fact moves to its home as the work lands.

The formal proposal of the m5 run's work. The run's record is the [Project Workbook](README.md).

| | |
|---|---|
| **Requested by** | The maintainer, through the milestone document |
| **Date** | 2026-10-10 |
| **Request** | [MILESTONE_5.md](../../MILESTONE_5.md) on `main` at `aa2133a`: its goal (line 6), scope (lines 14-44), tasks M5-1 to M5-16 (lines 62-79) and definition of done (lines 85-101) |
| **Run** | `m5`, planned on branch `m5/plan` from `origin/main` at `aa2133a` |

## Problem statement

### What happens today

The code is cited at `aa2133a`. The measurements come from `pax_cli` built from it (the plan's own commits change no code), with the commands under [How it was measured](#how-it-was-measured).

**1. Every market is an island.**
- Each day's market phase collects buy orders from producers' inputs and from governments only (`crates/pax_engine/src/systems/market.rs:296-297`). A buyer is a `Producer` or a `Nation` (`market.rs:175-181`), and every sell offer belongs to a producer (`market.rs:202-208`; sellers are paid at `market.rs:825-828`). Nothing buys in one market to sell in another.
- A scenario can't describe a connection between markets: `ScenarioFile` has no such entries and refuses unknown ones (`crates/pax_data/src/schema.rs:156-179`). `scenarios/two_states/scenario.toml:3` says the two markets don't trade, "so each state runs complete local chains".
- [D14](../../DECISIONS.md#d14-market-hierarchy-and-inter-market-trade) sets the principles of trade between markets, and nothing implements them.

So price gaps never close. For each of `two_states`' 12 goods, the table gives its mean price in each market over the 20th year and the gap `G` between them ([defined below](#the-gap-statistic)). Nine goods are cheaper in Highland: six of the seven raw goods (grain, fish, timber, cotton, coal, iron ore), and cloth, steel and tools. Three are cheaper in Lowland: clothes, furniture and wine, the seventh raw good (its vineyards need no inputs, `data/production.toml:19-25`).

| Good | Lowland, mean | Highland, mean | Cheaper in | Gap `G` | Lowland ÷ Highland, daily lowest to highest |
|---|---|---|---|---|---|
| grain | 1.262 | 0.970 | Highland | 1.302 | 1.301 to 1.302 |
| fish | 2.103 | 1.931 | Highland | 1.089 | 1.084 to 1.092 |
| timber | 1.454 | 0.881 | Highland | 1.650 | 1.646 to 1.654 |
| cotton | 0.804 | 0.409 | Highland | 1.963 | 1.290 to 3.347 |
| coal | 1.519 | 1.050 | Highland | 1.447 | 1.441 to 1.455 |
| iron_ore | 2.034 | 1.639 | Highland | 1.241 | 1.235 to 1.247 |
| steel | 15.331 | 11.136 | Highland | 1.377 | 1.363 to 1.388 |
| cloth | 5.702 | 3.298 | Highland | 1.729 | 0.606 to 2.723 |
| clothes | 11.375 | 46.567 | Lowland | 4.094 | 0.150 to 0.272 |
| furniture | 7.408 | 19.227 | Lowland | 2.595 | 0.382 to 0.386 |
| wine | 11.264 | 20.634 | Lowland | 1.832 | 0.542 to 0.548 |
| tools | 25.801 | 17.805 | Highland | 1.449 | 1.437 to 1.459 |

A single day's price says little for some goods. Highland's cotton, cloth and clothes cycle within the year: its cloth ranges from 2.10 to 9.41, above Lowland's 5.70 on 50 of the 360 days, its clothes from 41.8 to 75.9, and its cotton from 0.24 to 0.62. That is why `G` is a statistic over a year. It is stable: measured over year 19, or over years 16-20, every `G` is within 0.001 of the table's; over year 5 or year 10 it is within 0.04, and the same market is cheaper for every good.

#### The gap statistic

The charter's objective O3 uses the same statistic for the autarky run and for the run with trade ([charter](02-project-charter.md#o3-trade-works-and-arbitrages-prices)):
- **Window `W`:** days 6841 to 7200 of a 7200-day run, the 20th year. It is the final year that `crates/pax_data/tests/economic_bands.rs` measures (lines 24 and 31).
- **Mean price `P̄(m, g)`:** the mean of market `m`'s daily price of good `g` over the days of `W`, the same days in both markets.
- **Gap `G(g)`** `= max(P̄(Lowland, g), P̄(Highland, g)) ÷ min(P̄(Lowland, g), P̄(Highland, g))`, at least 1. The cheaper market is the one a route would export the good from.

**2. Capacity is fixed at load, so output can't grow.**
- `Producers.capacity`, the "Maximum number of workers" (`crates/pax_engine/src/world.rs:106-107`), is written only when a producer row is pushed (`world.rs:287`). Only the scenario loader and the benchmark push rows (`crates/pax_data/src/lib.rs:576`, `crates/pax_data/src/bench.rs:96`): no system changes a capacity or adds a producer.
- A producer type can't say what building more capacity would cost (`crates/pax_engine/src/defs.rs:65-80`), and the loader refuses unknown fields (`schema.rs:85-98`).
- Retained cash has one use: dividends above the reserve (`crates/pax_engine/src/systems/firms.rs:106-119`).

So `two_states` keeps the 253,500 worker slots its 24 producers load with (`scenarios/two_states/scenario.toml`), and real GDP is flat:
- **The measure:** real GDP is `pax_cli report`'s. It is the mean daily household plus government consumption (C+G) over a period, deflated by a Laspeyres index of the period's last prices with day 1's traded quantities as the basket, so it is in day-1 prices (`crates/pax_cli/src/report.rs:7-11`).
- **Five-year periods:** with `--every 1800` it is 4,189.82, 4,203.24, 4,202.73 and 4,202.52 a day, with unemployment at 0.8% throughout.
- **Yearly:** with `--every 360`, the values for years 16-20 lie between 4,202.51 and 4,202.92.
- **Nominal:** C+G over the final year is 5,023.82 a day, inside the [4,500, 5,500] band that `economic_bands.rs:45` asserts.

[MILESTONE_2.md](../../MILESTONE_2.md#measured-state-of-the-simulation) records the same flat economy.

**3. Militancy has no consequences.**
- Militancy is updated every month (`crates/pax_engine/src/systems/politics.rs:17-30`), and the code says it "has no effects yet" (`world.rs:66-67`, `politics.rs:9-10`).
- A labour pool supplies its whole size, whatever its militancy (`crates/pax_engine/src/systems/labor.rs:57`). Outside the politics system, militancy is read only to report it (`market.rs:762`).

So in `two_states` the mean militancy settles at 0.124 (the same report) and costs neither the economy nor the state anything. Income tax, which drives militancy ([D19](../../DECISIONS.md#d19-militancy)), carries no economic cost through unrest.

**4. Players can't see or steer trade or investment.**
- The only commands set one of a nation's three rates, each `{ nation, rate }` (`crates/pax_engine/src/command.rs:17-25`), and `World::validate` assumes that shape (`command.rs:52-56`).
- The wire `Command` union mirrors them (`schemas/common.fbs:65-69`), the bridge encodes only those (`crates/pax_godot/src/encode.rs:63`), and the client's nation panel has exactly three sliders (`client/ui/nation_panel.gd:20-25`).
- Scenario command logs and saves record a command as `type`, `nation` and `rate` only (`schema.rs:245-253`; `crates/pax_data/src/save.rs:128-133` and `save.rs:199-200`).
- `DayUpdate` carries no view of trade or construction (`schemas/server.fbs:150-164`).

**5. The measures and the gates can't see what the milestone adds.**
- `pax_cli report` measures C+G for all markets together, "Expenditure GDP of this closed economy without investment" (`crates/pax_cli/src/report.rs:7-8`), deflated by one index over every market's prices (`report.rs:38-41`). The engine reports household and government spending only as world totals (`crates/pax_engine/src/tick.rs:39-41` and `:52-53`). So investment, trade between markets, and each market's own real GDP (which M5-5's "mutual real GDP growth" compares) can't be measured today.
- The test that compares 1, 2, 3 and 8 threads runs `mini_valley` only (`crates/pax_data/tests/determinism.rs:12-14` and `:31-36`), whose frozen definitions switch the newer labour mechanisms off (`scenarios/mini_valley/defs/rules.toml:26-29`); CI checks `two_states`' golden hashes at 1 and 4 threads (`.github/workflows/ci.yml:112-117`).
- The session replay test submits income-tax commands only (`crates/pax_server/tests/session_replay.rs:24-25`).

#### How it was measured

On a release build of `aa2133a` (`cargo build --release -p pax_cli`):
- prices: `pax_cli run scenarios/two_states --days 7200 --every 1 --market lowland`, and the same with `--market highland`. The means, `G` and the daily ratios are computed from the printed daily prices (four decimals) over the days of `W`;
- real GDP, unemployment, militancy and the rest: `pax_cli report scenarios/two_states --days 7200 --every 1800`, and the same with `--every 360`.

### What should happen

The milestone's goal (`MILESTONE_5.md:6`) and its [definition of done](../../MILESTONE_5.md#-definition-of-done), as outcomes. Their rules are [D17](../../DECISIONS.md#d17-inter-market-trade-routes-merchants-and-tariffs), [D27](../../DECISIONS.md#d27-capital-investment-and-capacity-expansion) and [D28](../../DECISIONS.md#d28-economic-unrest-strikes-and-riots), accepted on 2026-10-10 with the maintainer's answers ([run rules](README.md#run-rules)).
- Goods move between connected markets when a price gap pays for the move, so gaps shrink towards the cost of transport, while each market still clears by itself and both sides gain.
- A nation can tax imports. The tariff cuts the flow, and all of it reaches the importing treasury.
- Productive capacity follows profit and idle labour: it grows through real construction goods bought on the market, new producers appear where workers wait, and idle capacity shrinks. Over 20 years `two_states` grows, with unemployment in its band.
- Discontent costs output (strikes) and stock (riots), and riots trigger the state's automatic security transfer.
- Players see and steer trade and construction from the client, in single player and multiplayer, through commands that are checked, logged and replayed like today's.
- All of it conserves money every day, gives the same results at any thread count and on every platform, and keeps the tick within its budget.

### Who is affected

| Who | How |
|---|---|
| Players (single player and multiplayer, through the Godot client) | An economy that trades, grows and reacts to unrest; new views and controls |
| Hosts of dedicated servers ([HOSTING.md](../../HOSTING.md)) | A new protocol minor version (1.6 today, `crates/pax_protocol/src/lib.rs:74-76`): the new controls need a client that knows them ([D22](../../DECISIONS.md#d22-wire-protocol-and-client-sessions)) |
| Scenario authors and modders ([DATA_FORMAT.md](../../DATA_FORMAT.md)) | New entries in scenarios and data files, and new rules |
| Players with saves | New state in the snapshot (`SNAPSHOT_FORMAT = 1`, `crates/pax_data/src/snapshot.rs:44`) and new command fields in saves (`SAVE_FORMAT = 1`, `save.rs:63`); a save whose scenario files change is refused ([D23](../../DECISIONS.md#d23-server-loop-pacing-flow-control-and-saves)) |
| Developers and agents ([ONBOARDING.md](../../ONBOARDING.md)) | New tables, systems and tick steps; the decisions D17, D27 and D28 amend; `two_states`' golden hashes re-recorded |
| The maintainer | Checks the choices the run makes within the contract; keeps waivers, contract changes and the playtest ([charter](02-project-charter.md#authorisation)) |

## Service requirements

Outcomes, not mechanisms. The Analysis stage (`03-requirements-specification.md`, planned) turns each into testable R-F, R-N and R-D requirements. "DoD n" is item n of the milestone's definition of done.

| Id | The system must | Source |
|---|---|---|
| SR-1 | Move goods between markets that a scenario connects, driven by price gaps, with a share lost in transit and a day's delay. In `two_states`, the gap `G` of every good whose gap exceeds its route's friction narrows, and each market's real GDP rises above what it is without the route. Every market still clears by itself | DoD 1; scope 1; M5-5; D14, D17 |
| SR-2 | Let a nation set an ad valorem tariff on imports. A higher tariff reduces the flow, and the importing treasury receives exactly the tariffs paid | DoD 1; D17 |
| SR-3 | Pay trade profits to the trade's owner by kind (the origin market's capitalists, merchant POPs, or the state); add trading capital to routes whose price gap persists for lack of it; wind up trade that keeps losing money; let a scenario seed starting traders of any kind, the only source of state-chartered traders in this run ([Initial assessment](#initial-assessment), item 3). Money moves and is never created | DoD 1; scope 1; D17 |
| SR-4 | Let producers expand from retained earnings when they are profitable and liquid and have idle workers, buying real construction goods (tools, timber, steel) on the market; let the market's capitalists or the state found producers in provinces with unemployed workers; shrink capacity that stays idle | DoD 2; scope 2; D27 |
| SR-5 | In `two_states` over 20 years, raise total capacity above the 253,500 worker slots it loads with, and raise real GDP (C+G in day-1 prices, the measure above) above what the same build gives without investment, with unemployment in its band | DoD 2; M5-10 |
| SR-6 | Cut labour supply, and so output, where militancy exceeds a strike threshold, without paying the strikers. Where a province's militancy exceeds a riot threshold, destroy part of its producers' output stock and pay an automatic security transfer from its treasury, which relieves militancy only through life needs | DoD 3; D28; run decisions 3 and 4 |
| SR-7 | Let a player view trade routes, set tariffs, follow construction and found producers from the Godot client, in single player and multiplayer. Each command is checked for permission and validity, stamped, logged, saved and replayed identically | DoD 4; scope 4; D21, D22, D24 |
| SR-8 | Conserve outside money on every tick through every new flow, and account for goods exactly: bought, in transit, delivered, lost and consumed | DoD 1 and 5; D5 |
| SR-9 | Give identical results at 1, 2, 3 and 8 threads, and on Linux, Windows and macOS, for worlds that trade, invest and riot; change golden hashes only on purpose, saying why | DoD 5; D3, D11 |
| SR-10 | Keep the tick within 100 ms/day at about 1M POP rows on 8 threads with the new mechanisms running, and make no change more than 20% slower than its base | DoD 5; D13 |
| SR-11 | Evolve the wire protocol and the save formats so that an older client, save or snapshot is either still read correctly or refused with a clear error, never misread | D10, D22, D23 |
| SR-12 | Report what the milestone adds beside today's measures, without redefining them, so that each claim of the definition of done can be checked against today's baselines: investment spending, trade flows and tariffs between markets, each market's own consumption and real GDP, capacity and construction, strikes and riots | DoD 1-3; M5-5, M5-10 |
| SR-13 | Keep the documents true at every merge: decisions, system documents, data formats, the protocol, and the milestone's task rows | [AGENTS.md §8](../../../AGENTS.md#8-documentation-maintenance) |

## Urgency

**Medium.**
- Nothing is broken. Money is conserved and asserted every day (`crates/pax_engine/src/tick.rs:111-112`), the economy is stable (0.8% unemployment above), and the tick is within its budget (about 91 ms/day at M2 content, [PERFORMANCE.md](../../PERFORMANCE.md#tick-d13)).
- The need is the roadmap's. M5 is the next milestone, and Milestone 6 "builds directly on" it (`docs/MILESTONE_6.md:8`). Until it lands the economy can't grow and unrest has no effect.
- No outside deadline applies. The stop time bounds this run, not the need: work left at 07:00 stays in the approved plan for a later run.

## Sponsorship

- **Sponsor:** the maintainer. They set the milestone (#70), accepted its decisions for this run on 2026-10-10 at 00:55 (+08:00), and authorised merging ([run rules](README.md#run-rules)).
- **Milestone served:** [Milestone 5](../../MILESTONE_5.md), the prerequisite of [Milestone 6](../../MILESTONE_6.md).
- **Decisions it relies on:**

| Kind | Decisions |
|---|---|
| Accepted by the maintainer for this run, recorded on `m5/plan` in `bd08ee6` | [D17](../../DECISIONS.md#d17-inter-market-trade-routes-merchants-and-tariffs) with run decision 2; [D27](../../DECISIONS.md#d27-capital-investment-and-capacity-expansion); [D28](../../DECISIONS.md#d28-economic-unrest-strikes-and-riots) with run decisions 3 and 4 |
| Amended by those three, each when the code that needs it lands (their "Amends" lines) | [D4](../../DECISIONS.md#d4-tick-schedule-and-goods-persistence), [D5](../../DECISIONS.md#d5-money-model-and-stock-flow-consistency), [D6](../../DECISIONS.md#d6-firms-production-wages-ownership), [D14](../../DECISIONS.md#d14-market-hierarchy-and-inter-market-trade) rule 5, [D21](../../DECISIONS.md#d21-commands-and-command-logs), [D24](../../DECISIONS.md#d24-multiplayer-authority) |
| Binding as they stand | [D1](../../DECISIONS.md#d1-market-clearing-bounded-tâtonnement-with-pro-rata-rationing), [D2](../../DECISIONS.md#d2-consumer-demand-linear-expenditure-system-stone-geary), [D3](../../DECISIONS.md#d3-determinism-fixed-point-everything), [D7](../../DECISIONS.md#d7-pop-accounting), [D8](../../DECISIONS.md#d8-ecs-hand-rolled-struct-of-arrays), [D9](../../DECISIONS.md#d9-data-format-toml), [D10](../../DECISIONS.md#d10-network-model-server-authoritative-deterministic-core), [D11](../../DECISIONS.md#d11-determinism-harness-and-golden-files), [D12](../../DECISIONS.md#d12-frontend-godot-with-a-rust-gdextension-bridge), [D13](../../DECISIONS.md#d13-performance-budget), [D15](../../DECISIONS.md#d15-nations-treasuries-income-tax-and-transfers), [D16](../../DECISIONS.md#d16-government-consumption), [D18](../../DECISIONS.md#d18-labour-mobility), [D19](../../DECISIONS.md#d19-militancy), [D20](../../DECISIONS.md#d20-migration-within-a-market), [D22](../../DECISIONS.md#d22-wire-protocol-and-client-sessions), [D23](../../DECISIONS.md#d23-server-loop-pacing-flow-control-and-saves), [D25](../../DECISIONS.md#d25-occupational-migration-within-a-market), [D26](../../DECISIONS.md#d26-births-follow-employment-band-aid) |

- **Decisions it may need.** None is drafted, and a task that needs one is blocked on it, never guessed:
  - making D1's adaptive step the default, if the new markets' orders push the tick over D13 (D1's own revisit criterion);
  - a share registry ([D6](../../DECISIONS.md#d6-firms-production-wages-ownership), ownership), only if a design can't keep owners at the profession and the state;
  - firm failure ([D5](../../DECISIONS.md#d5-money-model-and-stock-flow-consistency)), only if a design lets a merchant or producer end up owing money;
  - a temporary measure that changes results ([docs/README.md](../../README.md#milestone-lifecycle), "Temporary measures");
  - not in this run: extending D17's Amends line to a command that charters merchants, if the maintainer wants one ([Initial assessment](#initial-assessment), item 3).

## Initial assessment

**Within the existing decisions.** With D17, D27 and D28 accepted for this run, the milestone can be planned and built on accepted decisions alone, and no new decision is needed to start. The later stages must settle the points below as choices within D17, D27 and D28, each recorded for the maintainer to check. The Analysis stage settled items 1 and 2 as the [requirements specification](03-requirements-specification.md#choices-this-page-makes-within-the-contract)'s choices C1 to C27, and added C28 and C29 for two points it found: how a merchant's loss is measured, and which task first prints the month-end day's time. The Design stage adds the [system specification](04-system-specification.md#24-choices-this-design-makes-within-the-contract)'s choices S1 to S30, about how it is built, and finds no new decision needed. The Baseline plan stage adds the [baseline plan](05-baseline-project-plan.md#plan-choices)'s choices B1 to B18, about how the run builds, checks and schedules it, none needing a new decision either.

**1. Where the milestone and its designs disagree:**
- **Merchant goods:** dense per-good merchant columns (M5-2, `MILESTONE_5.md:65`; `docs/TRADE.md:39-41`) against a sparse cargo table (`docs/DATA_MODEL_M5_M6.md:25`).
- **Routes and links.** Run decision 2 presupposes links distinct from routes. A link carries `τ` and a capacity, and a route follows a path of links. Its capacity is the minimum over those links, taken whole by every route that uses them. So scenarios declare links, as `DATA_MODEL_M5_M6.md:163` has it, and the open choice is only how routes, with D17's per-route `margin` and `k` (M5-1), are declared alongside them. For example, `[[route]]` entries as in `TRADE.md:97-108` could name two markets with their `margin` and `k`, and take the path, `τ` and capacity from the links. The choice also says whether M5's direct routes (`TRADE.md:111`) are exactly one link each.
- **Tariff timing:** paid on landing (`TRADE.md:49`) against assessed at purchase and paid on landing (`DATA_MODEL_M5_M6.md:165`).
- **Tariff granularity:** one rate per importing nation, per good, or per partner (below).
- **Project columns:** `project_budget` and `state_owned` (M5-6, `MILESTONE_5.md:69`; `docs/INVESTMENT.md:80-83`) against a derived reserve and `owner_nation` (`DATA_MODEL_M5_M6.md:167-169`).
- **Recipe file:** `producer_types.toml`, named by M5-6, doesn't exist. Producer types are in `data/production.toml` (`DATA_MODEL_M5_M6.md:170`).
- **Merchant profession:** Commercial merchants need a merchant profession (`TRADE.md:74`), which `data/professions.toml` lacks (lines 1-38).

**Tariff granularity.** D17 says a tariff is ad valorem on the origin price, paid to the importing nation's treasury (`docs/DECISIONS.md:506`), but not what a rate is set per. The designs differ, and the choice decides the shape of `SetTariff`, its log and save format, and its view:

| Rate set per | Source | `SetTariff`, and its log and save format | D24 | Protocol |
|---|---|---|---|---|
| Importing nation: one rate on every import from another nation's (or a stateless) market | `TRADE.md:113` | `{ nation, rate }`, the shape of today's commands (`command.rs:17-25`), so logs and saves keep their fields (`schema.rs:245-253`, `save.rs:194-200`) | No conflict rule needed, as for today's rate commands: a nation sets only its own imports' tariff, and a session commands only its own nation (D24, "Permissions") | A `SetTariff` table appended to `union Command` (`common.fbs:65-69`); one `[Fixed]` column appended to `NationTable` (`server.fbs:87-97`) |
| Importing nation and good | `DATA_MODEL_M5_M6.md:166` | `{ nation, good, rate }`: logs and saves gain a `good` field, a save-format change (`SAVE_FORMAT`, `save.rs:63`) | As above (`DATA_MODEL_M5_M6.md:166`) | As above, with a `good` field; the view carries nations × goods rates |
| Importing nation and partner | `TRADE.md:49`, `tariff_rate(B, A)` | `{ nation, partner, rate }`: logs and saves gain a partner, which needs a value for "stateless" | As above | Nations × (nations + 1) rates. Per-partner rates are what customs unions need, which `TRADE.md:114` puts later |
| Importing nation, partner and good | — | `{ nation, partner, good, rate }` | As above | Nations² × goods rates |

Whichever the Design stage picks, D17's Amends line already covers `SetTariff` under D21 and D24 (`DECISIONS.md:512`). D24 asks for a conflict rule only for a command that can conflict with another player's (its "Order" bullet, `DECISIONS.md:465`), and a nation's tariffs touch only its own imports (`DATA_MODEL_M5_M6.md:166`), so `SetTariff` needs none, for the reason in the table. The planner leans to the per-good rate, for the Design stage to confirm: it is the data model's choice, and per-partner rates belong with the customs unions that come later.

**2. What the decisions leave to mechanism:**
- **Riot transfer size:** no rule names the size of the riot security transfer (`docs/REBELLIONS.md`, "New state").
- **Working members:** run decision 3 doesn't say who counts as a pool's working members when it has both strikers and unemployed people. Today unemployment is shared by the whole pool (`labor.rs:8-9`).
- **Strike formula range:** a data check must keep `1 − strike_rate × (militancy − threshold)` within [0, 1] (D28's formula).
- **`FoundProducer` and the initial world:** loaders check logged commands against a scenario's initial world (`docs/DECISIONS.md:385`). That is exact today only because each command sets one rate and commands don't interact (`crates/pax_data/src/lib.rs:185-186`), so the design must say how `FoundProducer` stays valid that way.
- **The flow rule's "gap":** `q* = capacity_share × min(1, gap / (k · p_A))` (`TRADE.md:54`) doesn't say what `gap` is. If it is the netback's excess over `p_A(1 + margin)`, the flow starts from zero at the margin. If it is the excess over `p_A` alone, the flow jumps to `margin / k` of capacity there. The choice also sets the price ratio at which the flow reaches capacity, which the charter's convergence measure depends on ([O3b](02-project-charter.md#o3-trade-works-and-arbitrages-prices)).
- **M5-5's goods:** M5-5 and `TRADE.md:142` assume "Lowland (cheap cloth, dear tools)". The [measured gaps](#what-happens-today) say otherwise: cloth and tools are both cheaper in Highland, with steel and every raw good but wine, while clothes, furniture and wine are cheaper in Lowland. Each market still makes some goods cheaper, so comparative advantage holds. The Analysis stage restates M5-5's test on the measured pattern.

**3. In or out of scope: a command that charters merchants is out.** D17 lets the treasury found chartered merchants "by command only" (its Entry and exit rule, `DECISIONS.md:508`; `TRADE.md:75` and `:82`), and M5-16 says "(Chartered by command)". But nothing else in the request has such a command:
- the milestone's wire additions are `SetTariffCommand` and `FoundProducerCommand` only (`MILESTONE_5.md:34`);
- DoD 4 asks for tariffs and for founding factories only;
- D17's Amends line names D21 and D24 for `SetTariff` alone (`DECISIONS.md:512`), and run decision 5 keeps that line as written.

A charter command would amend D21 and D24 beyond that line, which is the maintainer's to decide. So in this run chartered merchants come only from scenario seeding (`[[merchant]]`, `TRADE.md:84-91`). Dynamic entry (M5-16) founds Private and Commercial merchants only, never a chartered one, which is what D17's "by command only" requires.

**4. A known inconsistency, left for the maintainer.** D28's Amends line (`DECISIONS.md:537`) still reads "D6 (striking workers' pay, once settled), D19 (the riot transfer's militancy relief, once settled)". Both points are now settled, and run decision 4 leaves D19 unchanged, so the line names an amendment that won't happen. Run decision 5 says to change nothing else in the entry, so the line stays as written. The plan PR's description says so ([charter](02-project-charter.md#assumptions), A11), so that the critic doesn't take it for an agent's edit, and no task amends D19.

**5. Open pull request #69** (the data-model review) corrects `DATA_MODEL_M5_M6.md`: merchants keyed by markets, a route-to-link table, and an owner profession for merchants. The run treats it as input, not as merged ([charter](02-project-charter.md#assumptions), A3).

**Feasibility, first look:** the tick is at about 91 of its 100 ms/day at M2 content, and price discovery is where the time goes ([PERFORMANCE.md](../../PERFORMANCE.md#tick-d13)). Merchants' and projects' orders join that phase, so the budget (D13) is the main technical risk. The baseline plan's feasibility study and risk register (planned) take it up.
