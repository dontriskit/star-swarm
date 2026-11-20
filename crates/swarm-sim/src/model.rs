//! Runs a policy trained by train/ppo.py: a small fully connected network read
//! from the weights file written by train/export.py. No ML runtime needed.

use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;

use crate::bot::Controller;
use crate::obs::{OBS_LEN, Observation};
use crate::sim::{Action, Config};

const MAGIC: &[u8; 8] = b"SWPOL011";
/// Action heads in the order the trainer uses: thrust, turn, weapons (nothing, guns, a
/// rocket, a mine), boost, buy, mode, deposit, brake.
const HEADS: [usize; 8] = [2, 3, 4, 2, crate::replay::BUY_CODES, crate::replay::VOTE_CODES, 2, 2];

struct Layer {
    inputs: usize,
    weights: Vec<f32>,
    biases: Vec<f32>,
}

impl Layer {
    fn read(f: &mut impl Read, inputs: usize, outputs: usize) -> io::Result<Self> {
        let mut floats = |n: usize| -> io::Result<Vec<f32>> {
            let mut bytes = vec![0u8; n * 4];
            f.read_exact(&mut bytes)?;
            Ok(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
        };
        Ok(Layer { inputs, weights: floats(inputs * outputs)?, biases: floats(outputs)? })
    }

    fn apply(&self, input: &[f32], relu: bool) -> Vec<f32> {
        self.weights
            .chunks_exact(self.inputs)
            .zip(&self.biases)
            .map(|(row, bias)| {
                let sum = row.iter().zip(input).map(|(w, x)| w * x).sum::<f32>() + bias;
                if relu { sum.max(0.0) } else { sum }
            })
            .collect()
    }
}

/// The network: two layers that read the observation, a memory cell (a GRU) whose content is
/// added to what they made of it, and one row of logits per action choice.
pub struct Net {
    read: [Layer; 2],
    /// The memory cell: what comes in and what it held, each giving the reset, update and
    /// candidate parts one after another.
    cell_in: Layer,
    cell_held: Layer,
    recall: Layer,
    logits: Layer,
    memory: usize,
}

impl Net {
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let bad = |what: String| io::Error::new(io::ErrorKind::InvalidData, what);
        let mut f = io::BufReader::new(std::fs::File::open(path)?);
        let mut magic = [0u8; 8];
        f.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(bad("not a Star Swarm policy file".into()));
        }
        let mut word = || -> io::Result<usize> {
            let mut b = [0u8; 4];
            f.read_exact(&mut b)?;
            Ok(u32::from_le_bytes(b) as usize)
        };
        let (obs_len, hidden, memory, heads) = (word()?, word()?, word()?, word()?);
        let sizes = (0..heads.min(16)).map(|_| word()).collect::<io::Result<Vec<_>>>()?;
        if obs_len != OBS_LEN {
            return Err(bad(format!("policy expects observations of {obs_len} values, this build produces {OBS_LEN}; retrain or re-export")));
        }
        if sizes != HEADS || hidden == 0 || hidden > 1 << 14 || memory == 0 || memory > 1 << 14 {
            return Err(bad(format!("unexpected policy shape: hidden {hidden}, heads {sizes:?}")));
        }
        let outputs = HEADS.iter().sum();
        Ok(Net {
            read: [Layer::read(&mut f, obs_len, hidden)?, Layer::read(&mut f, hidden, hidden)?],
            cell_in: Layer::read(&mut f, hidden, 3 * memory)?,
            cell_held: Layer::read(&mut f, memory, 3 * memory)?,
            recall: Layer::read(&mut f, memory, hidden)?,
            logits: Layer::read(&mut f, hidden, outputs)?,
            memory,
        })
    }

    /// The memory of a ship that remembers nothing yet.
    pub fn blank(&self) -> Vec<f32> {
        vec![0.0; self.memory]
    }

    /// Logits for every action choice, heads back to back. `state` is the ship's memory: it
    /// is read and then replaced by what the ship remembers after this decision.
    pub fn logits(&self, obs: &[f32], state: &mut [f32]) -> Vec<f32> {
        let mut hidden = self.read[1].apply(&self.read[0].apply(obs, true), true);
        let (from_in, from_held) = (self.cell_in.apply(&hidden, false), self.cell_held.apply(state, false));
        let m = self.memory;
        let squash = |x: f32| 1.0 / (1.0 + (-x).exp());
        for i in 0..m {
            let reset = squash(from_in[i] + from_held[i]);
            let update = squash(from_in[m + i] + from_held[m + i]);
            let candidate = (from_in[2 * m + i] + reset * from_held[2 * m + i]).tanh();
            state[i] = (1.0 - update) * candidate + update * state[i];
        }
        for (h, r) in hidden.iter_mut().zip(self.recall.apply(state, false)) {
            *h += r;
        }
        self.logits.apply(&hidden, false)
    }

    /// The most likely choice of every head.
    pub fn decide(&self, obs: &[f32], state: &mut [f32]) -> Action {
        let logits = self.logits(obs, state);
        let mut start = 0;
        let pick = HEADS.map(|size| {
            let head = &logits[start..start + size];
            start += size;
            (0..size).max_by(|a, b| head[*a].total_cmp(&head[*b])).unwrap_or(0)
        });
        Action {
            thrust: pick[0] == 1,
            turn: pick[1] as i8 - 1,
            fire: pick[2] == 1,
            rocket: pick[2] == 2,
            mine: pick[2] == 3,
            boost: pick[3] == 1,
            buy: crate::replay::buy_of(pick[4] as u8),
            vote: pick[5].checked_sub(1).and_then(|i| crate::sim::PLANS.get(i).copied()),
            deposit: pick[6] == 1,
            brake: pick[7] == 1,
        }
    }
}

