# Onboarding: Your First Week on Iron and Blood

A practical tour for new contributors. The rules live in [AGENTS.md](../AGENTS.md) and [DECISIONS.md](DECISIONS.md); this page shows where things are and how to make common changes without breaking them.

## 1. Run it (10 minutes)

```bash
cargo test                                                                     # everything, ~1 min
cargo run --release -p pax_cli -- run scenarios/two_states --days 720 --market highland
cargo run --release -p pax_cli -- report scenarios/two_states --days 7200 --every 720
cargo run --release -p pax_cli -- verify scenarios/two_states                  # determinism gate
```

`report` is the fastest way to see whether the economy is healthy. Its columns:
- GDP and real GDP;
- the price index;
- unemployment;
- wage share and tax take;
- life-needs coverage and the share of people deprived;
- militancy.

### Play it (M3, M4)

You need Godot 4.7 or later (the standard build). See [client/README.md](../client/README.md).

```bash
cargo build -p pax_godot -p pax_server    # the client's bridge and the server it launches
godot --path client                       # the client starts a local server and connects
```

Or run a server by itself and point any client at it:

```bash
cargo run --release -p pax_server -- --scenario scenarios/two_states --bind 127.0.0.1:7777 --saves saves --sandbox
cargo run --release -p pax_cli -- replay saves/<name>.toml   # replay a save; prints its final state_hash
```

- **In the client:** the map's buttons switch map modes, a click selects a province, the Nation tab's sliders set policy, and **Saves** saves and loads.
- **F3** shows the day's `state_hash`, which a bug report should quote (D23).
- **With other people (M4):** the start screen hosts or joins a game, over TLS; a dedicated server runs with `docker compose up`. See [HOSTING.md](HOSTING.md).

## 2. Map of the code

| Path | What lives there |
|---|---|
| `crates/pax_engine/src/fixed.rs` | `Fixed`: decimal fixed-point. **All simulation numbers.** |
| `…/alloc.rs` | Exact pro-rata splitting (largest remainder). Every split of money goes through it. |
| `…/world.rs` | The state: `Pops`, `Producers`, `Markets`, `Nations` tables (Struct-of-Arrays), `World` methods |
| `…/layout.rs` | Cached POP groupings (labour pools, owners, nations) and their self-validating fingerprint |
| `…/defs.rs` | Static definitions and rules (`rules.toml`) |
| `…/tick.rs` | `step` / `step_with`: the fixed system order |
| `…/command.rs` | Player commands (D21) |
| `…/systems/` | `labor`, `production`, `market`, `firms`, `government`, `mobility`, `politics`, `demographics` |
| `crates/pax_data/` | TOML schema (`schema.rs`), validation and world building (`lib.rs`), golden files |
| `crates/pax_cli/` | `run`, `report`, `record`, `verify`, `bench`, `replay` |
| `crates/pax_server/` | The game server (D10, D22, D23): the sim thread owns the `World`, tokio handles the network ([BACKEND_SCHEMA](BACKEND_SCHEMA.md) lists its modules) |
| `crates/pax_protocol/` | The wire format: generated FlatBuffers code (`scripts/gen-protocol.sh`, never edited by hand) and framing. Schemas are in `schemas/` |
| `crates/pax_godot/` | The client's bridge (D12): connection, decoding, the local-server launcher, `PaxKeys` |
| `crates/pax_map/`, `pax_content/` | Shared by both sides: the province-map reader, and the content-hash scheme |
| `client/` | The Godot project: GDScript UI only |
| `data/` | Game content: goods, professions, production, rules |
| `scenarios/mini_valley/` | **Frozen regression fixture**, with its own `defs/` |
| `scenarios/two_states/` | Content scenario: 12 goods, 2 markets, 2 nations, command log |

## 3. One tick, in order

