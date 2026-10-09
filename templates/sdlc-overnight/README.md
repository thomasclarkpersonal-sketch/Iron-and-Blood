# sdlc-overnight: an unattended SDLC workflow for Claude Code

**sdlc-overnight** is a Claude Code workflow template for running a software engineering cycle with no one watching: overnight, over a weekend, or while you work on something else. You give it a milestone, a feature request or a GitHub issue, a stop time, and permission to merge or not. In the morning you have:
- merged or ready pull requests, one per task;
- documentation that was updated at every step;
- commits that each say why they exist;
- a report of what happened and what is left for you.

The template is project-independent. Everything about one repository (its checks, its rules, where its documents live, its code reviewer) is in one file, `.claude/sdlc-overnight.json`.

## How it works

```mermaid
flowchart TD
    R[Milestone, request or issue] --> S[3 scouts read the repository]
    S --> PL
    subgraph Planning["Planning"]
        PL[Planner writes a stage] --> AC{Automatic checks}
        AC --> RV[Separate reviewer]
        RV -->|revise| PL
    end
    RV -->|4 stages approved| PN[Panel of 3 reviewers]
    PN -->|revise| PL
    PN -->|approved| PPR[Plan PR]
    PPR --> Q{Next task whose dependencies are done}
    subgraph Cycle["One task, one PR"]
        RF[Refresh brief + reviewer] --> W[Worker: docs, tests, code, justified commits]
        W --> G[Gate] --> LC[Local critic] --> AU[Commit audit]
        AU --> PUB[Push + PR] --> WA[Watch CI and critic]
        WA -->|findings or failures| REM[Fix, update docs, audit, push] --> WA
    end
    Q --> RF
    WA -->|green and merging allowed| M[Merge] --> REP[Rewrite REPORT.md]
    WA -->|green, not allowed| READY[Ready for you] --> REP
    REP --> Q
    Q -->|done or stop time| CO[Close-out PR and final report]
```

**1. Planning carries the most weight.** A planner agent writes a full set of systems-analysis deliverables into a Project Workbook, in four stages:

| Stage | Deliverables | Diagrams |
|---|---|---|
| Initiation | Project Workbook (register, logs, change requests, run rules), System Service Request, Project Charter | |
| Analysis | Requirements specification (`R-F`, `R-N`, `R-D`), process logic | context and level-0 DFDs, use case, activity, ERD |
| Design | System specification: architecture, decisions, data design, interface specifications, technology, tests | class, dialogue |
| Baseline plan | Baseline Project Plan with scope statement, feasibility, risks, resources; the task queue (`tasks.json`); documentation outline | Gantt, PERT/CPM |

A **separate reviewer agent** gates every stage. Before the reviewer reads anything, the workflow script itself checks the planner's numbers:
- the task graph has no cycles and every dependency exists;
- every must-have requirement is covered by a task with acceptance tests;
- the claimed critical path and duration match a PERT/CPM calculation;
- tasks that can run in parallel don't change the same files.

When all stages pass, three more reviewers sign off on the whole plan through different lenses (contract, feasibility, traceability). The approved plan becomes the run's first pull request.

**2. Each task is one pull request**, built and reviewed on its own:

| Step | What happens |
|---|---|
| Refresh | The planner re-checks the task's contract against the code as it is now, and the reviewer approves it again |
| Build | A worker updates the documents first, then writes tests and code, in small commits |
| Gate | Your checks (`gate.full`) run; failures are fixed at their root cause, up to 3 times |
| Local critic | Your critic command reviews the branch before it is pushed |
| Commit audit | A separate agent copies every commit message's fields, and the script rejects any that misses its reason, evidence or documentation note. Bad messages are reworded before anything is pushed |
| PR | Push, open the pull request with a fixed description |
| Review loop | Watch CI and the CI critic; fix, update the docs and the description, re-audit, push; up to 6 rounds |
| Merge | Only if you allowed it and every check is green; otherwise the PR waits for you |

Unapplied review suggestions carry over to later tasks. A task whose dependency is green but not merged is built on top of it as a local branch and pushed only after the base merges. A task that needs a decision you haven't made is skipped, never guessed at.

