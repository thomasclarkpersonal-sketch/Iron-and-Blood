# Performance Measurements

What the budgets in [DECISIONS.md](DECISIONS.md) (D13 for the tick, D22 for views, D23 for loading) measure at today, how to reproduce each number, and how they got there. Budgets are decisions; the numbers here are not, so updating them needs no decision.

Record a new measurement here when a change moves one by more than about 10%, with the date or task that took it. Keep the old number in the history column, so a regression has something to be compared against.

## Tick (D13)

All on 8 threads.

| D13 target | Command | Latest | History |
|---|---|---|---|
| M1: 1M POP rows, 1 market, 4 goods | `pax_cli bench scenarios/mini_valley --scale 170000 --threads 8` | **≈31 ms/day** (M4-11) | 45 (M1) → ≈37 (T4) → ≈31 |
| Long-term: measured at 1M rows / 3,000 markets / 4 goods | `pax_cli bench scenarios/mini_valley --scale 56 --regions 3000 --days 29` | **≈31 ms/day** (M4-11) | ≈35 (T5); 49 on M4-11's machine before it |
| M2 content: `two_states` (12 goods) at about 1M POP rows | `pax_cli bench scenarios/two_states --scale 55 --regions 1500 --days 29` | **≈91 ms/day** (M4-11) | 138 before M4-11 |
| The same world inside `pax_server` | `server_day_budget` test | **84.5 ms** (M4-11) | 126 before M4-11 |

- With `--scale`, `bench` runs the 29 days before the first month end by default: the copies `--scale` makes share identities, and month-end compaction would merge them past `u32` (MILESTONE_4, M4-11).
- M4-11 removed i128 division from `Fixed` and `allocate_raw` wherever the intermediate fits i64, with identical results.
- **Headroom:** the M2-content row is within budget with little room. The cost grows with goods and markets, so content growing towards D13's 50 goods needs more.
- **The per-day `state_hash`** costs about 31 ms at 1M POP rows (M3). FNV over 8-byte words would cut it to a few ms but changes every golden file (D11).
- **Where the time goes** (M2, N3; 3,000 markets / 36k producers): `clear_markets` 21 ms, `firms` 2.3 ms, everything else under 1 ms. Price discovery's cold-start iterations dominate.
- CI's `Benchmark regression` job compares a PR's base and head on the same runner (`scripts/bench-compare.sh`) and fails above +20%.

## Views and bandwidth (D22, D24)

At the D13 long-term scale (10,000 provinces, about 1M POP rows).

| What | Budget | Latest |
|---|---|---|
| Building the day's views | — | about 3 ms (M3-3) |
| `DayUpdate`, summary only | ≤ 16 KB | 8.3 KB on the schema (M3) |
| `DayUpdate`, every view subscribed | ≤ 128 KB | 91 KB on the schema (M3); 90.8 KB with the map and 10.7 KB without (M4-7) |
| A remote client at speed 3 | ≤ 100 KB/s (MILESTONE_4) | 53.5 KB/s; 107 KB/s at Fastest, the update cap's ceiling (`game::tests::remote_bandwidth_budget`, M4-7) |

## Saves (D23)

Measured at 990k POP rows / 3,000 markets.

| What | Latest |
|---|---|
| Replay | about 35 ms per day: roughly 4 minutes for a 20-year game, which is why loading reads the snapshot |
| Snapshot size | 40 MB |
| Writing the snapshot | 74 ms |
| Loading and verifying it (hash and table rules) | 67 ms, at any game length |