The order is binding (D4), and [ARCHITECTURE.md](ARCHITECTURE.md#-the-game-loop) lists it with the module for each step. In short: commands first (D21), then labour, production, the market (orders, then bounded tâtonnement per market in parallel, then pro-rata settlement), wages and taxes, and government transfers, every day; mobility, politics, demographics and POP compaction at month end.

After every tick, `step` **asserts that total money is unchanged**. If you break conservation, the very next test that runs a tick fails.

## 4. The five rules you'll trip over

1. **No floats in state.** Use `Fixed`. `mul`/`div` round down; `mul_ceil` rounds up. The clippy lint `float_arithmetic` is on; only presentation code (the CLI) may allow it.
2. **No `HashMap`/`HashSet` iteration** in anything that affects results. Use dense `Vec`s indexed by id, `BTreeMap`, or `groups::Groups`.
3. **Money only moves.**
   - Debit one account and credit another by the same amount.
   - To pay a group of POPs, use `World::credit_pops_by_size`.
   - To split a total, use `alloc::allocate`.
4. **Derived data is never stored as state.** The one exception is the self-validating `World::layout` cache (D7).
5. **Docs move with code.** If you change behaviour, update `DECISIONS.md` and the system doc in the same PR. The CI critic fails a PR whose code contradicts the decisions.

## 5. Recipes

### Add a good, profession or producer type (data only)

Edit `data/goods.toml`, `professions.toml` and `production.toml` (format in [DATA_FORMAT.md](DATA_FORMAT.md)).
- `mini_valley` is unaffected: it has its own frozen definitions.
- `two_states` results change, so re-record (see below), then run `report` for 20 years to check the economy is still healthy.

### Add a tunable rule

1. `defs.rs`: add the field to the right `…Rules` struct, with a doc comment explaining the maths.
2. `pax_data/src/schema.rs`: add the TOML field.
3. `pax_data/src/lib.rs`: copy it into the rules, and **validate its range**.
4. `data/rules.toml` **and** `scenarios/mini_valley/defs/rules.toml`: set values. For the fixture, use a value that keeps its behaviour, usually 0.
5. `crates/pax_engine/tests/common/mod.rs`: set it in the random-world rules.
6. `DATA_FORMAT.md`: add a row to the rules table.

### Add a POP column

The compiler walks you through it. `Pops` is destructured exhaustively in `World::group_pops_by_market`, `World::compact_pops` and `Pops::absorb`, so a new column is a compile error until you decide how it is permuted and merged. Also:
- push its initial value in `World::push_pop`;
- add it to `World::state_hash` (missing it makes determinism tests too weak).

### Add a system

1. Write `systems/<name>.rs` as a plain function over `&mut World`. Put the maths in the module docs and cite the decision (`Dnn`).
2. Call it from `tick.rs` at the right place in the order.
3. Update D4's schedule table, ARCHITECTURE.md's table and BACKEND_SCHEMA.md's system list.
4. If it moves money, add a case to `tests/conservation.rs`, or make sure the random worlds in `tests/common/` exercise it.
5. If it changes POP provinces or professions mid-tick, read the snapshot rules in `layout.rs`.

### Add a command

1. Add a variant to `Command` in `command.rs`.
2. **Put every check in `World::validate`** (D21). `World::apply` calls `validate` and then only assigns, so load-time and replay-time validity can never differ.
3. Parse it in `pax_data::parse_commands`. That already calls `World::validate`, so it needs no checks of its own.
4. Document it in DATA_FORMAT.md and D21.
5. Test both paths: `parse_commands` rejects a bad value at load, and `World::apply` leaves state unchanged for it.

## 6. Determinism and golden hashes

Every scenario pins state hashes in `golden.hashes`. The rules on length are in [D11](DECISIONS.md#d11-determinism-harness-and-golden-files): at least a year, and past the last logged command. They're verified in CI on Linux, Windows and macOS at 1 and 4 threads.
- **If you meant to change results,** re-record both scenarios and say so in the PR:

  ```bash
  cargo run --release -p pax_cli -- record scenarios/mini_valley
  cargo run --release -p pax_cli -- record scenarios/two_states
  ```

- **Economic bands:** `pax_data/tests/economic_bands.rs` (release builds) checks that `two_states`' year-20 GDP, unemployment, tax take, life needs, militancy and population stay within bands. Golden hashes say *something* changed; the bands say whether the economy still *behaves* the same. If you change economic behaviour on purpose, update the bands and say why in the PR.
- **If you didn't mean to,** `verify` names the first day that differs. Common causes:
  - iterating a `HashMap`;
  - a float sneaking in;
  - a parallel reduction that isn't a plain integer sum;
  - forgetting a column in `state_hash`.

## 7. Pull requests and CI

Before pushing, run:

```bash
cargo fmt --all && cargo clippy --all-targets --release -- -D warnings \
  && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps && cargo test --all
```

CI then runs these checks:

| Check | Fails when |
|---|---|
| Format, clippy, docs | Formatting, lints, broken doc links (including links to private items) |
| Tests and determinism gate | Any test, or a golden-hash mismatch |
| Determinism (Windows/macOS) | Cross-platform divergence |
| Benchmark regression | The tick is more than 20% slower than `main` (same runner, `mini_valley` and `two_states`) |
| Critic | Your PR changes 200+ lines (or changes `AGENTS.md`/`DECISIONS.md`, is sampled, or labelled `critic`) and has a **CRITICAL** architectural finding |

Act on the critic's comment as [AGENTS.md §9](../AGENTS.md#9-critic-feedback) says:
- Fix CRITICAL findings.
- Fix DEBT warnings in the same PR. They don't block, but they're re-reported on every push until fixed or waived by a maintainer (`critic-waive: <finding title>, <reason>`).
- Consider each suggestion. `/critic-followup` in Claude Code walks through a review for you.

To dispute a CRITICAL finding, reply on the PR citing the rule and ask an admin ([REPO_SETUP.md](REPO_SETUP.md)).

To have Claude draft a feature, open an issue with the *Feature request* template and label it `claude-plan` ([CLAUDE_FEATURE_PIPELINE.md](CLAUDE_FEATURE_PIPELINE.md)).

## 8. Where the design is going

- **What is decided:** [DECISIONS.md](DECISIONS.md)'s index lists every decision with its status. *Proposed* entries are designs waiting for the maintainer; read the one for an area before working on it.
- **What is being built:** the milestone documents linked from the [README](../README.md#-documentation). [MILESTONE_2.md](MILESTONE_2.md) has the measured state of the economy and the design questions still open.
- **Where things are written down:** [docs/README.md](README.md).

Open design questions still on proposal PRs: trade ([#18](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/18)), investment ([#21](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/21)) and rebellions ([#28](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/28)). Read those before working on any of these areas.
