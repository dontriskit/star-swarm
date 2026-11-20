//! Deterministic simulation core of Star Swarm.
//!
//! No GUI and no I/O. The headless training environment and the GUI are both
//! clients of [`World`]: they feed one [`Action`] per ship into
//! [`World::step`] and read the state back. Humans, scripted bots and trained
//! policies all act through the same [`Controller`] interface.

pub mod bot;
pub mod geo;
pub mod model;
pub mod obs;
pub mod replay;
pub mod rng;
pub mod sim;

pub use bot::{Controller, ScriptedBot, ScriptedCommander};
pub use geo::Vec3;
pub use model::{HybridBot, ModelBot, Net};
pub use obs::{MateObs, Near, OBS_LEN, Observation, Scan, observe};
pub use replay::{Replay, Setup};
pub use sim::*;
