# Milestone 1: Closed Single-Market Economy

**Goal:** a headless, deterministic, money-conserving economy loop that a team can build on. This milestone proves the decisions in [DECISIONS.md](DECISIONS.md) in running code before any map, politics or UI work starts.

**Status: closed on 2026-10-08:** its last task (#15) merged before M2's first (#17). Every acceptance criterion below is met. This document is now a record: its open notes and M2 preview were current when it closed, and the live plan is [MILESTONE_2.md](MILESTONE_2.md).

## Scope

**In scope:**
- One nation, one or more markets with no trade between them, a handful of goods and professions.
- POPs that consume (D2), producers with Leontief recipes (D6), labour assignment, wages, dividends, and monthly demographics.
- Market clearing by bounded tâtonnement (D1).
- Fixed-point determinism (D3), an outside-money conservation check every tick (D5), the golden-hash replay gate (D11), and a performance benchmark (D13).
- A TOML data loader with validation (D9), and the `pax_cli` runner.

**Out of scope (M2+):**
- Inter-market trade and the market hierarchy (D14).
- Government, taxes and tariffs; banks and inside money.
- Promotion, migration, culture and religion.
- Politics and military.
- Network server and client (D10, D12).
- Save files.

## Acceptance criteria

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| A1 | Cargo workspace with `pax_engine` (no IO), `pax_data`, `pax_cli` | ✅ | `Cargo.toml` |
| A2 | `Fixed` type: exact decimal parsing, floor rounding, overflow panics | ✅ | `fixed.rs` unit tests |
| A3 | Largest-remainder allocation; all splits sum exactly | ✅ | `alloc.rs` tests (2,000 random splits) |
| A4 | Outside money exactly conserved every tick in arbitrary economies | ✅ | `tests/conservation.rs`: 200 random worlds × 60 days |
| A5 | Identical hashes across runs, thread counts (1/2/3/8) and snapshot/resume | ✅ | `pax_data/tests/determinism.rs` |
| A6 | One-year golden replay pinned and verified in CI | ✅ | `scenarios/mini_valley/golden.hashes`, `.github/workflows/ci.yml` |
| A7 | Loader rejects bad data and reports every error at once | ✅ | `pax_data/tests/validation.rs` |
| A8 | Reference scenario runs 5 years without collapse, with all goods traded | ✅ | `pax_cli run scenarios/mini_valley --days 1800` |
| A9 | 1M POP rows ≤ 100 ms/day on 8 threads | ✅ 45 ms (≈36 ms after T4) | `pax_cli bench scenarios/mini_valley --scale 170000 --threads 8` |
| A10 | CI fails on > 20% benchmark regression | ✅ | `Benchmark regression` job in `ci.yml` (`scripts/bench-compare.sh`) |
| A11 | Larger reference content (≥ 10 goods, ≥ 6 professions, ≥ 2 markets) runs 20 years stably | ✅ | `scenarios/two_states` (12 goods, 6 professions, 2 markets); `pax_data/tests/content_stability.rs` |
| A12 | Economy health report (GDP, unemployment, price index, wage share) in `pax_cli` | ✅ | `pax_cli report scenarios/mini_valley --days 1800` |

## Open tasks for the team

Each task is sized for one developer. All must keep `cargo test`, `clippy -D warnings` and `pax_cli verify` green.

| ID | Task | Notes |
|----|------|-------|
| T1 | ✅ **Benchmark gate.** A CI job that runs `pax_cli bench` and compares against a baseline. | Done: the `Benchmark regression` job builds the PR's base and head, times both on the same runner (5 alternating runs, median, 300k POP rows, 2 threads) and fails above +20%. A stored baseline was rejected because runner speed varies. |
| T2 | ✅ **Content and balance.** ≥ 10 goods, ≥ 6 professions, ≥ 2 markets, 20 stable years. | Done: `data/` holds 12 goods with real chains (ore + coal → steel → tools; cotton → cloth → clothes; timber → furniture) and 6 professions; the `two_states` scenario has two endowments. The stability test runs 20 years. Three engine fixes came out of it (D1, D6): the dust threshold `min_stock`, the liquidity rule, and the subsistence wage floor. `mini_valley` now has frozen `defs/`, and its golden hashes were re-recorded because the wage floor changes results. |
| T3 | ✅ **Health report.** `pax_cli report <scenario> --days N`: GDP, unemployment rate, Laspeyres price index, wage share of income, life-needs coverage. | Done: `crates/pax_cli/src/report.rs`. Engine `DayReport` gained `household_spending`, `input_spending`, `payouts` (wages, dividends); diagnostics only, hashes unchanged. |
| T4 | ✅ **Group caching.** Labour and owner groups were rebuilt every tick. | Done: `layout::PopLayout` (labour pools, owner pools, POP→market) is cached in `World`. It is self-validating: an O(N) input fingerprint each tick triggers a rebuild on any change (D7 amended). Largest-remainder allocation now uses O(n) selection instead of a sort, with identical results. **1M POPs: 43 → ~37 ms/day on 8 threads**; golden hashes unchanged. |
| T5 | ✅ **Per-market locality.** | Done, in four parts. Market passes use run-length per-market accumulators (`MarketRuns`) instead of a dense `markets × goods` vector per parallel job. The loader stores POP rows grouped by market. Orders and offers are grouped by market once, where filtering them per market was O(markets × producers). Price discovery is confined to the daily band, cutting mean iterations from 64 to 43. Measure with `bench --regions R`. 1M rows / 3,000 markets: 38 → 35 ms/day. Golden hashes re-recorded (band confinement changes the iteration path; aggregates unchanged). |
| T6 | ✅ **Heir lookup.** `demographics::update_population` scanned all POPs for each extinct POP (O(N²) worst case). | Done: one O(N) pass picks each province's heir (sizes are fixed while estates settle). Results are identical; golden hashes are unchanged. Tests in `tests/demographics.rs` (20,000 simultaneous extinctions, ties, empty province). |
| T7 | ✅ **Unemployment visibility.** Expose employed/unemployed per labour pool in `DayReport`. | Done: `DayReport::labour: Vec<LabourReport>` (province, profession, workforce, jobs, employed, `unemployed()`). Diagnostics only; hashes unchanged. Tested in `tests/labour_report.rs`. |
| T8 | ✅ **Property-test the market.** Rationing never gives a buyer more than its demand; sellers never deliver more than they offered; prices respect `max_daily_change`. | Done: `tests/market_properties.rs` (150 random worlds × 60 days), plus a per-seller delivery assertion in settlement. The random-world generator is shared in `tests/common/`. |

## Known risks and limitations

- **Tâtonnement convergence.** Markets still average about 43 of 64 iterations per day (most of `clear_markets` at 3,000 markets). A per-good adaptive step (grow it while the sign of `z` holds, halve it on a flip) should converge in about 15 iterations. It changes D1's tuning parameters, so it's left for a reviewed M2 change.

- **Reservation-price drift.** Wages track value added, and the reservation price tracks wages (D6). T2 confirmed this drift drives prices to the technical floor in fixed-quantity supply chains. It is now anchored by the subsistence wage floor (D6).
- **No unemployment benefit.** Unemployed POPs live off savings until they starve. This is intended until government spending exists (M2).
- **Aggregated regime approximation** in price discovery (D2). Monitor tâtonnement iteration counts in `DayReport.iterations`; consistently hitting `max_iterations` indicates the approximation is hurting.

## Milestone 2 preview (for planning)

> M2 is under way. Its live plan, status and the decisions waiting on the maintainer are in **[MILESTONE_2.md](MILESTONE_2.md)**.

1. Inter-market trade per D14: friction matrix, iceberg costs, tariffs.
2. Nations and treasuries. **M2-1 done (D15):** flat income tax on wages and dividends, a treasury in the money invariant, and per-capita transfers. **M2-2 done (D16):** government consumption of a basket of goods through the market. Still to do: tariffs (with D14).
3. Banking with inside money (D5): deposits, loans, bonds, defaults.
4. Promotion/demotion and migration as deterministic flows (D7). **Labour mobility within a province (D18) and migration within a market (D20) done.** Still to do: migration across markets, promotion, culture and religion columns, POP merge.
5. Share registry, investment, and firm bankruptcy (D6).
6. Save files: initial state + command log (D10).
7. Politics: militancy and consciousness in `Fixed`, driven by `life_needs`. **Militancy done (D19)**, without effects yet. Still to do: consciousness, rebellions, reforms.
