# How These Docs Are Organised

Every fact has **one home**. Other documents link to it rather than restating it, because a restated fact is a fact that drifts. `scripts/check_docs.py` (CI) checks the parts of this that a script can: links and their headings, decision citations, and the lists that name crates, systems and CLI commands.

## Where each kind of fact lives

| Kind of fact | Home | Not here |
|---|---|---|
| Rules code must follow, and why | [DECISIONS.md](DECISIONS.md): the binding contract, enforced by the critic | Mechanism, flags, measurements |
| Rules for contributors and agents | [AGENTS.md](../AGENTS.md) | Design |
| How a system works: maths, algorithm, tuning | The system document: [ECONOMY_SYSTEM](ECONOMY_SYSTEM.md), [POP_SYSTEM](POP_SYSTEM.md), [POLITICS_SYSTEM](POLITICS_SYSTEM.md), [MACROECONOMICS](MACROECONOMICS.md), [MAP_AND_LOGISTICS](MAP_AND_LOGISTICS.md) | Rules (link the decision) |
| The tick's system order | D4. [ARCHITECTURE.md](ARCHITECTURE.md#-the-game-loop) mirrors it with each step's module, and changes with it | Anywhere else: link to D4 |
| Rust structure: crates, tables, columns, modules | [BACKEND_SCHEMA.md](BACKEND_SCHEMA.md); field names are the code's | |
| File formats | [DATA_FORMAT.md](DATA_FORMAT.md) | |
| The wire protocol | the schemas in `schemas/`, explained by [NETWORK_PROTOCOL.md](NETWORK_PROTOCOL.md) | |
| Measured numbers | [PERFORMANCE.md](PERFORMANCE.md) | Decisions (they hold budgets, not measurements) |
| What is being built, in what order | the milestone documents (`MILESTONE_<n>.md`) | Design that outlives the milestone |
| Ideas not yet designed | vision documents, under a status banner (below) | |
| Options surveyed | [research/](research/) | |
| A feature's planning record (SSR, charter, specifications, baseline plan, diagrams) | its Project Workbook, `docs/workbooks/<slug>/`, under a status banner ([SDLC_WORKFLOW.md](SDLC_WORKFLOW.md#planning)) | Rules and mechanism: they move to DECISIONS.md and the system documents as the feature lands |
| How to use it | [ONBOARDING.md](ONBOARDING.md), [HOSTING.md](HOSTING.md), [REPO_SETUP.md](REPO_SETUP.md), [CLAUDE_FEATURE_PIPELINE.md](CLAUDE_FEATURE_PIPELINE.md), [SDLC_WORKFLOW.md](SDLC_WORKFLOW.md) | |
| The reusable SDLC workflow, for this and other repositories | [templates/sdlc-overnight/](../templates/sdlc-overnight/README.md): the script, guide, configuration reference, starter files and tests | This repository's settings: `.claude/sdlc-overnight.json` |

## Status of what a document describes

A system document can describe what exists, what is decided but not built, and what is only an idea. Readers, and agents especially, must be able to tell which is which:
- **Implemented** text describes the code and cites its decision.
- **Planned** text is labelled *planned* and names the decision or proposal it waits on (a *Proposed* entry in DECISIONS.md, or a proposal PR), not a milestone number. When nothing is drafted yet, it says so: *needs a decision, none drafted*. Milestones get renumbered and re-scoped; a decision number doesn't move.
- **Vision** documents, or vision sections, describe intended play before any design. They open with a status banner saying so and listing the decisions they need, like [COLONIZATION_SYSTEM.md](COLONIZATION_SYSTEM.md). Nothing in one is binding, and an agent never implements from one directly.

## Milestone lifecycle

1. **Planned:** a milestone document with its goal, scope, tasks and definition of done. Design it depends on lands first as **Proposed** entries in DECISIONS.md, on `main`, so it can be read and linked; a proposal on an unmerged branch can't be.
2. **Active:** tasks are ticked off as they merge. A task's row says what was done in a sentence or two; details belong in the PR and in the system documents.
3. **Closed:** the status line says so and the date. After that the document is a record and stays as it was. Anything left over moves to a GitHub issue or the next milestone's carried-over list.

**Temporary measures** (a limit, a band-aid) are recorded in the milestone's follow-ups with the condition for removing them, and the code that implements one names that follow-up in its doc comment, so it can be found and removed. A temporary measure that changes simulation results also needs a decision marked temporary (DECISIONS.md, "What an entry holds").
