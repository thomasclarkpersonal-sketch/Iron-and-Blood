# State, Politics & Fiscal Policy

The government is the largest single economic actor. In a stock-flow consistent economy, the state's balance sheet is tied directly to POP wealth: a government surplus drains liquidity from the private sector, and a deficit injects it. This is an **M2** system; the accounting rules below are already binding (D3, D5 in [DECISIONS.md](DECISIONS.md)).

## 🏛️ Fiscal Policy (the Balance Sheet)

Each nation has a treasury: a `cash: Fixed` account that is included in the outside-money invariant (D5). All rates are `Fixed` (D3); a floating-point tax rate would make money itself platform-dependent.

### 1. Revenue
*   **Income tax:** deducted when wages and dividends are paid (in `firms.rs`), *before* POPs spend. Brackets can be flat, progressive or regressive depending on law. A bracket rate applies to per-capita income; the POP's total tax is that times its size, rounded down. Every unit is transferred to the treasury, never discarded.
*   **Tariffs:** charged on goods flowing into the nation's market nodes (D14). The importer pays and the treasury receives the same amount.
*   **Minting / seigniorage:** the central bank creates outside money and credits it to the treasury. This is **the only mechanism allowed to change total cash**, and it is logged as an explicit mint event. More money chasing the same goods raises prices through ordinary market clearing; there is no separate inflation formula.
*   **Treasury bonds:** the treasury sells bonds to banks or POPs. A bond is an asset/liability pair (inside money); cash moves from the buyer to the treasury.

### 2. Expenditure
*   **State wages:** bureaucrats, officers and soldiers are paid like any other employer, into their labour pools.
*   **Purchases:** the government is a market buyer with a budget, for military upkeep, infrastructure maintenance and construction goods. Its demand enters price discovery like a producer's input order (D1).
*   **Subsidies:** transfers to a producer's cash.
*   **Social spending:** pensions and unemployment benefits are transfers to POP cash, split by size with largest remainder.
*   **Debt service:** interest paid to bondholders.
*   **Default:** bond claims are written off on both sides. Bondholders, typically capitalists and banks, lose *wealth*; no *cash* is destroyed. A default can bankrupt banks and set off a financial crisis.

## 🗳️ Politics and Ideologies

POPs hold political opinions shaped by their material conditions.

### Interest groups
POPs align with interest groups (Industrialists, Agrarians, Trade Unions, Devout, …). The alignment weight is a function of profession and economic condition (cash per capita, `life_needs`).
*   *Example:* a wealthy capitalist leans strongly Industrialist; a starving labourer leans Trade Union or Radical.

### Militancy vs. consciousness
Both are `Fixed` columns on `Pops`, updated at month end (D4):
*   **Militancy:** willingness to use violence against the state. It rises with unmet life needs (`1 − life_needs`) and with the tax burden, and decays slowly toward a baseline. High militancy produces rebellions, the one place genuine randomness is used (`rng::Stream::REBELLION`, D3).
*   **Consciousness:** political awareness. Driven by literacy, discretionary spending (`Y − N·C`) and technology. High consciousness creates demand for political reforms (voting rights, free press) and social reforms (minimum wage, pensions).

Exact update formulas will be specified in M2, together with the data fields for them in `professions.toml` and `rules.toml`.

### Laws & reforms
Enacted laws constrain the government's tax brackets, tariff ranges and social spending. Reforms can appease high-consciousness POPs but anger entrenched elite interest groups.
