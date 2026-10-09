# Colonization & Imperialism

Colonization in this engine is not simply painting the map; it is a profound economic and logistical endeavor. Expanding overseas stresses your naval supply chains and forces a critical decision regarding how you govern the new territory, directly impacting the global flow of wealth.

> [!NOTE]
> **Status: vision, not designed or implemented.** Nothing here is in the engine yet; it describes intended behaviour. Before it is built, it needs these decisions (new or amended `D#` entries):
> - **D19 (militancy):** an extension for the uprisings below. Today militancy rises with deprivation (`1 − life_needs`) and taxation and settles at an equilibrium: it doesn't compound, and a POP held exactly at subsistence (life needs met) isn't deprived. Disenfranchisement and wage suppression need an institutional or relative-deprivation term.
> - **D6 (wages and dividends):** how colonial institutions relate to the subsistence wage floor (`firms.subsistence_wage_multiple`, so "wages far below market value" can happen), and a cross-market ownership or share registry, so dividends can go to homeland owners or a treasury. That is a new money flow, with its conservation test (AGENTS.md §6). The "Foreign Investment mechanics" this doc mentions are part of this decision and aren't defined anywhere yet.
> - **D20 and D14 (migration):** migration across markets, from the homeland or from neighbouring regions, with friction. D20 currently keeps migration within one market.

## 🌍 The Scramble and Logistics
Before economic exploitation can begin, a territory must be claimed and physically integrated into your market.

*   **Technological Gating:** Inland colonization is initially blocked by severe attrition, disease (malaria), and hostile terrain. Specific medical and logistical technologies are required to push past the coastlines.
*   **Naval Supply Lines:** As outlined in [MILITARY_SYSTEM.md](MILITARY_SYSTEM.md), maintaining a colony requires an unbroken line of naval infrastructure. If your ports are blockaded or your convoy network is severed, the colony is isolated from the homeland's market. Its local economy will immediately crash, and any stationed armies will suffer catastrophic attrition.
*   **Infrastructure Investment:** Undeveloped colonies start with near-zero infrastructure, crippling supply throughput. The homeland must sink significant capital into building ports and dirt roads before large-scale resource extraction is mathematically possible.

## 🏛️ Colonial Institutions (Why Nations Fail)
When governing a colony, nations must choose a legal framework that dictates how the Social Accounting Matrix (SAM) distributes income. This choice represents the core thesis of *Why Nations Fail*.

### Extractive Institutions (Short-Term Exploitation)
Designed to funnel maximum wealth from the masses to a narrow homeland elite.
*   **Wage Suppression:** Local POPs are subjected to highly exploitative laws (forced labor, company towns), artificially capping their wages far below market value.
*   **Wealth Export:** Because labor costs are artificially low, the local RGOs (mines, rubber plantations) generate massive dividends. 100% of these profits are exported back to the homeland's Treasury or Capitalist investors via the Foreign Investment mechanics.
*   **The Consequence:** The colony pumps incredibly cheap raw materials into the homeland's industry. However, local POPs remain trapped at bare subsistence. Lacking discretionary income, they buy zero manufactured goods and cannot afford education (literacy stagnates). The colony can never industrialize, and militancy will eventually boil over into violent, anti-colonial uprisings.

### Inclusive Institutions (Long-Term Development)
Designed around secure property rights, fair labor markets, and local wealth retention.
*   **Market Wages:** The colony is legally treated closer to a core state. Local POPs earn fair, competitive wages based on the standard supply-and-demand simulation.
*   **Local Wealth:** Dividends are retained by local colonial Capitalists, Aristocrats, and the colonial administration.
*   **The Consequence:** Because wealth stays locally, POPs acquire discretionary income (Stone-Geary demand). They demand everyday and luxury goods, pulling manufactured exports from the homeland and eventually spurring the creation of local factories. Literacy rises, unlocking advanced professions. Over decades, an inclusive colony transforms from a rural backwater into a wealthy, industrialized partner market.

## 🚢 Migration and Demographics
A colony's institutions directly dictate who is willing to move there:
*   **Extractive Colonies** attract only a tiny sliver of homeland elites (Administrators, Officers, and Aristocrats) sent to manage the extraction. Homeland workers will refuse to migrate there because the wages are suppressed.
*   **Inclusive Colonies** attract massive waves of lower- and middle-class migration from the homeland (and globally) seeking higher wages, property rights, and opportunity. This rapidly expands the colony's workforce and consumer base.

## 📖 Historical Case Studies (Mechanics in Action)
To demonstrate how these mechanics create emergent historical outcomes without scripted events, consider three distinct colonial models:

### The Spanish Latin American Model (Pure Extractive Colonialism)
*   **The Setup:** A colonial empire focused entirely on resource extraction (like silver from Potosí or sugar plantations) using forced labor systems (such as the *Encomienda* or *Mita* systems). 
*   **The Outcome:** The engine would model this through maximum wage suppression for the native POPs. A tiny sliver of homeland administrators oversees the RGOs (mines and plantations). 100% of the massive generated wealth is immediately exported back to the homeland (the Spanish Crown and mainland elites) as dividends or taxes.
*   **The Consequence:** The colony experiences zero local capital accumulation or industrialization, despite generating staggering amounts of global wealth. The native population suffers high mortality and zero literacy growth. When the homeland's military grip eventually weakens, the simmering native and criollo militancy erupts into continent-wide wars of independence, leaving behind impoverished nations lacking modern infrastructure or inclusive institutions.

### The Rhodesian Model (Exclusive Settler Colonialism)
*   **The Setup:** Unlike pure extraction colonies (like the Belgian Congo) where all wealth is shipped back to Europe, settler colonialism involves homeland citizens permanently migrating, investing capital, and building local infrastructure (farms, railways, cities). However, the legal institutions they establish are *exclusive*—designed to benefit only the settlers while disenfranchising the native majority.
*   **The Outcome:** The engine would simulate a dual economy. The settler minority (Capitalists, Aristocrats, and skilled labor) retains the immense wealth generated by their investments and enjoys high literacy and living standards. Meanwhile, the native majority is relegated to the disenfranchised labor pool, legally barred from higher wages or property ownership.
*   **The Consequence:** The colony actually becomes highly developed and locally wealthy (the settlers indeed "built the country" economically). However, because the native majority is trapped at bare subsistence, their grievance builds over decades (pending the D19 extension above). This dual-system guarantees a violent, protracted guerrilla war (e.g., the Rhodesian Bush War) as the disenfranchised majority eventually rises up against the wealthy settler minority.

### The Hong Kong Model (The Economic Gateway)
*   **The Setup:** A tiny, resource-poor coastal territory is seized (often via "Gunboat Diplomacy") and set up with highly inclusive economic institutions, secure property rights, and massive port infrastructure.
*   **The Outcome:** Because it is deeply integrated into the homeland's market but offers a level playing field locally, it becomes a trade nexus. It attracts massive migration from both the homeland and neighboring uncolonized regions.
*   **The Consequence:** The tiny rock rapidly industrializes. The local POPs gain high literacy and transition into Clerks and Capitalists. It organically transforms into a financial and manufacturing powerhouse that acts as the commercial gateway to the larger regional market, enriching the empire through sheer volume of trade rather than raw resource extraction.
