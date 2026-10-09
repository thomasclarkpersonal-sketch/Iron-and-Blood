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

The code is cited at `aa2133a`. The measurements come from `pax_cli` built from it (the plan's own commits change no code).

**1. Every market is an island.**
- Each day's market phase collects buy orders from producers' inputs and from governments only (`crates/pax_engine/src/systems/market.rs:296-297`). A buyer is a `Producer` or a `Nation` (`market.rs:175-181`), and every sell offer belongs to a producer (`market.rs:202-208`; sellers are paid at `market.rs:825-828`). Nothing buys in one market to sell in another.
- A scenario can't describe a connection between markets: `ScenarioFile` has no such entries and refuses unknown ones (`crates/pax_data/src/schema.rs:156-179`). `scenarios/two_states/scenario.toml:3` says the two markets don't trade, "so each state runs complete local chains".
- [D14](../../DECISIONS.md#d14-market-hierarchy-and-inter-market-trade) sets the principles of trade between markets, and nothing implements them.

So price gaps never close. `pax_cli run scenarios/two_states --days 7200 --every 1800 --market <key>` prints these prices; the gaps at year 20 are those of year 5:

| Good | Lowland, day 1800 | Highland, day 1800 | Lowland, day 7200 | Highland, day 7200 |
|---|---|---|---|---|
| clothes | 11.39 | 42.00 | 11.38 | 41.86 |
| furniture | 7.41 | 19.26 | 7.41 | 19.18 |
| wine | 11.28 | 20.67 | 11.27 | 20.58 |
| tools | 25.52 | 17.83 | 25.74 | 17.78 |
| steel | 15.18 | 11.19 | 15.30 | 11.12 |
| cloth | 5.72 | 2.13 | 5.70 | 2.39 |

**2. Capacity is fixed at load, so output can't grow.**
- `Producers.capacity`, the "Maximum number of workers" (`crates/pax_engine/src/world.rs:106-107`), is written only when a producer row is pushed (`world.rs:287`). Only the scenario loader and the benchmark push rows (`crates/pax_data/src/lib.rs:576`, `crates/pax_data/src/bench.rs:96`): no system changes a capacity or adds a producer.
- A producer type can't say what building more capacity would cost (`crates/pax_engine/src/defs.rs:65-80`), and the loader refuses unknown fields (`schema.rs:85-98`).
- Retained cash has one use: dividends above the reserve (`crates/pax_engine/src/systems/firms.rs:106-119`).

So `two_states` keeps the 253,500 worker slots its 24 producers load with (`scenarios/two_states/scenario.toml`), and real GDP is flat. `pax_cli report scenarios/two_states --days 7200 --every 1800` gives mean real GDP of 4,189.82, 4,203.24, 4,202.73 and 4,202.52 a day (in day-1 prices) over the four five-year periods, with unemployment at 0.8% throughout. [MILESTONE_2.md](../../MILESTONE_2.md#measured-state-of-the-simulation) records the same.

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
- `pax_cli report`'s GDP is household plus government consumption, "Expenditure GDP of this closed economy without investment" (`crates/pax_cli/src/report.rs:7-8`). Investment and trade between markets would not show in the measure the milestone's growth goal is stated in.
- The test that compares 1, 2, 3 and 8 threads runs `mini_valley` only (`crates/pax_data/tests/determinism.rs:12-14` and `:31-36`), whose frozen definitions switch the newer labour mechanisms off (`scenarios/mini_valley/defs/rules.toml:26-29`); CI checks `two_states`' golden hashes at 1 and 4 threads (`.github/workflows/ci.yml:112-117`).
- The session replay test submits income-tax commands only (`crates/pax_server/tests/session_replay.rs:24-25`).

### What should happen

The milestone's goal (`MILESTONE_5.md:6`) and its [definition of done](../../MILESTONE_5.md#-definition-of-done), as outcomes. Their rules are [D17](../../DECISIONS.md#d17-inter-market-trade-routes-merchants-and-tariffs), [D27](../../DECISIONS.md#d27-capital-investment-and-capacity-expansion) and [D28](../../DECISIONS.md#d28-economic-unrest-strikes-and-riots), accepted on 2026-10-10 with the maintainer's answers ([run rules](README.md#run-rules)).
- Goods move between connected markets when a price gap pays for the move, so gaps shrink towards the cost of transport, while each market still clears by itself.
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
| SR-1 | Move goods between markets that a scenario connects, driven by price gaps, with a share lost in transit and a day's delay, so that in `two_states` the gap of a traded good narrows towards its route's friction and margin. Every market still clears by itself | DoD 1; scope 1; D14, D17 |
| SR-2 | Let a nation set an ad valorem tariff on imports. A higher tariff reduces the flow, and the importing treasury receives exactly the tariffs paid | DoD 1; D17 |
| SR-3 | Pay trade profits to the trade's owner by kind (the origin market's capitalists, merchant POPs, or the state); add trading capital to routes whose price gap persists for lack of it; wind up trade that keeps losing money; let a scenario seed starting traders. Money moves and is never created | DoD 1; scope 1; D17 |
| SR-4 | Let producers expand from retained earnings when they are profitable and liquid and have idle workers, buying real construction goods (tools, timber, steel) on the market; let the market's capitalists or the state found producers in provinces with unemployed workers; shrink capacity that stays idle | DoD 2; scope 2; D27 |
| SR-5 | In `two_states` over 20 years, raise total capacity and real GDP above the M5-10 baselines (real GDP flat at about 4,200 a day in day-1 prices today, measured above), with unemployment in its band | DoD 2; M5-10 |
| SR-6 | Cut labour supply, and so output, where militancy exceeds a strike threshold, without paying the strikers. Where a province's militancy exceeds a riot threshold, destroy part of its producers' output stock and pay an automatic security transfer from its treasury, which relieves militancy only through life needs | DoD 3; D28; run decisions 3 and 4 |
| SR-7 | Let a player view trade routes, set tariffs, follow construction and found producers from the Godot client, in single player and multiplayer. Each command is checked for permission and validity, stamped, logged, saved and replayed identically | DoD 4; scope 4; D21, D22, D24 |
| SR-8 | Conserve outside money on every tick through every new flow, and account for goods exactly: bought, in transit, delivered, lost and consumed | DoD 1 and 5; D5 |
| SR-9 | Give identical results at 1, 2, 3 and 8 threads, and on Linux, Windows and macOS, for worlds that trade, invest and riot; change golden hashes only on purpose, saying why | DoD 5; D3, D11 |
| SR-10 | Keep the tick within 100 ms/day at about 1M POP rows on 8 threads with the new mechanisms running, and make no change more than 20% slower than its base | DoD 5; D13 |
| SR-11 | Evolve the wire protocol and the save formats so that an older client, save or snapshot is either still read correctly or refused with a clear error, never misread | D10, D22, D23 |
| SR-12 | Report what the milestone adds, so that each claim of the definition of done can be checked: GDP that counts investment and trade, flows and tariffs, capacity and construction, strikes and riots | DoD 1-3 |
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
  - a temporary measure that changes results ([docs/README.md](../../README.md#milestone-lifecycle), "Temporary measures").

## Initial assessment

**Within the existing decisions.** With D17, D27 and D28 accepted for this run, the milestone can be planned and built on accepted decisions alone, and no new decision is needed to start. The later stages must settle these points as choices within D17, D27 and D28, each recorded for the maintainer to check:
1. **Where the milestone and its designs disagree:**
   - dense per-good merchant columns (M5-2, `MILESTONE_5.md:65`; `docs/TRADE.md:39-41`) against a sparse cargo table (`docs/DATA_MODEL_M5_M6.md:25`);
   - `[[route]]` entries (M5-1; `TRADE.md:97-108`) against `[[link]]` entries with routes derived at load (`DATA_MODEL_M5_M6.md:163`);
   - the tariff paid on landing (`TRADE.md:49`) against assessed at purchase and paid on landing (`DATA_MODEL_M5_M6.md:165`);
   - `project_budget` and `state_owned` (M5-6, `MILESTONE_5.md:69`; `docs/INVESTMENT.md:80-83`) against a derived reserve and `owner_nation` (`DATA_MODEL_M5_M6.md:167-169`);
   - `producer_types.toml`, named by M5-6, doesn't exist: producer types are in `data/production.toml` (`DATA_MODEL_M5_M6.md:170`);
   - Commercial merchants need a merchant profession (`TRADE.md:74`), which `data/professions.toml` lacks (lines 1-38).
2. **What the decisions leave to mechanism:**
   - the size of the riot security transfer: no rule names it (`docs/REBELLIONS.md`, "New state");
   - who counts as a pool's working members when it has both strikers and unemployed people (run decision 3; today unemployment is shared by the whole pool, `labor.rs:8-9`);
   - a data check that keeps `1 − strike_rate × (militancy − threshold)` within [0, 1] (D28's formula);
   - how `FoundProducer` stays valid against a scenario's initial world, where loaders check logged commands (`docs/DECISIONS.md:385`);
   - the goods of M5-5's test: measured above, cloth is cheaper in Highland (2.39 against 5.70), not Lowland, while clothes, furniture and wine are cheaper in Lowland and tools and steel in Highland.
3. **Open pull request #69** (the data-model review) corrects `DATA_MODEL_M5_M6.md`: merchants keyed by markets, a route-to-link table, and an owner profession for merchants. The run treats it as input, not as merged ([charter](02-project-charter.md#assumptions), A3).

**Feasibility, first look:** the tick is at about 91 of its 100 ms/day at M2 content, and price discovery is where the time goes ([PERFORMANCE.md](../../PERFORMANCE.md#tick-d13)). Merchants' and projects' orders join that phase, so the budget (D13) is the main technical risk. The baseline plan's feasibility study and risk register (planned) take it up.
