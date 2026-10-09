# Data Model for Milestones 5 and 6

> [!NOTE]
> **Status: proposed, for review with the [Milestone 5](MILESTONE_5.md) and [Milestone 6](MILESTONE_6.md) drafts** and their proposed decisions (D17, D27–D32). Nothing here is binding. Once accepted, each table moves into [BACKEND_SCHEMA.md](BACKEND_SCHEMA.md) and each rule into the decision it amends, and this document is deleted.

The milestone drafts describe their new tables in prose. Drawing them as entity–relationship diagrams shows nine defects: storage that grows as the matrix D17 avoids, references to unstable POP rows, money holders outside the conservation check, and a revolt rule that would make every later save refuse to load. This document gives the corrected model, the tick order and politics loop as connected diagrams, and the amendments each proposed decision needs.

## How to read the diagrams

Every entity is a Struct-of-Arrays table (D8): an entity is a row index, an attribute is a column, and a relationship is a column holding another table's row index (`FK`). `Fixed[good]` is a row-major per-good column, `rows × goods` long. An attribute marked *optional* is `Option<u32>`.

Each table is one of four kinds, which decide whether it is hashed and saved:

| Kind | Lives in | In `state_hash` and the snapshot | Examples |
|---|---|---|---|
| **Definition** | `Defs`, from `data/` | No; the content hash covers it | goods, professions, producer types, laws |
| **Topology** | `Geography`, from the scenario | Checked equal to the scenario on load (`snapshot.rs`) | provinces, markets, links |
| **Load-time derived** | built at load, like the friction structure AGENTS.md §5 requires | No; rebuilt deterministically | routes |
| **State** | `World` | Yes | POPs, producers, merchants, claims |

## Rules the model follows

These come from the defects the drafts' prose hides. Each is a candidate for the decision that introduces the table.

1. **Per-good columns only on tables bounded by the world's size.** Tables that grow with routes or projects keep goods in a sparse child table (`CARGO`, `PROJECT_NEED`), sorted by key so iteration order is deterministic (D3). Dense `transit`, `stock` and `cost_basis` columns on merchants would cost 60,000 routes × 50 goods × 3 × 8 bytes = **72 MB** at 3,000 markets with 20 routes each. That is the dense matrix D17 sets out to avoid, hashed every day.
2. **Nothing references a POP row.** Compaction reorders and merges POP rows every month (D7). A POP's financial position is a POP column that follows D7's split, merge and heir rules. Every claim points at a stable table: banks, nations, producers.
3. **Producer rows are never removed or reordered**, because loans and projects reference them. A closed producer has capacity 0.
4. **No new holder of outside money unless it must hold cash.** Every holder is in `World::total_money` and the state hash (D5). M5 adds `MERCHANT.cash` and M6 adds `BANK.reserves`. Construction projects and founding hold none: no "investment pool" entity.
5. **What a game can change is state, never topology.** Market ownership changes in M6 (revolts, later conquest), so it moves from `Geography` into `MARKET` state.
6. **Derived data is computed, not stored (D7).** Interest-group clout, effective workforce, credit limits and a project's cash reserve are derived each time they are needed.

## Baseline: the model today

```mermaid
erDiagram
    NATION |o--o{ MARKET : "owns (scenario topology today)"
    MARKET ||--|{ PROVINCE : contains
    PROVINCE ||--o{ POP : "home of"
    PROVINCE ||--o{ PRODUCER : "site of"
    PROFESSION ||--o{ POP : "is"
    PRODUCER_TYPE ||--o{ PRODUCER : "is"
    PROFESSION ||--o{ PRODUCER_TYPE : "works / owns"
    GOOD ||--o{ PRODUCER_TYPE : "output"

    NATION {
        string key
        Fixed treasury "outside money"
        Fixed income_tax_rate
        Fixed transfer_rate
        Fixed consumption_rate
        Fixed[good] basket
    }
    MARKET {
        string key "topology"
        u32 nation FK "optional, geography.market_nation, topology"
        Fixed[good] price "state"
    }
    PROVINCE {
        string key "topology"
        u32 market FK
    }
    POP {
        u32 province FK "row index is NOT stable (D7)"
        u16 profession FK
        u32 size
        Fixed cash "outside money"
        Fixed life_needs
        Fixed militancy
    }
    PRODUCER {
        u16 kind FK
        u32 province FK
        u32 capacity
        u32 employed
        Fixed cash "outside money"
        Fixed wage
        Fixed value_added_avg
        Fixed output_stock
        Fixed[good] input_stock
    }
    PRODUCER_TYPE {
        string key "definition"
        u16 output FK
        u16 worker FK
        u16 owner FK "owner profession: dividends (D6)"
    }
```