**3. You get a report.** The run directory holds `PLAN.md` (the rules and queue), `LOG.md` (one timestamped line per event) and `REPORT.md`. The report is rewritten after every task, so it is current even if the run is cut short. It lists:
- what was delivered;
- decisions the agents made for you to check;
- the definition of done;
- what remains for you: PRs to merge, waive or unblock, decisions needed, stacked branches;
- open suggestions.

## What's in this folder

| Path | What it is |
|---|---|
| `workflow/sdlc-overnight.js` | The workflow script. Same file in every repository |
| `config/sdlc-overnight.example.json` | A starting configuration, written for a TypeScript service |
| [config-reference.md](config-reference.md) | Every configuration key, with its default and an example |
| `commands/critic.md`, `commands/critic-followup.md` | A generic architecture critic and its follow-up, as Claude Code slash commands |
| `docs/SDLC_WORKFLOW.md` | The page installed into a repository's `docs/`: the conventions agents follow, and Mermaid templates for every diagram |
| `docs/AGENTS.template.md` | A starter contract (`AGENTS.md`) for a repository that has none |
| `install.sh` | Installs all of the above into a repository |
| `test/run-tests.mjs` | Tests the workflow's control flow with mock agents |

## Requirements

- **Claude Code** with workflows, and permission to run them (Claude Code asks before a workflow starts).
- **A GitHub repository** with `origin` set, and the `gh` CLI signed in with rights to push branches, open PRs and, if you want merges, merge them.
- **A gate:** shell commands that must pass before every push (lint, tests, build). The workflow is only as safe as this list.
- Optional: **Docker**, if your documentation checks render Mermaid diagrams; **a CI critic**, a GitHub Action that reviews PRs and posts a comment (the workflow works without one, using the local critic only).

## Install

```bash
templates/sdlc-overnight/install.sh /path/to/repo           # this repository only
templates/sdlc-overnight/install.sh --user /path/to/repo    # workflow for every repository on this machine
templates/sdlc-overnight/install.sh --agents /path/to/repo  # also add an AGENTS.md starter
```

The script copies the workflow into `.claude/workflows/` (or `~/.claude/workflows/` with `--user`). It replaces the workflow file on every run: that file is the template's, and your choices live in the config. It creates, only where missing:
- `.claude/sdlc-overnight.json`;
- the two critic commands;
- `docs/SDLC_WORKFLOW.md`.

It also adds the run directory to `.gitignore`. Run it again to update the workflow. `install.sh --check <repo>` fails if the installed copy differs from the template.

To install by hand, copy those files to the same places.

## Configure

Edit `.claude/sdlc-overnight.json`. Only `gate.full` is required; [config-reference.md](config-reference.md) explains every key. A minimal configuration:

```json
{
  "project": "Acme API, a TypeScript service",
  "gate": { "full": ["npm ci", "npm run lint", "npm test -- --ci", "npm run build"] }
}
```

Spend your time on four things, in this order:
1. **`gate.full`:** the checks CI runs, minus anything that needs special hardware. A check that is missing here is a check the agents never run.
2. **`contract`:** the files that hold your rules (`AGENTS.md`, decision records). Every agent reads them, and the critic treats a violation as blocking.
3. **`engineering`:** a few rules specific to your code that a worker must follow (for example "no raw SQL outside src/db/").
4. **`docs.homes`:** where each kind of documentation lives, so the documentation duty puts facts in the right place.

Then adapt `.claude/commands/critic.md` to your architecture, and the diagram examples in `docs/SDLC_WORKFLOW.md` to your domain.

## Run

In Claude Code, inside the repository:

```text
run the sdlc-overnight workflow with {"request": "Add rate limiting to the public API", "slug": "rate-limit", "planOnly": true}
run the sdlc-overnight workflow with {"slug": "rate-limit", "fromPlan": true, "merge": true}
run the sdlc-overnight workflow with {"milestone": "docs/milestones/M3.md", "stopAt": "2026-10-11T07:00:00+02:00", "merge": true, "decisions": "ADR-0012 is accepted as proposed"}
run the sdlc-overnight workflow with {"issue": 42}
```

