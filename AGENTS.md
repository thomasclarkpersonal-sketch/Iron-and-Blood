# AI Agent Guidelines (AGENTS.md)

Welcome, AI Developer. When contributing to **Iron and Blood**, strictly follow these architectural and stylistic rules. They protect the performance and integrity of this socioeconomic simulation engine, and they apply to human contributors too.

Binding design decisions live in [docs/DECISIONS.md](docs/DECISIONS.md) (cited as D1–D14). If anything here or in another document conflicts with it, DECISIONS.md wins.

## 1. Core Architecture (Rust & Data-Oriented Design)
- **Language:** Rust (toolchain pinned in `rust-toolchain.toml`).
- **Pattern:** a hand-rolled ECS-style Struct-of-Arrays (D8). An entity is a dense row index, a component is a `Vec` column in `crates/pax_engine/src/world.rs`, and a system is a plain function called in a fixed order from `tick.rs`. Do not add an ECS framework without amending D8.
- **Rule:** no Object-Oriented patterns or pointer-chasing for simulation entities: no `Box<dyn Trait>` per entity, no `Rc<RefCell<T>>`, no arrays of structs for hot data. Keep data contiguous.
- When adding a component column, push it in the table's `push_*` method (all columns must stay the same length) and add it to `World::state_hash`.
- Derived data is never stored as state (D7). The one exception is the self-validating `World::layout` cache, which re-fingerprints its inputs each tick. Never add a cache that can't detect its own staleness.

## 2. Strict Decoupling
- **Engine purity:** `pax_engine` MUST stay agnostic of presentation, IO, files and networking. No rendering, Godot, Web, filesystem or socket code.
- **Crate responsibilities:**
  - `pax_engine`: fixed-point math, world state, tick systems. No IO.
  - `pax_data`: parsing and validating TOML data files into a `World` (D9); golden-hash file IO.
  - `pax_cli`: headless runner (run, record/verify golden hashes, benchmark).
  - `pax_server` *(M3, not yet created)*: network protocol and API boundary (D10).

## 3. Concurrency Mitigation
- **Avoid lock contention:** when many entities affect a shared resource (thousands of POPs and one market), use **Map-Reduce**. POPs compute in parallel (Map: rayon `fold` into per-thread accumulators); the market then processes the aggregate (Reduce). See `systems/market.rs`.
- **Deterministic parallelism:** parallel reductions may only *sum integers* (`Fixed`). Never reduce with min/max over ties, float math, or anything order-dependent. Results must be identical at any thread count (`tests/determinism.rs` checks 1/2/3/8 threads).

## 4. Strict Determinism (D3)
- **No floating point in simulation state, or in anything that feeds it.** This covers money, prices and debt, and also tax rates, literacy, militancy and every other ratio. Use `pax_engine::Fixed`. Floats are allowed only in presentation code (CLI output, client). Clippy's `float_arithmetic` lint enforces this.
- **Rounding is explicit:** `mul`/`div` round down; use `mul_ceil` where rounding down would let an agent use more than it has.
- **No `std::collections::HashMap`/`HashSet`** in simulation state or anywhere whose iteration order affects results. Use dense `Vec`s indexed by id, `BTreeMap`, or `groups::Groups`.
- **Randomness** only through `pax_engine::rng` (counter-based, keyed by seed/stream/day/entity). Add a new `Stream` constant per consumer.
- Overflow panics by design; do not switch to wrapping or saturating arithmetic to make a panic go away. Find the bug.

## 5. Performance over Flexibility
- **Pre-computation:** never run expensive algorithms (pathfinding, matrix inversion) in the daily tick. Pre-compute distance and friction matrices at load time or when infrastructure changes.
- **Budget (D13):** 1M POP rows ≤ 100 ms/day on 8 threads. Check with `pax_cli bench … --scale 170000 --threads 8` when touching hot loops.
- **Serialization:** no JSON on hot-path state sync; use a binary format (D10).

## 6. Economic Integrity (D5)
- **Stock-flow consistency:** money is never created or destroyed except by an explicit, logged mint/burn event (none exist yet). Every transfer debits and credits the same amount.
- **Splitting totals** (wages among POPs, revenue among sellers, cash of a splitting POP) MUST use `alloc::allocate`/`allocate_raw` (largest remainder), so parts sum exactly to the whole.
- `tick::step` asserts conservation every day. Add a case to `crates/pax_engine/tests/conservation.rs` for every new money flow; extend the shared random-world generator in `crates/pax_engine/tests/common/mod.rs` rather than writing a second one.

## 7. Determinism Gate (D11)
- `cargo run --release -p pax_cli -- verify scenarios/mini_valley` must pass.
- If you *intentionally* change simulation results, re-record (`pax_cli record`) and state that in your summary. Never re-record to silence an unexplained change.

## 8. Documentation Maintenance
- **Keep docs in sync:** `docs/` is the source of truth for the engine's design. When you alter core logic, add components, or change system behaviour, update `DECISIONS.md` and the relevant system document (`BACKEND_SCHEMA.md`, `ECONOMY_SYSTEM.md`, …) in the same change.
- **Inline documentation:** keep accurate rustdocs (`///`, `//!`) on structs, components and public functions. Explain the *why* and the *mathematics*, not just the *what*, and cite the decision (`D1`…) the code implements.
