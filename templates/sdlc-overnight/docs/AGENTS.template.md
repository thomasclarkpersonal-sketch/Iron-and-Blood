# AI Agent Guidelines (AGENTS.md)

<!-- A starting point for a repository's contract: the rules every agent and contributor
follows, and the rules the critic enforces as CRITICAL. Replace every <placeholder>, delete
what doesn't apply, and keep it short: a rule nobody can check is a wish. -->

These rules apply to every contributor, human or agent. Decisions that bind the design live in `<docs/adr/ or docs/DECISIONS.md>`; if anything here conflicts with a recorded decision, the decision wins.

## 1. Architecture
- **Structure:** <the modules, packages or services and what each may depend on>.
- **Boundaries:** <what must never cross a boundary: for example, no database access outside the repository layer>.
- **Patterns:** <the patterns to use and the ones to avoid, with the reason>.

## 2. Correctness rules
- <Invariants the code must keep: for example, money is integer cents; every input is validated at the edge>.
- <Rules about concurrency, determinism, error handling>.

## 3. Testing
- Every behaviour change comes with tests; every bug fix with a test that failed before it.
- <Which test suites exist and what each must cover>.
- The gate (`gate.full` in `.claude/sdlc-overnight.json`) passes before every push.

## 4. Documentation
- **Docs first:** documents that describe intended behaviour change in the same change as the code, ahead of it.
- **One home per fact:** <where each kind of fact lives: users in README.md, structure in docs/architecture.md, decisions in docs/adr/>. Link instead of restating.
- **Label what isn't built:** unbuilt work is called planned and names the decision it waits on.
- Doc comments explain the why, not only the what.

## 5. Commits and pull requests
- Commit messages follow the shape in `docs/SDLC_WORKFLOW.md` (Why, What, Evidence, Docs, Decisions).
- Stage explicit paths; never sweep a whole directory into a commit.
- Never push to `<main>`, force-push, or bypass branch protection.

## 6. Review
- The critic (`.claude/commands/critic.md`) reports CRITICAL findings (contract violations, which must be fixed), DEBT (fixed in the same change or waived by a maintainer with a reason) and SUGGESTIONS (optional).
- Only maintainers waive findings or approve changes to this contract. Agents never do.
