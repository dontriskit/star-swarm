//! Python bindings: the batched environment as one class that trades in raw bytes.
//!
//! Arrays cross the boundary as `bytes`; on the Python side
//! `numpy.frombuffer(..., dtype=numpy.float32)` turns them back into arrays.

use std::collections::HashMap;

use pyo3::buffer::PyBuffer;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use swarm_env::{Rewards, VecEnv};
use swarm_sim::{Config, OBS_LEN};

fn bytes_of(values: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding and any byte pattern is a valid u8; the slice
    // covers exactly the memory of `values` and borrows it for the same lifetime.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values)) }
}

#[pyclass]
struct Env {
    inner: VecEnv,
    obs: Vec<f32>,
    rewards: Vec<f32>,
    dones: Vec<u8>,
    scripted: Vec<u16>,
    actions: Vec<u16>,
    controlled: Vec<u8>,
}

#[pymethods]
impl Env {
    /// `rewards` overrides reward weights by name: collect, rock, buy, hit, kill, crash, death,
    /// base_hit, base_lost, win, lose, hurt, time, draw, deposit, base_level, rock_hit, edge, hull, bare, place. Teams `0..learners` take the actions passed to `step`;
    /// the rest fly on the scripted bot (default: every team is a learner).
    #[new]
    #[pyo3(signature = (envs, seed=0, frame_skip=3, teams=3, ships_per_team=9, rock_count=270, radius=1200.0, max_ticks=36000, rewards=None, learners=None, base_hp=1000.0, turret_damage=15.0))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        envs: usize,
        seed: u64,
        frame_skip: u32,
        teams: usize,
        ships_per_team: usize,
        rock_count: usize,
        radius: f32,
        max_ticks: u32,
        rewards: Option<HashMap<String, f32>>,
        learners: Option<usize>,
        base_hp: f32,
        turret_damage: f32,
    ) -> PyResult<Self> {
        let learners = learners.unwrap_or(teams);
        if learners > teams || !(base_hp > 0.0) {
            return Err(PyValueError::new_err("learners cannot exceed teams; base_hp must be positive"));
        }
        if envs == 0 || frame_skip == 0 || teams == 0 || ships_per_team == 0 || max_ticks == 0 || !(radius >= 100.0) {
            return Err(PyValueError::new_err("envs, frame_skip, teams, ships_per_team and max_ticks must be positive; radius at least 100"));
        }
        let mut pay = Rewards::default();
        for (name, value) in rewards.unwrap_or_default() {
            let slot = match name.as_str() {
                "collect" => &mut pay.collect,
                "rock" => &mut pay.rock,
                "buy" => &mut pay.buy,
                "hit" => &mut pay.hit,
                "kill" => &mut pay.kill,
                "crash" => &mut pay.crash,
                "death" => &mut pay.death,
                "base_hit" => &mut pay.base_hit,
                "base_lost" => &mut pay.base_lost,
                "win" => &mut pay.win,
                "lose" => &mut pay.lose,
                "hurt" => &mut pay.hurt,
                "time" => &mut pay.time,
                "draw" => &mut pay.draw,
                "deposit" => &mut pay.deposit,
                "base_level" => &mut pay.base_level,
                "rock_hit" => &mut pay.rock_hit,
                "hull" => &mut pay.hull,
                "bare" => &mut pay.bare,
                "place" => &mut pay.place,
                "edge" => &mut pay.edge,
                other => return Err(PyValueError::new_err(format!("unknown reward weight: {other}"))),
            };
            *slot = value;
        }
        let cfg = Config { teams, ships_per_team, rock_count, radius, max_ticks, base_hp, turret_damage: turret_damage.max(0.0), ..Config::default() };
        let inner = VecEnv::with_learners(cfg, envs, seed, frame_skip, pay, learners);
        let agents = inner.agents();
        let mut controlled = vec![0; agents];
        inner.controlled(&mut controlled);
        Ok(Env { obs: vec![0.0; agents * OBS_LEN], rewards: vec![0.0; agents], dones: vec![0; agents], scripted: vec![0; agents], actions: vec![0; agents], controlled, inner })
    }

    #[getter]
    fn agents(&self) -> usize {
        self.inner.agents()
    }

    #[getter]
    fn ships(&self) -> usize {
        self.inner.ships()
    }

    /// Seconds of game time in one simulation tick; a decision covers frame_skip of them.
    #[getter]
    fn tick_seconds(&self) -> f32 {
        self.inner.config().dt
    }

    /// Writes the float32 observations, shape (agents, OBS_LEN), straight into a buffer the
    /// caller owns (a numpy array, or the numpy view of a torch tensor), with no copy in between.
    fn observe_into(&mut self, out: &Bound<'_, PyAny>) -> PyResult<()> {
        let buffer = PyBuffer::<f32>::get(out)?;
        let wanted = self.inner.agents() * OBS_LEN;
        if buffer.readonly() || !buffer.is_c_contiguous() || buffer.item_count() != wanted {
            return Err(PyValueError::new_err(format!("observe_into needs a writable, contiguous float32 buffer of {wanted} values")));
        }
        // SAFETY: the buffer is float32 (checked by PyBuffer::<f32>::get), writable, contiguous and
        // exactly `wanted` items long (checked above); the GIL is held for the whole call, so no
        // Python code reads or resizes it while we write; and `buffer` keeps it alive until we return.
        let slice = unsafe { std::slice::from_raw_parts_mut(buffer.buf_ptr().cast::<f32>(), wanted) };
        self.inner.observe(slice);
        Ok(())
    }

    /// float32 observations, shape (agents, OBS_LEN), as a new bytes object. Simple, but it
    /// copies everything; [`Env::observe_into`] does not.
    fn observe<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyBytes> {
        self.inner.observe(&mut self.obs);
        PyBytes::new(py, bytes_of(&self.obs))
    }

    /// Takes one little-endian uint16 per agent:
    /// thrust | turn << 1 | fire << 3 | boost << 4 | buy << 5 | vote << 10 | deposit << 13,
    /// with turn in 0..=2 (right, straight, left); buy 0 for nothing, 1..=8 for an upgrade,
    /// 9..=12 for the next tier (as a Lancer, Twin, Scatter or Ram when leaving tier 1), 13 for an
    /// eco drone, 14 for a base repair, 15 for a shield drone, 16 for a combat drone, 17 to sell a
    /// drone (buying only works inside the base); deposit 1 to hand crystals over; and vote 0 to
    /// keep the ship's mode or 1..=4 to switch to farm, defend, attack A, attack B.
    /// Returns (rewards as float32, dones as uint8), one entry per agent.
    fn step<'py>(&mut self, py: Python<'py>, actions: &[u8]) -> PyResult<(Bound<'py, PyBytes>, Bound<'py, PyBytes>)> {
        if actions.len() != self.inner.agents() * 2 {
            return Err(PyValueError::new_err(format!("expected {} uint16 actions, got {} bytes", self.inner.agents(), actions.len())));
        }
        for (slot, bytes) in self.actions.iter_mut().zip(actions.chunks_exact(2)) {
            *slot = u16::from_le_bytes([bytes[0], bytes[1]]);
        }
        self.inner.step(&self.actions, &mut self.rewards, &mut self.dones);
        Ok((PyBytes::new(py, bytes_of(&self.rewards)), PyBytes::new(py, &self.dones)))
    }

    /// The scripted bot's action for every agent in the current state, as little-endian uint16.
    fn scripted<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyBytes> {
        self.inner.scripted(&mut self.scripted);
        let bytes: Vec<u8> = self.scripted.iter().flat_map(|a| a.to_le_bytes()).collect();
        PyBytes::new(py, &bytes)
    }

    /// Streams the first match slot to a file as it is played, for `swarm-gui --watch`.
    fn stream(&mut self, path: &str) -> PyResult<()> {
        self.inner.stream_to(path).map_err(|e| PyValueError::new_err(format!("cannot stream to {path}: {e}")))
    }

    /// uint8 per agent: 1 where `step` uses the action passed in, 0 where the ship is scripted.
    fn controlled<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.controlled)
    }

    /// Matches finished since the last call, seen from team 0, one list of floats each:
    /// minutes, crystals, upgrade levels, kills, deaths (per-ship averages), won, lost (0 or 1),
    /// slot, matches that slot had finished before, then the share of ships in each class
    /// (Basic, Lancer, Twin, Scatter, Ram), the mean level of each of the eight upgrades, the share of
    /// ships at tier 1, 2 and 3, the share of ship-time spent in each mode (farm, defend,
    /// attack A, attack B), the base level reached, crystals handed over per ship, drones per ship.
    fn take_stats(&mut self) -> Vec<Vec<f32>> {
        self.inner
            .take_stats()
            .into_iter()
            .map(|s| {
                let mut row = vec![s.minutes, s.collected, s.levels, s.kills, s.deaths, s.won as u8 as f32, s.lost as u8 as f32, s.env as f32, s.episode as f32];
                row.extend(s.classes);
                row.extend(s.upgrades);
                row.extend(s.tiers);
                row.extend(s.plans);
                row.extend([s.base_level, s.deposited, s.drones]);
                row
            })
            .collect()
    }
}

#[pymodule]
fn swarm_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Env>()?;
    m.add("OBS_LEN", OBS_LEN)?;
    Ok(())
}
