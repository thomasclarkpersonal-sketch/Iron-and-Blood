# Repository Setup (Admin Guide)

These are one-time steps for the repository admin. They set up GitHub hosting, CI, releases and the blocking AI critic. Steps 1–6 must be done in order; step 7 needs a CI run to exist first.

## 1. Make the first commit locally

Run these in the project folder (WSL):

```bash
git config --global user.name "Your Name"
git config --global user.email "you@example.com"
git add -A
git status          # check: no PDFs, no target/, no .env files listed
git commit -m "Initial Milestone 1: engine, data loader, CLI, design docs, CI/CD"
```

## 2. Create the GitHub repository and push

1. On GitHub, go to **New repository**. Choose a name and set it to **Private** (the code is unlicensed).
2. Do **not** add a README, .gitignore or licence; the repo already has them.
3. Push:

   ```bash
   git remote add origin git@github.com:<org-or-user>/<repo>.git
   git push -u origin main
   ```

The push triggers the first **CI** run. Check the **Actions** tab: all four CI jobs should be green.

## 3. Allow the actions the workflows use

Go to **Settings → Actions → General**.

- Under **Actions permissions**, choose either:
  - **Allow all actions and reusable workflows**, or
  - **Allow select actions**, and list these:
    ```
    actions/checkout@*, actions/upload-artifact@*, actions/download-artifact@*,
    dtolnay/rust-toolchain@*, Swatinem/rust-cache@*, anthropics/claude-code-action@*
    ```
- Under **Workflow permissions**, keep **Read repository contents** (the default). Each workflow asks for any extra permission it needs explicitly.

## 4. Install the Claude GitHub App

The critic runs through Anthropic's Claude Code GitHub Action, which needs the Claude GitHub App.