Money holders: `POP.cash`, `PRODUCER.cash`, `NATION.treasury`.

## Milestone 5: trade and investment

```mermaid
erDiagram
    MARKET ||--o{ LINK : "from"
    MARKET ||--o{ LINK : "to"
    LINK }o--o{ ROUTE : "paths made of (load time)"
    MARKET ||--o{ ROUTE : "origin"
    MARKET ||--o{ ROUTE : "destination"
    ROUTE ||--o{ MERCHANT : "traded by"
    NATION |o--o{ MERCHANT : "charters"
    MERCHANT ||--o{ CARGO : carries
    GOOD ||--o{ CARGO : "of"
    NATION ||--|{ TARIFF_RATE : "sets"
    GOOD ||--o{ TARIFF_RATE : "on"
    PRODUCER ||--o| PROJECT : "builds"
    PROJECT ||--|{ PROJECT_NEED : needs
    GOOD ||--o{ PROJECT_NEED : "of"
    NATION |o--o{ PRODUCER : "state-owned"
    PRODUCER_TYPE ||--o{ CONSTRUCTION_INPUT : "expansion recipe"
    GOOD ||--o{ CONSTRUCTION_INPUT : "of"

    LINK {
        u32 from FK "topology, scenario [[link]]"
        u32 to FK
        Fixed retention "1 - tau"
        Fixed capacity "units a day"
    }
    ROUTE {
        u32 origin FK "load-time derived"
        u32 destination FK "sorted by (origin, destination)"
        Fixed retention "product along the best path, at least min_retention"
        Fixed capacity "the path's bottleneck link"
    }
    MERCHANT {
        u32 route FK "state"
        u32 owner_nation FK "optional: None = origin market's owner profession (D6)"
        Fixed cash "outside money: in total_money"
    }
    CARGO {
        u32 merchant FK "sparse, sorted by (merchant, good, stage)"
        u16 good FK
        u8 stage "0 in transit, 1 for sale at destination"
        Fixed quantity
        Fixed cost "paid at origin"
        Fixed tariff_due "assessed at purchase, paid on landing"
    }
    TARIFF_RATE {
        Fixed[good] rate "NATION column tariff, row-major nation x good"
    }
    PRODUCER {
        u32 owner_nation FK "new, optional: dividends to the treasury"
        u16 idle_months "new: for depreciation"
    }
    PROJECT {
        u32 producer FK "unique: one project per producer"
        u32 slots "capacity added on completion"
        u64 started_day
    }
    PROJECT_NEED {
        u32 project FK "sparse, sorted by (project, good)"
        u16 good FK
        Fixed remaining
    }
    CONSTRUCTION_INPUT {
        u16 producer_type FK "definition, production.toml"
        u16 good FK
        Fixed per_slot
    }
```

