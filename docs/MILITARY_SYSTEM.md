# Warfare & Military Supply Chains

Warfare is an extension of economics and politics: a logistical effort that can bankrupt a nation or cause a demographic collapse. This is an **M2+** system. The economic accounting below follows D5 and D7 in [DECISIONS.md](DECISIONS.md).

## 🪖 The Military-Industrial Complex

Armies are not spawned from magic mana. They are built and maintained by consuming real goods.

### Peacetime upkeep
Standing armies are Soldier and Officer POPs, employed and paid by the state like any other employer. The government buys their upkeep on the market as a budget-constrained buyer (D1):
*   **Small arms and ammunition:** combat readiness.
*   **Canned food and uniforms:** soldiers' consumption, bought by the state on their behalf.
*   **Artillery and ships:** heavy equipment.

If the military budget is cut, these goods are not bought and soldiers' `life_needs` fall. Soldiers then demote to other professions (a deterministic flow, D7) or grow militant (risk of a coup).

## ⚔️ Mobilization and Demographic Shock

When a major war breaks out, a nation can **mobilize**.

1.  **Labour extraction:** a fraction of poor-strata POPs (farmers, labourers, craftsmen) is split off into Conscript POPs. The moving people take their share of cash, split by largest remainder (D7).
2.  **Economic shock:**
    *   **Supply drop:** labour pools shrink, so RGOs and factories employ fewer workers and output falls.
    *   **Demand spike:** the government must suddenly buy guns, ammunition and food for the conscripts.
3.  **The squeeze:** supply falls while demand jumps, so the prices of military goods rise in every market linked to the warring nation. Neutral industrial nations profit by exporting into those price gaps (D14).

Demobilization merges conscripts back into their origin POPs (D7).

## 🚂 Logistics and Supply Lines

Warfare in this era is fundamentally won through logistics. Because armies require real physical goods (ammunition, canned food, artillery) to maintain combat readiness, **Supply Line Mechanics** are a core pillar of combat:

*   **Physical Connection:** Merely purchasing goods on the market is not enough; they must physically reach the troops. Armies draw supply from their national market through a continuous path of controlled territories, railways, and naval convoys.
*   **Infrastructure Bottlenecks:** A province's infrastructure capacity (e.g., dirt roads versus advanced railways) dictates the volume of supply that can flow through it. Pushing a massive army into undeveloped colonial terrain will cause severe supply starvation and attrition, no matter how rich the state treasury is.
*   **Trade Route Interdiction:** Naval blockades and convoy raiding can sever overseas supply lines. A colonial army completely cut off from the homeland's market will quickly exhaust local supplies, causing combat effectiveness to plummet.
*   **Strategic Objectives:** Combat strategy moves beyond simply hunting enemy stacks. Protecting your logistical network while severing the enemy's—by capturing key railway hubs, straits, and ports—becomes the primary operational goal, perfectly synchronizing with the game's heavy focus on trade and colonization.

## 🩸 Casualties and Attrition

Casualties are permanent demographic losses.
*   Battle and attrition losses reduce the size of the Soldier or Conscript POP.
*   **The money stays:** a POP's cash is its total holdings, so the surviving members keep it (D7). If a regiment's POP is wiped out, its cash passes to its heir POP. Money is never destroyed by death.
*   Losses remove working-age people from the labour force. A devastating war leaves a demographic crater that cripples industrial capacity for a generation.

## 💰 The Cost of War

*   To pay for the spike in military goods, governments run deficits by selling treasury bonds (inside money, D5).
*   If the war drags on, a government may **mint** money (an explicit, logged event that raises outside money, so prices rise through the market) or **default** (bond claims are written off; bondholders lose wealth, but no cash is destroyed).
*   **War reparations:** a peace term can require the loser to pay a share of tax revenue to the victor. It is a treasury-to-treasury transfer: one debit, one credit.

## 🗺️ Command and Operations (Anti-Micro)

Victoria 2's strong economic-military connection was often hampered by excessive and tedious micromanagement during wars. To solve this, operational control shifts away from individual regiments, adapting to the historical era:

*   **Army Templates:** Players can design "templates" for squads or armies, allowing you to recruit, fund, and form balanced compositions (e.g., specific ratios of infantry, artillery, and cavalry) with a single action, rather than queuing and merging dozens of individual units manually.
*   **Era-Specific Combat Systems (Planned):** To reflect the historical evolution of warfare, the combat mechanics will transition over time:
    *   **Early/Mid-Game (Traditional Stacks):** Conflicts prior to the 1900s (such as the Franco-Prussian War) will rely on traditional maneuver warfare and stacks of armies, emphasizing positional strategy and concentrated forces.
    *   **Late-Game (Frontline System):** As technology advances into the 1900s and WW1-style trench warfare emerges, combat will shift to a macro-level **Frontline System**. Armies will be assigned to strategic fronts rather than moved province-by-province, reflecting the massive scale and reduced maneuverability of industrial warfare while alleviating micromanagement.
*   **Headquarters (HQ) Units (Proposed Expansion):** Dependent on scope, an HQ system could model the chain of command and intelligence gathering. HQ units would serve as strategic nerve centers that distribute operational directives, manage local supply flows, and pierce the "fog of war." Severing the connection to an HQ or capturing it would degrade combat efficiency and blind the enemy's intelligence network.
