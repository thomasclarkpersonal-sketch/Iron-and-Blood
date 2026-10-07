# Milestone 2: Nations, Population Dynamics and Trade

**Goal:** turn the single-market economy of [Milestone 1](MILESTONE_1.md) into a world of nations:
- governments that tax and spend;
- people who move to where the jobs are;
- political pressure that builds up;
- trade between markets.

Each feature lands as a numbered decision in [DECISIONS.md](DECISIONS.md).

## Status

| # | Feature | Decision | Status |
|---|---|---|---|
| 1 | Inter-market trade: routes, friction, merchants, tariffs | D14 (principles), [TRADE.md](TRADE.md) | 📝 **Design awaiting your answers** (6 open questions) |
| 2a | Nations, treasuries, flat income tax, transfers | D15 | ✅ Done |
| 2b | Government consumption through the market | D16 | ✅ Done |
| 3 | Banking: inside money (deposits, loans, bonds, defaults) | D5 (principles) | ⬜ Not started; needs design |
| 4a | Labour mobility between professions within a province | D18 | ✅ Done |
| 4b | Migration between provinces of a market | D20 | ✅ Done |
| 4c | POP compaction (merge duplicate identities, drop empty rows) | D7 | ✅ Done |
| 4d | Promotion to higher strata, culture and religion, migration across markets | D7 | ⬜ Not started |
| 5 | Investment and capacity growth | [INVESTMENT.md](INVESTMENT.md) | 📝 **Design awaiting your answers** (5 open questions) |
| 6 | Commands and replayable command logs (saves = scenario + log) | D21 | ✅ Done; a binary save format is still open (D10) |
| 7a | Militancy | D19 | ✅ Done (no effects yet) |
| 7b | Consciousness, rebellions, reforms, interest groups | — | ⬜ Not started; needs design (literacy, rebellion rules) |
| — | Adaptive tâtonnement step (performance) | D1 refinement | 🔍 **Opt-in implementation awaiting your review** |

## Decisions you need to make

These block the remaining big M2 items. Each has a proposal with defaults.

1. **Trade** ([TRADE.md](TRADE.md), PR "Proposal: inter-market trade"):
   - who owns merchants;
   - transit time;
   - margin and flow tuning;
   - tariff base;
   - static vs dynamic merchants;
   - friction-matrix size (trade horizon).
2. **Investment** ([INVESTMENT.md](INVESTMENT.md)):
   - who invests (producers vs owner POPs);
   - founding new producers;
   - the profit test;
   - maintenance and depreciation;
   - state industry.
3. **Adaptive price discovery** (PR "For review: opt-in adaptive tâtonnement"): it cuts cold-start iterations about 5× (42.7 → 8.2) and the tick by 34% at 3,000 markets. Adopt as the default?
4. **Rebellions:** what militancy *does* above a threshold. This is new design; nothing is proposed yet.

## Measured state of the simulation

`two_states`, 20 years, with all M2 features and its policy timeline:
- **Stability:** stable (the stability test passes in release).
- **Unemployment:** low and slowly rising, from 0.2% to about 1.5% over 3 years. Mobility fills vacancies. The remaining drift is structural (fixed capacity), and investment (item 5) addresses it.
- **Commands work:** raising the Lowland income tax to 12% on day 360 moves the combined tax take from 9.8% to 12%. Militancy rises from 0.08 to 0.12, its tax-driven equilibrium.

Performance (8 threads; D13 budget is 100 ms/day):

| Setup | ms/day |
|---|---|
| 1M POP rows, 1 market | ~36 |
| 2,000 markets | ~52 |
| 6,000 markets | ~118 (over budget, at twice the target market count) |

Price discovery dominates at scale, mostly cold-start iterations, which item 3 addresses.

## Next tasks (ready to pick up)

These need no further design decisions:

| ID | Task | Notes |
|---|---|---|
| N1 | Make `pax_cli bench --scale` create distinct identities (e.g. extra provinces), so month-end compaction doesn't merge the copies back | Today `--scale` rows collapse after day 29; use `--regions` meanwhile |
| N2 | ✅ `DayReport` militancy summary so the CLI stops reading raw columns | Done: `MilitancySummary`, tallied in the settlement pass at no extra cost |
| N3 | Profile and optimise `firms` at large producer counts (per-market subsistence cost is recomputed every day) | ~2.3 ms at 36k producers |
| N4 | Content: a third state and more consumer goods, with a 20-year stability check | Data only |
| N5 | ✅ Pin `two_states` 20-year aggregates as a regression test, not just stability | Done: `pax_data/tests/economic_bands.rs` checks GDP, unemployment, tax take, life needs, militancy and population bands |
