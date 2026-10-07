# Unrest and Rebellions (M2) — Design Proposal

**Status: proposal for review.** Nothing here is implemented yet. It gives [D19](DECISIONS.md#d19-militancy) militancy its consequences. On approval it becomes a numbered decision and is delivered in small PRs. Please answer the [open questions](#open-questions-for-review) first.

## Where we are

Every POP has `militancy ∈ [0, 1]`, updated monthly (D19). It rises with unmet life needs and income tax, and decays geometrically. In `two_states` it settles around 0.12 under a 12% tax. **Today it changes nothing.**

## Proposed mechanism

Unrest escalates in three stages, each driven by state the engine already has.

### 1. Strikes and lost productivity (deterministic, continuous)

A POP whose militancy exceeds `strike_threshold` works less:
- its effective labour supply falls by `strike_rate × (militancy − threshold)`;
- labour assignment (D4 step 1) sees fewer available workers, so output falls, wages per worker are unchanged, and prices rise.

This is the gentle, always-on feedback: discontent costs the economy before it costs the state. No randomness, so no new determinism concerns.

### 2. Riots (deterministic, monthly)

When the population-weighted militancy of a province exceeds `riot_threshold`, at month end:
- **destruction:** a share of the province's producers' *output stock* is destroyed. Goods are lost, never money (D5).
- **repression cost:** the owning nation's treasury pays a security cost to the province's POPs (wages for militia), as an ordinary transfer. That reduces militancy by `repression_relief`.

### 3. Rebellion (random, rare)

Above `rebellion_threshold`, each month a province has probability `p = rebellion_base × (militancy − threshold)` of rising.
- **The one place randomness enters the economy:** the draw uses `rng::Stream::REBELLION` keyed by `(seed, day, province)` (D3). It's deterministic and independent of thread count and iteration order.
- **A rebellion seizes the province's markets** (sets `market_nation` to `None`, i.e. stateless) until it is put down. The nation loses tax revenue there, and the rebels' militancy slowly falls with independence.
- **Putting it down** needs military units (MILITARY_SYSTEM.md, a later milestone). Until then, rebellions end on their own after `rebellion_months`, and the market returns to its nation.

### Political feedback

Each stage feeds back into policy pressure. With commands (D21), a player sees militancy rise in `pax_cli report`, and can respond:
- **lower taxes**, which removes the tax term;
- **raise transfers**, which improves life needs and removes the hunger term;
- or **accept the unrest**.

## New state

- **POPs:** none (militancy exists).
- **Province:** `in_revolt: bool` and `revolt_months: u16`.
- **Nation:** none (repression is a transfer).

New `[politics]` rules:
- `strike_threshold`, `strike_rate`;
- `riot_threshold`, `riot_destruction`, `repression_relief`;
- `rebellion_threshold`, `rebellion_base`, `rebellion_months`.

## Acceptance tests

1. **Strikes:** raising militancy above the threshold lowers employment and output, and money is conserved.
2. **Riots:** destroy stock (never money) and trigger repression transfers.
3. **Rebellions:** deterministic for a given seed at any thread count, and they flip `market_nation` and restore it after `rebellion_months`.
4. **Policy response:** in `two_states`, a command log raising the tax to 40% produces riots within a few years, and lowering it again calms them.

## Open questions for review

1. **Stages:** all three (proposed), or strikes only to start?
2. **Randomness:** rebellions are the first random mechanic. Is a probabilistic rising acceptable for a deterministic-replay game (it's seeded, so replays are identical), or should rebellions trigger deterministically at a threshold?
3. **What rebels want:** Victoria 2 had rebel types (nationalists, socialists, reactionaries) with demands. Should rebellions carry a *demand* (a policy that ends them, e.g. tax below X), which needs interest groups or ideology first?
4. **Repression:** should repression be automatic (proposed) or a player command (`Repress { province }`) with a treasury cost?
5. **Military dependency:** OK for rebellions to end by timeout until armies exist, or wait and build rebellions together with the military system?
