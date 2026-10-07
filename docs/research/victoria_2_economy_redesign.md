# Victoria 2: Rebuilding a Robust, Emergent POP-Driven Economy

> **Research note.** This surveys the options; [DECISIONS.md](../DECISIONS.md) records what was chosen. In summary:
> - Market clearing uses bounded tâtonnement, not order books (D1).
> - Consumer demand is Stone-Geary/LES (D2).
> - Production is Leontief, with alternative production methods instead of CES, because CES needs fractional powers that break fixed-point determinism (D3, D6).
> - Money follows SFC with endogenous inside money (D5).
> - Trade uses iceberg costs and price-gap flows (D14).
> - Labour mobility uses deterministic fractional flows rather than full search and matching (D7).

In Victoria 2, the economy is notoriously prone to liquidity traps, hard-coded buy-order priority exploits (e.g., Great Powers draining the world market before small nations get a single unit of grain), and prices that only shift post-hoc via artificial price adjustment bounds. 

To build a genuinely robust, emergent POP-driven economy, your foundational field is **Agent-Based Computational Economics (ACE)** combined with **Input-Output (Leontief) analysis** and **Stock-Flow Consistent (SFC) macroeconomics**.

---

## 1. Foundational Paradigms & Core Literature

### A. Agent-Based Computational Economics (ABM / ACE)
Instead of assuming a central clearinghouse solves the economy simultaneously, you simulate individual POPs and factories making decisions with bounded rationality, local information, and balance sheets.

* **Primary Text:** *Handbook of Computational Economics, Vol. 2: Agent-Based Computational Economics* (Tesfatsion & Judd).
* **Key Mechanics to Extract:**
  * **Adaptive Learning:** POPs adjust reservations using Reinforcement Learning or heuristics (e.g., Roth-Erev algorithm or simple Bush-Mosteller learning).
  * **Zero-Intelligence (ZI) Traders:** Gode & Sunder (1993) showed that double-auction market rules enforce high allocative efficiency even when agents trade using nearly random bidding with budget constraints.

### B. Stock-Flow Consistent (SFC) Macroeconomics
Vic 2's biggest flaw was money "disappearing" (unbacked money sinks, artisans destroying currency, tariffs vaporizing purchasing power). SFC modeling ensures strict double-entry conservation of every ounce of gold or fiat currency.

* **Primary Text:** *Monetary Economics: An Integrated Approach to Credit, Money, Income, Production and Wealth* (Godley & Lavoie).
* **Mathematical Core:**
  For any sector or POP $i$, changes in net financial assets $\Delta A_i$ must identically equal net cash flow:
  
  $$\Delta A_i = Y_i - C_i - T_i$$
  
  where $Y_i$ is income (wages, dividends, rents), $C_i$ is consumption/investment expenditure, and $T_i$ is taxes. Across all sectors $N$:
  
  $$\sum_{i=1}^N \Delta A_i = 0 \quad \text{(in a closed fiat system)}$$
  
  This mathematical discipline prevents inflation or hyper-deflation bugs caused by asymmetric transaction drops.

### C. Inter-Industry Linkages (Input-Output Economics)
To model how heavy industry depends on coal, steel, and machine parts without manual hard-coding:

* **Primary Text:** *Input-Output Analysis: Foundations and Extensions* (Miller & Blair).
* **Mathematical Core:**
  The classic Leontief system:
  
  $$\mathbf{x} = \mathbf{A}\mathbf{x} + \mathbf{d} \implies \mathbf{x} = (\mathbf{I} - \mathbf{A})^{-1}\mathbf{d}$$
  
  * $\mathbf{x}$ = total gross output vector across all goods.
  * $\mathbf{A}$ = intermediate input coefficient matrix (e.g., $A_{ij}$ is units of steel needed per unit of steam engine).
  * $\mathbf{d}$ = final demand from POPs and government.
  * $(\mathbf{I} - \mathbf{A})^{-1}$ = the Leontief Inverse, capturing cascading supply-chain impacts (e.g., an embargo on sulfur shuts down fertilizer, which cascades into grain yields).

