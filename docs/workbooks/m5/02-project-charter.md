# Project Charter: Milestone 5

> [!NOTE]
> **Status: planning record** for the "m5" run, kept by the SDLC workflow (docs/SDLC_WORKFLOW.md). Not binding: the contract wins, and each fact moves to its home as the work lands.

| | |
|---|---|
| **Title** | Milestone 5: Trade, Investment and Economic Unrest |
| **Slug** | `m5`: branches `m5/plan` and `m5/<task>`, this workbook (`docs/workbooks/m5/`), and the git-ignored run directory `out/m5-run/` in the maintainer's checkout |
| **Sponsor** | The maintainer |
| **Request** | [MILESTONE_5.md](../../MILESTONE_5.md), proposed formally in the [System Service Request](01-system-service-request.md) |

## Purpose

The m5 run plans and builds Milestone 5 overnight, unattended. It connects today's isolated markets through merchants that trade on price gaps ([D17](../../DECISIONS.md#d17-inter-market-trade-routes-merchants-and-tariffs)), lets productive capacity grow through investment in real construction goods and the founding of new producers ([D27](../../DECISIONS.md#d27-capital-investment-and-capacity-expansion)), gives militancy economic consequences through strikes and riots ([D28](../../DECISIONS.md#d28-economic-unrest-strikes-and-riots)), and gives players trade, tariff and construction controls in the Godot client. It keeps the engine's contract while doing so: money conserved every day ([D5](../../DECISIONS.md#d5-money-model-and-stock-flow-consistency)), results identical at any thread count and on every platform ([D3](../../DECISIONS.md#d3-determinism-fixed-point-everything), [D11](../../DECISIONS.md#d11-determinism-harness-and-golden-files)), the tick within its budget ([D13](../../DECISIONS.md#d13-performance-budget)), and the crate boundaries ([AGENTS.md §2](../../../AGENTS.md#2-strict-decoupling)). Planning comes first, one stage at a time under a separate reviewer. Then each task becomes one reviewed pull request, merged only when every check is green, until the queue is empty or the stop time comes.

## Objectives

"DoD n" is item n of the milestone's [definition of done](../../MILESTONE_5.md#-definition-of-done). A measure that compares two runs compares like with like: the same build and scenario, the same statistic, the same days.

| # | Objective | Acceptance measure |
|---|---|---|
| O1 | An approved plan on `main` | The four planning stages and the three-lens panel approve it; the plan PR, whose description quotes run decisions 1-5 (A11), merges with every required check green and the critic passing; the [deliverable register](README.md#deliverable-register) marks every planning deliverable approved |
| O2 | The maintainer's decisions recorded exactly | On `main`, D17, D27 and D28 read "Accepted (2026-10-10, by the maintainer for M5)", their Open lines are replaced by run decisions 2-4, and nothing else in them has changed (`bd08ee6`); `python3 scripts/check_docs.py` passes |
| O3 | Trade works and arbitrages prices | DoD 1 and M5-5: measures O3a to O3g, [below](#o3-trade-works-and-arbitrages-prices) |
| O4 | Investment grows the economy | DoD 2 and M5-10: measures O4a to O4e, [below](#o4-investment-grows-the-economy) |
| O5 | Unrest has economic consequences | DoD 3: tests show output falling above `strike_threshold`, and above `riot_threshold` stock destroyed and a security transfer paid that is exactly what the treasury pays ([REBELLIONS.md](../../REBELLIONS.md#acceptance-tests), tests 1-3, with run decisions 3 and 4); any change to `two_states`' results meets [the bands clause](#the-bands-clause) |
| O6 | Players control trade and construction | DoD 4: the client shows routes, sets tariffs, shows construction and founds producers, and the headless client smoke test exercises them; `crates/pax_server/tests/session_replay.rs` replays a session that uses the new commands to the server's final state hash. No command charters merchants ([out of scope](#out-of-scope)) |
| O7 | Budgets and determinism hold | DoD 5: identical hashes at 1, 2, 3 and 8 threads on a world that trades, invests and riots (today that comparison runs only on `mini_valley`, `crates/pax_data/tests/determinism.rs:12-14` and `:31-36`, and CI checks `two_states` at 1 and 4 threads, `.github/workflows/ci.yml:112-117`); `pax_cli verify` passes for both scenarios on Linux, Windows and macOS; `pax_cli bench` at about 1M POP rows on 8 threads, with the M5 mechanisms running, at most 100 ms/day and recorded in [PERFORMANCE.md](../../PERFORMANCE.md#tick-d13), with the month-end systems measured too, since `bench --scale` stops before the first month end (that page's first note); the Benchmark regression check green on every PR |
| O8 | Every merge is clean and explained | Every merged PR had every required check green and the critic passing or skipped; no CRITICAL finding is left; every DEBT finding is fixed, or left with a stated reason and a drafted waiver in `out/m5-run/REPORT.md`; every commit passes the commit audit; every golden-hash change says why ([D11](../../DECISIONS.md#d11-determinism-harness-and-golden-files)) |
| O9 | The documents stay true | After each merge, `check_docs.py` passes on `main`, the task's row in MILESTONE_5 is ticked, and the system documents describe the merged code ([AGENTS.md §8](../../../AGENTS.md#8-documentation-maintenance)) |
| O10 | The run ends with an honest record | At the stop time, or when the queue empties, `out/m5-run/REPORT.md` gives every task's outcome (merged, ready, stacked, blocked or not started) and the choices to check; if anything merged and time remains, the close-out PR brings MILESTONE_5's status up to date against its definition of done |

### O3: trade works and arbitrages prices

DoD 1 and M5-5. Each measure becomes a named test in the Analysis stage. The *autarky run* is the same build and scenario without the route and its merchants. `G` and `W` are the [SSR's gap statistic](01-system-service-request.md#the-gap-statistic): the ratio of the two markets' mean prices over the 20th year.
- **O3a, gaps narrow.** In `two_states` with M5-5's route, every good whose autarky `G` exceeds the route's friction band `(1 + margin) ÷ (1 − τ)` has a smaller `G` than in the autarky run.
  - With TRADE.md's example values (`τ` and `margin` both 0.05, `TRADE.md:103-105`) the band is 1.105, and every good exceeds it except fish (1.089 at `aa2133a`).
  - The SSR's table is the autarky run at `aa2133a`. The test computes its own autarky run, because tasks merged before M5-5 (strikes, investment) may move it.
- **O3b, convergence to the band** (TRADE.md test 1). In a test world with one route, no tariff, and a capacity that doesn't bind on the measured days, each traded good's ratio of destination to origin mean price settles within a tolerance of `(1 + margin) ÷ (1 − τ)`.
  - The Analysis stage sets the tolerance from the flow speed `k`. The flow grows with the gap (`TRADE.md:54`), so a flow below capacity is sustained by a ratio above the band, by an amount that `k` bounds.
  - Where capacity binds, as it may in `two_states`, the ratio can stay further above the band (TRADE.md test 4), and only O3a applies.
- **O3c, both markets gain** (M5-5's "mutual real GDP growth"; TRADE.md test 9). In O3a's run, each market's real GDP over `W` is above its real GDP in the autarky run. A market's real GDP is its own C+G deflated by its own Laspeyres index: its prices weighted by the autarky run's day-1 traded quantities in that market, the same weights in both runs. SR-12 adds this measure (planned); today's report has only the world's ([SSR](01-system-service-request.md#what-happens-today), problem 5).
- **O3d, tariffs** (TRADE.md test 3): a higher tariff reduces the volume on its route, and the importing treasury receives exactly the tariffs paid.
- **O3e, entry and exit** (TRADE.md test 6; M5-4 and M5-16): a persistent gap with too little merchant cash founds a merchant, a loss-making merchant winds up and returns its cash, and money is conserved through both.
- **O3f, conservation:** the daily money assert holds with random routes and high volumes, in `crates/pax_engine/tests/conservation.rs` through the shared `random_world` (`crates/pax_engine/tests/common/mod.rs:75`).
- **O3g, bands:** M5-5's change to `two_states` meets [the bands clause](#the-bands-clause).

### O4: investment grows the economy

DoD 2 and M5-10. The *baseline run* is the same build and scenario with investment switched off by its rules, as `mini_valley`'s frozen definitions will switch it off (A4). Comparing within one build keeps gains from trade (O3) from counting as growth.
- **O4a, capacity:** total producer capacity on day 7200 of `two_states` is above 253,500 worker slots, the load value that nothing changes today ([SSR](01-system-service-request.md#what-happens-today), problem 2).
- **O4b, real GDP:** real GDP over years 16-20 is at least 0.1% above the baseline run's (4,202.52 a day at `aa2133a`).
  - The measure is `pax_cli report`'s, C+G in day-1 prices (`crates/pax_cli/src/report.rs:7-11`); for years 16-20 it is the last row of `pax_cli report scenarios/two_states --days 7200 --every 1800`.
  - It leaves investment spending out, so construction alone can't raise it: growth has to reach consumption. SR-12's new figures (investment, trade, each market's GDP) are reported beside it, never folded into it.
  - 0.1% is ten times the spread of the yearly values over years 16-20 today (4,202.51 to 4,202.92), so a change that small can't pass for growth.
- **O4c, construction demand** (DoD 2's second bullet; INVESTMENT.md test 2): a running project places D1 buy orders, every day until they are delivered, for the construction goods its recipe still needs; and over the 20 years, the quantity traded of each of tools, timber and steel is above the baseline run's. The window is the whole run, not years 16-20, so that it holds however early the projects run.
- **O4d, unemployment:** unemployment on day 7200 is at most 2%, the upper edge of the band that `crates/pax_data/tests/economic_bands.rs:51-55` asserts. The lower edge, 0.3%, may move down for a stated reason, since investment employs the unemployed, as the test's header expects (`economic_bands.rs:6-8`). Raising the upper edge doesn't meet DoD 2.
- **O4e, bands:** every task that changes `two_states`' results meets [the bands clause](#the-bands-clause).

### The bands clause

`crates/pax_data/tests/economic_bands.rs` asserts `two_states`' aggregates at year 20. A pull request that changes `two_states`' results keeps every assertion, or moves it in the same pull request and says why, as the test's header asks (lines 6-8) and TRADE.md test 9 asks of trade. The values at `aa2133a` are from the [SSR's measurements](01-system-service-request.md#how-it-was-measured).

| Assertion | Line | Band | At `aa2133a` | Note |
|---|---|---|---|---|
| Nominal GDP per day (C+G), mean over the final year | 45 | 4,500 to 5,500 | 5,023.82 | Nominal growth of more than 9.5% breaks the upper edge |
| Unemployment among worker professions, last day | 53 | 0.3% to 2% | 0.8% | Only the lower edge may move under DoD 2 (O4d) |
| Employment at year 20 against year 5 | 63 | at least 99.5% | passes; −0.1% when set (line 58) | One-sided: a fall, such as strikes or depreciation may cause, trips it; growth can't |
| Tax take over the final year | 67 | 11.5% to 12.5% | 12.0% | |
| Life-needs coverage, last day | 74 | at least 0.98 | 0.992 | |
| Mean militancy, last day | 79 | 0.10 to 0.15 | 0.124 | |
| Population, last day | 86 | 245k to 256k | 250,523 | |

## Scope summary

### In scope
- The milestone's [In Scope](../../MILESTONE_5.md#in-scope) items 1-5 and its tasks M5-1 to M5-16, keeping their ids.
- Recording the maintainer's run decisions in DECISIONS.md (done on `m5/plan` in `bd08ee6`), and keeping true every document that they or the tasks touch.
- The parts of the system that change: `pax_engine` (tables, systems, the tick order, commands), `pax_data` (formats, saves and snapshots, the benchmark), the schemas and the generated `pax_protocol` code, `pax_server` (commands and views), `pax_godot` and `client/` (panels and a map mode), `data/` and `scenarios/two_states` (with its golden hashes), tests, and documents.

### Out of scope
- The milestone's [Out of Scope](../../MILESTONE_5.md#out-of-scope-milestone-6-and-milestone-7): Milestone 6 (workforce, banking, politics, revolutions) and Milestone 7 (military, colonisation).
- A command that charters merchants. D17 founds chartered merchants "by command only", but the milestone's wire additions, its DoD 4 and D17's Amends line include no such command ([SSR](01-system-service-request.md#initial-assessment), item 3). In this run chartered merchants come only from scenario seeding, and dynamic entry (M5-16) never founds one.
- Follow-ups the designs name: merchants on multi-hop horizon pairs (`docs/TRADE.md:111`), per-route `transit_days` (`TRADE.md:24`), customs unions (`TRADE.md:114`), wear or maintenance of capacity in use (`docs/INVESTMENT.md:21`), and a repression command (`docs/REBELLIONS.md:16` and `:19`).
- Anything that needs a decision that is neither accepted nor a run decision ([SSR](01-system-service-request.md#sponsorship), "Decisions it may need"): that task is reported blocked.
- Changing `mini_valley`'s results (assumption A4).
- Other work in flight: PR #69, the `sdlc-workflow` branch, M4's open question on what the rate limit covers ([MILESTONE_4.md](../../MILESTONE_4.md#open-follow-ups-owner-decisions)), the miners' famine calibration ([MILESTONE_2.md](../../MILESTONE_2.md#known-issues)), and the human playtest.

## Key stakeholders and roles

| Role | Who | In this run |
|---|---|---|
| Sponsor | The maintainer | Set the request (MILESTONE_5.md, #70); accepted D17, D27 and D28 and made run decisions 2-6; keeps what [Authorisation](#authorisation) leaves theirs; reads `out/m5-run/REPORT.md` in the morning |
| Scouts | Three read-only agents | Briefed the planner on the contract, the code and the risks before planning began |
| Planner | An agent, this workbook's author | Writes each planning stage; before each task is built, refreshes its brief against `main` as it then is; records any change to a task's contract as a change request |
| Reviewer | A separate agent at each stage, then a panel of three (contract, feasibility, traceability) | Checks every page, citation and diagram against the code and the contract; approves, or returns findings |
| Workers | One agent per task | Documents first, then tests with the code, in justified commits; one pull request per task |
| Gate and gate-fix | Agents | Run the local gate (CI's commands that need no Godot, bench runner or fuzzer, plus the Mermaid render) and repair failures at their root cause |
| Critic | The local critic agent and CI's `Critic` check, both running [critic.md](../../../.claude/commands/critic.md) | Review each PR against `main`'s contract; a CRITICAL finding blocks the merge ([AGENTS.md §9](../../../AGENTS.md#9-critic-feedback)) |
| Remediation | Agents running [critic-followup.md](../../../.claude/commands/critic-followup.md) | Fix CRITICAL and DEBT findings, or state why a DEBT finding stays; apply cheap suggestions and queue the rest |
| Commit auditor | An agent, checked by the workflow script | Stops any commit whose message misses the required shape before it is pushed |
| Merge and report | Agents | Squash-merge a green PR under the authorisation; rewrite the run report after every task |
| End users | Players, server hosts, scenario authors and modders, developers | Get the new features and the format changes ([SSR](01-system-service-request.md#who-is-affected)) |

## Assumptions and constraints

### Constraints

These bind the work. They are cited here, not restated.
- **The contract:** [AGENTS.md](../../../AGENTS.md), with its rules on crate purity ([§2](../../../AGENTS.md#2-strict-decoupling)), map-reduce with integer sums only ([§3](../../../AGENTS.md#3-concurrency-mitigation)), determinism ([§4](../../../AGENTS.md#4-strict-determinism-d3)), precomputation and the budget ([§5](../../../AGENTS.md#5-performance-over-flexibility)), conservation and largest-remainder splits ([§6](../../../AGENTS.md#6-economic-integrity-d5)), the golden-hash gate ([§7](../../../AGENTS.md#7-determinism-gate-d11)), documentation ([§8](../../../AGENTS.md#8-documentation-maintenance)) and the critic ([§9](../../../AGENTS.md#9-critic-feedback)); and [DECISIONS.md](../../DECISIONS.md), which wins any conflict.
- **The decisions this work touches:**

| Decision | Where it binds M5 |
|---|---|
| [D17](../../DECISIONS.md#d17-inter-market-trade-routes-merchants-and-tariffs), [D27](../../DECISIONS.md#d27-capital-investment-and-capacity-expansion), [D28](../../DECISIONS.md#d28-economic-unrest-strikes-and-riots) | The rules of trade, investment and unrest, accepted for this run with run decisions 2-4 |
| [D1](../../DECISIONS.md#d1-market-clearing-bounded-tâtonnement-with-pro-rata-rationing) | Merchants and projects trade through ordinary orders, rationed pro rata |
| [D3](../../DECISIONS.md#d3-determinism-fixed-point-everything) | `Fixed` only, randomness only through `rng`, no hash-map iteration |
| [D4](../../DECISIONS.md#d4-tick-schedule-and-goods-persistence) | The tick order, amended to the M5 order by D17, D27 and D28 |
| [D5](../../DECISIONS.md#d5-money-model-and-stock-flow-consistency) | Every new flow conserved; every new holder of money counted |
| [D6](../../DECISIONS.md#d6-firms-production-wages-ownership) | Wages and dividends, amended by D27 (the project reserve, state ownership, founding) and D28 (strikers' pay) |
| [D7](../../DECISIONS.md#d7-pop-accounting), [D8](../../DECISIONS.md#d8-ecs-hand-rolled-struct-of-arrays) | Struct-of-arrays tables; nothing references a POP row; derived data is never stored |
| [D9](../../DECISIONS.md#d9-data-format-toml) | Data and scenario files are parsed in `pax_data` |
| [D10](../../DECISIONS.md#d10-network-model-server-authoritative-deterministic-core), [D12](../../DECISIONS.md#d12-frontend-godot-with-a-rust-gdextension-bridge), [D22](../../DECISIONS.md#d22-wire-protocol-and-client-sessions) | One authoritative server; a generated, append-only protocol; a client bridge that never links the engine |
| [D11](../../DECISIONS.md#d11-determinism-harness-and-golden-files) | Golden files, re-recorded only on purpose |
| [D13](../../DECISIONS.md#d13-performance-budget) | 100 ms/day at 1M POP rows; CI fails a PR that is more than 20% slower |
| [D14](../../DECISIONS.md#d14-market-hierarchy-and-inter-market-trade) | The principles of trade, with rule 5 amended by D17 |
| [D15](../../DECISIONS.md#d15-nations-treasuries-income-tax-and-transfers), [D16](../../DECISIONS.md#d16-government-consumption) | Treasuries, income tax on wages and dividends, government orders |
| [D19](../../DECISIONS.md#d19-militancy) | Militancy, unchanged (run decision 4) |
| [D21](../../DECISIONS.md#d21-commands-and-command-logs) | Validity only in `World::validate`; logged commands checked against the initial world |
| [D23](../../DECISIONS.md#d23-server-loop-pacing-flow-control-and-saves) | Saves and snapshots |
| [D24](../../DECISIONS.md#d24-multiplayer-authority) | The permission check, and a conflict rule for any command that can conflict |
| [D18](../../DECISIONS.md#d18-labour-mobility), [D20](../../DECISIONS.md#d20-migration-within-a-market), [D25](../../DECISIONS.md#d25-occupational-migration-within-a-market), [D26](../../DECISIONS.md#d26-births-follow-employment-band-aid) | The labour flows and births that strikes and investment interact with |

- **The run rules:** the workbook's [run rules](README.md#run-rules).

### Assumptions

| # | Assumption | If it doesn't hold |
|---|---|---|
| A1 | `origin/main` is at `aa2133a` as planning starts | Each task's brief is refreshed against `main` before it is built; a difference that changes a contract becomes a change request |
| A2 | `TRADE.md`, `INVESTMENT.md`, `REBELLIONS.md` and `DATA_MODEL_M5_M6.md` hold mechanism, subordinate to D17, D27 and D28; the Design stage settles their disagreements ([SSR](01-system-service-request.md#initial-assessment)) as choices for the maintainer to check | A disagreement that needs a rule changed blocks its task |
| A3 | PR #69 stays the maintainer's: the run neither merges nor edits it | If it merges during the run, the refresh step takes its corrections in as change requests |
| A4 | `mini_valley` stays a frozen fixture whose golden hashes don't change, as D18, D20, D25 and D26 kept it; its frozen definitions switch each new mechanism off, and `two_states`' hashes are re-recorded by the tasks that change its results, each saying why | A task that must change `mini_valley` says why and re-records it ([AGENTS.md §7](../../../AGENTS.md#7-determinism-gate-d11)) |
| A5 | The workflow, its configuration and `docs/SDLC_WORKFLOW.md` live on the unmerged `sdlc-workflow` branch; this workbook links them at `f911fa7` | — |
| A6 | The maintainer can't be reached before the morning, and an open question would block approval, so the plan decides within the contract and lists its choices | — |
| A7 | CI's Benchmark regression job times each side's own `two_states` (`.github/workflows/ci.yml:154-163`), so a task that adds trade to `two_states` gives the head more work than the base | The baseline plan's risk register (planned) gives the mitigation |
| A8 | The merge authorisation covers this run's pull requests only: the plan PR, the task PRs and the close-out | — |
| A9 | A required check that can't run (an outage, say) is not green | The PR waits, then is parked with the reason |
| A10 | The workflow bounds its queue by `maxTasks` (12 unless the run was started with more), and M5 has 16 tasks. *Noted in the whole-plan revision, round 2:* this run was started with 16, the bound its baseline-plan stage prints (`.claude/workflows/sdlc-overnight.js:867` at `f911fa7`), and a run that resumes the plan must pass `maxTasks` 16 again, since loading the plan re-checks the bound (`:660`, `:1113-1114`; [baseline plan](05-baseline-project-plan.md#schedule)) | The baseline plan fits them into the run's bound, combining rows that belong in one PR (as M4-8 and M4-10 were one, #61) and keeping the milestone's ids |
| A11 | The critic judges the plan PR against `main`'s DECISIONS.md, where D17, D27 and D28 are still Proposed with Open lines, and it can't see the run decisions ([AGENTS.md §9](../../../AGENTS.md#9-critic-feedback)). So the plan PR's description quotes run decisions 1-5 verbatim as the authority for `bd08ee6`'s change to DECISIONS.md, including D28's amendment of D6. It also says that D28's Amends line stays as written under run decision 5 ([SSR](01-system-service-request.md#initial-assessment), item 4) | If the critic still reports the change CRITICAL, the plan PR is parked as a disputed finding for the maintainer ([REPO_SETUP.md](../../REPO_SETUP.md#handling-a-disputed-critical-finding)), never worked around. The baseline plan's risk register (planned) gives the mitigation |

## Target dates

Times are on 2026-10-10, +08:00. The planning targets assume two review rounds for Initiation, which is in its second, and one or two for each later stage (the workflow allows three). Each task's target is its bar in the baseline plan's Gantt chart (planned).

| Milestone | Target |
|---|---|
| Run started by the maintainer | 00:55 |
| Initiation approved | 02:10 |
| Analysis approved | 03:00 |
| Design approved | 03:50 |
| Baseline plan approved | 04:35 |
| Panel approved, plan sealed | 05:05 |
| Plan PR merged | 05:35 |
| Tasks M5-1 to M5-16 | From the plan PR's merge, in the task DAG's order |
| **Stop time**: nothing new starts | **07:00** (2026-10-10T07:00:00+08:00) |
| Close-out PR | After the queue empties, if a task merged and time remains |
| Final run report | At the stop time, or when the queue empties |

That leaves about an hour and a half for building. In the M4 run each PR took two to six critic rounds (its report, `out/m4-run/REPORT.md`, "Notes"), and M5's tasks are larger, so this run should expect to merge the first one to three tasks of the DAG. The rest stay in the approved plan, which a later run continues from (`fromPlan`, in SDLC_WORKFLOW.md's "Running it").

## Authorisation

### Authorised for this run

The maintainer's run decisions 5 and 6 ([quoted](README.md#run-rules)) authorise the run to:
- plan and build M5 on D17, D27 and D28 as accepted, and record them in DECISIONS.md (done on `m5/plan` in `bd08ee6`);
- squash-merge a run PR when every required check is green and the critic passes or skips;
- make choices within the contract and the run decisions, each listed for the maintainer in its commit (`Decisions:`), its PR description and the run report.

How the run reads that authorisation (choices for the maintainer to check):
- **"Every required check (CI, Benchmark regression, Critic)":** the checks the `main` ruleset requires ([REPO_SETUP.md §8](../../REPO_SETUP.md#8-protect-main-require-ci-and-block-merges-on-critical-findings)): Format, clippy, docs; Tests and determinism gate; Determinism (windows-latest); Determinism (macos-latest); Benchmark regression; and Critic. The run also waits for CI's other jobs on the PR (the client smoke test, the Docker image, fuzzing and the Mermaid render) to be green, as the M4 run did for the smoke test and fuzzing. That is the stricter reading.
- **Where the decisions were recorded:** in the plan PR rather than in each decision's first task PR; run decision 5 allows both. The critic judges every PR against `main`'s DECISIONS.md, so with the plan merged first, every task PR is judged against the accepted text.

### Stays the maintainer's
- **Waivers.** No agent writes a line that starts with `critic-waive` on GitHub; drafted waivers go only into the run report.
- **Contract changes.** Relaxing or contradicting a rule, the `contract-change` label, and any edit to `critic.yml`, `critic.md`, the ruleset, secrets or settings.
- **Decisions that are neither accepted nor run decisions** ([SSR](01-system-service-request.md#sponsorship), "Decisions it may need"): the task that needs one is reported blocked on a decision.
- **Disputed CRITICAL findings** ([REPO_SETUP.md](../../REPO_SETUP.md#handling-a-disputed-critical-finding)): the PR is parked with the `needs-human` label.
- **D28's Amends line**, which run decision 5 keeps as written although it names an amendment of D19 that won't happen ([SSR](01-system-service-request.md#initial-assessment), item 4).
- **Everything outside the run:** merging PR #69 or the `sdlc-workflow` branch, the locked worktrees, and the maintainer's checkout outside `out/m5-run/`.
- **The playtest, and closing the milestone.**
