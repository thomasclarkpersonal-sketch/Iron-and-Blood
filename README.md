# Iron and Blood (Victoria 2 Inspired Simulation)

Welcome to the central repository for **Iron and Blood**, a grand strategy and macroeconomic simulation engine inspired by Paradox Interactive's *Victoria 2*.

## 🎯 Project Vision
To build a highly concurrent, performant, and deeply interconnected socioeconomic simulation where the world is driven by its inhabitants ("POPs") and a fully independent, supply-and-demand-based global economy. A key philosophy of this project is **strict decoupling**: the mathematical backend is completely separated from the game UI, allowing rapid testing and iteration of core mechanics headlessly, without the overhead of large graphical assets.

## 🌟 Core Features
1. **POP System (Population Dynamics)**
   - Populations are modeled as distinct groups ("POPs") segmented by Location, Profession, Culture and Religion.
   - POPs are born, die, promote/demote, assimilate and migrate based on economic conditions and political policies.
   - POPs buy subsistence ("life needs") first and spend discretionary income across everyday and luxury goods (Stone-Geary demand).

2. **Independent Markets with Conserved Money**
   - **No Magic Money:** money flows in a closed loop and is exactly conserved, which is checked every tick.
   - **Production:** RGOs produce raw materials; factories refine them with fixed-coefficient recipes.
   - **Pricing:** prices are discovered daily from supply and demand, with fair pro-rata rationing and no queue priority.
   - **Trade:** goods flow between markets according to price gaps, transport friction and tariffs.

3. **Political & State Simulation**
   - Interest groups and ideologies are driven by the material conditions of the POPs.
   - Taxation, tariffs, and government spending directly influence the economy and POP wealth.

## 🚦 Status
**Milestones 1 and 3 are closed. Milestone 2 is partly done:** trade, banking and investment await design decisions. **Milestone 4 (multiplayer) is in progress.** The game so far:
- a deterministic, fixed-point economy with nations, taxes, government spending, labour mobility, migration and militancy (M1, M2);
- an outside-money conservation check on every tick, and golden-hash replay verification;
- an authoritative `pax_server` and a Godot client that plays `two_states` single player, with saves (M3);
- a headless CLI for runs, reports, replays and benchmarks.

See [docs/MILESTONE_4.md](docs/MILESTONE_4.md) for the current tasks and [docs/MILESTONE_2.md](docs/MILESTONE_2.md) for the decisions M2 is waiting on.

## 🚀 Quick Start
```bash
cargo test                                                              # unit, conservation and determinism tests
cargo run --release -p pax_cli -- run scenarios/mini_valley --days 365  # watch prices and volumes
cargo run --release -p pax_cli -- verify scenarios/mini_valley          # determinism gate
cargo run --release -p pax_cli -- report scenarios/mini_valley --days 1800  # GDP, prices, unemployment, wage share
cargo run --release -p pax_cli -- bench scenarios/mini_valley --scale 170000 --threads 8
```

```text
crates/pax_engine/   Pure simulation: Fixed, world tables, tick systems (no IO)
crates/pax_data/     TOML loading + validation, saves and snapshots, golden-hash files
crates/pax_cli/      Headless runner: run, report, record, verify, bench, replay
crates/pax_server/   The authoritative game server (single player and multiplayer)
crates/pax_protocol/ Generated FlatBuffers code and framing for the wire protocol
crates/pax_godot/    The Godot client's Rust bridge (GDExtension)
crates/pax_content/  The content-hash scheme both sides share
crates/pax_map/      The province-map reader both sides share
client/              The Godot 4 project (GDScript UI)
schemas/             FlatBuffers wire schemas
data/                Base definitions: goods, professions, production, rules
scenarios/           mini_valley (frozen regression fixture) and two_states (12 goods, 2 markets), each with pinned golden hashes
docs/                Design documentation (source of truth; docs/README.md says what lives where)
```

