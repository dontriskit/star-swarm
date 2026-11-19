//! Many matches stepped side by side, for training.
//!
//! Every ship of every match is one agent. Agents are numbered match by match,
//! ship by ship, and all buffers are flat arrays in that order.

use rayon::prelude::*;
use swarm_sim::replay::decode;
use swarm_sim::{Action, Config, Controller, Event, OBS_LEN, ScriptedBot, World, observe, replay};

/// What an agent is paid for. Training stages differ only in these weights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rewards {
    /// Per crystal picked up.
    pub collect: f32,
    /// Per rock destroyed, times the rock's size (1 to 3).
    pub rock: f32,
    /// Per upgrade level bought.
    pub buy: f32,
    /// Per point of damage dealt to an enemy ship.
    pub hit: f32,
    /// Per enemy ship destroyed.
    pub kill: f32,
    /// Per point of damage taken by flying into a rock (subtracted).
    pub crash: f32,
    /// Per death (subtracted).
    pub death: f32,
}

impl Default for Rewards {
    fn default() -> Self {
        Rewards { collect: 1.0, rock: 0.5, buy: 2.0, hit: 0.0, kill: 0.0, crash: 0.05, death: 5.0 }
    }
}

/// Per-ship averages over one finished match.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EpisodeStats {
    pub minutes: f32,
    pub collected: f32,
    pub levels: f32,
    pub kills: f32,
    pub deaths: f32,
}

impl EpisodeStats {
    fn of(world: &World) -> Self {
        let n = world.ships.len() as f32;
        let mean = |f: fn(&swarm_sim::Ship) -> u32| world.ships.iter().map(f).sum::<u32>() as f32 / n;
        EpisodeStats {
            minutes: world.tick as f32 * world.cfg.dt / 60.0,
            collected: mean(|s| s.collected),
            levels: mean(|s| s.levels.iter().map(|l| *l as u32).sum()),
            kills: mean(|s| s.kills),
            deaths: mean(|s| s.deaths),
        }
    }
}

struct Slot {
    world: World,
    index: u64,
    episode: u64,
}

pub struct VecEnv {
    cfg: Config,
    rewards: Rewards,
    frame_skip: u32,
    seed: u64,
    slots: Vec<Slot>,
    finished: Vec<EpisodeStats>,
}

impl VecEnv {
    /// `frame_skip` is how many ticks each chosen action is held for.
    pub fn new(cfg: Config, envs: usize, seed: u64, frame_skip: u32, rewards: Rewards) -> Self {
        assert!(envs > 0 && frame_skip > 0 && cfg.teams * cfg.ships_per_team > 0);
        let slots = (0..envs as u64)
            .map(|index| Slot { world: World::new(cfg.clone(), Self::seed_of(seed, index, 0, envs as u64)), index, episode: 0 })
            .collect();
        VecEnv { cfg, rewards, frame_skip, seed, slots, finished: Vec::new() }
    }

    /// Every match of every slot gets its own seed, so runs are reproducible and maps never repeat.
    fn seed_of(seed: u64, index: u64, episode: u64, envs: u64) -> u64 {
        seed.wrapping_add(index).wrapping_add(episode.wrapping_mul(envs))
    }

    pub fn envs(&self) -> usize {
        self.slots.len()
    }

    pub fn ships(&self) -> usize {
        self.cfg.teams * self.cfg.ships_per_team
    }

