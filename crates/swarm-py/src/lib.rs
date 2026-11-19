//! Python bindings: the batched environment as one class that trades in raw bytes.
//!
//! Arrays cross the boundary as `bytes`; on the Python side
//! `numpy.frombuffer(..., dtype=numpy.float32)` turns them back into arrays.

use std::collections::HashMap;

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
    scripted: Vec<u8>,
}

#[pymethods]
impl Env {
    /// `rewards` overrides reward weights by name: collect, rock, buy, hit, kill, crash, death.
    #[new]
    #[pyo3(signature = (envs, seed=0, frame_skip=3, teams=1, ships_per_team=4, rock_count=40, world_size=1400.0, max_ticks=1800, rewards=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        envs: usize,
        seed: u64,
        frame_skip: u32,
        teams: usize,
        ships_per_team: usize,
        rock_count: usize,
        world_size: f32,
        max_ticks: u32,
        rewards: Option<HashMap<String, f32>>,
    ) -> PyResult<Self> {
        if envs == 0 || frame_skip == 0 || teams == 0 || ships_per_team == 0 || max_ticks == 0 || !(world_size >= 400.0) {
            return Err(PyValueError::new_err("envs, frame_skip, teams, ships_per_team and max_ticks must be positive; world_size at least 400"));
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
                other => return Err(PyValueError::new_err(format!("unknown reward weight: {other}"))),
            };
            *slot = value;
        }
        let cfg = Config { teams, ships_per_team, rock_count, world_size, max_ticks, ..Config::default() };
        let inner = VecEnv::new(cfg, envs, seed, frame_skip, pay);
        let agents = inner.agents();
        Ok(Env { obs: vec![0.0; agents * OBS_LEN], rewards: vec![0.0; agents], dones: vec![0; agents], scripted: vec![0; agents], inner })
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

    /// float32 observations, shape (agents, OBS_LEN).
    fn observe<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyBytes> {
        self.inner.observe(&mut self.obs);
        PyBytes::new(py, bytes_of(&self.obs))
    }

    /// Takes one action byte per agent: thrust | turn << 1 | fire << 3 | buy << 4, with
    /// turn in 0..=2 (right, straight, left) and buy 0 for nothing or 1..=8 for an upgrade.
    /// Returns (rewards as float32, dones as uint8), one entry per agent.
    fn step<'py>(&mut self, py: Python<'py>, actions: &[u8]) -> PyResult<(Bound<'py, PyBytes>, Bound<'py, PyBytes>)> {
        if actions.len() != self.inner.agents() {
            return Err(PyValueError::new_err(format!("expected {} actions, got {}", self.inner.agents(), actions.len())));
        }
        self.inner.step(actions, &mut self.rewards, &mut self.dones);
        Ok((PyBytes::new(py, bytes_of(&self.rewards)), PyBytes::new(py, &self.dones)))
    }

    /// The scripted bot's action byte for every agent in the current state.
    fn scripted<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyBytes> {
        self.inner.scripted(&mut self.scripted);
        PyBytes::new(py, &self.scripted)
    }

    /// Matches finished since the last call, as per-ship averages:
    /// (minutes, crystals collected, upgrade levels, kills, deaths).
    fn take_stats(&mut self) -> Vec<(f32, f32, f32, f32, f32)> {
        self.inner.take_stats().into_iter().map(|s| (s.minutes, s.collected, s.levels, s.kills, s.deaths)).collect()
    }
}

#[pymodule]
fn swarm_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Env>()?;
    m.add("OBS_LEN", OBS_LEN)?;
    Ok(())
}
