//! Recorded matches: the seed plus every ship's action on every tick, with a
//! state hash every [`HASH_EVERY`] ticks so a diverging playback is detected.

use crate::sim::{Action, UPGRADES, World};
use std::io::{self, Read, Write};
use std::path::Path;

const MAGIC: &[u8; 8] = b"SWRP0001";
pub const HASH_EVERY: u32 = 300;

/// Packs an action into one byte: thrust, turn, fire, upgrade to buy.
pub fn encode(a: &Action) -> u8 {
    let buy = a.buy.map_or(0, |u| u as u8 + 1);
    a.thrust as u8 | ((a.turn.clamp(-1, 1) + 1) as u8) << 1 | (a.fire as u8) << 3 | buy << 4
}

pub fn decode(b: u8) -> Action {
    Action {
        thrust: b & 1 != 0,
        turn: ((b >> 1) & 3) as i8 - 1,
        fire: b & 8 != 0,
        buy: (b >> 4).checked_sub(1).and_then(|i| UPGRADES.get(i as usize).copied()),
    }
}

/// A match played with the default [`crate::Config`].
#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    pub seed: u64,
    pub ships: u16,
    frames: Vec<u8>,
    hashes: Vec<u64>,
}

impl Replay {
    pub fn new(seed: u64, ships: usize) -> Self {
        Replay { seed, ships: ships as u16, frames: Vec::new(), hashes: Vec::new() }
    }

    pub fn ticks(&self) -> u32 {
        (self.frames.len() / self.ships.max(1) as usize) as u32
    }

    /// Call right after `world.step(actions)`.
    pub fn record(&mut self, world: &World, actions: &[Action]) {
        self.frames.extend(actions.iter().map(encode));
        if world.tick % HASH_EVERY == 0 {
            self.hashes.push(world.state_hash());
        }
    }

    /// The actions to feed into the step that takes the world from `tick` to `tick + 1`.
    pub fn actions(&self, tick: u32) -> Option<Vec<Action>> {
        let n = self.ships as usize;
        let start = tick as usize * n;
        self.frames.get(start..start + n).map(|f| f.iter().map(|b| decode(*b)).collect())
    }

    /// The recorded hash for a world that has just reached `tick`, if one was taken there.
    pub fn hash_at(&self, tick: u32) -> Option<u64> {
        (tick > 0 && tick % HASH_EVERY == 0).then(|| self.hashes.get((tick / HASH_EVERY - 1) as usize).copied()).flatten()
    }

    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut f = io::BufWriter::new(std::fs::File::create(path)?);
        f.write_all(MAGIC)?;
        f.write_all(&self.seed.to_le_bytes())?;
        f.write_all(&self.ships.to_le_bytes())?;
        f.write_all(&(self.frames.len() as u64).to_le_bytes())?;
        f.write_all(&self.frames)?;
        f.write_all(&(self.hashes.len() as u32).to_le_bytes())?;
        for h in &self.hashes {
            f.write_all(&h.to_le_bytes())?;
        }
        f.flush()
    }

    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_string());
        let mut f = io::BufReader::new(std::fs::File::open(path)?);
        let mut magic = [0u8; 8];
        f.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(bad("not a Star Swarm replay"));
        }
        let mut b8 = [0u8; 8];
        let mut b2 = [0u8; 2];
        let mut b4 = [0u8; 4];
        f.read_exact(&mut b8)?;
        let seed = u64::from_le_bytes(b8);
        f.read_exact(&mut b2)?;
        let ships = u16::from_le_bytes(b2);
        f.read_exact(&mut b8)?;
        let len = u64::from_le_bytes(b8) as usize;
        if ships == 0 || len % ships as usize != 0 || len > 1 << 30 {
            return Err(bad("corrupt replay header"));
        }
        let mut frames = vec![0u8; len];
        f.read_exact(&mut frames)?;
        f.read_exact(&mut b4)?;
        let mut hashes = Vec::new();
        for _ in 0..u32::from_le_bytes(b4) {
            f.read_exact(&mut b8)?;
            hashes.push(u64::from_le_bytes(b8));
        }
        Ok(Replay { seed, ships, frames, hashes })
    }
}
