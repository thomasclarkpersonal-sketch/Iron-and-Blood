# Unrest and Rebellions (M5, M6): Design

**Status: design agreed with the owner (2026-10-09), not implemented yet; its decision is *Proposed* until accepted.** It gives [D19](DECISIONS.md#d19-militancy) militancy its consequences, in two milestones: **strikes and riots in M5**, **rebellions in M6**. Its rules are [D28](DECISIONS.md#d28-economic-unrest-strikes-and-riots) (M5) and [D32](DECISIONS.md#d32-political-revolutions-and-breakaway-markets) (M6 (binding once accepted); this document holds the mechanism. The owner's answers to the review questions are under [Decisions](#decisions-owner-2026-10-09).

## Where we are

Every POP has `militancy ∈ [0, 1]`, updated monthly (D19). It rises with unmet life needs and income tax, and decays geometrically. In `two_states` it settles around 0.12 under a 12% tax. **Today it changes nothing.**

## Decisions (owner, 2026-10-09)

| Question | Decision |
|---|---|
| Which stages | **All three, in two milestones:** strikes and riots in **M5**; rebellions in **M6**, with interest groups and reforms. |
| Random or threshold | Strikes and riots are **deterministic**. Rebellions are **seeded-random** (`rng::Stream::REBELLION`), identical on replay. |
| What rebels want | Rebels **demand reforms**, and a rebellion can end by **legislative compromise** once M6's reforms exist, or by timeout. |
| Repression | **Automatic** in M5: the treasury pays a security transfer when a province riots. M5 adds no repression command. |
| Military dependency | Rebellions arrive **before** the military: they end by compromise or timeout in M6. Putting them down by force comes with the military in **M7**. |

The repression row is read from the owner's M5 draft (a security transfer, and no repression command among M5's commands); a `Repress` command can be added later if wanted.

## Stage 1: strikes and lost productivity (M5; deterministic, continuous)

A POP whose militancy exceeds `strike_threshold` works less:
- its effective labour supply is `size × (1 − strike_rate × (militancy − strike_threshold))`;
- labour assignment (D4 step 1) sees fewer available workers, so output falls, wages per worker are unchanged, and prices rise.

This is the gentle, always-on feedback: discontent costs the economy before it costs the state. No randomness, so no new determinism concerns.

## Stage 2: riots (M5; deterministic, monthly)

When the population-weighted militancy of a province exceeds `riot_threshold`, at month end:
- **destruction:** a share `riot_destruction` of the province's producers' *output stock* is destroyed. Goods are lost, never money (D5).
- **repression, automatic:** the owning nation's treasury pays a security cost to the province's POPs (wages for militia), as an ordinary transfer, split by size (largest remainder). That reduces their militancy by `repression_relief`. A stateless province has no treasury to pay, so it riots without relief.

## Stage 3: rebellion (M6; seeded-random, rare)

Revolts take **markets**, not provinces, because nation ownership is per market (D15). Above `rebellion_threshold`, each month a market has probability `p = rebellion_base × (militancy − rebellion_threshold)` of rising, on its population-weighted militancy.
- **The one place randomness enters the economy:** the draw uses `rng::Stream::REBELLION` keyed by `(seed, day, market)` (D3). It's deterministic and independent of thread count and iteration order.
- **A rebellion takes the market** (it becomes stateless) until it ends. Market ownership is therefore game state, hashed and saved, not scenario geography, and a revolt record keeps the nation to restore (D32). The nation loses tax revenue there, and the rebels' militancy slowly falls with independence.
- **Demands:** a rebellion carries a demand, a reform it wants (from M6's reforms and interest groups), recorded with the revolt.
- **It ends** in one of two ways, and the market returns to its nation:
  - **compromise:** the nation passes the demanded reform;
  - **timeout:** after `rebellion_months`.

  Putting a rebellion down by force needs military units (MILITARY_SYSTEM.md), in M7.

## Political feedback

Each stage feeds back into policy pressure. With commands (D21), a player sees militancy rise in `pax_cli report` and the client, and can respond:
- **lower taxes**, which removes the tax term;
- **raise transfers**, which improves life needs and removes the hunger term;
- in M6, **pass the reform** a rebellion demands;
- or **accept the unrest**.

## New state

- **POPs:** none (militancy exists).
- **Market (M6):** its owning nation becomes state; a revolt record per revolting market holds the nation to restore, `revolt_months`, and the demanded reform.
- **Nation:** none (repression is a transfer).

New `[politics]` rules:
- M5: `strike_threshold`, `strike_rate`; `riot_threshold`, `riot_destruction`, `repression_relief`;
- M6: `rebellion_threshold`, `rebellion_base`, `rebellion_months`.

## Acceptance tests

**M5:**
1. **Strikes:** raising militancy above the threshold lowers employment and output, and money is conserved.
2. **Riots:** destroy stock (never money) and trigger the automatic repression transfer, which is exactly what the treasury pays.
3. **Policy response:** in `two_states`, a command log raising the tax to 40% produces riots within a few years, and lowering it again calms them.

**M6:**

4. **Rebellions:** deterministic for a given seed at any thread count; the market turns stateless and returns to the recorded nation when the demanded reform passes or after `rebellion_months`.
5. **Compromise:** passing the demanded reform ends the rebellion the next month end.
