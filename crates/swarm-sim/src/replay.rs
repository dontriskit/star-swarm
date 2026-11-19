//! Recorded matches: the seed plus every ship's action on every tick, with a
//! state hash every [`HASH_EVERY`] ticks so a diverging playback is detected.

use crate::sim::{Action, Config, UPGRADES, World};
use std::io::{self, Read, Write};
use std::path::Path;

const MAGIC: &[u8; 8] = b"SWRP0002";
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

/// The parts of [`Config`] that differ between matches; everything else is the default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setup {
    pub world_size: f32,
    pub teams: u16,
    pub ships_per_team: u16,
    pub rock_count: u32,
    pub max_ticks: u32,
}

impl Setup {
    pub fn of(cfg: &Config) -> Self {
        Setup {
            world_size: cfg.world_size,
            teams: cfg.teams as u16,
            ships_per_team: cfg.ships_per_team as u16,
            rock_count: cfg.rock_count as u32,
            max_ticks: cfg.max_ticks,
        }
    }

    pub fn config(&self) -> Config {
        Config {
            world_size: self.world_size,
            teams: self.teams as usize,
            ships_per_team: self.ships_per_team as usize,
            rock_count: self.rock_count as usize,
            max_ticks: self.max_ticks,
            ..Config::default()
        }
    }

    pub fn ships(&self) -> usize {
        self.teams as usize * self.ships_per_team as usize
    }
}

/// A recorded match: how it was set up, its seed, and every action.
#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    pub seed: u64,
    pub setup: Setup,
    frames: Vec<u8>,
    hashes: Vec<u64>,
}

impl Replay {
    pub fn new(seed: u64, cfg: &Config) -> Self {
        Replay { seed, setup: Setup::of(cfg), frames: Vec::new(), hashes: Vec::new() }
    }

    /// The world this replay starts from.
    pub fn world(&self) -> World {
        World::new(self.setup.config(), self.seed)
    }

    pub fn ticks(&self) -> u32 {
        (self.frames.len() / self.setup.ships().max(1)) as u32
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
        let n = self.setup.ships();
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
        f.write_all(&self.setup.world_size.to_le_bytes())?;
        f.write_all(&self.setup.teams.to_le_bytes())?;
        f.write_all(&self.setup.ships_per_team.to_le_bytes())?;
        f.write_all(&self.setup.rock_count.to_le_bytes())?;
        f.write_all(&self.setup.max_ticks.to_le_bytes())?;
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
        f.read_exact(&mut b4)?;
        let world_size = f32::from_le_bytes(b4);
        f.read_exact(&mut b2)?;
        let teams = u16::from_le_bytes(b2);
        f.read_exact(&mut b2)?;
        let ships_per_team = u16::from_le_bytes(b2);
        f.read_exact(&mut b4)?;
        let rock_count = u32::from_le_bytes(b4);
        f.read_exact(&mut b4)?;
        let max_ticks = u32::from_le_bytes(b4);
        let setup = Setup { world_size, teams, ships_per_team, rock_count, max_ticks };
        f.read_exact(&mut b8)?;
        let len = u64::from_le_bytes(b8) as usize;
        let sane = world_size.is_finite() && world_size >= 100.0 && teams > 0 && teams <= 64 && ships_per_team > 0 && rock_count <= 100_000;
        if !sane || len % setup.ships() != 0 || len > 1 << 30 {
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
        Ok(Replay { seed, setup, frames, hashes })
    }
}
