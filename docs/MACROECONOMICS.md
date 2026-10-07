# Advanced Macroeconomics & System Stability

This document covers the economic theory behind the simulation's stability. Legacy games like *Victoria 2* suffered late-game crashes caused by **liquidity traps**, **money destruction** and **inflexible price boundaries**. The binding rules are D1, D2, D5 and D6 in [DECISIONS.md](DECISIONS.md).

## 1. The Core Problem: The Liquidity Trap & Deflation

Legacy economies collapsed for two reasons:
1. **Money black holes:** events, arbitrary building costs or deleted POPs permanently removed money from the loop.
2. **Hoarding (velocity collapse):** high-strata POPs piled up idle cash.

The Equation of Exchange:

$$M \times V = P \times Q$$

*(money supply × velocity = price level × real output)*

As industrialisation multiplies real output `Q`, a static `M` with falling `V` forces the price level `P` down. Wages fall with it, POPs can no longer afford their needs, and the economy spirals.

## 2. Preventing Economic Collapse

### A. No black holes: stock-flow consistency (D5)

- **Outside money** (`Σ cash` of all agents) is constant except for explicit, logged mint/burn events. `tick::step` asserts this every day. `tests/conservation.rs` runs 200 randomised economies through it.
- Every transfer debits and credits the same amount. Every split of a total uses largest-remainder allocation, so no rounding residue leaks.
- When people die, their money stays with the survivors of their POP. An extinct POP's cash goes to an heir (D7).
- **One world currency** until at least M3; per-nation currencies would require a foreign-exchange market.

For every agent `i` (Godley & Lavoie):

$$\Delta A_i = Y_i - C_i - T_i, \qquad \sum_i \Delta A_i = 0 \text{ (closed system, no minting)}$$

### B. Keeping money moving

- **POPs spend a fixed fraction of cash each day** (`spend_rate`). A larger hoard therefore means larger spending: velocity cannot fall to zero.
- **Firms pay out cash above a reserve** as dividends (D6), so profits return to households instead of piling up in firms.

### C. The financial sector (M2): inside money

Banks are planned for M2 and must follow D5:

- Deposits, loans and treasury bonds are **asset/liability pairs**. Lending creates a loan (bank asset, borrower liability) and a deposit (borrower asset, bank liability) at the same time. This is endogenous money, consistent with the Post-Keynesian SFC literature.
- The invariant extends to `Σ financial assets − Σ financial liabilities = Σ outside money`.
- Banks lend to POPs (consumer credit), to capitalists (factory construction) and to governments (bonds).
- **Defaults** write off both sides of a claim. They destroy *wealth* (net worth), never *cash*.
- **Central-bank minting** (seigniorage, gold) is the only way to change outside money. It is a logged event with the issuing treasury as recipient.

## 3. Price Discovery and Elasticity

The old plan to "find the exact Walrasian equilibrium every tick" is replaced by **bounded tâtonnement** (D1). Each market iterates `p ← p(1 + λz)` on demand and supply *functions* for a limited number of iterations. It caps the daily price move, and rations any remaining imbalance pro rata. Markets are discovered in parallel with rayon. Because Stone-Geary demand aggregates exactly (D2), discovery costs `O(professions × goods × iterations)` per market, independent of POP count.

**Elasticity is emergent, not a per-tier constant.** Under Stone-Geary demand the price elasticity of good `i` is:

$$\varepsilon_i = -1 + \frac{\gamma_i (1 - \beta_i)}{x_i}$$

- **Subsistence goods** (large `γ`) are inelastic, especially for poor POPs, whose purchases are mostly `γ`.
- **Discretionary goods** (`γ = 0`) have elasticity −1 with respect to their own price and a positive income elasticity. As real wages rise, demand shifts towards everyday and luxury goods automatically.

This replaces the earlier `Demand(P) = BaseDemand·(BasePrice/P)^E` curve. That curve needed fractional powers, which conflict with fixed-point determinism (D3), and it needed a hand-tuned exponent per good.

## 4. Labor Market & Wage Stickiness

A realistic economy needs realistic wages. If a factory has a bad day it should not cut wages to zero and cause instant starvation.

- **Wages are sticky** (D6). The target wage is `labor_share × smoothed value added / workers`, and the actual wage closes only `1/wage_stickiness_days` of the gap per day.
- **Unprofitable producers stop buying inputs** (shutdown rule) and **stop overproducing** (inventory targeting), instead of burning cash.
- A producer that runs out of cash pays what it can. Workers are paid pro rata, and there is no debt in M1.
- *M2:* bank loans let a producer bridge losses before it cuts wages, and bankruptcy hands its remaining cash to creditors, then owners.
- *M2:* search-and-matching style labour flows (promotion/demotion, migration toward higher wages) as deterministic fractional flows (D7).
