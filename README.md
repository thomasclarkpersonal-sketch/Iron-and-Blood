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
**Milestone 1 (closed single-market economy) is complete, and Milestone 2 has started** (nations, income tax and transfers: D15). It includes:
- fixed-point determinism;
- an outside-money conservation check on every tick;
- golden-hash replay verification;
- a TOML data loader;
- a headless CLI, benchmarked at about 45 ms per simulated day for 1M POPs on 8 threads.

See [docs/MILESTONE_1.md](docs/MILESTONE_1.md) for acceptance criteria and open team tasks.

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
crates/pax_data/     TOML loading + validation, golden-hash files
crates/pax_cli/      Headless runner
data/                Base definitions: goods, professions, production, rules
scenarios/           mini_valley (frozen regression fixture) and two_states (12 goods, 2 markets), each with pinned golden hashes
docs/                Design documentation (source of truth)
```

## 📚 Documentation
New to the project? Start with **[Onboarding](docs/ONBOARDING.md)**: a codebase tour and recipes for common changes. **[Design Decisions](docs/DECISIONS.md)** is binding and wins over any other document.

* [Milestone 1](docs/MILESTONE_1.md): scope, acceptance criteria, team task list, M2 preview.
* [Milestone 2](docs/MILESTONE_2.md): M2 status, decisions waiting on you, next tasks.
* [System Architecture Overview](docs/ARCHITECTURE.md): engine design and the tick schedule.
* [Backend Schema](docs/BACKEND_SCHEMA.md): workspace, SoA tables, systems.
* [Data Format](docs/DATA_FORMAT.md): TOML definition and scenario files.
* [The POP System](docs/POP_SYSTEM.md): demographics, needs, and state transitions.
* [The Economy Simulator](docs/ECONOMY_SYSTEM.md): market clearing, production, wages.
* [Macroeconomics](docs/MACROECONOMICS.md): money, banking, stability.
* [Politics & State](docs/POLITICS_SYSTEM.md): fiscal policy, taxation, interest groups, and reforms.
* [Map & Logistics](docs/MAP_AND_LOGISTICS.md): geography, iceberg transport costs, and migration.
* [Military & Supply](docs/MILITARY_SYSTEM.md): mobilization shocks and war debt.
* Research notes: [Victoria 2 economy redesign](docs/research/victoria_2_economy_redesign.md), [reference textbook summaries](docs/research/economic_textbooks_summary.md).
* [Contributing](CONTRIBUTING.md) and [AGENTS.md](AGENTS.md): rules for humans and AI agents.
* [Repository setup](docs/REPO_SETUP.md): one-time admin steps for GitHub, CI and the blocking critic.
* [Claude feature pipeline](docs/CLAUDE_FEATURE_PIPELINE.md): issue → plan → docs + code via labels.

## 🛠️ Tech Stack & Environment
* **Simulation engine:** Rust, with a hand-rolled Struct-of-Arrays ECS (D8), decimal fixed-point math (D3) and rayon for parallel map-reduce.
* **Data:** TOML (D9).
* **Development environment:** WSL/Linux; CI also verifies determinism on Windows and macOS.
* **Network:** server-authoritative with a binary protocol, in M3 (D10).
* **Frontend:** Godot or Web, to be decided before M3 (D12). The engine is client-agnostic.
* **Containerization:** Docker for `pax_server` deployment from M3. The headless M1/M2 tools need no container.

> The PDFs in `Reference Books/` are copyrighted and git-ignored. Keep them local; do not commit or redistribute them.
