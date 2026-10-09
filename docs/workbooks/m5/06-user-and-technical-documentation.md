# User and Technical Documentation: Milestone 5

> [!NOTE]
> **Status: planning record** for the "m5" run, kept by the SDLC workflow (docs/SDLC_WORKFLOW.md). Not binding: the contract wins, and each fact moves to its home as the work lands.

The outline of the documentation Milestone 5 writes: who reads each part, what it covers, the one document it lives in, and the task that writes it. **Everything here is planned**: each task writes its part in the same pull request as its code, documents first ([SDLC_WORKFLOW.md](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/blob/f911fa707da7f981525683259f56b9846ef49024/docs/SDLC_WORKFLOW.md), "Documentation duty"), and the close-out makes this page final, linking every finished document. Homes follow [docs/README.md](../../README.md#where-each-kind-of-fact-lives): rules in DECISIONS.md; mechanism in the system documents; Rust structure in BACKEND_SCHEMA.md; file formats in DATA_FORMAT.md; the wire protocol in NETWORK_PROTOCOL.md; measurements in PERFORMANCE.md; users in ONBOARDING.md and HOSTING.md. A fact gets one home, and every other mention links to it.

The tasks are those of the [baseline plan](05-baseline-project-plan.md#the-task-queue); their contracts in [`tasks.json`](tasks.json) name the documents each one updates.

## User documentation

### Players, in the Godot client

| What they need | Home | Task |
|---|---|---|
| The Trade tab: the selected market's routes, their flows, merchants and tariffs, and the "and N more" lines of the capped lists | [client/README.md](../../../client/README.md) (its bullets and layout table) | M5-14 |
| The tariff sliders: per mille, the player's nation (any nation in sandbox), sent on release | client/README.md, beside the policy sliders' bullet | M5-14 |
| The Construction tab: projects and what they still need, founding options with cost and workers, pending requests, the founding results a month-end update brings (and that a skipped update loses them, S30), and when Found is disabled | client/README.md | M5-14 |
| The Trade flow map mode and its legend | client/README.md; ONBOARDING.md's "Play it" | M5-14 |
| `--tab=Trade`, `--tab=Construction`, `--map-mode=7`, and what `--smoke` now checks | client/README.md's options table | M5-14 |
| A save made before M5 doesn't load after it (format and content) | [HOSTING.md](../../HOSTING.md)'s saves bullet, and the release notes the close-out writes | M5-3 (save format 2), M5-8 (format 3), close-out |

### Developers and analysts, with `pax_cli`

| What they need | Home | Task |
|---|---|---|
| `bench`'s new lines: the slowest day, and the first month-end day with the days before it (always in ms, never ms/day) | [ONBOARDING.md](../../ONBOARDING.md), section 1, and `bench`'s usage text in `crates/pax_cli/src/main.rs` | M5-1 |
| `bench --warmup DAYS` | ONBOARDING.md; the usage text | M5-7 |
| `report --market KEY`: one market's own GDP, prices and real GDP, an informational measure | ONBOARDING.md, section 1; the usage text | M5-5 |
| `report`'s new columns, each after today's: `tariffs`, `iceberg%`; `trade`; `merch.div`, `wound_up`; `invest`, `started`, `capacity`; `founded`; `shrunk`; `strike%`; `riots`; `m.founded` | ONBOARDING.md's list of `report`'s columns | M5-2, M5-3, M5-4, M5-7, M5-8, M5-9, M5-11, M5-12, M5-16, each its own |
| The code map: `horizon.rs`, and `trade`, `investment` and `unrest` among the systems | ONBOARDING.md, section 2 | M5-1, M5-2, M5-7, M5-11 |
| How to pay a group of POPs now that wages go by working members (`credit_pops_by_working`) | ONBOARDING.md, section 4, rule 3 | M5-11 |
| The "Add a command" recipe: a command now comes with its wire table, the server's arms and `describe_command` (C27) | ONBOARDING.md, section 5 | M5-3 |

### Hosts of servers

| What they need | Home | Task |
|---|---|---|
| Compatibility: `protocol_major` is unchanged, so HOSTING.md's rule stands; a 1.6 client plays on without the new views | [NETWORK_PROTOCOL.md](../../NETWORK_PROTOCOL.md) section 8, which HOSTING.md links; HOSTING.md unchanged | M5-3, M5-8, M5-13, M5-14 (one minor version each) |
| Saves that survive restarts but not a save-format change | HOSTING.md's saves bullet | M5-3 |
| No new server setting | nothing to write: `docker/entrypoint.sh`'s list is unchanged | — |

### Scenario authors and modders

| What they need | Home | Task |
|---|---|---|
| `[[link]]` and `[[route]]`, the one-link rule, and the load errors | [DATA_FORMAT.md](../../DATA_FORMAT.md), scenario directory | M5-1 |
| `[[merchant]]` | DATA_FORMAT.md, scenario directory | M5-4 |
| `expansion = { inputs, step }` in production.toml | DATA_FORMAT.md, `production.toml` | M5-6 |
| The `merchant` profession | DATA_FORMAT.md, `professions.toml` | M5-4 |
| `[trade]`, `[investment]` and the new `[politics]` keys: each key, its range, and that it is required | DATA_FORMAT.md, `rules.toml` | M5-1, M5-4, M5-16 (trade); M5-7, M5-8, M5-9 (investment); M5-11, M5-12 (politics) |
| `set_tariff` and `found_producer` in command logs, with their fields | DATA_FORMAT.md, command log | M5-3, M5-8 |
| Save format 2, then 3, read first and refused by name | DATA_FORMAT.md, save files | M5-3, M5-8 |
| The snapshot's new blocks | `pax_data::snapshot`'s documentation (its format comment, `crates/pax_data/src/snapshot.rs:4-21`), to which DATA_FORMAT.md's save files section points | M5-2, M5-3, M5-4, M5-6, M5-8, M5-9, M5-16 |

## Technical documentation

### Rules: DECISIONS.md

Each text is [Design 2.3](04-system-specification.md#23-amended-decision-texts-proposed)'s, landing with its code under the accepted Amends line named there (S1).

| Entry | Change | Task |
|---|---|---|
| [D4](../../DECISIONS.md#d4-tick-schedule-and-goods-persistence) | The lead sentence, row 0 (arrival) and the goods-persist sentence | M5-2 |
| D4 | Row 1's strikes | M5-11 |
| D4 | Row 3's merchants | M5-3 |
| D4 | Row 3's construction projects, row 6c written with "complete projects, start expansions", the households sentence | M5-7 |
| D4 | Row 6b (riots) | M5-12 |
| D4 | Row 4's merchants' dividends; 6c's "then merchants: wind up loss-makers" | M5-4 |
| D4 | 6c's "shrink idle capacity" | M5-9 |
| D4 | 6c's "found the producers the state asked for" and "found producers for owners" | M5-8 |
| D4 | 6c's "found new merchants" | M5-16 |
| [D5](../../DECISIONS.md#d5-money-model-and-stock-flow-consistency) | The outside-money bullet names merchants | M5-2 |
| [D6](../../DECISIONS.md#d6-firms-production-wages-ownership) | The wage split among working members, "Striking workers forgo wages." (run decision 3) | M5-11 |
| D6 | Dividends above the project's reserve | M5-7 |
| D6 | Merchants' dividends; the ownership bullet's chartered merchants | M5-4 |
| D6 | The state's producers' dividends, the founding bullet, the ownership bullet's founded producers | M5-8 |
| [D14](../../DECISIONS.md#d14-market-hierarchy-and-inter-market-trade) | Rule 5's sparse horizon | M5-1 |
| [D19](../../DECISIONS.md#d19-militancy) | Its status sentence only: "Its effects are strikes and riots (D28)." The rule stays (run decision 4) | M5-11 |
| [D21](../../DECISIONS.md#d21-commands-and-command-logs) | `SetTariff` and the unknown good, the fifth Validation sub-bullet's first sentence, the command-log fields | M5-3 |
| D21 | `FoundProducer`, its four errors, the sub-bullet's founding parts | M5-8 |
| [D22](../../DECISIONS.md#d22-wire-protocol-and-client-sessions) | Its Views list gains `TradeRouteView` and `InvestmentLedgerView` | M5-13 |
| [D24](../../DECISIONS.md#d24-multiplayer-authority) | The Order bullet's sentence for `SetTariff` | M5-3 |
| D24 | The same sentence's `FoundProducer` part | M5-8 |

D17, D27 and D28 themselves were recorded in `bd08ee6` (run decision 5) and change no further. D28's Amends line stays as written, the maintainer's to edit ([SSR](01-system-service-request.md#initial-assessment), item 4).

### Mechanism: the system documents and the M5 designs

| Document | Sections | Task |
|---|---|---|
| [MAP_AND_LOGISTICS.md](../../MAP_AND_LOGISTICS.md) | The pathfinding tip, which still says to precompute a friction matrix between all market nodes, describes the sparse horizon and links D17 | M5-1 |
| [ECONOMY_SYSTEM.md](../../ECONOMY_SYSTEM.md) | "Market Hierarchy and Trade": merchants as D1 buyers and sellers, one day of transit, tariffs per good | M5-3, then M5-5 (`two_states` trades) |
| ECONOMY_SYSTEM.md | "Wages and dividends": merchants' dividends (M5-4), the project reserve (M5-7), state producers and founding (M5-8), the wage split among working members (M5-11) | M5-4, M5-7, M5-8, M5-11 |
| [POLITICS_SYSTEM.md](../../POLITICS_SYSTEM.md) | "Militancy vs. consciousness": militancy's effects, strikes then riots | M5-11, M5-12 |
| [POP_SYSTEM.md](../../POP_SYSTEM.md) | Strikers in the labour pool: neither employed nor unemployed; wages by working members | M5-11 |
| [MACROECONOMICS.md](../../MACROECONOMICS.md) | Checked by M5-7 and M5-10 for statements that investment makes stale; no change is expected | M5-7, M5-10 |
| [TRADE.md](../../TRADE.md) | Its status line and each part as it is built: links, routes and horizon (M5-1); arrival (M5-2); orders, offers, tariffs (M5-3); kinds, dividends, exit with `month_profit` (C28), the seed's `from`/`to` (S23), the income-tax line marked *planned: needs D15 amended, none drafted* (C7) (M5-4); test 9 on the measured goods (M5-5); entry (M5-16) | M5-1 to M5-5, M5-16 |
| [INVESTMENT.md](../../INVESTMENT.md) | Recipes and project state, with `project_budget` and `producer_types.toml` corrected (M5-6); expansion and construction (M5-7); founding, with `state_owned` corrected to `owner_nation` (M5-8); depreciation (M5-9); the acceptance tests' measured figures (M5-10) | M5-6 to M5-10 |
| [REBELLIONS.md](../../REBELLIONS.md) | Stage 1 built (M5-11); stage 2 built, its unnamed transfer rule named `riot_security_rate` (M5-12) | M5-11, M5-12 |
| [DATA_MODEL_M5_M6.md](../../DATA_MODEL_M5_M6.md) | Each M5 table noted as moved to BACKEND_SCHEMA.md as it is built; the document stays for M6's tables | M5-1, M5-2, M5-6 |

### Rust structure: BACKEND_SCHEMA.md and ARCHITECTURE.md

| Document | Sections | Task |
|---|---|---|
| [BACKEND_SCHEMA.md](../../BACKEND_SCHEMA.md) | The workspace tree: `horizon.rs` (M5-1); `trade`, `investment`, `unrest` on the `systems/` line, which `scripts/check_docs.py:116-130` requires (M5-2, M5-7, M5-11) | M5-1, M5-2, M5-7, M5-11 |
| BACKEND_SCHEMA.md | State schema: `TradeNetwork` (M5-1); `Merchants`, `Cargo` (M5-2); `Nations::tariff` (M5-3); the merchant columns (M5-4); `Projects`, `ProjectNeeds` (M5-6); `Producers::owner_nation`, `FoundingRequests` (M5-8); `Producers::idle_months` (M5-9); `RouteGaps` (M5-16), each replacing its "Planned tables" line | each table's task |
| BACKEND_SCHEMA.md | `DayReport`: `trade`, `merchant_dividends`, `merchants`, spending by market, `investment`, `riots`, `LabourReport::striking` | M5-2, M5-3, M5-4, M5-5, M5-7, M5-11, M5-12 |
| BACKEND_SCHEMA.md | The API boundary: the new commands and views | M5-3, M5-8, M5-13, M5-14 |
| [ARCHITECTURE.md](../../ARCHITECTURE.md) | "The Game Loop", which mirrors D4 with each step's module: steps 0, 1, 3, 4, 6b and 6c | M5-2, M5-3, M5-4, M5-7, M5-8, M5-9, M5-11, M5-12, M5-16 |
| `crates/pax_engine/src/tick.rs`'s module table (`tick.rs:7-16`), which restates the order | The same rows, in the same pull requests | as ARCHITECTURE.md |

### File formats and the wire protocol

- **[DATA_FORMAT.md](../../DATA_FORMAT.md):** as the [modders' table](#scenario-authors-and-modders) above.
- **[NETWORK_PROTOCOL.md](../../NETWORK_PROTOCOL.md):** section 4's commands and the `DayUpdate` table with sizes (M5-3, M5-8, M5-13), section 5 on the new commands' checks (M5-3, M5-8), `MapMode` (M5-14), and section 8's history: 1.7 `SetTariff` (M5-3), 1.8 `FoundProducer` (M5-8), 1.9 the views, `StaticData.routes` and `Subscribe`'s fields (M5-13), 1.10 `TradeFlow` (M5-14). The schemas in `schemas/` stay the protocol's home, with their own doc comments (Design 3.5).

### Measurements: PERFORMANCE.md

| Rows | Task |
|---|---|
| "Views and bandwidth": the update with both views at their caps, a remote client at speed 3, view building | M5-13 |
| "Tick (D13)": `two_states` with M5's content, the cold 29 days, the 30 days with the first month-end day's median and range, a warmed world with its warm-up, beside M5-1's baseline and the history; the horizon at scale (R-N13) | M5-15 |

### Milestone and workbook

- **[MILESTONE_5.md](../../MILESTONE_5.md):** each task ticks its own row with a sentence or two ([docs/README.md](../../README.md#milestone-lifecycle)); M5-6's names `production.toml`, M5-5's the measured goods, and M5-16's says entry never charters (R-F16). The close-out brings the status and the definition of done up to date.
- **This workbook:** each task adds a correspondence-log entry, any change request and its actual Gantt bar; the review log gains its rounds.

### Doc-comment obligations

Every new or changed public item gets rustdoc giving its why, its mathematics and its decision ([AGENTS.md §8](../../../AGENTS.md#8-documentation-maintenance)), with the rounding of each operation where it matters (D3). In particular:

| Item | What its documentation must say | Task |
|---|---|---|
| `horizon.rs`, `trade_horizon`, `resolve_route` | PL-1's search, the tie-break, why rounding down per link is exact, the cost; D17, D14 rule 5, AGENTS.md §5 | M5-1 |
| `bench`'s timing | Which line CI's gate reads, and why the new lines say ms (S9) | M5-1 |
| `systems/trade.rs` (module docs) | Arrival (PL-4), orders and offers (PL-2, PL-3), settlement's hand-offs by key (S5, S6), dividends (PL-5), exit (PL-6, C28), entry (PL-7); D17, D5, D6 | M5-2, M5-3, M5-4, M5-16 |
| `Merchants`, `Cargo`, `RouteGaps` and their columns | Units, invariants (R-D9, R-D10, R-D15), and for `owner_nation` that its pin to the origin's nation is M5's check while market ownership is topology (S26) | M5-2, M5-4, M5-16 |
| `Nations::tariff`, `Command::SetTariff` | Per importing nation and good, assessed at purchase (C2, C3); validity on definitions only (R-D13); D17, D21, D24 | M5-3 |
| `systems/investment.rs` (module docs) | PL-8 to PL-14, C19's order, the reserve derived and never stored (D7, C11, C12, C24); D27, D6 | M5-7, M5-8, M5-9 |
| `Projects`, `ProjectNeeds`, `FoundingRequests`, `Producers::owner_nation`, `Producers::idle_months` | Keys and invariants (R-D11, R-D12, R-D16, S4, S26) | M5-6, M5-8, M5-9 |
| `Command::FoundProducer` | That it only records a request, answered at the next month end (C15); D21, D24, D27 | M5-8 |
| `systems/unrest.rs`, `working_members`, `credit_pops_by_working`, `riot` | PL-15 and PL-16, why strikers are neither employed nor unemployed (C16), run decisions 3 and 4; D28, D19, D6 | M5-11, M5-12 |
| `Pops::militancy` | Its effects, strikes and riots, replacing "no effects yet" (S20) | M5-11 |
| `StateHasher::u8s`, `option_u32s`; `World::state_hash` | PL-18's rule: new state hashed only where it holds something, and why `mini_valley`'s file stands | M5-2, M5-4 |
| `snapshot.rs`'s format comment; `SNAPSHOT_FORMAT`; `SAVE_FORMAT`, `read_format` | Each new block; why the format is read first (S15) | M5-2, M5-3, M5-4, M5-6, M5-8, M5-9, M5-16 |
| `bench::replicate_with_nations`, `--warmup` | What is copied, the warm state included (R-N12, R-N25) | M5-1, M5-7, and each table's task |
| `views::trade_routes`, `tariffs`, `investment_ledger`, `trade_flow` | Their order (the most traded first), units, and that they are built from state and reports, never stored (D22, S17) | M5-13, M5-14 |
| `view.rs`'s `MAX_ROUTES`, `MAX_GOODS_PER_ROUTE`, `MAX_LEDGER_ROWS` | The budget arithmetic behind each cap (Design 3.5, S28) | M5-13 |
| `pax_godot`'s new encoders, decoders and `PaxClient` functions | What each sends or returns, as `submit_policy` does today; D12, D22 | M5-13 |
| `client/ui/trade_panel.gd`, `construction_panel.gd` | Their signals and functions (GDScript doc comments), and S29's Found rule | M5-14 |

## What the close-out finalises

When the queue is done, the close-out pull request (the [Gantt chart](05-baseline-project-plan.md#gantt-chart)'s last bar):
- marks this page final, with a link to every document as it stands on `main`;
- writes the release notes for players and hosts: the new controls, protocol 1.10's compatibility, and the save break;
- checks that every row above was written by its task, and lists any that wasn't as a follow-up.