---

## 2. Microeconomic Mechanics for Game Systems

### A. Consumer Demand: Moving Beyond Vic 2’s Tiered Hard Caps
Vic 2 used rigid tiers: Life Needs, Everyday Needs, and Luxury Needs. A richer mathematical approach uses **Stone-Geary Utility** or **Constant Elasticity of Substitution (CES)**.

**Stone-Geary Utility (Linear Expenditure System - LES):**

$$U(x_1, \dots, x_n) = \prod_{k=1}^n (x_k - \gamma_k)^{\beta_k}, \quad \sum \beta_k = 1$$

* $\gamma_k > 0$: The hard subsistence floor for good $k$ (e.g., grain, coal). If $x_k \le \gamma_k$, the POP enters starvation/militancy.
* $Y - \sum p_k \gamma_k$: The supernumerary income (discretionary spending).

**Demand function per good:**

$$x_i(p, Y) = \gamma_i + \frac{\beta_i}{p_i}\left(Y - \sum_{k} p_k \gamma_k\right)$$

* **Game result:** Rich POPs spend their surplus on luxury goods according to preferences $\beta_i$, while poor POPs are consumed entirely by survival floors $\gamma_i$. Price elasticity dynamically changes as real wages rise.

### B. Market Clearing: Fixing the World Market Queue
Instead of giving GP #1 first pick of global goods at yesterday’s frozen price, consider one of two mechanics:

1. **Walrasian Tâtonnement (Numerical General Equilibrium):**
   Compute excess demand $z_i(p) = D_i(p) - S_i(p)$. Update prices dynamically:
   
   $$\dot{p}_i = \lambda_i z_i(p)$$
   
   Use an iterative solver like the Scarf Algorithm or Newton-Raphson across tick intervals to clear regional prices while incorporating transport friction.

2. **Double Auctions with Order Books (Agent-Centric):**
   Treat regional markets like decentralized exchange books where POPs and factories place bids and asks. Friction is added by physical infrastructure (ports, railways) acting as **Iceberg Transport Costs** (Samuelson, 1954): shipping 1 unit from Province $A$ to Province $B$ requires consuming $\tau \cdot d(A, B)$ units along the way.

---

## 3. Architecture Blueprint for Your Engine

| Subsystem | Theoretical Foundation | What it Replaces in Vic 2 |
| :--- | :--- | :--- |
| **POP Consumption** | Stone-Geary / AIDS (Almost Ideal Demand System) | Rigid 3-tier hardcoded checklists |
| **Factory Production** | Constant Elasticity of Substitution (CES) or Leontief with substitution | Fixed recipe ratios that choke on minor shortages |
| **Trade & Logistics** | Gravity Model of Trade / Iceberg Transport Costs | Magical instant World Market queue |
| **Money Supply** | Endogenous Money (Post-Keynesian SFC) | Hard-coded gold mines + artificial currency sinks |
| **Labor & Migration** | Search and Matching Theory (Mortensen-Pissarides) | Instantaneous RGO-to-craftsman promotion |

---

## 4. Recommended Reading List

### Agent-Based Economics
* ***Agent-Based Macroeconomics: An Introduction*** – Domenico Delli Gatti et al. *(Focuses directly on balance-sheet agents, bankruptcy, and credit networks).*

### Industrial Structure & General Equilibrium
* ***Microeconomic Analysis*** – Hal R. Varian *(Chapters on General Equilibrium, Walrasian equilibria, and production functions).*
* ***Applied General Equilibrium: An Introduction*** – John B. Shoven & John Whalley.

### Monetary System & Flows
* ***Monetary Economics*** – Wynne Godley & Marc Lavoie *(Essential reading if you want to avoid systemic economic collapse from untracked cash leakage).*
