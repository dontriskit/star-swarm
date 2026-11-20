//! Recorded matches: the seed plus every ship's action on every tick, with a
//! state hash every [`HASH_EVERY`] ticks so a diverging playback is detected.

use crate::sim::{Action, Buy, Config, DroneKind, ORDNANCE, PLANS, TIER2, UPGRADES, World};
use std::io::{self, Read, Write};
use std::path::Path;

const MAGIC: &[u8; 8] = b"SWRP000D";
pub const HASH_EVERY: u32 = 300;

/// How many values [`buy_code`] can take.
pub const BUY_CODES: usize = 1 + UPGRADES.len() + TIER2.len() + 5 + ORDNANCE.len();

/// What to buy as a small number: 0 nothing, 1 to 8 an upgrade, 9 to 12 the next tier
/// (as a Lancer, Twin, Scatter or Ram when leaving tier 1), 13 an eco drone, 14 a base repair,
/// 15 a shield drone, 16 a combat drone, 17 selling a drone.
pub fn buy_code(buy: Option<Buy>) -> u8 {
    match buy {
        None => 0,
        Some(Buy::Upgrade(u)) => u as u8 + 1,
        Some(Buy::Tier(class)) => (UPGRADES.len() + TIER2.iter().position(|c| *c == class).map_or(0, |i| i + 1)) as u8,
        Some(Buy::Drone(DroneKind::Eco)) => 13,
        Some(Buy::Repair) => 14,
        Some(Buy::Drone(DroneKind::Shield)) => 15,
        Some(Buy::Drone(DroneKind::Combat)) => 16,
        Some(Buy::SellDrone) => 17,
        // 18 to 21: a stack of small rockets, big rockets, small mines, big mines.
        Some(Buy::Ordnance(kind)) => 18 + kind as u8,
    }
}

/// The inverse of [`buy_code`]; unknown codes buy nothing.
pub fn buy_of(code: u8) -> Option<Buy> {
    match code as usize {
        0 => None,
        n if n <= UPGRADES.len() => Some(Buy::Upgrade(UPGRADES[n - 1])),
        n if n <= UPGRADES.len() + TIER2.len() => Some(Buy::Tier(TIER2[n - UPGRADES.len() - 1])),
        13 => Some(Buy::Drone(DroneKind::Eco)),
        14 => Some(Buy::Repair),
        15 => Some(Buy::Drone(DroneKind::Shield)),
        16 => Some(Buy::Drone(DroneKind::Combat)),
        17 => Some(Buy::SellDrone),
        n @ 18..=21 => Some(Buy::Ordnance(ORDNANCE[n - 18])),
        _ => None,
    }
}

/// How many values the vote takes: no new vote, or one of the plans.
pub const VOTE_CODES: usize = 1 + PLANS.len();

/// Packs an action into 16 bits: thrust (bit 0), turn (bits 1-2: right, straight, left),
/// fire (bit 3), boost (bit 4), what to buy (bits 5-9, see [`buy_code`]), the mode (bits 10-12:
/// 0 keeps the current one, 1 to 4 farm, defend, attack A, attack B), deposit (bit 13), and brake (bit 14).
pub fn encode(a: &Action) -> u16 {
    let buy = buy_code(a.buy) as u16;
    let vote = a.vote.map_or(0, |p| p as u16 + 1);
    // Weapons share bits 3 and 15: guns alone are bit 3, a rocket bit 15, a mine both.
    let (guns, special) = if a.mine { (true, true) } else if a.rocket { (false, true) } else { (a.fire, false) };
    a.thrust as u16 | ((a.turn.clamp(-1, 1) + 1) as u16) << 1 | (guns as u16) << 3 | (special as u16) << 15 | (a.boost as u16) << 4 | buy << 5 | vote << 10 | (a.deposit as u16) << 13 | (a.brake as u16) << 14
}

