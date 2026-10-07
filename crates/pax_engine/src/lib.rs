//! # pax_engine
//!
//! The pure simulation core of Iron and Blood: fixed-point arithmetic,
//! Struct-of-Arrays world state and the tick systems that mutate it.
//!
//! The crate performs no IO and knows nothing about rendering, networking or
//! file formats (`AGENTS.md` §2). Data files are turned into a [`World`] by
//! `pax_data`; the world is advanced with [`tick::step`].
//!
//! Design decisions referenced throughout as `D1`…`D14` are recorded in
//! `docs/DECISIONS.md`.

pub mod alloc;
pub mod command;
pub mod defs;
pub mod fixed;
pub mod groups;
pub mod hash;
pub mod layout;
pub mod rng;
pub mod systems;
pub mod tick;
pub mod world;

pub use command::{Command, CommandError};
pub use fixed::Fixed;
pub use tick::{DayReport, step};
pub use world::World;
