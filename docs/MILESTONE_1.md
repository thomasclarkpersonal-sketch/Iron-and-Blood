# Milestone 1: Closed Single-Market Economy

**Goal:** a headless, deterministic, money-conserving economy loop that a team can build on. This milestone proves the decisions in [DECISIONS.md](DECISIONS.md) in running code before any map, politics or UI work starts.

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
| A9 | 1M POP rows ≤ 100 ms/day on 8 threads | ✅ 45 ms | `pax_cli bench scenarios/mini_valley --scale 170000 --threads 8` |
| A10 | CI fails on > 20% benchmark regression | ✅ | `Benchmark regression` job in `ci.yml` (`scripts/bench-compare.sh`) |
| A11 | Larger reference content (≥ 10 goods, ≥ 6 professions, ≥ 2 markets) runs 20 years stably | ⬜ | Task T2 |
| A12 | Economy health report (GDP, unemployment, price index, wage share) in `pax_cli` | ✅ | `pax_cli report scenarios/mini_valley --days 1800` |

## Open tasks for the team

Each task is sized for one developer. All must keep `cargo test`, `clippy -D warnings` and `pax_cli verify` green.

| ID | Task | Notes |
|----|------|-------|
| T1 | ✅ **Benchmark gate.** A CI job that runs `pax_cli bench` and compares against a baseline. | Done: the `Benchmark regression` job builds the PR's base and head, times both on the same runner (5 alternating runs, median, 300k POP rows, 2 threads) and fails above +20%. A stored baseline was rejected because runner speed varies. |
| T2 | **Content and balance.** Grow `data/` to ≥ 10 goods (e.g. coal, iron, steel, cloth, cotton, fish, liquor) and a two-market scenario; tune until 20 years are stable. | Expect to find engine edge cases; write a regression test for each. |
| T3 | ✅ **Health report.** `pax_cli report <scenario> --days N`: GDP, unemployment rate, Laspeyres price index, wage share of income, life-needs coverage. | Done: `crates/pax_cli/src/report.rs`. Engine `DayReport` gained `household_spending`, `input_spending`, `payouts` (wages, dividends); diagnostics only, hashes unchanged. |
| T4 | ✅ **Group caching.** Labour and owner groups were rebuilt every tick. | Done: `layout::PopLayout` (labour pools, owner pools, POP→market) is cached in `World` and rebuilt only after `invalidate_pop_layout` (`push_pop` calls it). Debug builds verify the cache every tick. Largest-remainder allocation now uses O(n) selection instead of a sort, with identical results. **1M POPs: 43.3 → 36.0 ms/day on 8 threads (−17%)**; golden hashes unchanged. |
| T5 | **Per-market locality.** Settlement keeps a `markets × goods` accumulator per rayon job. With ~3,000 markets that is too large. Keep POP rows sorted by market and reduce per market range. | Required for the D13 long-term target. |
| T6 | ✅ **Heir lookup.** `demographics::update_population` scanned all POPs for each extinct POP (O(N²) worst case). | Done: one O(N) pass picks each province's heir (sizes are fixed while estates settle). Results are identical; golden hashes are unchanged. Tests in `tests/demographics.rs` (20,000 simultaneous extinctions, ties, empty province). |
| T7 | ✅ **Unemployment visibility.** Expose employed/unemployed per labour pool in `DayReport`. | Done: `DayReport::labour: Vec<LabourReport>` (province, profession, workforce, jobs, employed, `unemployed()`). Diagnostics only; hashes unchanged. Tested in `tests/labour_report.rs`. |
| T8 | ✅ **Property-test the market.** Rationing never gives a buyer more than its demand; sellers never deliver more than they offered; prices respect `max_daily_change`. | Done: `tests/market_properties.rs` (150 random worlds × 60 days), plus a per-seller delivery assertion in settlement. The random-world generator is shared in `tests/common/`. |

## Known risks and limitations

- **Reservation-price drift.** Wages track value added and the reservation price tracks wages (D6). In a bilateral market with an inelastic buyer, prices can drift slowly. Inventory targeting damps it; T2 will show whether a floor on the mark-up is needed.
- **No unemployment benefit.** Unemployed POPs live off savings until they starve. This is intended until government spending exists (M2).
- **Aggregated regime approximation** in price discovery (D2). Monitor tâtonnement iteration counts in `DayReport.iterations`; consistently hitting `max_iterations` indicates the approximation is hurting.

## Milestone 2 preview (for planning)

1. Inter-market trade per D14: friction matrix, iceberg costs, tariffs.
2. Nations and treasuries: income tax on wages, tariffs, government consumption, a treasury in the money invariant.
3. Banking with inside money (D5): deposits, loans, bonds, defaults.
4. Promotion/demotion and migration as deterministic flows (D7); culture and religion columns; POP split/merge.
5. Share registry, investment, and firm bankruptcy (D6).
6. Save files: initial state + command log (D10).
7. Politics: militancy and consciousness in `Fixed`, driven by `life_needs`.