pub fn decode(b: u16) -> Action {
    Action {
        thrust: b & 1 != 0,
        turn: (((b >> 1) & 3) as i8 - 1).clamp(-1, 1),
        fire: b & 8 != 0 && b & (1 << 15) == 0,
        rocket: b & 8 == 0 && b & (1 << 15) != 0,
        mine: b & 8 != 0 && b & (1 << 15) != 0,
        boost: b & 16 != 0,
        buy: buy_of(((b >> 5) & 31) as u8),
        vote: (((b >> 10) & 7) as usize).checked_sub(1).and_then(|i| PLANS.get(i).copied()),
        deposit: b & (1 << 13) != 0,
        brake: b & (1 << 14) != 0,
    }
}

/// The parts of [`Config`] that differ between matches; everything else is the default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setup {
    /// Radius of the planet.
    pub radius: f32,
    pub teams: u16,
    pub ships_per_team: u16,
    pub rock_count: u32,
    pub max_ticks: u32,
    /// Training changes how strong bases are from stage to stage.
    pub base_hp: f32,
    pub turret_damage: f32,
}

impl Setup {
    pub fn of(cfg: &Config) -> Self {
        Setup {
            radius: cfg.radius,
            teams: cfg.teams as u16,
            ships_per_team: cfg.ships_per_team as u16,
            rock_count: cfg.rock_count as u32,
            max_ticks: cfg.max_ticks,
            base_hp: cfg.base_hp,
            turret_damage: cfg.turret_damage,
        }
    }

    pub fn config(&self) -> Config {
        Config {
            radius: self.radius,
            teams: self.teams as usize,
            ships_per_team: self.ships_per_team as usize,
            rock_count: self.rock_count as usize,
            max_ticks: self.max_ticks,
            base_hp: self.base_hp,
            turret_damage: self.turret_damage,
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
    frames: Vec<u16>,
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
        f.write_all(&self.setup.radius.to_le_bytes())?;
        f.write_all(&self.setup.teams.to_le_bytes())?;
        f.write_all(&self.setup.ships_per_team.to_le_bytes())?;
        f.write_all(&self.setup.rock_count.to_le_bytes())?;
        f.write_all(&self.setup.max_ticks.to_le_bytes())?;
        f.write_all(&self.setup.base_hp.to_le_bytes())?;
        f.write_all(&self.setup.turret_damage.to_le_bytes())?;
        f.write_all(&(self.frames.len() as u64).to_le_bytes())?;
        for frame in &self.frames {
            f.write_all(&frame.to_le_bytes())?;
        }
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
        let radius = f32::from_le_bytes(b4);
        f.read_exact(&mut b2)?;
        let teams = u16::from_le_bytes(b2);
        f.read_exact(&mut b2)?;
        let ships_per_team = u16::from_le_bytes(b2);
        f.read_exact(&mut b4)?;
        let rock_count = u32::from_le_bytes(b4);
        f.read_exact(&mut b4)?;
        let max_ticks = u32::from_le_bytes(b4);
        f.read_exact(&mut b4)?;
        let base_hp = f32::from_le_bytes(b4);
        f.read_exact(&mut b4)?;
        let turret_damage = f32::from_le_bytes(b4);
        let setup = Setup { radius, teams, ships_per_team, rock_count, max_ticks, base_hp, turret_damage };
        f.read_exact(&mut b8)?;
        let len = u64::from_le_bytes(b8) as usize;
        let sane = radius.is_finite() && radius >= 100.0 && teams > 0 && teams <= 64 && ships_per_team > 0 && rock_count <= 100_000;
        if !sane || len % setup.ships() != 0 || len > 1 << 30 {
            return Err(bad("corrupt replay header"));
        }
        let mut bytes = vec![0u8; len * 2];
        f.read_exact(&mut bytes)?;
        let frames = bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        f.read_exact(&mut b4)?;
        let mut hashes = Vec::new();
        for _ in 0..u32::from_le_bytes(b4) {
            f.read_exact(&mut b8)?;
            hashes.push(u64::from_le_bytes(b8));
        }
        Ok(Replay { seed, setup, frames, hashes })
    }
}
