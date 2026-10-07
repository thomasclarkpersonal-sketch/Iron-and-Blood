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
| 1 | Inter-market trade: routes, friction, merchants, tariffs | D14 (principles), TRADE.md in [#18](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/18) | 📝 **Design awaiting your answers** (6 open questions) |
| 2a | Nations, treasuries, flat income tax, transfers | D15 | ✅ Done |
| 2b | Government consumption through the market | D16 | ✅ Done |
| 3 | Banking: inside money (deposits, loans, bonds, defaults) | D5 (principles) | ⬜ Not started; needs design |
| 4a | Labour mobility between professions within a province | D18 | ✅ Done |
| 4b | Migration between provinces of a market | D20 | ✅ Done |
| 4c | POP compaction (merge duplicate identities, drop empty rows) | D7 | ✅ Done |
| 4d | Promotion to higher strata, culture and religion, migration across markets | D7 | ⬜ Not started |
| 5 | Investment and capacity growth | INVESTMENT.md in [#21](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/21) | 📝 **Design awaiting your answers** (5 open questions) |
| 6 | Commands and replayable command logs (saves = scenario + log) | D21 | ✅ Done; a binary save format is still open (D10) |
| 7a | Militancy | D19 | ✅ Done (no effects yet) |
| 7b | Consciousness, rebellions, reforms, interest groups | — | 📝 Rebellions proposed in [#28](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/28); literacy, consciousness and reforms not started |
| — | Adaptive tâtonnement step (performance) | D1 refinement | 🔍 **Opt-in implementation awaiting your review** ([#25](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/25)) |

## Decisions you need to make

These block the remaining big M2 items. Each has a proposal with defaults on an open PR; the design docs (TRADE.md, INVESTMENT.md, REBELLIONS.md) land on `main` when you accept them.

1. **Trade** (TRADE.md, [#18](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/18)):
   - who owns merchants;
   - transit time;
   - margin and flow tuning;
   - tariff base;
   - static vs dynamic merchants;
   - friction-matrix size (trade horizon).
2. **Investment** (INVESTMENT.md, [#21](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/21)):
   - who invests (producers vs owner POPs);
   - founding new producers;
   - the profit test;
   - maintenance and depreciation;
   - state industry.
3. **Adaptive price discovery** ([#25](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/25)): it cuts cold-start iterations about 5× (42.7 → 8.2) and the tick by 34% at 3,000 markets. Adopt as the default?
4. **Rebellions** (REBELLIONS.md, [#28](https://github.com/thomasclarkpersonal-sketch/Iron-and-Blood/pull/28)): what militancy *does* above a threshold.

## Measured state of the simulation

`two_states`, 20 years, with all M2 features and its policy timeline:
- **Stability:** stable (the stability test passes in release).
- **Unemployment:** rises by about one point every two years: 1.0% at year 2, 4.9% at year 10, 9.6% at year 20. Mobility fills the vacancies that exist. The drift is structural (population grows about 0.2% a year against fixed capacity), and investment (item 5) addresses it. Real GDP falls about 9% over the same 20 years for the same reason.
- **Deprivation:** life-needs coverage stays above 0.98, but the share of people deprived climbs from about 10% to about 20% by year 18.
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
| N1 | ~~Make `bench --scale` create distinct identities~~ **Decided against (for now):** the CI benchmark gate runs the *base* binary with the same flags, so changing `--scale`'s meaning would produce a spurious regression. The pitfall is documented in `pax_cli`'s help; use `--regions` for runs past day 29. A new flag would be the way to add it | Revisit with a new flag if needed |
| N2 | ✅ `DayReport` militancy summary so the CLI stops reading raw columns | Done: `MilitancySummary`, tallied in the settlement pass at no extra cost |
| N3 | Profile the market phase at scale | Measured at 3,000 markets / 36k producers: `clear_markets` 21 ms, `firms` 2.3 ms, everything else < 1 ms. Reusing discovery buffers saved ~4% (allocation isn't the bottleneck; iteration count is, see the adaptive-step review PR) |
| N4 | Content: a third state and more consumer goods, with a 20-year stability check | Data only |
| N5 | ✅ Pin `two_states` 20-year aggregates as a regression test, not just stability | Done: `pax_data/tests/economic_bands.rs` checks GDP, unemployment, tax take, life needs, militancy and population bands |
