//! Deterministic simulation core of Star Swarm.
//!
//! No GUI and no I/O. The headless training environment and the GUI are both
//! clients of [`World`]: they feed one [`Action`] per ship into
//! [`World::step`] and read the state back. Humans, scripted bots and trained
//! policies all act through the same [`Controller`] interface.

pub mod bot;
pub mod obs;
pub mod rng;
pub mod sim;

pub use bot::{Controller, ScriptedBot};
pub use obs::{Near, OBS_LEN, Observation, observe};
pub use sim::*;