1. Run `/install-github-app` in Claude Code inside this repo.
   - Your `gh` login needs the `workflow` scope; if the installer complains, run `gh auth refresh -s workflow` first.
   - The installer also stores a **`CLAUDE_CODE_OAUTH_TOKEN`** repository secret (usage is billed to the installing user's Claude subscription). `critic.yml` uses this secret.
   - It may push its own workflow files on an `add-claude-github-actions-…` branch. `claude.yml` (`@claude` mentions) is already in this repo. **Do not merge `claude-code-review.yml`**: it reviews every PR and duplicates `critic.yml`. Delete that branch.
2. Manual alternative: open https://github.com/apps/claude, click **Install**, select this repository only, then do step 5.

## 5. Optional: bill an API key instead of a subscription

Skip this if step 4 created `CLAUDE_CODE_OAUTH_TOKEN`. Use an API key when the critic's cost should not count against one person's subscription (recommended once several people open PRs).

1. Create a key at https://console.anthropic.com → **API Keys**. Use a dedicated key for CI.
   - Recommended: set a monthly **spend limit** on the workspace it belongs to. Each review runs Opus 5.5 with up to 40 turns.
2. On GitHub: **Settings → Secrets and variables → Actions → New repository secret**.
   - Name: `ANTHROPIC_API_KEY`
   - Value: the key
3. In `.github/workflows/critic.yml`, replace the `claude_code_oauth_token:` line with `anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}` (the comment above it shows the exact line).

## 6. Create the critic labels

```bash
gh label create critic --color B60205 --description "Force an architectural critic review"
gh label create contract-change --color D93F0B --description "Maintainer approves relaxing AGENTS.md / DECISIONS.md rules"
```

or in the UI: **Issues → Labels → New label**.

## 7. Run the critic once (needed before it can be required)

GitHub only lets you mark a check as required after it has run at least once.

1. Create a branch with a small change and open a pull request.
2. Add the `critic` label to the PR.
3. Under the PR's **Checks**, confirm that **Critic** runs and posts a "🧐 Architectural critic" comment.

## 8. Protect `main`: require CI and block merges on CRITICAL findings

Go to **Settings → Rules → Rulesets → New ruleset → New branch ruleset**.

| Setting | Value |
|---|---|
| Ruleset name | `main protection` |
| Enforcement status | **Active** |
| Bypass list | **Repository admin** role only (the emergency override, see below) |
| Target branches | **Add target → Include default branch** |
| Restrict deletions | ✅ |
| Block force pushes | ✅ |
| Require a pull request before merging | ✅, with **Required approvals: 1** and **Dismiss stale approvals when new commits are pushed** |
| Require status checks to pass | ✅, then **Add checks** (see below) |
| Require branches to be up to date before merging | ✅ (recommended: the determinism gate must pass on the merged result) |

Required status checks, as GitHub lists them:
- `Format, clippy, docs`
- `Tests and determinism gate`
- `Determinism (windows-latest)`
- `Determinism (macos-latest)`
- `Benchmark regression`
- `Critic`

Click **Create**. (`Benchmark regression` runs on every PR, so requiring it never leaves a PR waiting.)

How the blocking works:

| PR situation | `Critic` check | Merge |
|---|---|---|
| Not selected (under 200 lines and 20 files, not sampled, no label) | skipped, which counts as passing | allowed |
| Selected, no CRITICAL findings, open DEBT | ✅ passes with a warning; the comment asks for the debt to be fixed or waived | allowed (but see AGENTS.md §9) |
| Selected, no CRITICAL findings, no open DEBT | ✅ passes | allowed |
| Selected, CRITICAL findings | ❌ fails | **blocked** until fixed; the critic re-runs on every push |
| Critic errored or produced a malformed report | ❌ fails (fail-closed) | blocked; use **Re-run jobs** |
| Draft PR or PR from a fork | skipped | allowed. Keep contributors on in-repo branches (forks get no secrets, so they can't be reviewed) |

## 9. Optional: protect release tags

**Settings → Rules → Rulesets → New tag ruleset**:
- target pattern `v*`;
- enable **Restrict creations**, **Restrict updates** and **Restrict deletions**;
- bypass list: admins.

Only admins can then publish releases (`git tag v0.1.0 && git push origin v0.1.0`).

## Handling a disputed CRITICAL finding

The critic is an AI reviewer and can be wrong. If a team member believes a finding is a false positive:

1. They reply on the PR with their reasoning, citing the rule in `AGENTS.md` or `docs/DECISIONS.md`.
2. An admin reviews it.
   - If the critic is wrong, the admin merges using the ruleset bypass ("Merge without waiting for requirements to be met").
   - If the critic misreads a rule, fix the wording in `.claude/commands/critic.md` or `docs/DECISIONS.md` so it doesn't happen again.
3. Do **not** remove the `critic` label to dodge a review. On a small PR, the next push would then skip the check and unblock the merge. Treat that as a process violation.

## Waiving DEBT and approving contract changes

**DEBT waivers.** DEBT findings don't block, but the critic re-reports them on every push until they're fixed. If one should not be fixed in this PR, a maintainer (anyone with write access) comments on the PR:

```text
critic-waive: Unbounded retry loop in parse_commands, tracked in #42
```

The waiver must name the finding's title and give a reason. On the next run (push, or **Re-run jobs**), the critic lists the finding under *Waived debt*, and it no longer counts as open. The critic sees only `critic-waive` lines from people with write access and ignores the rest of the thread. A waiver can never clear a CRITICAL finding.

**Contract changes.** The critic judges a PR against `main`'s `AGENTS.md`, `docs/DECISIONS.md` and `.claude/commands/critic.md`, so a PR can't change the rules it is judged by.
- Adding a new decision is normal.
- Removing, relaxing or contradicting an existing rule is CRITICAL.
- If the change is intended, add the `contract-change` label. The critic re-runs and reports the relaxation as DEBT instead, so it stays visible.

PRs that touch these files are always reviewed.

## PRs that change `critic.yml` or `claude.yml`

For safety, the Claude action refuses to run a workflow whose file differs from the version on `main`. A PR that edits `critic.yml` therefore gets no review, and the `Critic` check fails closed. This is intentional: otherwise, editing the workflow would be a way to skip review. An admin reviews such PRs by hand and merges them with the ruleset bypass.

## Troubleshooting

If the `Critic` check fails with "The critic produced no report", open the job and read the **Explain critic failure** step, which shows Claude's error message.

| Message | Fix |
|---|---|
| Invalid API key / authentication error / OAuth token expired | Regenerate with `claude setup-token`, then `gh secret set CLAUDE_CODE_OAUTH_TOKEN` |
| Rate limit / usage limit | The subscription's usage limit was hit. Re-run later, or switch to an API key (step 5) |
| Model not available | Change `--model` in `critic.yml` |

## Tuning

These values are at the top of `.github/workflows/critic.yml`:

| Variable | Default | Meaning |
|---|---|---|
| `CRITIC_MIN_LINES` | 200 | Additions + deletions that make a PR "big" |
| `CRITIC_MIN_FILES` | 20 | Changed files that make a PR "big" |
| `CRITIC_SAMPLE_ONE_IN` | 5 | Random sampling rate for smaller PRs |

To cut cost, change `--model claude-opus-5-5` to `claude-sonnet-5-5` in the same file.
