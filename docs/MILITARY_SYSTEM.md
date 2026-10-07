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

## 🩸 Casualties and Attrition

Casualties are permanent demographic losses.
*   Battle and attrition losses reduce the size of the Soldier or Conscript POP.
*   **The money stays:** a POP's cash is its total holdings, so the surviving members keep it (D7). If a regiment's POP is wiped out, its cash passes to its heir POP. Money is never destroyed by death.
*   Losses remove working-age people from the labour force. A devastating war leaves a demographic crater that cripples industrial capacity for a generation.

## 💰 The Cost of War

*   To pay for the spike in military goods, governments run deficits by selling treasury bonds (inside money, D5).
*   If the war drags on, a government may **mint** money (an explicit, logged event that raises outside money, so prices rise through the market) or **default** (bond claims are written off; bondholders lose wealth, but no cash is destroyed).
*   **War reparations:** a peace term can require the loser to pay a share of tax revenue to the victor. It is a treasury-to-treasury transfer: one debit, one credit.
