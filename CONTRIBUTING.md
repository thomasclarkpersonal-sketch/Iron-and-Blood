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
