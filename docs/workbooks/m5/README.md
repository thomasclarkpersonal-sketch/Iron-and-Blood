# Project Workbook: the m5 run

> [!NOTE]
> **Status: planning record** for the "m5" run, kept by the SDLC workflow (docs/SDLC_WORKFLOW.md). Not binding: the contract wins, and each fact moves to its home as the work lands.

The ongoing record of the run that plans and builds [Milestone 5](../../MILESTONE_5.md): what it delivers, what was said and decided, and how each review went. The run's log and its morning report are in `out/m5-run/` in the maintainer's checkout, which git ignores.

## Deliverable register

| Deliverable | File | SDLC phase | Status | Last reviewed at |
|---|---|---|---|---|
| Project Workbook | `README.md` (this page) | 1. Initiation; kept up to date all run | draft | under review: Initiation, round 1 |
| System Service Request | [01-system-service-request.md](01-system-service-request.md) | 1. Initiation | draft | under review: Initiation, round 1 |
| Project Charter | [02-project-charter.md](02-project-charter.md) | 1. Initiation | draft | under review: Initiation, round 1 |
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

## Change requests

A change to a task's contract after the plan is approved, and who decided it.

| Id | Task | Change | Reason | Decision | Decided by |
|---|---|---|---|---|---|
| — | — | None yet | — | — | — |

## Review log

| Stage or task | Round | Commit | Verdict | Findings, and how each was handled |
|---|---|---|---|---|
| Initiation | 1 | `bd08ee6` (decisions recorded) and this round's workbook commit | pending | — |

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
