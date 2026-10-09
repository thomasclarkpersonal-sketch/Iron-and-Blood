# Project Workbook: the m5 run

> [!NOTE]
> **Status: planning record** for the "m5" run, kept by the SDLC workflow (docs/SDLC_WORKFLOW.md). Not binding: the contract wins, and each fact moves to its home as the work lands.

The ongoing record of the run that plans and builds [Milestone 5](../../MILESTONE_5.md): what it delivers, what was said and decided, and how each review went. The run's log and its morning report are in `out/m5-run/` in the maintainer's checkout, which git ignores.

## Deliverable register

| Deliverable | File | SDLC phase | Status | Last reviewed at |
|---|---|---|---|---|
| Project Workbook | `README.md` (this page) | 1. Initiation; kept up to date all run | draft | `67386ff` (Initiation round 1: revise); round 2 under review |
| System Service Request | [01-system-service-request.md](01-system-service-request.md) | 1. Initiation | draft | `67386ff` (Initiation round 1: revise); round 2 under review |
| Project Charter | [02-project-charter.md](02-project-charter.md) | 1. Initiation | draft | `67386ff` (Initiation round 1: revise); round 2 under review |
| Requirements specification | `03-requirements-specification.md` | 2. Analysis | planned | — |
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

## Change requests

A change to a task's contract after the plan is approved, and who decided it.

| Id | Task | Change | Reason | Decision | Decided by |
|---|---|---|---|---|---|
| — | — | None yet | — | — | — |

## Review log

| Stage or task | Round | Commit | Verdict | Findings, and how each was handled |
|---|---|---|---|---|
| Initiation | 1 | `bd08ee6` (decisions recorded) and `67386ff` (workbook) | revise | 3 major, 5 minor; each handled in round 2 ([below](#initiation-round-1-findings)) |
| Initiation | 2 | `ff14d1b` (data model) and this round's workbook commit | pending | — |

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
