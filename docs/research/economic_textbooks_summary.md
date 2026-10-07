# Victoria 2 Economic Modeling: Reference Textbook Summaries

> **Research note.** This is background reading, not a specification. Binding choices are in [DECISIONS.md](../DECISIONS.md).

This document consolidates the most relevant insights from three reference texts on agent-based and input-output economics, specifically filtered to address the core design problems of a grand strategy game economy like *Victoria 2* (e.g., Pop simulation, factory input-output networks, decentralized global markets, and supply/demand).

---

## 1. Introduction to Agent-Based Economics (Gallegati, Palestrini & Russo, eds.)
*Focus: Simulating individual economic actors, market dynamics, and emergent macroeconomic behavior.*

* **Agent-Based Simulation of Pops:** Instead of using a singular "representative agent" to model a nation's economy, models must use heterogeneous, distinct actors (Pops, factories, governments) with varying attributes and goals. Pops exhibit "bounded rationality"—meaning they don't possess perfect foresight or universal optimization. They use heuristics, adapt to past outcomes, and adjust incrementally, which fits perfectly with game AI updating consumption/production based on local market signals.
* **Network Interactions & Localized Markets:** Markets do not magically clear to an equilibrium. Trade occurs through localized network interactions. Prices adjust dynamically based on surpluses/shortages, and because there is no central auctioneer, markets experience friction, mismatches, and delayed clearing. This mimics the realistic supply chain bottlenecks and localized shortages found in deep economic strategy games.
* **Discrete Time & Conservation of Money:** Careful handling of simulated time is crucial. Using discrete ticks for production, consumption, and wage-paying ensures that the money supply and physical goods are strictly conserved, preventing leaks in the game's closed-loop economy.
* **Emergent Booms and Busts:** Combining simple micro-level rules (like tariffs or a single resource shortage) naturally produces booms, busts, and economic fluctuations without needing random external events. Changes cascade through the network, affecting wealth distribution which in turn impacts aggregate demand (crucial for Pop militancy and political stability).

## 2. Handbook of Computational Economics, Vol. 2: Agent-Based Computational Economics (Tesfatsion & Judd, eds., 2006)
*Note: the local PDF's filename credits Amman, Kendrick & Rust, who edited Vol. 1. The contents are Vol. 2.*

*Focus: Agent-driven procurement, survival mechanics, and macroeconomic emergence.*

* **Removing the Walrasian Auctioneer:** Traditional models rely on an omniscient mechanism that instantly clears markets by setting perfect equilibrium prices. A decentralized economy (like Victoria 2's global market) must rely on "agent-driven procurement processes" where prices and trade volumes are determined dynamically through local interactions, resulting in "out-of-equilibrium" states.
* **Pop Survival as the Primary Driver:** Pops should be modeled as autonomous agents where the primary goal is survival. If basic subsistence needs are not met, they face starvation, migration, or death. Procurement of goods is driven by survival before luxury. Pops generate demands based on preferences and budget constraints.
* **Firms, Rationing, and Insolvency:** Producers (factories/RGOs) and consumers (Pops) interact directly. Producers set their own prices based on limited information. When supply doesn't meet demand, explicit rationing mechanisms must be defined (e.g., prioritizing nations by prestige). Firms failing to cover costs face insolvency, while profitable ones distribute dividends to shareholders (Capitalists), fueling expansion.
* **Institutions as Market Protocols:** Trade agreements, spheres of influence, and tariffs act as the "market protocols" that structure agent interactions, heavily impacting the global flow of wealth. Macroeconomic variables (GDP, inflation) are emergent properties of these structured interactions rather than top-down dictates.

## 3. Input-Output Analysis: Foundations and Extensions (Miller & Blair, 3rd ed., 2022)
*Focus: Factory production, resource transformation, supply chains, and market structures.*

* **Technical Coefficients for Factories:** The core Leontief model maps how the output of one sector becomes the input of another via fixed-proportion production functions (technical coefficients). This directly mirrors Victoria 2's factory requirements (e.g., an artillery factory requiring a strict ratio of steel, explosives, and machine parts with constant returns to scale).
* **Commodity-by-Industry Matrices:** The *Use* matrix details the inputs factories consume, and the *Make* matrix details what they produce. This separation is ideal for modeling factories that create secondary byproducts or for representing multiple production methods (artisanal vs. industrialized) for the same fungible commodity.
* **Social Accounting Matrices (SAMs):** This expands the input-output model to explicitly track income distribution. In game terms: factory revenues pay for inputs, and the "value added" is distributed as wages and dividends to laborers, clerks, and capitalists. Pops then spend this income, feeding back into the "Final Demand" vector.
* **Supply Chain Linkages & Bottlenecks:** *Backward linkages* track upstream dependencies (tanks require steel, which requires iron/coal). *Forward linkages* identify critical supply bottlenecks where a shortage of a base resource throttles the entire downstream industrial economy. A shock in final demand (e.g., massive military buildup) propagates through this network, stimulating total output and income.
