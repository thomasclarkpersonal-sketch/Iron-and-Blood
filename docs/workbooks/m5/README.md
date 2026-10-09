# Project Workbook: the m5 run

> [!NOTE]
> **Status: planning record** for the "m5" run, kept by the SDLC workflow (docs/SDLC_WORKFLOW.md). Not binding: the contract wins, and each fact moves to its home as the work lands.

The ongoing record of the run that plans and builds [Milestone 5](../../MILESTONE_5.md): what it delivers, what was said and decided, and how each review went. The run's log and its morning report are in `out/m5-run/` in the maintainer's checkout, which git ignores.

## Deliverable register

| Deliverable | File | SDLC phase | Status | Last reviewed at |
|---|---|---|---|---|
| Project Workbook | `README.md` (this page) | 1. Initiation; kept up to date all run | approved for Initiation; kept up to date | `041d9e5` (Initiation round 2: approve) |
| System Service Request | [01-system-service-request.md](01-system-service-request.md) | 1. Initiation | approved; corrected in Analysis round 1 (minor finding 6, [below](#initiation-round-2-minor-findings)) | `041d9e5` (Initiation round 2: approve) |
| Project Charter | [02-project-charter.md](02-project-charter.md) | 1. Initiation | approved | `041d9e5` (Initiation round 2: approve) |
| Requirements specification | [03-requirements-specification.md](03-requirements-specification.md) | 2. Analysis | revised in Analysis round 2; under review | `96e8084` (Analysis round 1: revise) |
| System specification | `04-system-specification.md` | 3. Design | planned | — |
| Baseline project plan, with the project scope statement | `05-baseline-project-plan.md` | 4. Baseline plan | planned | — |
| Task DAG: the task queue, one pull request per task | `tasks.json` | 4. Baseline plan | planned | — |
| User and technical documentation | `06-user-and-technical-documentation.md` | 4. Baseline plan (outline); finished at close-out | planned | — |

## Correspondence log

Append-only, oldest first. Times are +08:00.

| When | Entry |
|---|---|
| 2026-10-10 00:55 | **Request.** The maintainer started the m5 run on [MILESTONE_5.md](../../MILESTONE_5.md), on `main` at `aa2133a`, with the merge authorisation, the stop time of 07:00 and run decisions 1-6 (quoted under [Run rules](#run-rules)) |
| 2026-10-10 00:55 | **Maintainer decisions.** D17, D27 and D28 accepted as written (run decision 1); D17's route capacity and D28's strike pay and riot relief settled (2-4); how to record them (5); the run rules (6) |
| 2026-10-10 01:01 | **Planning started.** Initiation round 1 on `m5/plan`, from `origin/main` at `aa2133a`, with the scouts' briefs on the contract, the code and the risks |
| 2026-10-10 01:22 | **Decisions recorded**, as run decision 5 asks, in `bd08ee6`: D17, D27 and D28 marked Accepted (2026-10-10, by the maintainer for M5), their Open lines replaced by run decisions 2-4, nothing else in them changed; the documents that called them Proposed or open brought up to date |
| 2026-10-10 01:35 | **Review round.** Initiation round 1 (this page, the [System Service Request](01-system-service-request.md) and the [Project Charter](02-project-charter.md)) submitted to the reviewer |
| 2026-10-10 01:35 | **Noted.** Open PR #69 (the data-model review) changes `DATA_MODEL_M5_M6.md`, which M5 builds on. It is the maintainer's and stays outside the run ([charter](02-project-charter.md#assumptions), A3) |
| 2026-10-10 01:35 | **Noted.** `docs/SDLC_WORKFLOW.md` and the workflow's configuration are on the unmerged `sdlc-workflow` branch, so this workbook links them at `f911fa7` |
| 2026-10-10 01:39 | **Review round.** Initiation round 1 at `67386ff`: revise, with 3 major and 5 minor findings relayed to the planner. How each was handled is in the [review log](#review-log) |
| 2026-10-10 01:41 | **Planning.** Initiation round 2 started on `m5/plan` at `67386ff` |
| 2026-10-10 01:50 | **Documents.** `ff14d1b` marks `DATA_MODEL_M5_M6.md`'s open questions 1-3, and its two P28 amendment rows, as answered, as its banner already said |
| 2026-10-10 01:57 | **Measured.** `two_states` without trade: all 12 goods' price gaps as means over the 20th year, and real GDP year by year ([SSR](01-system-service-request.md#what-happens-today)) |
| 2026-10-10 01:57 | **Noted for the maintainer.** D28's Amends line still names amendments of D6 and D19 "once settled". Run decision 4 leaves D19 unchanged, and run decision 5 keeps the line as written. The plan PR's description says so ([SSR](01-system-service-request.md#initial-assessment), item 4; [charter](02-project-charter.md#assumptions), A11) |
| 2026-10-10 01:57 | **Scope.** A command that charters merchants is out of scope: chartered merchants come only from scenario seeding in this run ([SSR](01-system-service-request.md#initial-assessment), item 3) |
| 2026-10-10 02:00 | **Review round.** Initiation round 2 (this page, the SSR and the charter, with `ff14d1b`) submitted to the reviewer |
| 2026-10-10 02:08 | **Review round.** Initiation round 2 at `041d9e5`: approve, with 0 blocking, 0 major and 7 minor findings. The minor findings are carried into the Analysis ([below](#initiation-round-2-minor-findings)) |
| 2026-10-10 02:10 | **Planning.** Analysis round 1 started on `m5/plan` at `041d9e5` |
| 2026-10-10 02:58 | **Documents.** [Requirements specification](03-requirements-specification.md): 102 requirements (62 functional, 24 non-functional, 16 data rules), 23 choices within the contract, 17 process-logic rules, and eight diagrams (context, level 0, three level 1, use case, activity, ER) |
| 2026-10-10 02:58 | **Noted for the maintainer.** Merchant dividends are paid untaxed: D15 withholds income tax from producers' payments only, and taxing merchants (`TRADE.md:61`) would amend D15, which no decision does ([requirements](03-requirements-specification.md#choices-this-page-makes-within-the-contract), C7; R-F62) |
| 2026-10-10 02:58 | **Noted for the maintainer.** `FoundProducer` records a request that the next month end's investment step funds or drops, so `World::apply` checks nothing and a logged command stays valid against the initial world ([requirements](03-requirements-specification.md#choices-this-page-makes-within-the-contract), C15) |
| 2026-10-10 03:01 | **Review round.** Analysis round 1 (the requirements specification, this page, and one correction to the SSR) submitted to the reviewer |
| 2026-10-10 03:14 | **Review round.** Analysis round 1 at `96e8084`: revise, with 2 major and 13 minor findings. How each was handled is in the [review log](#analysis-round-1-findings) |
| 2026-10-10 03:16 | **Planning.** Analysis round 2 started on `m5/plan` at `96e8084` |
| 2026-10-10 03:25 | **Measured.** The D13 benchmark of `aa2133a` on the run's machine (`two_states`, `--scale 55 --regions 1500 --threads 8`): 69.0 and 70.2 ms/day over 29 days, 69.7 and 71.0 over 30, so the first month-end day, compaction of the copies included, takes about 92 to 94 ms ([requirements](03-requirements-specification.md#non-functional), R-N10 and R-N26) |
| 2026-10-10 03:58 | **Documents.** [Requirements specification](03-requirements-specification.md), round 2: 110 requirements (67 functional, 27 non-functional, 16 data rules), 27 choices within the contract, 18 process-logic rules, and the same eight diagrams, revised |
| 2026-10-10 03:58 | **Noted for the maintainer.** Every M5 route is one link, a best path of its horizon pair; the trade horizon checks the routes when a scenario loads and isn't kept. Multi-link routes, where run decision 2's bottleneck rule bites, stay the follow-up the charter excludes ([requirements](03-requirements-specification.md#choices-this-page-makes-within-the-contract), C1) |
| 2026-10-10 03:58 | **Noted for the maintainer.** `SetTariff`'s and `FoundProducer`'s wire forms, server checks and log and save fields land with their engine commands in M5-3 and M5-8, not in M5-13: the server's exhaustive conversions (NETWORK_PROTOCOL §5) and `pax_data`'s command descriptions don't compile otherwise. `protocol_minor` rises once per schema-changing task, as in M4 (C27) |
| 2026-10-10 03:58 | **Noted for the maintainer.** Every task that adds daily or month-end work measures the D13 benchmark before and after it, and `pax_cli bench` gains a `--warmup` so construction is timed at 1M rows (C26; R-N25 to R-N27). Construction spends only cash above the day's input budgets, restart reserve and wage bill (C24) |
| 2026-10-10 03:59 | **Review round.** Analysis round 2 (the requirements specification, this page, and the SSR's count of choices) submitted to the reviewer |

## Change requests

A change to a task's contract after the plan is approved, and who decided it.

| Id | Task | Change | Reason | Decision | Decided by |
|---|---|---|---|---|---|
| — | — | None yet | — | — | — |

## Review log

| Stage or task | Round | Commit | Verdict | Findings, and how each was handled |
|---|---|---|---|---|
| Initiation | 1 | `bd08ee6` (decisions recorded) and `67386ff` (workbook) | revise | 3 major, 5 minor; each handled in round 2 ([below](#initiation-round-1-findings)) |
| Initiation | 2 | `ff14d1b` (data model) and `041d9e5` (workbook) | approve | 7 minor; each carried into Analysis round 1 ([below](#initiation-round-2-minor-findings)) |
| Analysis | 1 | `96e8084` | revise | 2 major, 13 minor; each handled in round 2 ([below](#analysis-round-1-findings)) |
| Analysis | 2 | this round's workbook commit | pending | — |

### Initiation, round 1: findings

The reviewer's log line counts 6 minor findings; 5 were relayed to the planner, and each relayed finding is handled below.

| # | Severity | Finding | How it was handled |
|---|---|---|---|
| 1 | major | O3's autarky baseline measured 6 of the 12 goods, on single days, while some prices cycle; the SSR's list of which goods are cheaper where was incomplete | The SSR now measures all 12 goods as the mean over the 20th year, the same days in both markets, and defines that statistic, `G` ([The gap statistic](01-system-service-request.md#the-gap-statistic)). It shows each good's daily range and the statistic's stability across years. O3a compares the same statistic and window with the autarky run of the same build. The SSR now says that cloth, steel, tools and six raw goods are cheaper in Highland, and clothes, furniture and wine in Lowland (Initial assessment, item 2). One correction to the finding's wording: wine is a raw good too (`data/production.toml:19-25`), so it is six of the seven raw goods that are cheaper in Highland |
| 2 | major | O4 didn't say which GDP it is judged on, and let only the unemployment band move | O4b is judged on `pax_cli report`'s C+G real GDP in day-1 prices, comparable with 4,202.52. Its baseline is the same build with investment off, and it must be at least 0.1% higher (ten times the yearly spread). SR-5 and SR-12 now say that the new figures (investment, trade, each market's GDP) are reported beside that measure, never folded into it. [The bands clause](02-project-charter.md#the-bands-clause) lists every assertion of `economic_bands.rs`, including nominal GDP's [4,500, 5,500] and the employment check, with today's values. One correction: the employment check is one-sided (`economic_bands.rs:63`), so growth can't trip it; a fall can |
| 3 | major | Tariff granularity and a command that charters merchants were missing from the initial assessment | The SSR's Initial assessment, item 1, compares four granularities and what each implies for `SetTariff`, its log and save format, D24 and the protocol; the Design stage picks one, and the planner leans to per good. Item 3 puts a charter command out of scope: neither the milestone's wire additions, DoD 4 nor D17's Amends line has one, and run decision 5 keeps that line as written. Chartered merchants come only from seeding, and M5-16 never founds one (SR-3; the charter's out-of-scope list; O6) |
| 4 | minor | The SSR presented `[[route]]` against `[[link]]` as open, though run decision 2 presupposes links | The SSR's Initial assessment, item 1, now says that run decision 2 implies links with a route's capacity the minimum over its links, so the open choice is only how routes and their `margin` and `k` are declared beside the links |
| 5 | minor | D28's stale Amends line was recorded only in a commit message; `DATA_MODEL_M5_M6.md:383` still posed route capacity as open | The SSR's Initial assessment, item 4, the correspondence log and the charter (A11, and "Stays the maintainer's") record the line for the maintainer, and the plan PR's description is to say so (A11). `ff14d1b` marks the data model's open questions 1-3 answered, and its two P28 rows too, which posed the same settled points |
| 6 | minor | The critic, judging against `main`'s DECISIONS.md, may read the plan PR's acceptance of D17, D27 and D28 as an agent's | Charter A11: the plan PR's description quotes run decisions 1-5 verbatim as the authority. If the critic still reports it CRITICAL, the PR is parked as a disputed finding for the maintainer, never worked around; the mitigation goes to the baseline plan's risk register. O1 refers to it |
| 7 | minor | O3's formula holds only with no tariff and no binding capacity | O3b states its conditions (one route, no tariff, capacity not binding) and sets the tolerance from `k`; where capacity binds, only O3a applies. The SSR adds the related open point: what `gap` means in the flow rule, which sets where the flow reaches capacity (Initial assessment, item 2) |
| 8 | minor | M5-5's "mutual real GDP growth" had no acceptance measure | O3c: each market's real GDP over the 20th year is above the autarky run's, on a per-market measure that SR-12 adds; SR-1 says the same |

### Initiation, round 2: minor findings

The stage was approved with these minor findings, from the reviewer's log line. Each is handled in Analysis round 1:

| # | Finding | How it was handled |
|---|---|---|
| 1 | O3a's friction band ignores the route's direction and the tariff | R-F20 states the band of the route from the good's cheaper market, `(1 + margin + t·(1 − τ)) ÷ (1 − τ)`, with the importer's tariff `t` |
| 2 | `mini_valley` out of scope, against A4's fallback and the new hashed columns | R-N8: new tables and columns are hashed and snapshotted so that a world without their content hashes as before, as nations are (`world.rs:633-641`); R-F59 switches every mechanism off in its frozen definitions |
| 3 | O7 gives no method for measuring the month-end systems | R-N10 names the commands: `--days 30` with `--scale 55 --regions 1500` reaches the first month end on the full table; R-F56 makes `bench` print that day's time |
| 4 | O4 and O5 lack `conservation.rs` cases (AGENTS.md §6), and §1 is uncited | R-N3 lists a case per new money flow through the shared `random_world`; R-N19 cites AGENTS.md §1 |
| 5 | O5 has no measure for run decisions 3 and 4 | R-F35's test pays a wholly striking POP nothing; R-F39's test finds militancy unchanged by the riot step, and a month later unchanged against a run without the transfer |
| 6 | The SSR attributes "no conflict rule" to D17 | Corrected in the SSR's Initial assessment, item 1: D24's "Order" bullet asks for a conflict rule only where a command can conflict with another player's, and a nation's tariffs touch only its own imports (`DATA_MODEL_M5_M6.md:166`) |
| 7 | M5-16's row says "(Chartered by command)", against the charter's exclusion of a charter command | R-F16: entry never founds a Chartered merchant; R-F60: no charter command (won't); M5-16's pull request rewords its row when it ticks it |

### Analysis, round 1: findings

All 15 were relayed and each is handled in round 2 of the [requirements specification](03-requirements-specification.md). Requirement ids are stable: none was renumbered, and the new ones (R-F63 to R-F67, R-N25 to R-N27) follow the last.

| # | Severity | Finding | How it was handled |
|---|---|---|---|
| 1 | major | PL-8 counted unclaimed unemployment by POP size, so strikers counted as unemployed, against R-F36 and C16 | The reviewer's first option. PL-8 now counts a pool's available workers, its working members by PL-15's formula from the POPs' sizes and militancy when the step runs (at month end after mobility and politics), and `U = max(0, available − jobs − claimed)`, so strikers never count; it shows the reviewer's example (100 people, 80 jobs, 30 striking: `U = 0`). R-F24, R-F28, R-F29 and R-F46 cite PL-8. C16 now says strikers withhold their labour, and that D20, D25 and D26 keep counting by size as their rules say; the claim that strikers "have jobs they refuse" is gone. The 6.0 level-1 DFD gives 6.4 to 6.6 the POPs' sizes and militancy, the strike rules and the producers' capacity |
| 2 | major | R-F54, R-F55, R-N10 and R-N12 traced only to M5-15, so earlier tasks' tests and the bench gates couldn't use them | The figures are split by the task that adds each flow: R-F54 trade flows per route and good and per market (M5-2, M5-3), R-F63 merchants' dividends, wind-ups and foundings (M5-4, M5-16), R-F64 C+G by market (M5-5), R-F65 investment (M5-7 to M5-9), R-F66 riots (M5-12), strikers staying R-F36's (M5-11); R-F55's `--market` is M5-5's, and R-F67 prints each figure in the task that adds it. R-N12 is traced to the tasks that add each copied table (M5-1, M5-2, M5-6, M5-8, M5-16). The D13 check is three requirements: R-N10, the cold 29 days, for every task that adds daily work (M5-2, M5-3, M5-4, M5-5, M5-7, M5-11); R-N26, through the first month end, for every task that adds month-end work (M5-4, M5-7 to M5-9, M5-12, M5-16); and R-N27, a warmed-up world in which projects run, for M5-7, with R-N25's `--warmup`. M5-15 records all three. Re-tracing found one more ordering fault, now fixed: an engine command can't land without its wire and file forms (`crates/pax_server/src/commands.rs:1-7`; `crates/pax_data/src/lib.rs:202-211`), so those move from M5-13 to M5-3 and M5-8 (C27; R-F42 to R-F44) |
| 3 | minor | R-N8's hash rule covered new tables, not new columns on `PRODUCER`, which `mini_valley` has | R-N8 and the new PL-18 name them: new tables are hashed only when they have rows, and `PRODUCER.owner_nation`, `PRODUCER.idle_months` and `NATION.tariff` only once a row differs from its default (`None`, 0, 0), each behind a tag; the snapshot always carries them, and its hash check still holds because the rule reads only the state (C25). A planned test sets one `idle_months` and back |
| 4 | minor | R-N16's "become 2" didn't fit separately merged pull requests | R-N16: every pull request that changes a layout raises its format by one, and a file of any other format is refused with an error naming it, checked before anything else is read. The save reader parses the whole file before its check today (`save.rs:231-237`), so the requirement says so |
| 5 | minor | C1 let a route span several links, against the charter's exclusion of multi-hop merchants | One link per route. C1 says why, and how run decision 2 still holds; R-D2 requires a link joining the route's markets that is a best path of their horizon pair, and refuses a route that a longer path beats; R-F4, PL-1 and the ER diagram follow, and R-F61 (won't) names multi-link routes |
| 6 | minor | DFD flows missing: `tariff_due`'s source, the tariffs for 3.5, the route origin, jobs for 6.6, projects started to 8.0, and the initial world for 9.0 | Added `D3.2` to 3.3 and 3.4 (tariffs owed), `D2` to 3.3 and 3.4 (route origin), `D3.3` to 3.5 (tariffs), `D1` to 3.3 to 3.5 (rules), `D3.6` to 6.4 and 6.6 (capacity per pool), `D3.5` and the strike rules to 6.4 to 6.6, `D3.7` to 6.3, and 6.4 to 8.0 (projects started). At level 0, 1.0 to 9.0 carries the initial world and content hash a load checks against; it is internal, so the levels still balance |
| 7 | minor | The activity diagram drew no fork for the POP map-reduce, no riot loop, and ended the panic in the client lane | A rayon lane with four fork/join pairs (consumer aggregation, discovery, settlement's passes A and B), each join an integer-sum reduce; the riot step loops over provinces; the panic has its own end in the tick lane |
| 8 | minor | R-N20 kept the horizon and route attributes as derived data, against D7's one-cache rule | Nothing derived is kept: the horizon is computed when a scenario loads, checks the routes and is dropped, and a route refers to its link by row, topology fixed for the game (R-F3, R-N20, R-D14). R-N20 cites D17's Routes bullet, D14 rule 5 and AGENTS.md §1 and §5, and adds no cache |
| 9 | minor | PL-10's construction budget could spend the restart reserve and today's wages | Bounded by cash less today's input budgets, the restart reserve and today's wage bill, at the opening prices (PL-10, C24, R-F25) |
| 10 | minor | R-N10 depended on R-F56, a should, and the month-end time includes compaction of the copies | R-N26 measures the 30-day mean directly and derives the month-end day as `30 × mean₃₀ − 29 × mean₂₉` from medians of three runs, R-F56's print being only more precise; it says that under `--scale` that day compacts the 55 copies, so the figure is an upper bound. Measured at `aa2133a`: 92 to 94 ms for that day against about 70 for the others |
| 11 | minor | R-D5 was in no task row | R-D5 is traced by key: `enabled` and `profit_margin` to M5-7, `investor_reserve_days` to M5-8, `slack` and `idle_months_before_shrink` to M5-9; R-D4 and R-D6 likewise. A script checked that every requirement is in a task row and every id cited exists |
| 12 | minor | `TradeRouteView`'s per-route figures had no source | R-F54 reports, per route and good, the units bought, landed and lost and the tariff paid; R-F45 and R-F47 cite it |
| 13 | minor | R-F40's three years against R-N6's 720 days | R-F40 has one timeline (40% on day 1080, back to 12% on day 1800; strikers and a riot before day 1800; no riot from day 2520 to 2880), and R-N6 runs that same world and log through day 1800, so data that meets R-F40 meets R-N6 |
| 14 | minor | C15 didn't say why `FoundProducer` needs no D24 conflict rule | C15: outside sandbox a nation has one session, and a request touches only that nation's treasury and its own province's workers; sandbox seats command any nation by design, and stamp order resolves them as it does today's rate commands |
| 15 | minor | R-F55's own-basket real GDP differs from O3c's | R-F55 is informational; R-F20's test computes O3c's measure from R-F64's figures with the autarky basket in both runs, and R-F33's test computes `pax_cli report`'s the same way |

## Run rules

- **Merge authorisation:** auto-merge when every required check is green and the critic passes or skips (run decision 6). How the run reads it is in the [charter](02-project-charter.md#authorisation).
- **Stop time:** 2026-10-10T07:00:00+08:00. After it nothing new starts; a pull request in flight gets one last check, and then the final report is written.
- **The maintainer's run decisions**, quoted as given:

> Given by the maintainer on 2026-10-10 at 00:55 (+08:00) for this M5 run:
> 1. D17 (inter-market trade), D27 (capital investment) and D28 (strikes and riots) are ACCEPTED as written in docs/DECISIONS.md.
> 2. D17's open point: each route takes its bottleneck link's capacity (routes do not share link capacity pro rata).
> 3. D28's open point on strike pay: striking workers forgo wages. The pool's wages are split, by alloc::allocate, among its working (non-striking) members only (amends D6).
> 4. D28's open point on riot relief: the riot security transfer does NOT relieve militancy directly. It acts only through life needs, as today, so D19 is unchanged.
> 5. Record these in docs/DECISIONS.md, in the plan PR or the first PR that touches each decision. Mark D17, D27 and D28 Accepted (2026-10-10, by the maintainer for M5), and replace their Open lines with the answers above. Change nothing else in them.
> 6. Run rules as in the M3 and M4 runs: auto-merge only when every required check (CI, Benchmark regression, Critic) is green and the critic passes or skips. Never use --admin or a ruleset bypass, push to main, force-push, edit the ruleset, secrets or settings, edit critic.yml or .claude/commands/critic.md, add the contract-change label, or post critic-waive comments. Stage explicit paths only. Never touch the maintainer's checkout outside out/m5-run/, and never touch the locked worktrees .claude/worktrees/docs-structure-fixes and .claude/worktrees/sdlc-workflow.

The contract and the process, linked rather than restated:
- [AGENTS.md](../../../AGENTS.md): the rules for every contributor, agents included.
- [docs/DECISIONS.md](../../DECISIONS.md): the binding decisions; it wins any conflict.
- [.claude/commands/critic.md](../../../.claude/commands/critic.md): what the critic checks.
- [docs/SDLC_WORKFLOW.md](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/blob/f911fa707da7f981525683259f56b9846ef49024/docs/SDLC_WORKFLOW.md): how this run works. It is on the unmerged `sdlc-workflow` branch, linked at `f911fa7`.