/// Flies one ship with a trained network. Make one per ship: like in training,
/// it decides every `hold` ticks and keeps that action in between.
pub struct ModelBot {
    net: Arc<Net>,
    hold: u32,
    phase: u32,
    held: Action,
    obs: Vec<f32>,
    /// What the ship remembers from one decision to the next.
    state: Vec<f32>,
}

impl ModelBot {
    /// The trainer's default is one decision every 3 ticks.
    pub const HOLD: u32 = 3;

    pub fn new(net: Arc<Net>, hold: u32) -> Self {
        let state = net.blank();
        ModelBot { net, hold: hold.max(1), phase: 0, held: Action::default(), obs: vec![0.0; OBS_LEN], state }
    }
}

impl Controller for ModelBot {
    fn act(&mut self, obs: &Observation, cfg: &Config) -> Action {
        if self.phase == 0 {
            obs.fill(cfg, &mut self.obs);
            self.held = self.net.decide(&self.obs, &mut self.state);
        } else {
            // An upgrade is bought once per decision, not on every held tick.
            self.held.buy = None;
        }
        self.phase = (self.phase + 1) % self.hold;
        self.held
    }
}

/// A trained miner that still takes orders: the network flies the ship while
/// it is told to mine, the scripted bot takes over for every other order and,
/// with `self_defence`, whenever an enemy comes within its fighting range.
/// The network has only ever been trained to mine.
pub struct HybridBot {
    model: ModelBot,
    script: crate::bot::ScriptedBot,
    self_defence: bool,
}

impl HybridBot {
    /// How close an enemy has to be before a mining ship hands over to the scripted bot.
    const THREAT_RANGE: f32 = 260.0;

    pub fn new(net: Arc<Net>, self_defence: bool) -> Self {
        HybridBot { model: ModelBot::new(net, ModelBot::HOLD), script: crate::bot::ScriptedBot::default(), self_defence }
    }
}

impl Controller for HybridBot {
    fn act(&mut self, obs: &Observation, cfg: &Config) -> Action {
        let threatened = self.self_defence && obs.ships.iter().any(|s| s.enemy && s.dist < Self::THREAT_RANGE);
        if obs.order != crate::sim::OrderKind::Mine || threatened {
            self.script.act(obs, cfg)
        } else {
            self.model.act(obs, cfg)
        }
    }
}