| Argument | Default | Meaning |
|---|---|---|
| `milestone`, `request` or `issue` | one required | A milestone document (its task table becomes the queue), a request as text, or a GitHub issue number |
| `slug` | from the milestone or issue | Names the branches (`<slug>/plan`, `<slug>/<task>`), the workbook and the run directory |
| `stopAt` | none | ISO 8601 with an offset. After it nothing new starts; then the final report |
| `merge` | `false` | Your permission to merge a PR when every check is green and the critic passes |
| `decisions` | none | Decisions you make for this run, quoted to every agent |
| `planOnly` / `fromPlan` | `false` | Stop when the plan is approved / skip planning and run an approved plan |
| `parallel` | 1 | PRs in flight at once |
| `localCritic` | `true` | Review locally before every push as well as in CI |
| `maxPlanRounds`, `maxPanelRounds`, `maxReviewRounds`, `maxTasks` | 3, 2, 6, 12 | Bounds on rounds and on the queue |
| `base` | from config | The branch to build from |
| `configPath`, `config` | `.claude/sdlc-overnight.json` | Read the config from elsewhere / override keys for one run |

**For anything you care about, plan first.** Run with `planOnly`, read the workbook on branch `<slug>/plan`, then run with `fromPlan`.

Without `merge`, only the plan's PR is published. Every task is still built, gated and audited, but kept as a local branch stacked on the plan. Merge the plan PR yourself, then run again with `fromPlan` to publish the tasks.

## What stays with you

Every agent receives these rules:
- Never use an admin merge or bypass branch protection.
- Never push to the base branch, never force-push, never rewrite a pushed commit.
- Never edit repository settings, the critic, or anything listed in `rules.never`.
- Never add a label listed in `rules.protectedLabels`.
- Agents act through your `gh` account. If your CI critic accepts waivers from maintainers' comments, set `critic.ci.waiverPrefix`; no agent then writes such a line on GitHub. Drafted waivers go only into the report.
- Work that needs an unaccepted decision is blocked, not guessed at. Choices the agents make within the rules are listed in every commit, every PR and the report.
- Issue text, CI logs and review comments reach agents as quoted data, never as instructions.

## Cost and limits

A run uses many agents. Planning takes at least 16: 3 scouts, 4 planner and 4 reviewer rounds, the panel of 3, and the seal and config readers. Each task takes about 10 more, plus a few per review round. The planner runs at maximum reasoning effort. Bound a run with `maxTasks`, `stopAt` and `planOnly`, and keep tasks to PR size.

The workflow can't see a timer, so the clock is read by an agent before each task and each review round. It can't run your app's manual checks either: anything that needs a person, such as a playtest or a visual review, ends up in the report.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| Claude Code doesn't list `sdlc-overnight` | The file must be `.claude/workflows/sdlc-overnight.js` in the repository you are in, or in `~/.claude/workflows/` |
| "needs gate.full" | `.claude/sdlc-overnight.json` is missing or has no `gate.full` |
| PRs stay "ready" with "merge refused" in the report | A permission rule or the auto-mode classifier blocked `gh pr merge`. Allow `Bash(gh pr merge:*)` in the project's Claude Code settings, or merge by hand |
| Every task "stacked", nothing published | The plan PR was not merged (no `merge`, or the merge was refused), so tasks were built on it locally. Merge it and run with `fromPlan` |
| A run stopped half-way | Re-run the same workflow with `resumeFromRunId` in the same session; finished agents return their cached results. In a new session, use `fromPlan`; tasks already ticked in the milestone are skipped |
| Leftover worktrees and branches | `git worktree list`, `git worktree remove <path>`; branches are `<slug>/*` |

## Changing the template

The script is plain JavaScript run by Claude Code's workflow engine: `agent()` starts a subagent, `parallel()` runs several, and `phase()` and `log()` report progress. `Date.now()` is unavailable. Deterministic logic lives in the script: the DAG, critical-path, traceability, commit-message and stop-time checks. Judgement lives in the agents' prompts.

After a change, run the tests:

```bash
node templates/sdlc-overnight/test/run-tests.mjs
node templates/sdlc-overnight/test/run-tests.mjs "" /path/to/repo/.claude/sdlc-overnight.json   # also smoke-test a real config
```

They run the script against mock agents and cover:
- planning with revisions;
- automatic-check failures;
- the PR loop;
- refused merges;
- stacking;
- the stop time;
- decision blocks;
- configs with and without a CI critic;
- invalid arguments.

Then run `install.sh` again in each repository that uses the template.