    pub fn agents(&self) -> usize {
        self.envs() * self.ships()
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// The match currently running in one slot.
    pub fn world(&self, env: usize) -> &World {
        &self.slots[env].world
    }

    /// Writes every agent's observation; `out` holds `agents() * OBS_LEN` values.
    pub fn observe(&self, out: &mut [f32]) {
        let ships = self.ships();
        assert_eq!(out.len(), self.agents() * OBS_LEN);
        self.slots.par_iter().zip(out.par_chunks_mut(ships * OBS_LEN)).for_each(|(slot, out)| {
            for (ship, row) in out.chunks_mut(OBS_LEN).enumerate() {
                observe(&slot.world, ship).fill(&slot.world.cfg, row);
            }
        });
    }

    /// What the scripted bot would do for every agent, in the action encoding of [`VecEnv::step`].
    pub fn scripted(&self, out: &mut [u8]) {
        let ships = self.ships();
        assert_eq!(out.len(), self.agents());
        self.slots.par_iter().zip(out.par_chunks_mut(ships)).for_each(|(slot, out)| {
            let mut bot = ScriptedBot;
            for (ship, action) in out.iter_mut().enumerate() {
                *action = replay::encode(&bot.act(&observe(&slot.world, ship), &slot.world.cfg));
            }
        });
    }

    /// Holds each agent's action (one byte, see [`swarm_sim::replay::encode`]) for
    /// `frame_skip` ticks. `rewards` and `dones` get one entry per agent. A match
    /// that ends is replaced by a fresh one at once; its agents get `dones` = 1
    /// and the next observation comes from the new match.
    pub fn step(&mut self, actions: &[u8], rewards: &mut [f32], dones: &mut [u8]) {
        let ships = self.ships();
        let agents = self.agents();
        assert!(actions.len() == agents && rewards.len() == agents && dones.len() == agents);
        let (pay, frame_skip, seed, envs, cfg) = (self.rewards, self.frame_skip, self.seed, self.envs() as u64, &self.cfg);
        let finished: Vec<EpisodeStats> = self
            .slots
            .par_iter_mut()
            .zip(actions.par_chunks(ships))
            .zip(rewards.par_chunks_mut(ships))
            .zip(dones.par_chunks_mut(ships))
            .filter_map(|(((slot, actions), rewards), dones)| {
                rewards.fill(0.0);
                dones.fill(0);
                let mut held: Vec<Action> = actions.iter().map(|b| decode(*b)).collect();
                for _ in 0..frame_skip {
                    for event in slot.world.step(&held) {
                        match *event {
                            Event::Collected { ship } => rewards[ship as usize] += pay.collect,
                            Event::RockDestroyed { by: Some(ship), size } => rewards[ship as usize] += pay.rock * size as f32,
                            Event::RockDestroyed { by: None, .. } => {}
                            Event::Bought { ship, .. } => rewards[ship as usize] += pay.buy,
                            Event::Hit { by, damage, .. } => rewards[by as usize] += pay.hit * damage,
                            Event::Crashed { ship, damage } => rewards[ship as usize] -= pay.crash * damage,
                            Event::Killed { by, victim } => {
                                rewards[victim as usize] -= pay.death;
                                if let Some(by) = by {
                                    rewards[by as usize] += pay.kill;
                                }
                            }
                        }
                    }
                    // An upgrade is bought once, not once per held tick.
                    held.iter_mut().for_each(|a| a.buy = None);
                    if slot.world.done() {
                        let stats = EpisodeStats::of(&slot.world);
                        slot.episode += 1;
                        slot.world = World::new(cfg.clone(), Self::seed_of(seed, slot.index, slot.episode, envs));
                        dones.fill(1);
                        return Some(stats);
                    }
                }
                None
            })
            .collect();
        self.finished.extend(finished);
    }

    /// Stats of the matches that ended since the last call.
    pub fn take_stats(&mut self) -> Vec<EpisodeStats> {
        std::mem::take(&mut self.finished)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mining() -> Config {
        Config { teams: 1, ships_per_team: 4, rock_count: 40, world_size: 1400.0, max_ticks: 900, ..Config::default() }
    }

    fn run(env: &mut VecEnv, steps: usize) -> (f32, Vec<EpisodeStats>, Vec<f32>) {
        let agents = env.agents();
        let (mut actions, mut rewards, mut dones) = (vec![0u8; agents], vec![0f32; agents], vec![0u8; agents]);
        let mut obs = vec![0f32; agents * OBS_LEN];
        let (mut total, mut stats) = (0.0, Vec::new());
        for _ in 0..steps {
            env.scripted(&mut actions);
            env.step(&actions, &mut rewards, &mut dones);
            total += rewards.iter().sum::<f32>();
            stats.extend(env.take_stats());
        }
        env.observe(&mut obs);
        (total, stats, obs)
    }

    #[test]
    fn scripted_agents_earn_reward_and_matches_restart() {
        let mut env = VecEnv::new(mining(), 8, 1, 3, Rewards::default());
        let (total, stats, obs) = run(&mut env, 700);
        assert!(total > 0.0, "scripted bots earned {total}");
        // 900 ticks at 3 per step is 300 steps a match: two rounds of 8 matches have ended.
        assert_eq!(stats.len(), 16);
        assert!(stats.iter().all(|s| (s.minutes - 0.5).abs() < 1e-3 && s.collected > 0.0));
        assert!(obs.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn same_seed_same_rollout_whatever_the_thread_schedule() {
        let roll = || {
            let mut env = VecEnv::new(mining(), 16, 9, 3, Rewards::default());
            let (total, _, obs) = run(&mut env, 400);
            (total.to_bits(), obs.iter().map(|v| v.to_bits() as u64).sum::<u64>())
        };
        assert_eq!(roll(), roll());
    }

    #[test]
    fn a_bought_upgrade_is_charged_once_per_decision() {
        let cfg = Config { rock_count: 0, ..mining() };
        let mut env = VecEnv::new(cfg, 1, 1, 3, Rewards::default());
        env.slots[0].world.ships[0].cargo = 100;
        let buy = replay::encode(&Action { buy: Some(swarm_sim::Upgrade::Damage), ..Action::default() });
        let (mut rewards, mut dones) = (vec![0f32; 4], vec![0u8; 4]);
        env.step(&[buy, 0, 0, 0], &mut rewards, &mut dones);
        assert_eq!(env.world(0).ships[0].levels[swarm_sim::Upgrade::Damage as usize], 1);
        assert_eq!(rewards[0], Rewards::default().buy);
    }
}
