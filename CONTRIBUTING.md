# Contributing

## Setup

The toolchain is pinned in `rust-toolchain.toml`; `rustup` installs it automatically. Develop inside WSL or Linux.

```bash
cargo build --release
cargo test
cargo run --release -p pax_cli -- run scenarios/mini_valley --days 365
```

## Before every pull request

```bash
cargo fmt --all
cargo clippy --all-targets --release -- -D warnings
cargo test --all
cargo run --release -p pax_cli -- verify scenarios/mini_valley
```

CI runs the same commands, and also the determinism gate on Windows and macOS.

## CI/CD pipelines (`.github/workflows/`)

| Workflow | Runs on | What it does |
|---|---|---|
| `ci.yml` | every PR and push to `main` | fmt, clippy, rustdoc, debug and release tests, the determinism gate (Linux at 1 and 4 threads, plus Windows and macOS), and a 1M-POP benchmark smoke run |
| `critic.yml` | selected PRs | AI architectural review with [`/critic`](.claude/commands/critic.md), posted as a PR comment. **CRITICAL findings fail the `Critic` check and block the merge** |
| `claude.yml` | a comment, issue or review mentioning `@claude` | Claude answers questions or investigates on request (read-only repo access) |
| `claude-feature.yml` | label `claude-plan` / `claude-implement` on an issue | Claude plans a feature, then writes docs, code and tests on `claude/issue-<n>` ([guide](docs/CLAUDE_FEATURE_PIPELINE.md)) |
| `release.yml` | pushing a tag `vX.Y.Z` | tests and determinism gate on 3 platforms, then a GitHub Release with `pax_cli` binaries plus `data/` and `scenarios/` |

**The critic reviews a PR if any of these is true:**
- it changes ≥ 200 lines or ≥ 20 files;
- it was randomly sampled (1 in 5 PRs, decided once per PR number);
- it carries the `critic` label.

A selected PR is re-reviewed on every push. DEBT warnings and suggestions are advisory; only CRITICAL findings block. If you believe a CRITICAL finding is wrong, reply on the PR citing the rule and ask an admin. Do not remove the `critic` label to dodge a review.

Repository admins: the one-time setup (GitHub App and its token secret, label, ruleset making the checks required) is in [docs/REPO_SETUP.md](docs/REPO_SETUP.md).

**Cutting a release** (admins):

```bash
git tag v0.1.0 && git push origin v0.1.0
```

`v0.x` and `-rc` tags are published as pre-releases.

## Golden hashes: when `verify` fails

`verify` failing means simulation results changed.

- **Unintended** (a refactor, a performance change): you introduced a behaviour change or non-determinism. Fix it; do not re-record.
- **Intended** (new rule, retuned data): re-record and commit the file in the same PR, and say so in the PR description:

  ```bash
  cargo run --release -p pax_cli -- record scenarios/mini_valley
  ```

Reviewers should treat an unexplained `golden.hashes` diff as a blocking issue.

## Rules of the codebase

Read [AGENTS.md](AGENTS.md); it applies to humans too. In short:

1. **No floats in simulation state.** Use `Fixed`. Clippy's `float_arithmetic` lint is on.
2. **No `std::HashMap`/`HashSet` in simulation state or iteration.** Use dense `Vec`s, `BTreeMap`, or `groups::Groups`.
3. **Money moves, it is never created.** Debit and credit the same amount; split totals with `alloc::allocate`.
4. **`pax_engine` does no IO.** Files belong in `pax_data`; output belongs in `pax_cli`/`pax_server`.
5. **Docs move with code.** If you change a rule, update [docs/DECISIONS.md](docs/DECISIONS.md) and the relevant system document in the same PR.

## Branches and commits

- Work on feature branches off `main` and merge through pull requests with one approving review.
- Write commit messages in the imperative mood ("Add tariff transfer"), and give simulation-affecting commits a `[sim]` prefix so they are easy to find when bisecting a desync.