## 📚 Documentation
New to the project? Start with **[Onboarding](docs/ONBOARDING.md)**: a codebase tour and recipes for common changes. **[Design Decisions](docs/DECISIONS.md)** is binding and wins over any other document. [How the docs are organised](docs/README.md) says where each kind of fact lives.

* [Milestone 1](docs/MILESTONE_1.md): scope, acceptance criteria, team task list, M2 preview.
* [Milestone 2](docs/MILESTONE_2.md): M2 status, decisions waiting on you, next tasks.
* [Milestone 3](docs/MILESTONE_3.md): playable single player: `pax_server`, Godot client, saves (closed).
* [Milestone 4](docs/MILESTONE_4.md): multiplayer: lobby, authority, lag rules, hosting.
* [Milestone 5](docs/MILESTONE_5.md): trade, capital investment, construction, economic unrest.
* [Milestone 6](docs/MILESTONE_6.md): workforce composition, strata mobility, banking, politics & reforms.
* [Hosting a multiplayer game](docs/HOSTING.md): from the client, or a dedicated server with Docker.
* [Network protocol](docs/NETWORK_PROTOCOL.md): the wire format between server and client.
* [System Architecture Overview](docs/ARCHITECTURE.md): engine design and the tick schedule.
* [Backend Schema](docs/BACKEND_SCHEMA.md): workspace, SoA tables, systems.
* [Performance](docs/PERFORMANCE.md): what the tick, views and saves measure at against their budgets.
* [Data Format](docs/DATA_FORMAT.md): TOML definition and scenario files.
* [The POP System](docs/POP_SYSTEM.md): demographics, needs, and state transitions.
* [The Economy Simulator](docs/ECONOMY_SYSTEM.md): market clearing, production, wages.
* [Macroeconomics](docs/MACROECONOMICS.md): money, banking, stability.
* [Politics & State](docs/POLITICS_SYSTEM.md): fiscal policy, taxation, interest groups, and reforms.
* [Map & Logistics](docs/MAP_AND_LOGISTICS.md): geography, iceberg transport costs, and migration.
* [Inter-market trade](docs/TRADE.md): the M5 merchant design (accepted, not implemented yet).
* [Investment and capacity growth](docs/INVESTMENT.md): the M5 design for producer expansion and founding (accepted, not implemented yet).
* [Military & Supply](docs/MILITARY_SYSTEM.md): mobilization shocks and war debt.
* [Colonization & Imperialism](docs/COLONIZATION_SYSTEM.md): colonial logistics and extractive vs inclusive institutions (vision).
* Research notes: [Victoria 2 economy redesign](docs/research/victoria_2_economy_redesign.md), [reference textbook summaries](docs/research/economic_textbooks_summary.md).
* [Contributing](CONTRIBUTING.md) and [AGENTS.md](AGENTS.md): rules for humans and AI agents.
* [Repository setup](docs/REPO_SETUP.md): one-time admin steps for GitHub, CI and the blocking critic.
* [Claude feature pipeline](docs/CLAUDE_FEATURE_PIPELINE.md): issue → plan → docs + code via labels.

## 🛠️ Tech Stack & Environment
* **Simulation engine:** Rust, with a hand-rolled Struct-of-Arrays ECS (D8), decimal fixed-point math (D3) and rayon for parallel map-reduce.
* **Data:** TOML (D9).
* **Development environment:** WSL/Linux; CI also verifies determinism on Windows and macOS.
* **Network:** server-authoritative; size-prefixed FlatBuffers over TCP (D10, D22). Single player in M3, multiplayer in M4. See [NETWORK_PROTOCOL.md](docs/NETWORK_PROTOCOL.md).
* **Frontend:** Godot 4, with a GDScript UI and a Rust GDExtension bridge for the protocol (D12). The engine is client-agnostic.
* **Containerization:** Docker for dedicated multiplayer servers (M4-8: `docker compose up`, see `compose.yaml`). Single player launches `pax_server` directly, and the headless tools need no container.

> The PDFs in `Reference Books/` are copyrighted and git-ignored. Keep them local; do not commit or redistribute them.
