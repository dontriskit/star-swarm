//! Reads the stream a trainer writes while it plays (see `VecEnv::stream_to`): each match's
//! seed and setup, then every ship's action on every tick. Replaying them shows exactly what
//! happens in training, because the simulation is deterministic.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use swarm_sim::{Action, Config, replay};

const MAGIC: &[u8; 8] = b"SWLIVE02";
/// Bytes in an episode record after its tag.
const EPISODE_LEN: usize = 8 + 4 + 2 + 2 + 4 + 4 + 4 + 4 + 2;

pub enum Item {
    /// A new match: its setup and seed.
    Episode(Box<Config>, u64),
    /// Every ship's action for one tick.
    Tick(Vec<Action>),
    /// The state hash the trainer had after the tick just played.
    Hash(u64),
}

pub struct Stream {
    pub path: PathBuf,
    file: File,
    /// Bytes read but not yet a whole record.
    pending: Vec<u8>,
    /// Ships per match in the episode being read; 0 before the first one.
    ships: usize,
    started: bool,
    /// Matches whose header has been read so far.
    pub episodes: usize,
    /// How many teams (the first ones) fly on the policy being trained; the rest are scripted.
    pub models: usize,
    /// Ticks read so far.
    pub ticks: usize,
    pub queue: VecDeque<Item>,
}

impl Stream {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(0))?;
        Ok(Stream { path: path.to_path_buf(), file, pending: Vec::new(), ships: 0, started: false, episodes: 0, models: 0, ticks: 0, queue: VecDeque::new() })
    }

    /// Ticks waiting to be played.
    pub fn backlog(&self) -> usize {
        self.queue.iter().filter(|i| matches!(i, Item::Tick(_))).count()
    }

    /// Reads whatever the trainer has written since the last call and queues the whole records in it.
    pub fn poll(&mut self) {
        let mut fresh = Vec::new();
        if self.file.read_to_end(&mut fresh).is_err() {
            return;
        }
        self.pending.extend(fresh);
        let mut at = 0;
        if !self.started {
            if self.pending.len() < MAGIC.len() {
                return;
            }
            if &self.pending[..MAGIC.len()] != MAGIC {
                // Not a stream this build understands: read nothing from it.
                self.pending.clear();
                return;
            }
            self.started = true;
            at = MAGIC.len();
        }
        let bytes = &self.pending;
        let word = |from: usize, len: usize| -> u64 { bytes[from..from + len].iter().rev().fold(0u64, |v, b| v << 8 | *b as u64) };
        while at < bytes.len() {
            let body = at + 1;
            match bytes[at] {
                b'E' if bytes.len() >= body + EPISODE_LEN => {
                    let seed = word(body, 8);
                    let cfg = Config {
                        radius: f32::from_bits(word(body + 8, 4) as u32),
                        teams: word(body + 12, 2) as usize,
                        ships_per_team: word(body + 14, 2) as usize,
                        rock_count: word(body + 16, 4) as usize,
                        max_ticks: word(body + 20, 4) as u32,
                        base_hp: f32::from_bits(word(body + 24, 4) as u32),
                        turret_damage: f32::from_bits(word(body + 28, 4) as u32),
                        ..Config::default()
                    };
                    self.ships = cfg.teams * cfg.ships_per_team;
                    self.models = word(body + 32, 2) as usize;
                    self.episodes += 1;
                    self.queue.push_back(Item::Episode(Box::new(cfg), seed));
                    at = body + EPISODE_LEN;
                }
                b'T' if self.ships > 0 && bytes.len() >= body + self.ships * 2 => {
                    let actions = (0..self.ships).map(|i| replay::decode(word(body + i * 2, 2) as u16)).collect();
                    self.queue.push_back(Item::Tick(actions));
                    self.ticks += 1;
                    at = body + self.ships * 2;
                }
                b'H' if bytes.len() >= body + 8 => {
                    self.queue.push_back(Item::Hash(word(body, 8)));
                    at = body + 8;
                }
                // A record still being written: wait for the rest.
                b'E' | b'T' | b'H' => break,
                // Garbage: nothing after it can be trusted.
                _ => {
                    at = bytes.len();
                }
            }
        }
        self.pending.drain(..at);
    }
}
