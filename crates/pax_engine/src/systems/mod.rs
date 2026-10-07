//! Tick systems. Each is a plain function over [`crate::World`] columns and is
//! called by [`crate::tick::step`] in a fixed order (see `docs/ARCHITECTURE.md`).

pub mod demographics;
pub mod firms;
pub mod labor;
pub mod market;
pub mod production;
