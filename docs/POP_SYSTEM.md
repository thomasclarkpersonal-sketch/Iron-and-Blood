# The POP System (Population Dynamics)

POPs (Portions of Population) are the atomic unit of the simulation: a group of people in one province who share the same characteristics. Binding rules: D2 (demand) and D7 (POP accounting) in [DECISIONS.md](DECISIONS.md).

## 🧬 POP Data Structure

POPs are rows in the `Pops` table ([BACKEND_SCHEMA.md](BACKEND_SCHEMA.md#pops)).

**Identity (key):**
*   **Province.** The market, state and nation are *derived* from the province, never stored on the POP.
*   **Profession:** e.g. farmer, labourer, craftsman, clerk, capitalist, aristocrat, soldier.
*   **Culture** and **Religion** *(M2)*.

**State:**
*   **Size:** number of people.
*   **Cash:** the POP's **total** holdings, not per capita. Money stays with the survivors when size changes.
*   **Life needs:** subsistence satisfaction `[0, 1]` from the last market day.
*   *M2:* **Literacy** (promotion chance, research), **Militancy** (likelihood of rebelling) and **Consciousness** (demand for reforms). All are `Fixed`, never floats (D3).

### 💼 Planned (M2+): Workforce Composition and Labor Laws

> [!NOTE]
> **Status: planned, not implemented.** Today a POP has one `size` (above), and D2, D7, D18 and D20 are defined on it. Before this is built it needs a decision (a new `D#`): how `size` relates to the columns below (for example `size = workforce_male + workforce_female + dependents`), which column D2's demand, D7's splits and merges, demographics and the D18/D20 labour pools each read, and how a law change moves people between columns under D7's largest-remainder rule. It must also say whether mobilization ([MILITARY_SYSTEM.md](MILITARY_SYSTEM.md)) draws conscripts only from `workforce_male`, and how the split POP's cash is shared when conscripts leave (D7).

The planned split of a POP's size:
*   `workforce_male`: Adult men available for employment or conscription.
*   `workforce_female`: Adult women available for employment (varies heavily by social laws).
*   `dependents`: Children, elderly, and non-working spouses. Dependents consume goods but do not work.

To maintain strict performance budgets (D13), we do not track separate POPs for children or working women. Instead, social dynamics are handled via **Effective Workforce** calculations based on national laws:

*   **Child Labor:** If legal, factories are permitted to hire a percentage of the `dependents` column. This artificially boosts the nation's industrial throughput and the POP's household income. However, during the demographic tick, working children incur a massive penalty to the POP's **Literacy** growth and a slight increase in dependent mortality. 
*   **The Reform Shock:** Passing "Compulsory Schooling" or outlawing child labor instantly removes those dependents from the effective workforce. This creates a fascinating historical dilemma: reforming labor laws causes a sudden, painful economic crash (labor shortages and lower household income) in exchange for the long-term technological dominance driven by high literacy.
*   **Women in the Workforce:** Similar to WW1 mobilization, laws can gradually shift people from the `dependents` pool into the `workforce_female` pool, unlocking massive industrial reserves when male workers are conscripted to the frontlines.

## 🔄 POP Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Living : scenario start
    Living --> Market : daily, buy needs (D2)
    Market --> Income : daily, wages + dividends
    Income --> Living
    Living --> Mobility : month end, profession change / migration (D18, D20)
    Mobility --> Split_Merge : move ⌊N × rate⌋ people with their share of cash
    Split_Merge --> Living
    Living --> Demographics : month end, growth or starvation
    Demographics --> Living
    Demographics --> Extinct : size reaches 0
    Extinct --> [*] : cash passes to heir POP
```

## 🛒 Needs System (D2)

Victoria 2's three tiers map onto a single Stone-Geary (Linear Expenditure System) demand function. They are no longer three separate shopping passes.

| Tier | In data | Behaviour |
|---|---|---|
| **Life needs** (grain, fish) | `subsistence` γ | Bought first. If the budget cannot cover them, the whole budget buys a scaled-down basket and `life_needs < 1`. |
| **Everyday needs** (clothes, furniture) | `preference` β on common goods | Bought from discretionary income `Y − N·C`. |
| **Luxury needs** (liquor, coffee, cars) | `preference` β on luxury goods | Same formula. Richer POPs have more discretionary income, so luxury demand rises with wealth automatically. |

### Consumption logic

```mermaid
flowchart TD
    Start["Budget Y = cash × spend_rate"] --> Check{"Y ≥ N × subsistence cost?"}
    Check -->|No: deprived| Deprived["Spend all of Y on a scaled<br/>subsistence basket: x = γY/C"]
    Deprived --> Starve["life_needs < 1: population decline,<br/>later militancy"]
    Check -->|Yes: comfortable| Comfortable["Buy N·γ, then split Y − N·C<br/>across goods by β"]
    Comfortable --> Thrive["life_needs = 1: growth;<br/>discretionary spending feeds promotion (M2)"]
```

If a market is short, every buyer receives the same fraction of what it asked for (D1), so `life_needs` reflects market scarcity as well as poverty.

## 📈 Demographics (month end)

- Rate: `+growth_rate` when `life_needs = 1`, otherwise `−starvation_rate × (1 − life_needs)`.
- `ΔN = ⌊N × rate⌋`, a deterministic flow with no dice rolls.
- Cash is unchanged: survivors inherit. If a POP dies out, its cash passes to the largest living POP in the same province, at that month end or the first one after anyone lives there, so money is never destroyed or frozen (D5, D7).

## 🔀 Labour Mobility (D18)
Each month, within a province, unemployed workers move to professions with vacancies (D18). Then surplus workers migrate to provinces of the same market that have vacancies in their profession (D20). Both take their share of cash with them. Migration across markets and promotion to higher strata come later.

## 🧮 Merging and Splitting

Merging and splitting keep the number of POP rows bounded:

*   **Splitting:** when `⌊N × rate⌋` people promote, migrate or are conscripted, they move to the POP with the target identity. A new row is created only if none exists; lookup goes through a sorted identity index, never a `HashMap` (D3). They take cash in proportion to their share of the POP, split by largest remainder.
*   **Merging:** below a size threshold (e.g. 50 people) a POP merges into the most similar POP in the province. Cash adds up. Literacy and militancy become size-weighted averages in `Fixed`.
*   **Implemented:** rows are compacted at month end (`World::compact_pops`). Rows sharing `(province, profession)` merge and empty, cashless rows are dropped, so row indices stay dense.
*   **Still M2:** merging *small* POPs into the most similar identity (needs culture and religion columns).
