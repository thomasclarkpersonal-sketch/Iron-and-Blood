# sdlc-overnight configuration reference

The workflow reads `.claude/sdlc-overnight.json` from the repository root when it starts (another path with the `configPath` argument). Keys you leave out take the defaults below, and nested objects merge key by key. A run can override any key with the `config` argument, for example `{"config": {"gate": {"full": ["make quick"]}}}`. Lists are replaced, not merged.

Text values are pasted into agents' prompts, so write them as instructions to a careful colleague: specific, short, and checkable.

## Top level

| Key | Default | Meaning |
|---|---|---|
| `project` | `"this project"` | One line naming the project and its stack. Agents see it in every role |
| `base` | `"main"` | The branch tasks start from and PRs target. The `base` argument overrides it |
| `runDir` | `".sdlc-runs/<slug>"` | Where `PLAN.md`, `LOG.md` and `REPORT.md` go, relative to the repository root. Must be git-ignored; `install.sh` takes care of it |
| `sharedFiles` | `[]` | Files every task may change (lock files, changelogs). Exempt from the parallel-ownership check |
| `pastRuns` | `[]` | Reports of earlier runs for the risk scout to learn from. Reports in `runDir` are found anyway |

## `contract`: the rules

| Key | Default | Meaning |
|---|---|---|
| `files` | `["AGENTS.md"]` | The files that hold the rules every contribution follows. Every agent is told they win over anything else |
| `decisions` | `null` | The decision record (a file such as `docs/DECISIONS.md`, or an ADR index). With one set, work that needs an unaccepted decision is blocked, proposed decisions are added to it as Proposed, and documents cite it |
| `decisionCitation` | `"its decision id"` | How a decision is cited, as an instruction: `"its ADR number (ADR-0007)"` |

## `docs`: where documentation lives

| Key | Default | Meaning |
|---|---|---|
| `index` | `null` | A document that says where each kind of fact lives, if you have one |
| `homes` | `"the README and the docs/ directory"` | The same in one sentence, used by the documentation duty: `"README.md for users; docs/architecture.md for structure; docs/api.md for endpoints"` |
| `workbookDir` | `"docs/workbooks"` | Where each run's Project Workbook goes (`<workbookDir>/<slug>/`) |
| `workflowDoc` | `"docs/SDLC_WORKFLOW.md"` | The page with the diagram conventions and commit rules (the template's `docs/SDLC_WORKFLOW.md`) |
| `milestones` | `"docs/MILESTONE_<n>.md"` | The milestone documents' path pattern; `<n>` is the number. A matching `milestone` argument gives the slug |
| `milestoneSlugPrefix` | `"m"` | Slug prefix for a milestone run: `docs/MILESTONE_5.md` gives `m5` |
| `milestoneRow` | tick the task's row with a sentence | What a task does to the milestone document when it finishes |

## `gate`: the checks

| Key | Default | Meaning |
|---|---|---|
| `full` | `[]` (**required**) | Commands that must all pass before every push, run in order from the repository root. Mirror your CI |
| `docs` | `[]` | The documentation checks: the whole gate for the plan's PR, and run by planners and reviewers after writing documents. Link checkers and diagram renderers belong here |
| `conditional` | `[]` | Checks that apply only sometimes, written as instructions: `"If src/db/migrations/ changed: npm run db:migrate:check"` |
| `env` | `[]` | Shell lines run before any command, for example a shared build cache: `"export CARGO_TARGET_DIR=\"$ROOT/target/sdlc\""`. `$ROOT` is the main checkout |

Agents report a check that can't run (a missing tool) as skipped, never as passed. Keep `full` to what a machine can run unattended.

## `engineering`, `design`, `risks`: project knowledge

| Key | Default | Meaning |
|---|---|---|
| `engineering` | `[]` | Rules every worker follows while coding, one per entry: `"Every endpoint validates its input with the shared zod schemas."` |
| `design.analysis` | `""` | Added to the requirements stage: how to state process logic for this project (units, types, rounding) |
| `design.erd` | `""` | How to draw entity–relationship diagrams for this project's data model |
| `design.classDiagram` | `""` | How to draw class diagrams when the code isn't object-oriented (for example, data-oriented tables) |
| `design.architecture` | `""` | What the architecture section must cover (layering, scheduling, concurrency) |
| `design.feasibility` | `""` | Recurring costs to weigh (a performance budget, a cloud bill) |
| `design.tests` | `""` | Tests the design must plan for beyond unit tests |
| `risks` | generic list | What the risk scout looks for: `["database migrations on large tables", "breaking changes to public endpoints"]` |

## `regression`: a baseline that must not change by accident

`null` by default. For projects with snapshot tests, golden files or recorded hashes:

```json
"regression": {
  "name": "Golden hashes",
  "verify": "make golden-verify",
  "rerecord": "make golden-record"
}
```

The plan states whether the run changes the baseline on purpose. If it doesn't, a failing `verify` is treated as a bug and re-recording is forbidden. PR descriptions get a section named after it.

## `critic`: the code reviewer

| Key | Default | Meaning |
|---|---|---|
| `command` | `".claude/commands/critic.md"` | The critic's rules, run locally before each push. `null` makes the workflow use built-in generic rules |
| `followup` | `".claude/commands/critic-followup.md"` | How findings are triaged and fixed |
| `ci` | `null` | Your CI critic, if a GitHub Action reviews PRs and comments. Without one, the watcher treats the critic as skipped and relies on the local review |

`ci` has these keys:

| Key | Meaning |
|---|---|
| `check` | The name of the CI check the critic reports as (`"Critic"`) |
| `marker` | The text its comment starts with (`"<!-- my-critic -->"`), so imitations are ignored |
| `author` | The comment's author (`"github-actions"`) |
| `label` | A label that forces the critic to run on a PR; added to every PR the run opens. Optional |
| `waiverPrefix` | If your critic accepts waivers from maintainers' comments (`"critic-waive"`), set it: no agent then writes a line starting with it on GitHub, because agents act through your account |

## `rules`, `merge`, `labels`: what agents may do

| Key | Default | Meaning |
|---|---|---|
| `rules.never` | `[]` | Extra things no agent may do, added to the built-in list (admin merges, bypasses, pushing to the base, force-pushing, rewriting pushed commits, editing settings or the critic command): `"editing .github/workflows/"` |
| `rules.protectedLabels` | `[]` | Labels only a maintainer may add (`"contract-change"`) |
| `merge.method` | `"squash"` | `squash`, `merge` or `rebase`, passed to `gh pr merge` |
| `labels.needsHuman` | `"needs-human"` | Label for a PR the run couldn't finish |
| `labels.needsWaiver` | `"needs-waiver"` | Label for a PR whose remaining findings are debt left with a reason |