What changes from the draft, and why:
- **`LINK` and `ROUTE` are separate.** Links are scenario data, the edges with `τ` and capacity. Routes are the trade horizon's paths, computed from links at load and rebuilt only when infrastructure changes (AGENTS.md §5). They are load-time derived, not state: never hashed or saved, as AGENTS.md §5 asks of the friction matrix. Merchants need a `ROUTE` table to point at, and D14's capacity throttle needs a column; the draft has neither.
- **Goods in transit and for sale are `CARGO` rows** (rule 1). A merchant usually carries a few goods, so the table holds what is shipped, not merchants × goods.
- **The tariff is assessed when the merchant buys and paid when the goods land.** A merchant buys only what it can pay for including the tariff, so its cash always covers `Σ tariff_due`. In the draft, the merchant owes the tariff on landing from cash it may already have spent, and the only ways out create money or strand goods. Assessing at purchase also fixes the rate, so a tariff changed by a command overnight can't reprice goods already at sea. The importer is the destination market's owner; a stateless market levies nothing.
- **Tariffs are a `NATION` column per good**, like `basket`. `SetTariff { nation, good, rate }` changes only the commanding nation's own imports, so it needs no conflict rule under D24. Tariffs by trading partner (customs unions) can come later as a separate relation.
- **Ownership is one optional column, `owner_nation`, on both merchants and producers.** None means D6's rule: the owner profession in the market. Some means the treasury: chartered merchants and state-founded producers. This covers the draft's *Private* and *Chartered* kinds. *Commercial* needs a merchant profession that doesn't exist yet, and can be data later: an owner profession per merchant type.
- **No investment pool.** Founding creates a `PRODUCER` row with capacity 0 and a `PROJECT`, so founding and expansion are one mechanism. The founder's money moves into the new producer's cash once: from the market's owner-profession POPs, split by their cash with largest remainder, or from the treasury for `FoundProducer`. No new money holder is needed (rule 4).
- **A project's cash reserve is derived, not stored:** `Σ remaining × today's price`. The money stays in `PRODUCER.cash`, and D6's dividend rule pays out only what exceeds wages, the restart reserve and this reserve. The draft's `project_budget` column would be a second copy of money already in the producer's cash.
- **Construction recipes live in `production.toml`.** The draft's M5-6 names `producer_types.toml`, which doesn't exist.

Money holders after M5: the baseline's, plus `MERCHANT.cash`.

## Milestone 6: society, banking and politics

**Society and banking.** A POP's bank is found through its province, which never changes market:

```mermaid
erDiagram
    MARKET ||--|{ PROVINCE : contains
    PROVINCE ||--o{ POP : "home of"
    PROVINCE ||--o{ PRODUCER : "site of"
    MARKET ||--|| BANK : "has one"
    BANK ||--o{ POP : "owes deposits (via province, no FK)"
    BANK ||--o{ PRODUCER : "owes deposits (via province, no FK)"
    BANK ||--o{ LOAN : "lends"
    PRODUCER ||--o{ LOAN : "owes"
    BANK ||--o{ BOND : "holds"
    NATION ||--o{ BOND : "issues"

    POP {
        u32 men "new: replaces size, with women and dependents"
        u32 women "new"
        u32 dependents "new: eat, don't work unless laws allow"
        Fixed literacy "new"
        Fixed consciousness "new"
        Fixed deposits "new: claim on its market's bank, follows D7 like cash"
    }

    PRODUCER {
        Fixed deposits "new: claim on its market's bank"
    }

    BANK {
        u32 market FK "unique, markets never change, so neither does a POP's bank"
        Fixed reserves "outside money: in total_money"
    }

    LOAN {
        u32 bank FK "sorted by (bank, producer)"
        u32 producer FK "stable: producer rows are never removed"
        Fixed principal
        Fixed rate
        u64 due_day
    }

    BOND {
        u32 bank FK "only banks hold bonds"
        u32 nation FK
        Fixed principal
        Fixed coupon
        u64 due_day
    }
```

**Politics and revolts:**

```mermaid
erDiagram
    NATION |o--o{ MARKET : "owns (now state)"
    MARKET ||--o| REVOLT : "may be in"
    NATION ||--o{ REVOLT : "lost the market"
    LAW ||--o{ REVOLT : "demanded"
    LAW_GROUP ||--|{ LAW : "options"
    NATION ||--|{ NATION_LAW : "enacts one per group"
    LAW ||--o{ NATION_LAW : "chosen"
    PROFESSION ||--o{ PROFESSION_INTEREST : "leans"
    INTEREST_GROUP ||--o{ PROFESSION_INTEREST : "draws"
    NATION ||--|{ GROUP_APPROVAL : "per group"
    INTEREST_GROUP ||--o{ GROUP_APPROVAL : "of"

    MARKET {
        u32 owner FK "moved from topology to state: optional, hashed, saved"
    }

    REVOLT {
        u32 market FK "unique: revolts take markets, not provinces"
        u32 from_nation FK "restored on resolution"
        u16 demand FK "the law that ends it"
        u64 started_day
    }

    LAW_GROUP {
        string key "definition: franchise, child_labour, schooling, min_wage, tax"
    }

    LAW {
        u16 group FK "definition"
        string key
        Fixed parameter "e.g. child labour share, wage-floor multiple"
    }

    NATION_LAW {
        u16 law FK "NATION column law, row-major nation x law group"
    }

    INTEREST_GROUP {
        string key "definition"
    }

    PROFESSION_INTEREST {
        u16 profession FK "definition, professions.toml"
        u16 group FK
        Fixed weight "many-to-many, weights per profession sum to 1"
    }

    GROUP_APPROVAL {
        Fixed approval "NATION column, row-major nation x group"
    }
```

What changes from the draft, and why:
- **A POP's size is derived: `men + women + dependents`.** D29 adds the three columns without saying how they relate to `size`. Storing `size` too would be derived state (D7). Each decision that reads people must say which columns it reads:

  | Reader | Reads |
  |---|---|
  | Demand (D2), life needs, D7 cash splits | everyone: `men + women + dependents` |
  | Labour pools (D18, D20, D25), strikes (D28) | the *effective* workforce: `men`, plus `women` and `dependents` as the nation's laws allow; derived, never stored |
  | Births | add to `dependents`. Their cost in goods replaces D26's band-aid |
  | Ageing (new) | moves `⌊dependents × ageing_rate⌋` into `men` and `women` with largest remainder |
  | Mobility, migration, promotion | move a household: one count split across the three columns with largest remainder, so each column stays exact |
- **POPs hold deposits, not bonds** (rule 2). A per-POP bond holding would need an issuer per row. `POP.deposits` is a claim on the bank of the POP's own market, and a POP's market never changes, because provinces don't change market. If revolts or conquest changed the issuer instead, the claim would silently move to another nation. Capitalists' demand for bonds goes through their bank, which holds `BOND`s. This narrows M6's "capitalist POPs purchase bonds".
- **Loans and bonds are two tables, not one polymorphic `CLAIM`.** A `(kind, id)` reference can't be checked by `World::check_tables` the way a typed foreign key can, and the two have different rules: default, coupons, deficit financing.
- **One bank per market.** Its `reserves` are outside money. Inside money is `POP.deposits` and `PRODUCER.deposits` (bank liabilities) against `LOAN` and `BOND` (bank assets). D5's invariant becomes checkable per bank: `reserves + Σ loans + Σ bonds − Σ deposits = equity`. A negative result means the bank fails.
- **Cash stays the only means of payment in the tick** *(recommended; this is the central open decision)*. A loan credits the borrower's deposits, which is endogenous money (D5). Spending them converts deposits to cash from the bank's reserves, so a reserve ratio in `rules.toml` limits lending, and the market's hot loop keeps a single kind of money. The draft doesn't say which money buys goods.
- **A credit limit comes from cash flow, not net worth.** D30's "positive net worth" needs producer assets to be valued, and nothing values capacity. A limit of `k × value_added_avg` is derived from existing state.
- **Interest groups draw on professions through weights** (`PROFESSION_INTEREST`, many-to-many), as POP_SYSTEM describes, and clout is derived each month. The only new state is each nation's `approval` of each group.
  - The draft's one-to-one table maps professions that don't exist: clergy, clerks, merchants. Its sixth group, Armed Forces, has no members until soldiers exist (M7). Adding professions is data, but it belongs in the task list.
- **Laws are definitions; a nation's choice is a column.** Reforms set it with `SetLaw { nation, group, law }`, checked in `World::validate` (D21). Each law's parameters feed the rules other decisions read: the wage floor (D6), tax brackets (D15), labour participation (D29). Those effective rules are derived per nation, never copied.
- **Revolts take markets, and market ownership becomes state** (rule 5). D32 writes `market_nation = None` for a province, but `market_nation` is per market. The snapshot loader refuses a save whose geography differs from the scenario's (`pax_data::snapshot`), so as drafted, **every save after a revolt would refuse to load.**
  - `MARKET.owner` becomes hashed, saved state. The layout cache already fingerprints it (D7).
  - `REVOLT` remembers the nation to restore and the law that ends the revolt; the draft has nowhere to keep either.
  - Draw the RNG per market: `rng::Stream::REBELLION` keyed by `(seed, stream, day, market)`.

Money holders after M6: the M5 holders, plus `BANK.reserves`.

## Tick order (M5)

The draft's tick diagram has no edges, so the order isn't shown. It also leaves out commands, government transfers and the month-end systems, and it doesn't say whether riots read this month's militancy. D4 must be amended to this order or another the owner picks:

```mermaid
flowchart TD
    C[0. Commands, D21] --> A[1. Arrival: cargo lands, iceberg loss, tariff paid]
    A --> L[2. Labour: effective workforce, strikes]
    L --> P[3. Production, and construction goods delivered to projects]
    P --> M[4. Market: orders from households, producers, projects, governments and merchants, then discovery, then settlement: purchases become cargo in transit]
    M --> F[5. Firms: wages, then dividends above wage, restart and project reserves]
    F --> G[5b. Government transfers]
    G --> E{Month end?}
    E -- no --> Done[Conservation assert]
    E -- yes --> Mob[6. Mobility: D18, D20, D25]
    Mob --> Pol[7. Politics: militancy]
    Pol --> R[8. Riots, on this month's militancy]
    R --> I[9. Investment: start, complete and depreciate projects]
    I --> Dem[10. Demographics, then compaction]
    Dem --> Done
```

## The politics loop (M6)

In the draft's loop, `Clout` leads nowhere, `Revolt` has no consequence, `Literacy` has no input, and unmet demands aren't connected to material conditions. Connected:

```mermaid
flowchart TD
    Mat[Material conditions: wages, life needs, taxes] --> Mil[Militancy]
    Mat --> Cl[Interest-group clout]
    Laws[National laws] --> Edu[Schooling and child labour] --> Lit[Literacy]
    Lit --> Con[Consciousness]
    Mat --> Con
    Con --> Dem[Demands for reforms]
    Cl --> Parl[Parliament: votes weighted by clout]
    Dem --> Parl
    Parl -->|enacts| Laws
    Laws --> Work[Effective workforce, wage floor, tax brackets]
    Work --> Mat
    Laws --> App[Group approval]
    App --> Mil
    Mil -->|above thresholds| Rev[Revolt: market leaves the nation]
    Dem --> Rev
    Rev -->|lost taxes, no transfers| Mat
    Laws -->|demand met| End[Revolt ends, market returns]
```

## Amendments each proposed decision needs

A proposed decision that changes an accepted one must amend it explicitly in the same change, or the critic reports decision drift.

| Proposed | Amends | What |
|---|---|---|
| D17 trade | D4 | Arrival step and merchant orders (tick order above) |
| | D14 rule 5 | Sparse routes from links, computed at load, instead of a dense matrix |
| | D5, D6 | `MERCHANT.cash` in `total_money`; merchant dividends follow `owner_nation` or the owner profession |
| | D21, D24 | `SetTariff` and its validation; no conflict rule needed, since it touches only one's own imports |
| D27 investment | D6 | Dividends only above the project reserve; `owner_nation` for state-founded producers; founding transfers from owner POPs |
| | D4 | The month-end investment step |
| | D21, D24 | `FoundProducer`: only in the commanding nation's own markets, paid from its treasury |
| D28 unrest | D6 | Striking workers' pay: the pool's wages split by working members, or strikes cost strikers nothing |
| | D19 | Whether a riot's transfer gives "militancy relief" directly. Today it would only act through life needs |
| | D4 | Riots after politics at month end |
| D29 workforce | D7, D2, D18, D20, D25 | The reader table above; split and merge across three columns |
| | D26 | Superseded by births into `dependents` |
| D30 banking | D5 | Means of payment, `BANK.reserves` in `total_money`, the per-bank invariant |
| | D7 | `deposits` follow `cash`'s split, merge and heir rules |
| | D6 | Firm failure: loans first, then owners |
| | D15, D21 | Coupons, deficit financing, `IssueBonds` |
| D31 politics | D6, D15, D19 | Wage-floor and tax-bracket laws; consciousness beside militancy; approval's effect on militancy |
| | D21 | `SetLaw`, with the parliamentary check in `World::validate` |
| D32 revolutions | D15, D23 | Market ownership becomes state, hashed and saved; the snapshot's geography check no longer covers it |
| | D3 | The rebellion stream keyed by market |

## Open questions for the owner

1. **Route capacity:** each route takes its bottleneck link's capacity (simple), or routes share link capacity pro rata (D14 rule 3, a per-link pass each day)?
2. **Merchants per route:** one, or several sharing it (the draft's `capacity_share`)?
3. **Who funds founding:** the market's owner-profession POPs, by cash, or only the treasury in M5?
4. **Bank granularity:** one per market (proposed: a POP's bank never changes), or one per nation?
5. **Means of payment:** cash only, with deposits converted at the bank (proposed), or deposits spendable in the market?
6. **Bonds:** held by banks only (proposed), or by POPs directly, which needs a per-nation POP column?
7. **New professions** for M6 (clergy, clerks, perhaps merchants): which, and which interest groups they lean to.
